use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use ast_grep_core::Pattern;
use ast_grep_language::LanguageExt;
use crossbeam_channel::Sender;
use jwalk::WalkDir;
use rayon::prelude::*;

use crate::file_encoding::{read_text_file, read_text_file_as, FileEncoding, FileEncodingPreference};
use crate::i18n::{Tr, UiLanguage};
use crate::lang::SupportedLanguage;

/// 検索モード
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SearchMode {
    /// AST パターン検索（マッチ範囲は ast-grep 本体・CLI と同じ）
    AstGrep,
    /// 上と同じ検索。コードパネルで CLI 風のテキスト出力を表示
    AstGrepRaw,
    /// 通常の文字列検索
    PlainText,
    /// 正規表現検索
    Regex,
}

impl SearchMode {
    pub fn is_ast_mode(self) -> bool {
        matches!(self, Self::AstGrep | Self::AstGrepRaw)
    }
}

/// バックグラウンド検索から UI へ送るメッセージ
#[derive(Debug)]
pub enum SearchMessage {
    FileResult(FileResult),
    Progress { scanned: usize },
    Done {
        elapsed_ms: u64,
        /// `max_search_hits > 0` のとき、収集件数が上限に達した
        hit_limit_reached: bool,
    },
    #[allow(dead_code)]
    Error(String),
}

/// 1ファイルのマッチ結果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileResult {
    pub path: PathBuf,
    pub relative_path: String,
    /// このファイルを解析した言語（ハイライト・JSON 用）。Auto モード時は拡張子から決定。
    #[serde(default = "default_source_language")]
    pub source_language: SupportedLanguage,
    /// このファイルを読み込む際に実際に使った文字コード
    #[serde(default = "default_text_encoding")]
    pub text_encoding: FileEncoding,
    pub matches: Vec<MatchItem>,
}

fn default_source_language() -> SupportedLanguage {
    SupportedLanguage::Rust
}

fn default_text_encoding() -> FileEncoding {
    FileEncoding::Utf8
}

