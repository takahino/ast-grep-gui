use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use ast_grep_language::LanguageExt;
use crossbeam_channel::Sender;
use jwalk::WalkDir;
use rayon::prelude::*;

use crate::ast_pattern::compile_strategies;
use crate::file_encoding::{read_text_file, read_text_file_as, FileEncoding, FileEncodingPreference};
use crate::i18n::{Tr, UiLanguage};
use crate::lang::SupportedLanguage;
use crate::receiver_hint;

/// 検索モード
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SearchMode {
    /// AST パターン検索（マッチ範囲は ast-grep 本体・CLI と同じ）
    AstGrep,
    /// スペース区切りトークンを順序通りに検索（空白の有無は問わない）
    TokenSearch,
    /// 通常の文字列検索
    PlainText,
    /// 正規表現検索
    Regex,
}

impl SearchMode {
    pub fn is_ast_mode(self) -> bool {
        matches!(self, Self::AstGrep)
    }
}

/// 文字列検索モード（`SearchMode::PlainText`）のオプション
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct PlainTextSearchOptions {
    /// 大文字小文字を区別しない
    #[serde(default)]
    pub case_insensitive: bool,
    /// 前後が空白（Unicode `is_whitespace`）または行頭／行末で区切られた一致
    #[serde(default)]
    pub whole_word: bool,
}

/// バックグラウンド検索から UI へ送るメッセージ
#[derive(Debug)]
pub enum SearchMessage {
    FileResult {
        job_id: usize,
        file: FileResult,
    },
    Progress {
        job_id: usize,
        scanned: usize,
    },
    Done {
        job_id: usize,
        elapsed_ms: u64,
        /// `max_search_hits > 0` のとき、収集件数が上限に達した
        hit_limit_reached: bool,
    },
    Error {
        job_id: usize,
        msg: String,
    },
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

#[derive(Debug)]
struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut line_starts = Vec::with_capacity(source.bytes().filter(|&b| b == b'\n').count() + 1);
        line_starts.push(0);
        for (idx, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(idx + 1);
            }
        }
        Self { line_starts }
    }

    /// バイトオフセットを (0-based 行インデックス, 行内バイトオフセット) に変換
    fn byte_offset_to_line_col(&self, source: &str, byte_offset: usize) -> (usize, usize) {
        let capped = byte_offset.min(source.len());
        let line = self.line_starts.partition_point(|&start| start <= capped).saturating_sub(1);
        let col = capped.saturating_sub(self.line_starts[line]);
        (line, col)
    }
}

/// 型ヒントセル（表・エクスポート）の表示区分。`—` 一種類だと「スロットなし」と「推定失敗」が区別しづらいため分ける。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeHintCell {
    /// 推定できた型文字列
    Inferred(String),
    /// このマッチでは該当キャプチャがない（列は他行の最大に合わせたパディング）
    NoSlot,
    /// スロットはあるが型を推定できなかった
    Unknown,
}

impl TypeHintCell {
    /// Markdown / HTML / Excel 向けの短い記号（`·` = スロットなし、`?` = 推定失敗）
    pub fn as_export_str(&self) -> &str {
        match self {
            TypeHintCell::Inferred(s) => s.as_str(),
            TypeHintCell::NoSlot => "·",
            TypeHintCell::Unknown => "?",
        }
    }
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
    /// 単一メタ変数名 → 構文ベースで推定した型（表示専用・best-effort）
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub type_hints: BTreeMap<String, String>,
}

impl MatchItem {
    /// パターン内メタ変数名順の型ヒント（欠損は空文字列）
    pub fn type_hint_for_metavar(&self, metavar: &str) -> Option<&str> {
        self.type_hints.get(metavar).map(|s| s.as_str())
    }