/// 1マッチの情報
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MatchItem {
    pub line_start: usize, // 1-based
    pub col_start: usize,  // byteオフセット（行内）
    pub line_end: usize,
    pub col_end: usize,
    pub matched_text: String,
    /// マッチが及ぶ行のソース上の全文（`line_start`〜`line_end` の行を `\n` で連結）
    #[serde(default)]
    pub span_lines_text: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

impl MatchItem {
    /// マッチ範囲を含む行の全文＋前後コンテキスト（表の「元コード」列・エクスポート用）
    pub fn program_with_context(&self) -> String {
        let center = if self.span_lines_text.is_empty() {
            self.matched_text.as_str()
        } else {
            self.span_lines_text.as_str()
        };
        let mut s = String::new();
        for line in &self.context_before {
            s.push_str(line);
            s.push('\n');
        }
        s.push_str(center);
        if !self.context_after.is_empty() {
            if !center.is_empty() && !s.ends_with('\n') {
                s.push('\n');
            }
            for line in &self.context_after {
                s.push_str(line);
                s.push('\n');
            }
        }
        s.trim_end_matches('\n').to_string()
    }

    /// 後方互換: `program_with_context()` と同じ（従来の「マッチ＋コンテキスト」一体表示を置き換え）
    pub fn text_with_context(&self) -> String {
        self.program_with_context()
    }
}

/// ファイルフィルタ文字列をパースして正規表現のリストを返す
///
/// フォーマット: `*.rs;src/*.java;test_.*` のように `;` 区切りで指定する。
/// 各エントリはファイル名全体に対してマッチされる。
/// 空文字列の場合は `None`（言語デフォルト拡張子を使用）を返す。
fn parse_file_filter(filter: &str) -> Option<Vec<regex::Regex>> {
    let trimmed = filter.trim();
    if trimmed.is_empty() {
        return None;
    }
    let patterns: Vec<regex::Regex> = trimmed
        .split(';')
        .filter_map(|p| {
            let p = p.trim();
            if p.is_empty() {
                return None;
            }
            // glob風の `*` を `.*` に変換（簡易サポート）
            let regex_pat = p.replace('.', r"\.").replace('*', ".*");
            // ファイル名全体にマッチさせるため ^ と $ でアンカー
            regex::Regex::new(&format!("^{regex_pat}$")).ok()
        })
        .collect();
    if patterns.is_empty() { None } else { Some(patterns) }
}

/// バックグラウンドで検索を実行する
///
/// - jwalk で並列ディレクトリ走査（不要なディレクトリを事前フィルタ）
/// - rayon で各ファイルを並列処理
/// - `par_bridge()` でウォーク結果をストリーミング処理（全収集せずに開始）
pub fn spawn_search(
    search_dir: String,
    pattern: String,
    lang: SupportedLanguage,
    search_mode: SearchMode,
    context_lines: usize,
    file_filter: String,
    file_encoding_preference: FileEncodingPreference,
    max_file_size_bytes: u64,
    max_search_hits: usize,
    skip_dirs_str: String,
    ui_lang: UiLanguage,
    tx: Sender<SearchMessage>,
    egui_ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let start = Instant::now();
        let pattern_str = Arc::new(pattern);
        let search_dir_path = Arc::new(Path::new(&search_dir).to_path_buf());
        let tx = Arc::new(tx);
        let scanned = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let hits_accepted = Arc::new(AtomicUsize::new(0));
        let hit_limit_reached = Arc::new(AtomicBool::new(false));

        // 正規表現モードはここでコンパイル（ファイルごとに再コンパイルしない）
        let compiled_regex: Option<Arc<regex::Regex>> = if search_mode == SearchMode::Regex {
            match regex::Regex::new(pattern_str.as_str()) {
                Ok(re) => Some(Arc::new(re)),
                Err(e) => {
                    let msg = Tr(ui_lang).err_regex_compile(e);
                    let _ = tx.send(SearchMessage::Error(msg));
                    egui_ctx.request_repaint();
                    return;
                }
            }
        } else {
            None
        };

        // スキップディレクトリを HashSet に変換（O(1) ルックアップ）
        let skip_dirs: std::collections::HashSet<String> = skip_dirs_str
            .split(';')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // ファイルフィルタの解析
        let custom_patterns = parse_file_filter(&file_filter);
        // PlainText/Regex でファイルフィルタ未指定の場合は全ファイルを対象にする
        let use_lang_filter = search_mode.is_ast_mode() && custom_patterns.is_none();
        let ext_set: std::collections::HashSet<&str> = if use_lang_filter {
            match lang {
                SupportedLanguage::Auto => SupportedLanguage::union_extensions_for_auto_filter(),
                _ => lang.extensions().iter().copied().collect(),
            }
        } else {
            std::collections::HashSet::new()
        };

        // jwalk: スキップするディレクトリを process_read_dir で除外し走査負荷を削減
        let walker = WalkDir::new(&search_dir)
            .process_read_dir(move |_, _, _, children| {
                children.retain(|entry_result| {
                    let Ok(entry) = entry_result else { return true };
                    if !entry.file_type.is_dir() {
                        return true;
                    }
                    let name = entry.file_name.to_string_lossy();
                    !skip_dirs.contains(name.as_ref())
                });
            });

        // par_bridge() でイテレータを直接 rayon に渡し、全ファイル収集前から処理開始
        walker
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                if !e.file_type().is_file() {
                    return false;
                }
                let file_name = e.file_name().to_string_lossy();
                if let Some(patterns) = &custom_patterns {
                    // カスタムパターンに1つでもマッチすれば対象
                    patterns.iter().any(|re| re.is_match(&file_name))
                } else if use_lang_filter {
                    // AstGrep モードのみ言語拡張子でフィルタ
                    e.path()
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext_set.contains(ext))
                        .unwrap_or(false)
                } else {
                    // PlainText/Regex はすべてのファイルを対象
                    true
                }
            })
            .par_bridge() // rayon 並列処理に橋渡し
            .for_each(|entry| {
                let path = entry.path();

                if max_search_hits != 0
                    && hits_accepted.load(Ordering::Relaxed) >= max_search_hits
                {
                    return;
                }

                // 巨大ファイルをスキップ（メタデータ取得のみ）
                if let Ok(meta) = std::fs::metadata(&path) {
                    if meta.len() > max_file_size_bytes {
                        return;
                    }
                }

                let relative_path = path
                    .strip_prefix(search_dir_path.as_path())
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();

                let file_lang = if lang == SupportedLanguage::Auto {
                    match SupportedLanguage::from_path(&path) {
                        Some(l) => l,
                        None => {
                            if search_mode.is_ast_mode() {
                                return;
                            }
                            SupportedLanguage::Rust
                        }
                    }
                } else {
                    lang
                };

                // ファイル読み込み（ファイルサイズに合わせてバッファを事前確保）
                let decoded = match read_text_file(&path, file_encoding_preference) {
                    Ok(decoded) => decoded,
                    Err(_) => return, // バイナリ等は無視
                };
                let text_encoding = decoded.encoding;
                let source = decoded.text;

                if max_search_hits != 0
                    && hits_accepted.load(Ordering::Relaxed) >= max_search_hits
                {
                    return;
                }

                let count = scanned.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                // 進捗を適度な頻度で通知（毎ファイルは重すぎる）
                if count % 50 == 0 {
                    let _ = tx.send(SearchMessage::Progress { scanned: count });
                    egui_ctx.request_repaint();
                }

                let lines: Vec<&str> = source.lines().collect();

                let hits_acc = Arc::clone(&hits_accepted);
                let limit_flag = Arc::clone(&hit_limit_reached);

                let matches: Vec<MatchItem> = match search_mode {
                    SearchMode::AstGrep | SearchMode::AstGrepRaw => {
                        let ast_lang = match file_lang.to_support_lang() {
                            Some(l) => l,
                            None => return,
                        };
                        // ast-grep でパターンマッチ
                        // Pattern::try_new で検証し、無効パターン（MultipleNode等）はスキップ
                        let compiled_pat = match Pattern::try_new(pattern_str.as_str(), ast_lang) {
                            Ok(p) => p,
                            Err(_) => return,
                        };

                        let root = ast_lang.ast_grep(&source);
                        let mut out = Vec::new();
                        for node in root.root().find_all(&compiled_pat) {
                            if !try_accept_hit(&hits_acc, max_search_hits, &limit_flag) {
                                break;
                            }
                            let matched_node = node.get_node().clone();
                            let node_range = matched_node.range();
                            // ast-grep 本体（CLI）と同じノード範囲を使う
                            let matched_end = node_range.end.min(source.len());
                            let (line_start, col_start) =
                                byte_offset_to_line_col(&source, node_range.start);
                            let (line_end, col_end) =
                                byte_offset_to_line_col(&source, matched_end);
                            let matched_text = source
                                .get(node_range.start..matched_end)
                                .map(str::to_owned)
                                .unwrap_or_else(|| node.text().to_string());
                            out.push(build_match_item(
                                line_start, col_start, line_end, col_end,
                                matched_text, &lines, context_lines,
                            ));
                        }
                        out
                    }
                    SearchMode::PlainText => {
                        // 行ごとにスキャンして部分一致を検索
                        let needle = pattern_str.as_str();
                        let mut result = Vec::new();
                        'lines: for (line_idx, line) in lines.iter().enumerate() {
                            let mut search_start = 0;
                            while let Some(byte_pos) = line[search_start..].find(needle) {
                                if !try_accept_hit(&hits_acc, max_search_hits, &limit_flag) {
                                    break 'lines;
                                }
                                let col_start = search_start + byte_pos;
                                let col_end = col_start + needle.len();
                                let matched_text = line[col_start..col_end].to_string();
                                result.push(build_match_item(
                                    line_idx, col_start, line_idx, col_end,
                                    matched_text, &lines, context_lines,
                                ));
                                search_start = col_end;
                                if search_start >= line.len() { break; }
                            }
                        }
                        result
                    }
                    SearchMode::Regex => {
                        // 正規表現で全体を検索（行跨ぎマッチも対応）
                        let re = compiled_regex.as_ref().unwrap();
                        let mut out = Vec::new();
                        for mat in re.find_iter(&source) {
                            if !try_accept_hit(&hits_acc, max_search_hits, &limit_flag) {
                                break;
                            }
                            let (line_start, col_start) =
                                byte_offset_to_line_col(&source, mat.start());
                            let (line_end, col_end) =
                                byte_offset_to_line_col(&source, mat.end());
                            out.push(build_match_item(
                                line_start, col_start, line_end, col_end,
                                mat.as_str().to_string(), &lines, context_lines,
                            ));
                        }
                        out
                    }
                };

                if !matches.is_empty() {
                    let _ = tx.send(SearchMessage::FileResult(FileResult {
                        path: path.to_path_buf(),
                        relative_path,
                        source_language: file_lang,
                        text_encoding,
                        matches,
                    }));
                    egui_ctx.request_repaint();
                }
            });

        // 最終進捗を送信
        let final_count = scanned.load(std::sync::atomic::Ordering::Relaxed);
        let _ = tx.send(SearchMessage::Progress { scanned: final_count });
        let elapsed_ms = start.elapsed().as_millis() as u64;
        let hit_limit_reached = hit_limit_reached.load(Ordering::Relaxed);
        let _ = tx.send(SearchMessage::Done {
            elapsed_ms,
            hit_limit_reached,
        });
        egui_ctx.request_repaint();
    });
}

/// 0-based の行範囲について、前後 `context_lines` 行分のコンテキストを取り出す（`build_match_item` と同一ロジック）
fn slice_context_lines(
    lines: &[&str],
    line_start_0: usize,
    line_end_0: usize,
    context_lines: usize,
) -> (Vec<String>, Vec<String>) {
    let ctx_before_start = line_start_0.saturating_sub(context_lines);
    let ctx_after_end = (line_end_0 + context_lines).min(lines.len().saturating_sub(1));

    let context_before = if line_start_0 > 0 {
        lines[ctx_before_start..line_start_0]
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        vec![]
    };

    // context_lines==0 のとき ctx_after_end==line_end となり (line_end+1)..=line_end は無効なので分岐する
    let context_after = if line_end_0 + 1 < lines.len() && line_end_0 + 1 <= ctx_after_end {
        lines[(line_end_0 + 1)..=ctx_after_end]
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        vec![]
    };

    (context_before, context_after)
}

/// `line_start_0`〜`line_end_0`（0-based・両端含む）の行をソース行として連結する
fn join_span_lines(lines: &[&str], line_start_0: usize, line_end_0: usize) -> String {
    if lines.is_empty() || line_start_0 >= lines.len() {
        return String::new();
    }
    let end = line_end_0.min(lines.len() - 1);
    if line_start_0 > end {
        return String::new();
    }
    lines[line_start_0..=end].join("\n")
}