    /// 表・エクスポート用: 列キーごとに「型あり / スロットなし / 推定失敗」を判定する。
    pub fn type_hint_cell(&self, key: &str) -> TypeHintCell {
        if key.ends_with("#arity") {
            let raw = self.type_hint_for_metavar(key).unwrap_or("");
            if raw.trim().is_empty() {
                return TypeHintCell::Unknown;
            }
            return TypeHintCell::Inferred(raw.trim().to_string());
        }
        if let Some((base, slot_idx)) = multi_slot_index(key) {
            let arity = self.multi_capture_arity(base);
            if slot_idx >= arity {
                return TypeHintCell::NoSlot;
            }
            let raw = self.type_hint_for_metavar(key).unwrap_or("");
            if raw.trim().is_empty() {
                return TypeHintCell::Unknown;
            }
            return TypeHintCell::Inferred(raw.trim().to_string());
        }
        let raw = self.type_hint_for_metavar(key).unwrap_or("");
        if raw.trim().is_empty() {
            TypeHintCell::Unknown
        } else {
            TypeHintCell::Inferred(raw.trim().to_string())
        }
    }

    fn multi_capture_arity(&self, base: &str) -> usize {
        let k = format!("{base}#arity");
        if let Some(s) = self.type_hints.get(&k) {
            if let Ok(n) = s.parse::<usize>() {
                return n;
            }
        }
        self.type_hints
            .keys()
            .filter_map(|key| {
                key.strip_prefix(&format!("{base}#"))
                    .and_then(|rest| rest.parse::<usize>().ok())
            })
            .max()
            .map(|m| m.saturating_add(1))
            .unwrap_or(0)
    }
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

/// `ARGS#0` のような複数ノードスロット列キーを `Some(("ARGS", 0))` に。`ARGS#arity` は `None`。
fn multi_slot_index(key: &str) -> Option<(&str, usize)> {
    let (base, rest) = key.rsplit_once('#')?;
    if rest == "arity" {
        return None;
    }
    let idx = rest.parse::<usize>().ok()?;
    Some((base, idx))
}

/// ファイルフィルタ文字列をパースして正規表現のリストを返す
///
/// フォーマット: `*.rs;src/*.java;test_.*` のように `;` 区切りで指定する。
/// 各エントリはファイル名全体に対してマッチされる。
/// 空文字列の場合は `None`（言語デフォルト拡張子を使用）を返す。
/// 行内の `[start, end)` が空白または行頭／行末で区切られた「塊」か。
/// `regex` クレートは先読み・後読みをサポートしないため、単語単位はこれで判定する。
fn is_whitespace_delimited_token(line: &str, start: usize, end: usize) -> bool {
    if start > 0 {
        if let Some(c) = line[..start].chars().next_back() {
            if !c.is_whitespace() {
                return false;
            }
        }
    }
    if end < line.len() {
        if let Some(c) = line[end..].chars().next() {
            if !c.is_whitespace() {
                return false;
            }
        }
    }
    true
}

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
    plain_text_options: PlainTextSearchOptions,
    context_lines: usize,
    file_filter: String,
    file_encoding_preference: FileEncodingPreference,
    max_file_size_bytes: u64,
    max_search_hits: usize,
    skip_dirs_str: String,
    ui_lang: UiLanguage,
    job_id: usize,
    tx: Sender<SearchMessage>,
    egui_ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let start = Instant::now();
        let pattern_str = Arc::new(pattern);
        // 大文字小文字を区別しないときだけリテラル正規表現を使う（`regex` は look-around 非対応のため、
        // 「単語単位」はマッチ後に `is_whitespace_delimited_token` で判定する）
        let plain_text_ci_re: Option<Arc<regex::Regex>> =
            if search_mode == SearchMode::PlainText && plain_text_options.case_insensitive {
                let escaped = regex::escape(pattern_str.as_str());
                match regex::RegexBuilder::new(&escaped)
                    .case_insensitive(true)
                    .build()
                {
                    Ok(re) => Some(Arc::new(re)),
                    Err(e) => {
                        let msg = Tr(ui_lang).err_regex_compile(e);
                        let _ = tx.send(SearchMessage::Error {
                            job_id,
                            msg,
                        });
                        egui_ctx.request_repaint();
                        return;
                    }
                }
            } else {
                None
            };
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
                    let _ = tx.send(SearchMessage::Error {
                        job_id,
                        msg,
                    });
                    egui_ctx.request_repaint();
                    return;
                }
            }
        } else {
            None
        };

        // TokenSearch モード用の正規表現をコンパイル（トークンを \s* で結合）
        let token_search_re: Option<Arc<regex::Regex>> = if search_mode == SearchMode::TokenSearch {
            let tokens: Vec<&str> = pattern_str.split_whitespace().collect();
            if tokens.is_empty() {
                let _ = tx.send(SearchMessage::Error { job_id, msg: "パターンが空です".into() });
                egui_ctx.request_repaint();
                return;
            }
            let regex_pattern = tokens
                .iter()
                .map(|t| regex::escape(t))
                .collect::<Vec<_>>()
                .join(r"\s*");
            match regex::Regex::new(&regex_pattern) {
                Ok(re) => Some(Arc::new(re)),
                Err(e) => {
                    let msg = Tr(ui_lang).err_regex_compile(e);
                    let _ = tx.send(SearchMessage::Error { job_id, msg });
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
                    let _ = tx.send(SearchMessage::Progress {
                        job_id,
                        scanned: count,
                    });
                    egui_ctx.request_repaint();
                }

                let lines: Vec<&str> = source.lines().collect();
                let line_index = LineIndex::new(&source);

                let hits_acc = Arc::clone(&hits_accepted);
                let limit_flag = Arc::clone(&hit_limit_reached);

                let matches: Vec<MatchItem> = match search_mode {
                    SearchMode::AstGrep => {
                        let ast_lang = match file_lang.to_support_lang() {
                            Some(l) => l,
                            None => return,
                        };
                        let compiled_patterns =
                            compile_strategies(pattern_str.as_str(), file_lang, ast_lang);
                        if compiled_patterns.is_empty() {
                            return;
                        }

                        let root = ast_lang.ast_grep(&source);
                        let mut out = Vec::new();
                        let metavar_names = pattern_single_metavariables(pattern_str.as_str());
                        let multi_metavar_names = pattern_multi_metavariables(pattern_str.as_str());
                        let want_type_hints =
                            !metavar_names.is_empty() || !multi_metavar_names.is_empty();
                        for compiled_pat in compiled_patterns {
                            for node in root.root().find_all(&compiled_pat) {
                                if !try_accept_hit(&hits_acc, max_search_hits, &limit_flag) {
                                    break;
                                }
                                let matched_node = node.get_node().clone();
                                let node_range = matched_node.range();
                                // ast-grep 本体（CLI）と同じノード範囲を使う
                                let matched_end = node_range.end.min(source.len());
                                let (line_start, col_start) =
                                    line_index.byte_offset_to_line_col(&source, node_range.start);
                                let (line_end, col_end) =
                                    line_index.byte_offset_to_line_col(&source, matched_end);
                                let matched_text = source
                                    .get(node_range.start..matched_end)
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| node.text().to_string());
                                let type_hints = if want_type_hints {
                                    let hint_ctx = receiver_hint::RecvHintContext {
                                        file_path: path.as_path(),
                                        source: source.as_str(),
                                    };
                                    let mut hints = BTreeMap::new();
                                    for name in &metavar_names {
                                        if let Some(capture) = node.get_env().get_match(name) {
                                            if let Some(h) = receiver_hint::infer_capture_type(
                                                file_lang,
                                                name.as_str(),
                                                capture,
                                                Some(&hint_ctx),
                                            ) {
                                                if !h.is_empty() {
                                                    hints.insert(name.clone(), h);
                                                }
                                            }
                                        }
                                    }
                                    for multi_name in &multi_metavar_names {
                                        let nodes = node.get_env().get_multiple_matches(multi_name);
                                        hints.insert(
                                            format!("{multi_name}#arity"),
                                            nodes.len().to_string(),
                                        );
                                        for (i, cap) in nodes.iter().enumerate() {
                                            let key = format!("{multi_name}#{i}");
                                            let h = receiver_hint::infer_capture_type(
                                                file_lang,
                                                multi_name.as_str(),
                                                cap,
                                                Some(&hint_ctx),
                                            )
                                            .unwrap_or_default();
                                            hints.insert(key, h);
                                        }
                                    }
                                    hints
                                } else {
                                    BTreeMap::new()
                                };
                                out.push(build_match_item(
                                    line_start,
                                    col_start,
                                    line_end,
                                    col_end,
                                    matched_text,
                                    &lines,
                                    context_lines,
                                    type_hints,
                                ));
                            }
                            if !out.is_empty() {
                                break;
                            }
                        }
                        out
                    }
                    SearchMode::PlainText => {
                        let needle = pattern_str.as_str();
                        let mut result = Vec::new();
                        let whole = plain_text_options.whole_word;

                        if let Some(re) = plain_text_ci_re.as_ref() {
                            'lines_re: for (line_idx, line) in lines.iter().enumerate() {
                                for mat in re.find_iter(line) {
                                    if whole
                                        && !is_whitespace_delimited_token(line, mat.start(), mat.end())
                                    {
                                        continue;
                                    }
                                    if !try_accept_hit(&hits_acc, max_search_hits, &limit_flag) {
                                        break 'lines_re;
                                    }
                                    let col_start = mat.start();
                                    let col_end = mat.end();
                                    let matched_text = line[col_start..col_end].to_string();
                                    result.push(build_match_item(
                                        line_idx,
                                        col_start,
                                        line_idx,
                                        col_end,
                                        matched_text,
                                        &lines,
                                        context_lines,
                                        BTreeMap::new(),
                                    ));
                                }
                            }
                        } else if whole {
                            'lines_w: for (line_idx, line) in lines.iter().enumerate() {
                                let mut search_start = 0;
                                while let Some(byte_pos) = line[search_start..].find(needle) {
                                    let col_start = search_start + byte_pos;
                                    let col_end = col_start + needle.len();
                                    if !is_whitespace_delimited_token(line, col_start, col_end) {
                                        search_start = col_start.saturating_add(1);
                                        continue;
                                    }
                                    if !try_accept_hit(&hits_acc, max_search_hits, &limit_flag) {
                                        break 'lines_w;
                                    }
                                    let matched_text = line[col_start..col_end].to_string();
                                    result.push(build_match_item(
                                        line_idx,
                                        col_start,
                                        line_idx,
                                        col_end,
                                        matched_text,
                                        &lines,
                                        context_lines,
                                        BTreeMap::new(),
                                    ));
                                    search_start = col_end;
                                    if search_start >= line.len() {
                                        break;
                                    }
                                }
                            }
                        } else {
                            // 行ごとにスキャンして部分一致（大小区別・単語境界なしの高速パス）
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
                                        line_idx,
                                        col_start,
                                        line_idx,
                                        col_end,
                                        matched_text,
                                        &lines,
                                        context_lines,
                                        BTreeMap::new(),
                                    ));
                                    search_start = col_end;
                                    if search_start >= line.len() {
                                        break;
                                    }
                                }
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
                                line_index.byte_offset_to_line_col(&source, mat.start());
                            let (line_end, col_end) =
                                line_index.byte_offset_to_line_col(&source, mat.end());
                            out.push(build_match_item(
                                line_start, col_start, line_end, col_end,
                                mat.as_str().to_string(), &lines, context_lines,
                                BTreeMap::new(),
                            ));
                        }
                        out
                    }
                    SearchMode::TokenSearch => {
                        // ソース全体を対象に検索（\s* が改行にもマッチするため行跨ぎに対応）
                        let re = token_search_re.as_ref().unwrap();
                        let mut out = Vec::new();
                        for mat in re.find_iter(&source) {
                            if !try_accept_hit(&hits_acc, max_search_hits, &limit_flag) {
                                break;
                            }
                            let (line_start, col_start) =
                                line_index.byte_offset_to_line_col(&source, mat.start());
                            let (line_end, col_end) =
                                line_index.byte_offset_to_line_col(&source, mat.end());
                            out.push(build_match_item(
                                line_start, col_start, line_end, col_end,
                                mat.as_str().to_string(), &lines, context_lines, BTreeMap::new(),
                            ));
                        }
                        out
                    }
                };

                if !matches.is_empty() {
                    let _ = tx.send(SearchMessage::FileResult {
                        job_id,
                        file: FileResult {
                            path: path.to_path_buf(),
                            relative_path,
                            source_language: file_lang,
                            text_encoding,
                            matches,
                        },
                    });
                    egui_ctx.request_repaint();
                }
            });

        // 最終進捗を送信
        let final_count = scanned.load(std::sync::atomic::Ordering::Relaxed);
        let _ = tx.send(SearchMessage::Progress {
            job_id,
            scanned: final_count,
        });
        let elapsed_ms = start.elapsed().as_millis() as u64;
        let hit_limit_reached = hit_limit_reached.load(Ordering::Relaxed);
        let _ = tx.send(SearchMessage::Done {
            job_id,
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

/// AstGrep パターン内の複数ノードキャプチャ `$$$NAME` の名前を出現順に列挙（`$$$`・`$$$_` は含めない）。
pub fn pattern_multi_metavariables(pattern: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let bytes = pattern.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'$'
            && i + 2 < bytes.len()
            && bytes[i + 1] == b'$'
            && bytes[i + 2] == b'$'
        {
            i += 3;
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if start < i {
                let name = &pattern[start..i];
                if !name.starts_with('_') && seen.insert(name.to_string()) {
                    out.push(name.to_string());
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

/// AstGrep パターン内の単一メタ変数 `$NAME` を列挙する（`$$$MULTI` はスキップ）。
pub fn pattern_single_metavariables(pattern: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let bytes = pattern.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            if i + 2 < bytes.len() && bytes[i + 1] == b'$' && bytes[i + 2] == b'$' {
                i += 3;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                continue;
            }
            i += 1;
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if start < i {
                let name = &pattern[start..i];
                if seen.insert(name.to_string()) {
                    out.push(name.to_string());
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

/// 型ヒント表・エクスポート用の列キー（単一 `$A` のあと、各 `$$$M` について `M#arity`、`M#0`…`M#(n-1)`）。
///
/// スロット列数 `n` は現在の `results` 内で当該メタ変数に付いた `M#i`（数値 `i`）の最大個数。
pub fn type_hint_column_keys(pattern: &str, results: &[FileResult]) -> Vec<String> {
    let mut keys = pattern_single_metavariables(pattern);
    for m in pattern_multi_metavariables(pattern) {
        keys.push(format!("{m}#arity"));
        let n = max_multi_slot_count(results, &m);
        for i in 0..n {
            keys.push(format!("{m}#{i}"));
        }
    }
    keys
}

fn max_multi_slot_count(results: &[FileResult], multi_name: &str) -> usize {
    let prefix = format!("{multi_name}#");
    let mut max_n = 0usize;
    for file in results {
        for item in &file.matches {
            for k in item.type_hints.keys() {
                if let Some(rest) = k.strip_prefix(&prefix) {
                    if let Ok(idx) = rest.parse::<usize>() {
                        max_n = max_n.max(idx.saturating_add(1));
                    }
                }
            }
        }
    }
    max_n
}

/// パターンに単一または複数ノードメタ変数があれば型ヒント列・計算を有効にする
pub fn pattern_wants_type_hints(pattern: &str) -> bool {
    !pattern_single_metavariables(pattern).is_empty() || !pattern_multi_metavariables(pattern).is_empty()
}

/// 後方互換: 旧名（任意 `$NAME` で型ヒントが有効）
#[allow(dead_code)]
#[inline]
pub fn pattern_contains_dollar_recv(pattern: &str) -> bool {
    pattern_wants_type_hints(pattern)
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
    type_hints: BTreeMap<String, String>,
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
        type_hints,
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
    /// 文字列検索モード時のみ有効（それ以外は無視）
    #[serde(default)]
    pub plain_text_options: PlainTextSearchOptions,
}

pub(crate) fn default_max_search_hits() -> usize {
    100_000
}

/// 検索統計
#[derive(Debug, Default, Clone, serde::Serialize)]
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

#[cfg(test)]
mod tests {
    use super::{
        pattern_multi_metavariables, pattern_single_metavariables, FileResult, LineIndex, MatchItem,
        TypeHintCell,
    };
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[test]
    fn pattern_single_metavariables_lists_names_skips_multi() {
        let p = "$RECV.$METHOD($$$ARGS)";
        assert_eq!(
            pattern_single_metavariables(p),
            vec!["RECV".to_string(), "METHOD".to_string()]
        );
    }

    #[test]
    fn pattern_multi_metavariables_finds_ellipsis_names() {
        let p = "$RECV.$METHOD($$$ARGS)";
        assert_eq!(pattern_multi_metavariables(p), vec!["ARGS".to_string()]);
        assert!(pattern_multi_metavariables("$$$").is_empty());
        assert!(pattern_multi_metavariables("$$$_").is_empty());
    }

    #[test]
    fn pattern_single_metavariables_only_multi_is_empty() {
        assert!(pattern_single_metavariables("$$$BODY").is_empty());
    }

    #[test]
    fn type_hint_column_keys_expands_multi_slots() {
        let pattern = "$A($$$ARGS)";
        let mut hints = BTreeMap::new();
        hints.insert("ARGS#arity".to_string(), "2".to_string());
        hints.insert("ARGS#0".to_string(), "t0".to_string());
        hints.insert("ARGS#1".to_string(), "t1".to_string());
        let results = vec![FileResult {
            path: PathBuf::from("x"),
            relative_path: "x".to_string(),
            matches: vec![MatchItem {
                line_start: 1,
                col_start: 0,
                line_end: 1,
                col_end: 1,
                matched_text: String::new(),
                span_lines_text: String::new(),
                context_before: vec![],
                context_after: vec![],
                type_hints: hints,
            }],
            source_language: crate::lang::SupportedLanguage::Rust,
            text_encoding: crate::file_encoding::FileEncoding::Utf8,
        }];
        let keys = super::type_hint_column_keys(pattern, &results);
        assert_eq!(keys, vec!["A", "ARGS#arity", "ARGS#0", "ARGS#1"]);
    }

    #[test]
    fn type_hint_cell_no_slot_vs_unknown() {
        let mut m = MatchItem {
            line_start: 1,
            col_start: 0,
            line_end: 1,
            col_end: 1,
            matched_text: String::new(),
            span_lines_text: String::new(),
            context_before: vec![],
            context_after: vec![],
            type_hints: BTreeMap::new(),
        };
        m.type_hints.insert("ARGS#arity".to_string(), "2".to_string());
        m.type_hints.insert("ARGS#0".to_string(), "int".to_string());
        m.type_hints.insert("ARGS#1".to_string(), String::new());
        assert_eq!(m.type_hint_cell("ARGS#2"), TypeHintCell::NoSlot);
        assert_eq!(m.type_hint_cell("ARGS#1"), TypeHintCell::Unknown);
        assert_eq!(
            m.type_hint_cell("ARGS#0"),
            TypeHintCell::Inferred("int".to_string())
        );
    }

    #[test]
    fn line_index_maps_offsets_across_lines() {
        let source = "alpha\nbeta\ngamma";
        let index = LineIndex::new(source);

        assert_eq!(index.byte_offset_to_line_col(source, 0), (0, 0));
        assert_eq!(index.byte_offset_to_line_col(source, 3), (0, 3));
        assert_eq!(index.byte_offset_to_line_col(source, 6), (1, 0));
        assert_eq!(index.byte_offset_to_line_col(source, 10), (1, 4));
        assert_eq!(index.byte_offset_to_line_col(source, 11), (2, 0));
        assert_eq!(index.byte_offset_to_line_col(source, source.len()), (2, 5));
    }

    #[test]
    fn line_index_handles_trailing_newline() {
        let source = "a\n";
        let index = LineIndex::new(source);

        assert_eq!(index.byte_offset_to_line_col(source, 0), (0, 0));
        assert_eq!(index.byte_offset_to_line_col(source, 1), (0, 1));
        assert_eq!(index.byte_offset_to_line_col(source, 2), (1, 0));
    }
}