/// MatchItem を生成するヘルパー（0-based の行/列を受け取り 1-based に変換）
fn build_match_item(
    line_start: usize,
    col_start: usize,
    line_end: usize,
    col_end: usize,
    matched_text: String,
    lines: &[&str],
    context_lines: usize,
) -> MatchItem {
    let (context_before, context_after) = slice_context_lines(lines, line_start, line_end, context_lines);
    let span_lines_text = join_span_lines(lines, line_start, line_end);

    MatchItem {
        line_start: line_start + 1, // 1-based
        col_start,
        line_end: line_end + 1,
        col_end,
        matched_text,
        span_lines_text,
        context_before,
        context_after,
    }
}

/// UI の「コンテキスト行数」変更に合わせ、既存の検索結果の前後コンテキストだけを再計算する（再検索はしない）
pub fn refresh_match_contexts(results: &mut [FileResult], context_lines: usize) {
    for file in results.iter_mut() {
        let Ok(source) = read_text_file_as(&file.path, file.text_encoding.clone()) else {
            continue;
        };
        let lines: Vec<&str> = source.lines().collect();
        for m in &mut file.matches {
            let line_start_0 = m.line_start.saturating_sub(1);
            let line_end_0 = m.line_end.saturating_sub(1);
            let (before, after) = slice_context_lines(&lines, line_start_0, line_end_0, context_lines);
            m.context_before = before;
            m.context_after = after;
            m.span_lines_text = join_span_lines(&lines, line_start_0, line_end_0);
        }
    }
}

/// バイトオフセットを (0-based 行インデックス, 行内バイトオフセット) に変換
fn byte_offset_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let prefix = &source[..byte_offset.min(source.len())];
    let line = prefix.chars().filter(|&c| c == '\n').count();
    let col = prefix.rfind('\n').map(|i| byte_offset - i - 1).unwrap_or(byte_offset);
    (line, col)
}

/// ファイル出力用の検索条件（UI の検索パラメータと対応）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchConditions {
    pub search_dir: String,
    pub pattern: String,
    pub selected_lang: SupportedLanguage,
    pub context_lines: usize,
    pub file_filter: String,
    #[serde(default)]
    pub file_encoding_preference: FileEncodingPreference,
    pub max_file_size_mb: u64,
    /// 収集するヒット数の上限（0 で無制限）
    #[serde(default = "default_max_search_hits")]
    pub max_search_hits: usize,
    pub skip_dirs: String,
    pub search_mode: SearchMode,
}

pub(crate) fn default_max_search_hits() -> usize {
    100_000
}

/// 検索統計
#[derive(Debug, Default, Clone)]
pub struct SearchStats {
    pub total_matches: usize,
    pub total_files: usize,
    pub elapsed_ms: u64,
    pub scanned: usize,
    /// ヒット上限により結果が打ち切られた
    pub hit_limit_reached: bool,
}

/// `max == 0` のときは無制限。それ以外は CAS でカウンタを安全にインクリメントし、上限超えなら `false` を返す。
fn try_accept_hit(hits: &AtomicUsize, max: usize, limit_reached: &AtomicBool) -> bool {
    if max == 0 {
        return true;
    }
    loop {
        let current = hits.load(Ordering::Relaxed);
        if current >= max {
            limit_reached.store(true, Ordering::Relaxed);
            return false;
        }
        if hits
            .compare_exchange(current, current + 1, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            return true;
        }
    }
}
