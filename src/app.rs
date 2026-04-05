use std::path::PathBuf;

use crossbeam_channel::Receiver;
use eframe::egui;

use crate::batch::{
    BatchReport, BatchRunResult, BatchRunnerState, PatternJob, SINGLE_SEARCH_JOB_ID,
};
use crate::file_encoding::{FileEncoding, FileEncodingPreference};
use crate::highlight::Highlighter;
use crate::i18n::{Tr, UiLanguage, UiLanguagePreference};
use crate::lang::SupportedLanguage;
use crate::search::{
    refresh_match_contexts, spawn_search, FileResult, MatchItem, SearchConditions, SearchMessage,
    SearchMode, SearchStats,
};
use crate::pattern_assist::PatternSuggestion;
use crate::rewrite::{RewriteMessage, RewritePreview};
use crate::terminal::TerminalState;
use crate::ui::{
    batch_report_panel, code_panel, file_panel, help_popup, pattern_assist_popup,
    regex_visualizer_popup, rewrite_popup, status_bar, table_panel, table_preview_popup,
    terminal_panel, toolbar,
};

/// 表モードでダブルクリックしたファイルをコードビューと同じ表示で開く
#[derive(Debug, Clone)]
pub struct TablePreviewState {
    pub path: PathBuf,
    pub relative_path: String,
    /// ダブルクリックしたマッチ位置（見出し用、1-based）
    pub line: usize,
    pub col: usize,
    pub matches: Vec<MatchItem>,
    pub source_language: SupportedLanguage,
    pub text_encoding: FileEncoding,
    /// 初回のみ該当行へスクロール（1-based）。開いた直後に `take()` で消費
    pub pending_scroll_line: Option<usize>,
}

/// 検索の状態
#[derive(Debug, Clone)]
pub enum SearchState {
    Idle,
    Running,
    Done,
    Error(String),
}

/// 結果の表示モード
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ViewMode {
    Code,  // コードビュー
    Table, // 表形式
    /// バッチ検索の集約レポート
    BatchReport,
}

#[derive(Debug, Clone, Copy)]
pub struct TableRowRef {
    pub file_idx: usize,
    pub match_idx: usize,
}

/// Rewrite（プレビュー / 適用）の進行状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewritePhase {
    Idle,
    /// プレビュー生成中
    Previewing,
    /// ディスクへの書き戻し中
    Applying,
}

fn count_display_lines(text: &str) -> usize {
    if text.is_empty() {
        1
    } else {
        text.bytes().filter(|b| *b == b'\n').count() + 1
    }
}

fn table_row_line_units(m: &MatchItem) -> usize {
    let center_lines = if m.span_lines_text.is_empty() {
        count_display_lines(&m.matched_text)
    } else {
        count_display_lines(&m.span_lines_text)
    };
    (m.context_before.len() + center_lines + m.context_after.len()).max(1)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PersistedAppState {
    search_dir: String,
    pattern: String,
    selected_lang: SupportedLanguage,
    context_lines: usize,
    view_mode: ViewMode,
    file_filter: String,
    #[serde(default)]
    file_encoding_preference: FileEncodingPreference,
    max_file_size_mb: u64,
    /// 収集ヒット上限（0 = 無制限）
    #[serde(default = "crate::search::default_max_search_hits")]
    max_search_hits: usize,
    skip_dirs: String,
    search_mode: SearchMode,
    /// UI 表示言語（自動 / 日本語 / 英語）
    #[serde(default)]
    ui_language_preference: UiLanguagePreference,
    /// パターン入力履歴（新しい順、最大30件）
    #[serde(default)]
    pattern_history: Vec<String>,
    /// Regex visualiser 用のテスト文字列
    #[serde(default)]
    regex_visualizer_test_text: String,
    /// AST 置換テンプレート（`--rewrite` 相当）
    #[serde(default)]
    rewrite_template: String,
    /// バッチ検索ジョブ一覧
    #[serde(default)]
    batch_jobs: Vec<PatternJob>,
    /// 新規ジョブ ID（1 から）
    #[serde(default = "default_next_pattern_job_id")]
    next_pattern_job_id: usize,
}

fn default_next_pattern_job_id() -> usize {
    1
}

impl Default for PersistedAppState {
    fn default() -> Self {
        Self {
            search_dir: String::new(),
            pattern: String::new(),
            selected_lang: SupportedLanguage::Auto,
            context_lines: 2,
            view_mode: ViewMode::Code,
            file_filter: String::new(),
            file_encoding_preference: FileEncodingPreference::default(),
            max_file_size_mb: 10,
            max_search_hits: 100_000,
            skip_dirs: ".git;.hg;.svn;target;node_modules;dist;build;.cache;.next;vendor;__pycache__;venv;.venv".to_string(),
            search_mode: SearchMode::AstGrep,
            ui_language_preference: UiLanguagePreference::default(),
            pattern_history: Vec::new(),
            regex_visualizer_test_text: String::new(),
            rewrite_template: String::new(),
            batch_jobs: Vec::new(),
            next_pattern_job_id: 1,
        }
    }
}

const APP_STATE_KEY: &str = "ast_grep_gui_app_state";

pub struct AstGrepApp {
    pub search_dir: String,
    pub pattern: String,
    pub selected_lang: SupportedLanguage,
    pub context_lines: usize,
    pub selected_file_idx: Option<usize>,
    pub show_help: bool,
    pub search_state: SearchState,
    pub results: Vec<FileResult>,
    pub stats: SearchStats,
    pub highlighter: Highlighter,
    /// パターン支援ポップアップの表示フラグ
    pub show_pattern_assist: bool,
    /// 正規表現 visualiser の表示フラグ
    pub show_regex_visualizer: bool,
    /// パターン支援：入力スニペット
    pub pattern_assist_snippet: String,
    /// パターン支援：生成結果
    pub pattern_assist_results: Vec<PatternSuggestion>,
    /// ファイルクリック時にスクロールする対象行（1-based）、消費後はNone
    pub pending_scroll_line: Option<usize>,
    /// 結果の表示モード
    pub view_mode: ViewMode,
    /// ファイル名/拡張子フィルタ（;区切りの正規表現、空なら言語デフォルト）
    pub file_filter: String,
    /// テキストファイル読み込み時の文字コード設定
    pub file_encoding_preference: FileEncodingPreference,
    /// スキップするファイルサイズの上限（MB単位）
    pub max_file_size_mb: u64,
    /// 収集するヒット数の上限（0 = 無制限）
    pub max_search_hits: usize,
    /// スキップするディレクトリ名（;区切り）
    pub skip_dirs: String,
    /// 検索モード（AstGrep / PlainText / Regex）
    pub search_mode: SearchMode,
    /// UI 表示言語
    pub ui_language_preference: UiLanguagePreference,
    /// パターン支援ポップアップに転送するスニペット（Some のとき自動的に反映）
    pub pending_pattern_assist_snippet: Option<String>,
    /// 表モード: ダブルクリックで開くコードプレビュー（None で非表示）
    pub table_preview: Option<TablePreviewState>,
    /// 表モード: 最後にクリックしたデータ行（0 始まりのフラットインデックス）
    pub table_last_clicked_row: Option<usize>,
    /// 表モード: フラットな行インデックス -> (file_idx, match_idx)
    pub table_rows: Vec<TableRowRef>,
    /// 表モード: 各行の表示行数
    pub table_row_units: Vec<usize>,
    /// 表モード: 各行の先頭表示行オフセット（prefix sum）
    pub table_row_prefix_units: Vec<usize>,
    /// 表モード: コンテキスト行数変更直後にこの行の先頭へスクロール（描画後にクリア）
    pub(crate) table_scroll_to_row: Option<usize>,
    /// パターン入力履歴（新しい順、最大30件）
    pub pattern_history: Vec<String>,
    /// Regex visualiser: パターンに対して試すテキスト
    pub regex_visualizer_test_text: String,
    /// パターンサジェストポップアップで現在選択中の候補インデックス
    pub pattern_suggest_idx: Option<usize>,
    /// ターミナルパネルの表示フラグ
    pub show_terminal: bool,
    /// ターミナルパネルの状態（初回表示時に初期化）
    pub terminal: Option<TerminalState>,
    /// ターミナルパネルの高さ（0.0 = 未設定）
    pub terminal_height: f32,

    /// AST 一括置換のテンプレート文字列（`$VAR` など）
    pub rewrite_template: String,
    /// 置換プレビュー結果
    pub rewrite_preview: Option<RewritePreview>,
    /// プレビュー進捗（done / total）
    pub rewrite_preview_progress: Option<(usize, usize)>,
    pub rewrite_phase: RewritePhase,
    /// プレビュー / 適用のエラーメッセージ
    pub rewrite_error: Option<String>,
    /// 置換プレビュー ウィンドウを表示
    pub show_rewrite_popup: bool,
    /// プレビュー内で選択中のファイルインデックス
    pub rewrite_selected_file_idx: usize,
    /// 書き戻し完了などの短い通知
    pub rewrite_status_note: Option<String>,

    // バックグラウンド検索チャンネル
    result_rx: Option<Receiver<SearchMessage>>,
    /// Rewrite プレビュー生成
    rewrite_rx: Option<Receiver<RewriteMessage>>,
    /// Rewrite ディスク適用
    rewrite_apply_rx: Option<Receiver<Result<usize, Vec<(PathBuf, String)>>>>,
    // repaintのためにContextをキャッシュ
    cached_ctx: Option<egui::Context>,
    /// 表示に反映済みのコンテキスト行数（変更時に結果の前後行だけ再計算する）
    last_context_lines_applied: usize,

    // ── バッチ検索 ──
    pub batch_jobs: Vec<PatternJob>,
    pub next_pattern_job_id: usize,
    /// バッチ実行中のみ Some（直列でジョブを回す）
    pub batch_runner: Option<BatchRunnerState>,
    /// 最後に完了したバッチレポート（レポート画面用）
    pub batch_report: Option<BatchReport>,
    /// `batch_jobs` のインデックス: ジョブ編集ウィンドウを開く
    pub batch_edit_list_index: Option<usize>,
}

impl AstGrepApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let persisted = cc
            .storage
            .and_then(|storage| eframe::get_value::<PersistedAppState>(storage, APP_STATE_KEY))
            .unwrap_or_default();

        Self {
            search_dir: persisted.search_dir,
            pattern: persisted.pattern,
            selected_lang: persisted.selected_lang,
            context_lines: persisted.context_lines,
            selected_file_idx: None,
            show_help: false,
            search_state: SearchState::Idle,
            results: Vec::new(),
            stats: SearchStats::default(),
            highlighter: Highlighter::new(),
            show_pattern_assist: false,
            show_regex_visualizer: false,
            pattern_assist_snippet: String::new(),
            pattern_assist_results: Vec::new(),
            pending_scroll_line: None,
            view_mode: persisted.view_mode,
            file_filter: persisted.file_filter,
            file_encoding_preference: persisted.file_encoding_preference,
            max_file_size_mb: persisted.max_file_size_mb,
            max_search_hits: persisted.max_search_hits,
            skip_dirs: persisted.skip_dirs,
            search_mode: persisted.search_mode,
            ui_language_preference: persisted.ui_language_preference,
            pending_pattern_assist_snippet: None,
            table_preview: None,
            table_last_clicked_row: None,
            table_rows: Vec::new(),
            table_row_units: Vec::new(),
            table_row_prefix_units: vec![0],
            table_scroll_to_row: None,
            pattern_history: persisted.pattern_history,
            regex_visualizer_test_text: persisted.regex_visualizer_test_text,
            pattern_suggest_idx: None,
            show_terminal: false,
            terminal: None,
            terminal_height: 0.0,
            rewrite_template: persisted.rewrite_template,
            rewrite_preview: None,
            rewrite_preview_progress: None,
            rewrite_phase: RewritePhase::Idle,
            rewrite_error: None,
            show_rewrite_popup: false,
            rewrite_selected_file_idx: 0,
            rewrite_status_note: None,
            result_rx: None,
            rewrite_rx: None,
            rewrite_apply_rx: None,
            cached_ctx: None,
            last_context_lines_applied: persisted.context_lines,
            batch_jobs: persisted.batch_jobs,
            next_pattern_job_id: persisted.next_pattern_job_id.max(1),
            batch_runner: None,
            batch_report: None,
            batch_edit_list_index: None,
        }
    }

    /// 現在の実効 UI 言語
    pub fn ui_lang(&self) -> UiLanguage {
        self.ui_language_preference.effective()
    }

    /// 翻訳アクセサ
    pub fn tr(&self) -> Tr {
        Tr(self.ui_lang())
    }

    /// バッチ実行中の場合 `(現在番号, 総ジョブ数)`（1-based）
    pub fn batch_job_progress(&self) -> Option<(usize, usize)> {
        let r = self.batch_runner.as_ref()?;
        if r.ordered_indices.is_empty() {
            return None;
        }
        Some((r.active_idx + 1, r.ordered_indices.len()))
    }

    /// パターン支援で使う言語（自動モード時は Rust パーサでスニペットを解析）
    pub fn pattern_assist_resolve_lang(&self) -> SupportedLanguage {
        match self.selected_lang {
            SupportedLanguage::Auto => SupportedLanguage::Rust,
            x => x,
        }
    }

    /// エクスポートに埋め込む現在の検索条件
    pub fn search_conditions_for_export(&self) -> SearchConditions {
        SearchConditions {
            search_dir: self.search_dir.clone(),
            pattern: self.pattern.clone(),
            selected_lang: self.selected_lang,
            context_lines: self.context_lines,
            file_filter: self.file_filter.clone(),
            file_encoding_preference: self.file_encoding_preference,
            max_file_size_mb: self.max_file_size_mb,
            max_search_hits: self.max_search_hits,
            skip_dirs: self.skip_dirs.clone(),
            search_mode: self.search_mode,
        }
    }

    fn persisted_state(&self) -> PersistedAppState {
        PersistedAppState {
            search_dir: self.search_dir.clone(),
            pattern: self.pattern.clone(),
            selected_lang: self.selected_lang,
            context_lines: self.context_lines,
            view_mode: self.view_mode,
            file_filter: self.file_filter.clone(),
            file_encoding_preference: self.file_encoding_preference,
            max_file_size_mb: self.max_file_size_mb,
            max_search_hits: self.max_search_hits,
            skip_dirs: self.skip_dirs.clone(),
            search_mode: self.search_mode,
            ui_language_preference: self.ui_language_preference,
            pattern_history: self.pattern_history.clone(),
            regex_visualizer_test_text: self.regex_visualizer_test_text.clone(),
            rewrite_template: self.rewrite_template.clone(),
            batch_jobs: self.batch_jobs.clone(),
            next_pattern_job_id: self.next_pattern_job_id,
        }
    }

    /// 検索を開始する
    pub fn start_search(&mut self) {
        if self.search_dir.is_empty() || self.pattern.is_empty() {
            return;
        }

        self.batch_runner = None;

        // 検索履歴に追加（最新を先頭に、重複排除、最大30件）
        let pat = self.pattern.trim().to_string();
        if !pat.is_empty() {
            self.pattern_history.retain(|h| h != &pat);
            self.pattern_history.insert(0, pat);
            self.pattern_history.truncate(30);
        }

        self.results.clear();
        self.stats = SearchStats::default();
        self.selected_file_idx = None;
        self.table_preview = None;
        self.table_last_clicked_row = None;
        self.table_rows.clear();
        self.table_row_units.clear();
        self.table_row_prefix_units.clear();
        self.table_row_prefix_units.push(0);
        self.table_scroll_to_row = None;
        self.highlighter.clear_cache();
        self.search_state = SearchState::Running;
        self.clear_rewrite_state();

        let Some(ctx) = self.cached_ctx.clone() else {
            return;
        };

        let (tx, rx) = crossbeam_channel::unbounded();
        self.result_rx = Some(rx);

        spawn_search(
            self.search_dir.clone(),
            self.pattern.clone(),
            self.selected_lang,
            self.search_mode,
            self.context_lines,
            self.file_filter.clone(),
            self.file_encoding_preference,
            self.max_file_size_mb * 1024 * 1024,
            self.max_search_hits,
            self.skip_dirs.clone(),
            self.ui_lang(),
            SINGLE_SEARCH_JOB_ID,
            tx,
            ctx,
        );
    }

    /// 現在のツールバー設定をコピーしたジョブをバッチ一覧に追加する
    pub fn add_pattern_job_from_current(&mut self) {
        let id = self.next_pattern_job_id;
        self.next_pattern_job_id = self.next_pattern_job_id.saturating_add(1);
        let label = format!("{} {}", self.tr().batch_job_default_label_prefix(), id);
        self.batch_jobs.push(PatternJob::from_app_snapshot(
            id,
            label,
            self.pattern.clone(),
            self.search_dir.clone(),
            self.selected_lang,
            self.context_lines,
            self.file_filter.clone(),
            self.file_encoding_preference,
            self.max_file_size_mb,
            self.max_search_hits,
            self.skip_dirs.clone(),
            self.search_mode,
        ));
    }

    /// 有効なジョブを順に実行するバッチ検索
    pub fn start_batch_search(&mut self) {
        let ordered_indices: Vec<usize> = self
            .batch_jobs
            .iter()
            .enumerate()
            .filter(|(_, j)| j.is_runnable())
            .map(|(i, _)| i)
            .collect();

        if ordered_indices.is_empty() {
            self.search_state = SearchState::Error(self.tr().batch_no_runnable_jobs().to_string());
            return;
        }

        let Some(ctx) = self.cached_ctx.clone() else {
            return;
        };

        let runs: Vec<BatchRunResult> = ordered_indices
            .iter()
            .map(|&list_idx| {
                let j = &self.batch_jobs[list_idx];
                BatchRunResult {
                    job_id: j.id,
                    label: j.label.clone(),
                    conditions: j.to_conditions(),
                    results: Vec::new(),
                    stats: SearchStats::default(),
                    error: None,
                }
            })
            .collect();

        self.batch_runner = Some(BatchRunnerState {
            ordered_indices,
            active_idx: 0,
            runs,
            started: std::time::Instant::now(),
        });

        self.results.clear();
        self.stats = SearchStats::default();
        self.selected_file_idx = None;
        self.table_preview = None;
        self.table_last_clicked_row = None;
        self.table_rows.clear();
        self.table_row_units.clear();
        self.table_row_prefix_units.clear();
        self.table_row_prefix_units.push(0);
        self.table_scroll_to_row = None;
        self.highlighter.clear_cache();
        self.search_state = SearchState::Running;
        self.clear_rewrite_state();
        self.batch_report = None;

        self.spawn_search_for_current_batch_job(ctx);
    }

    fn spawn_search_for_current_batch_job(&mut self, ctx: egui::Context) {
        let Some(runner) = &mut self.batch_runner else {
            return;
        };
        let Some(&list_idx) = runner.ordered_indices.get(runner.active_idx) else {
            return;
        };
        let job = self.batch_jobs[list_idx].clone();

        let (tx, rx) = crossbeam_channel::unbounded();
        self.result_rx = Some(rx);

        spawn_search(
            job.search_dir.clone(),
            job.pattern.clone(),
            job.selected_lang,
            job.search_mode,
            job.context_lines,
            job.file_filter.clone(),
            job.file_encoding_preference,
            job.max_file_size_mb * 1024 * 1024,
            job.max_search_hits,
            job.skip_dirs.clone(),
            self.ui_lang(),
            job.id,
            tx,
            ctx,
        );
    }

    fn finish_batch_search(&mut self) {
        let Some(runner) = self.batch_runner.take() else {
            return;
        };
        self.result_rx = None;
        let total_elapsed_ms = runner.started.elapsed().as_millis() as u64;
        self.batch_report = Some(BatchReport {
            total_elapsed_ms,
            runs: runner.runs,
        });
        self.search_state = SearchState::Done;
        self.view_mode = ViewMode::BatchReport;
    }

    /// 検索を停止する（チャンネルをドロップ）
    pub fn stop_search(&mut self) {
        self.result_rx = None;
        self.batch_runner = None;
        self.search_state = SearchState::Idle;
    }

    /// 結果をクリアする
    pub fn clear_results(&mut self) {
        self.results.clear();
        self.stats = SearchStats::default();
        self.selected_file_idx = None;
        self.table_preview = None;
        self.table_last_clicked_row = None;
        self.table_rows.clear();
        self.table_row_units.clear();
        self.table_row_prefix_units.clear();
        self.table_row_prefix_units.push(0);
        self.table_scroll_to_row = None;
        self.search_state = SearchState::Idle;
        self.highlighter.clear_cache();
        self.last_context_lines_applied = self.context_lines;
        self.clear_rewrite_state();
    }

    /// ツールバーで変更したコンテキスト行数を、検索結果の表示に反映する
    fn sync_match_contexts_from_ui(&mut self) {
        if self.results.is_empty() {
            self.last_context_lines_applied = self.context_lines;
            return;
        }
        if self.context_lines != self.last_context_lines_applied {
            refresh_match_contexts(&mut self.results, self.context_lines);
            self.rebuild_table_row_metrics();
            self.last_context_lines_applied = self.context_lines;
            if let Some(ref mut preview) = self.table_preview {
                if let Some(fr) = self.results.iter().find(|f| f.path == preview.path) {
                    preview.matches = fr.matches.clone();
                }
                preview.pending_scroll_line = Some(preview.line);
            }
        }
    }

    fn push_table_row(&mut self, row: TableRowRef, m: &MatchItem) {
        self.table_rows.push(row);
        let units = table_row_line_units(m);
        self.table_row_units.push(units);
        let next = self.table_row_prefix_units.last().copied().unwrap_or(0) + units;
        self.table_row_prefix_units.push(next);
    }

    fn rebuild_table_row_metrics(&mut self) {
        let mut rows = Vec::new();
        let mut units = Vec::new();
        let mut prefix_units = vec![0];

        for (file_idx, file) in self.results.iter().enumerate() {
            for (match_idx, m) in file.matches.iter().enumerate() {
                rows.push(TableRowRef { file_idx, match_idx });
                let row_units = table_row_line_units(m);
                units.push(row_units);
                let next = prefix_units.last().copied().unwrap_or(0) + row_units;
                prefix_units.push(next);
            }
        }

        self.table_rows = rows;
        self.table_row_units = units;
        self.table_row_prefix_units = prefix_units;
    }

    /// 検索結果が変わるタイミングで Rewrite の一時状態を消す（テンプレート文字列は保持）
    pub fn clear_rewrite_state(&mut self) {
        self.rewrite_preview = None;
        self.rewrite_preview_progress = None;
        self.rewrite_phase = RewritePhase::Idle;
        self.rewrite_rx = None;
        self.rewrite_apply_rx = None;
        self.show_rewrite_popup = false;
        self.rewrite_error = None;
        self.rewrite_selected_file_idx = 0;
        self.rewrite_status_note = None;
    }

    /// 置換プレビューをバックグラウンドで生成する（AST / ast-grep raw のみ）
    pub fn start_rewrite_preview(&mut self) {
        if !self.search_mode.is_ast_mode() {
            return;
        }
        if self.results.is_empty()
            || self.pattern.trim().is_empty()
            || self.rewrite_template.trim().is_empty()
        {
            return;
        }
        let Some(ctx) = self.cached_ctx.clone() else {
            return;
        };

        self.rewrite_error = None;
        self.rewrite_status_note = None;
        self.rewrite_preview = None;
        self.rewrite_preview_progress = Some((0, self.results.len()));
        self.rewrite_phase = RewritePhase::Previewing;

        let (tx, rx) = crossbeam_channel::unbounded();
        self.rewrite_rx = Some(rx);

        crate::rewrite::spawn_rewrite_preview(
            self.results.clone(),
            self.pattern.clone(),
            self.rewrite_template.clone(),
            self.file_encoding_preference,
            tx,
            ctx,
        );
    }

    /// プレビュー内容をディスクに書き戻し、その後検索をやり直す
    pub fn start_rewrite_apply(&mut self) {
        let Some(preview) = self.rewrite_preview.clone() else {
            return;
        };
        if preview.files.is_empty() {
            return;
        }
        let Some(ctx) = self.cached_ctx.clone() else {
            return;
        };

        self.rewrite_error = None;
        self.rewrite_status_note = None;
        self.rewrite_phase = RewritePhase::Applying;

        let (tx, rx) = crossbeam_channel::unbounded();
        self.rewrite_apply_rx = Some(rx);

        crate::rewrite::spawn_apply_rewrite(preview.files, tx, ctx);
    }

    fn drain_rewrite_messages(&mut self) {
        let Some(rx) = &self.rewrite_rx else {
            return;
        };

        let messages: Vec<RewriteMessage> = rx.try_iter().collect();
        for msg in messages {
            match msg {
                RewriteMessage::Progress { done, total } => {
                    self.rewrite_preview_progress = Some((done, total));
                }
                RewriteMessage::Done(preview) => {
                    self.rewrite_preview = Some(preview);
                    self.rewrite_rx = None;
                    self.rewrite_phase = RewritePhase::Idle;
                    self.rewrite_preview_progress = None;
                    self.show_rewrite_popup = true;
                    self.rewrite_selected_file_idx = 0;
                }
                RewriteMessage::Error(e) => {
                    self.rewrite_error = Some(e);
                    self.rewrite_rx = None;
                    self.rewrite_phase = RewritePhase::Idle;
                    self.rewrite_preview_progress = None;
                }
            }
        }
    }

    fn drain_rewrite_apply_messages(&mut self) {
        let Some(rx) = &self.rewrite_apply_rx else {
            return;
        };
        let Ok(result) = rx.try_recv() else {
            return;
        };
        self.rewrite_apply_rx = None;
        self.rewrite_phase = RewritePhase::Idle;
        match result {
            Ok(n) => {
                self.show_rewrite_popup = false;
                self.rewrite_preview = None;
                self.start_search();
                self.rewrite_status_note = Some(self.tr().rewrite_applied_ok(n));
            }
            Err(errs) => {
                let msg = errs
                    .iter()
                    .map(|(p, e)| format!("{}: {e}", p.display()))
                    .collect::<Vec<_>>()
                    .join("\n");
                self.rewrite_error = Some(msg);
            }
        }
    }

    /// バックグラウンド検索からのメッセージを処理する
    fn drain_messages(&mut self) {
        let Some(rx) = &self.result_rx else { return };

        let messages: Vec<SearchMessage> = rx.try_iter().collect();

        for msg in messages {
            if self.batch_runner.is_some() {
                match msg {
                    SearchMessage::FileResult { job_id, file } => {
                        let Some(runner) = &mut self.batch_runner else {
                            continue;
                        };
                        let Some(run) = runner.runs.get_mut(runner.active_idx) else {
                            continue;
                        };
                        if run.job_id != job_id {
                            continue;
                        }
                        run.stats.total_matches += file.matches.len();
                        run.stats.total_files += 1;
                        run.results.push(file);
                    }
                    SearchMessage::Progress { job_id, scanned } => {
                        let Some(runner) = &mut self.batch_runner else {
                            continue;
                        };
                        let Some(run) = runner.runs.get_mut(runner.active_idx) else {
                            continue;
                        };
                        if run.job_id == job_id {
                            run.stats.scanned = scanned;
                            self.stats.scanned = scanned;
                        }
                    }
                    SearchMessage::Done {
                        job_id,
                        elapsed_ms,
                        hit_limit_reached,
                    } => {
                        let Some(runner) = &mut self.batch_runner else {
                            continue;
                        };
                        let Some(run) = runner.runs.get_mut(runner.active_idx) else {
                            continue;
                        };
                        if run.job_id != job_id {
                            continue;
                        }
                        run.stats.elapsed_ms = elapsed_ms;
                        run.stats.hit_limit_reached = hit_limit_reached;
                        let list_idx = runner.ordered_indices[runner.active_idx];
                        let ctx_lines = self.batch_jobs[list_idx].context_lines;
                        refresh_match_contexts(&mut run.results, ctx_lines);

                        runner.active_idx += 1;
                        if runner.active_idx < runner.ordered_indices.len() {
                            let Some(ctx) = self.cached_ctx.clone() else {
                                self.batch_runner = None;
                                self.result_rx = None;
                                self.search_state = SearchState::Idle;
                                continue;
                            };
                            self.spawn_search_for_current_batch_job(ctx);
                        } else {
                            self.finish_batch_search();
                        }
                    }
                    SearchMessage::Error { job_id, msg } => {
                        let Some(runner) = &mut self.batch_runner else {
                            continue;
                        };
                        let Some(run) = runner.runs.get_mut(runner.active_idx) else {
                            continue;
                        };
                        if run.job_id != job_id {
                            continue;
                        }
                        run.error = Some(msg);

                        runner.active_idx += 1;
                        if runner.active_idx < runner.ordered_indices.len() {
                            let Some(ctx) = self.cached_ctx.clone() else {
                                self.batch_runner = None;
                                self.result_rx = None;
                                self.search_state = SearchState::Idle;
                                continue;
                            };
                            self.spawn_search_for_current_batch_job(ctx);
                        } else {
                            self.finish_batch_search();
                        }
                    }
                }
            } else {
                match msg {
                    SearchMessage::FileResult { job_id, file: file_result } => {
                        if job_id != SINGLE_SEARCH_JOB_ID {
                            continue;
                        }
                        let file_idx = self.results.len();
                        self.stats.total_matches += file_result.matches.len();
                        self.stats.total_files += 1;
                        for (match_idx, m) in file_result.matches.iter().enumerate() {
                            self.push_table_row(TableRowRef { file_idx, match_idx }, m);
                        }
                        self.results.push(file_result);
                    }
                    SearchMessage::Progress { job_id, scanned } => {
                        if job_id == SINGLE_SEARCH_JOB_ID {
                            self.stats.scanned = scanned;
                        }
                    }
                    SearchMessage::Done {
                        job_id,
                        elapsed_ms,
                        hit_limit_reached,
                    } => {
                        if job_id != SINGLE_SEARCH_JOB_ID {
                            continue;
                        }
                        self.stats.elapsed_ms = elapsed_ms;
                        self.stats.hit_limit_reached = hit_limit_reached;
                        self.search_state = SearchState::Done;
                        self.result_rx = None;
                        // 検索中にスライダーが変わった場合も含め、現在の設定でコンテキストを揃える
                        refresh_match_contexts(&mut self.results, self.context_lines);
                        self.rebuild_table_row_metrics();
                        self.last_context_lines_applied = self.context_lines;
                    }
                    SearchMessage::Error { job_id, msg } => {
                        if job_id != SINGLE_SEARCH_JOB_ID {
                            continue;
                        }
                        self.search_state = SearchState::Error(msg);
                        self.result_rx = None;
                    }
                }
            }
        }
    }
}

impl eframe::App for AstGrepApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Context をキャッシュ（spawn_search で使用）
        self.cached_ctx = Some(ctx.clone());

        let title = self.tr().window_title().to_string();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        // バックグラウンドからメッセージをドレイン
        self.drain_messages();
        self.drain_rewrite_messages();
        self.drain_rewrite_apply_messages();

        // コンテキスト行数: Shift + Page Up / Page Down（修飾キー付きのためテキスト入力中も利用可）
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::SHIFT, egui::Key::PageUp) {
                self.context_lines = (self.context_lines + 1).min(10);
            }
            if i.consume_key(egui::Modifiers::SHIFT, egui::Key::PageDown) {
                self.context_lines = self.context_lines.saturating_sub(1);
            }
        });

        // ヘルプポップアップ
        help_popup::show(self, ctx);

        // パターン支援ポップアップ
        pattern_assist_popup::show(self, ctx);

        // 正規表現 visualiser
        regex_visualizer_popup::show(self, ctx);

        // 上部ツールバー
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(4.0);
            toolbar::show(self, ui);
            ui.add_space(4.0);
        });

        // コンテキスト行数が変わる直前: 表モードではホバー行へ追従スクロール用の行番号を記録
        if self.view_mode == ViewMode::Table
            && !self.results.is_empty()
            && self.context_lines != self.last_context_lines_applied
        {
            self.table_scroll_to_row = self.table_last_clicked_row;
        }

        self.sync_match_contexts_from_ui();

        // 表モード: マッチ行のコードプレビュー（sync 後に描画し、コンテキスト更新と一致させる）
        table_preview_popup::show(self, ctx);

        // Rewrite プレビュー / 確認
        rewrite_popup::show(self, ctx);

        // ターミナルパネル（ステータスバーより先に宣言して、その上に表示）
        if self.show_terminal {
            let terminal = self
                .terminal
                .get_or_insert_with(|| TerminalState::new(self.file_encoding_preference));
            terminal.file_encoding_preference = self.file_encoding_preference;

            let panel_id = egui::Id::new("terminal_panel");

            // 初回は画面の半分をデフォルト高さとして使う
            if self.terminal_height <= 0.0 {
                self.terminal_height = ctx.screen_rect().height() * 0.5;
            }

            // PanelState に直接希望の高さを書き込む（古い誤った値を上書き）。
            // egui は PanelState.rect.height() をパネル高さとして使うため、
            // available_rect から逆算した rect を作って注入する。
            let avail = ctx.available_rect();
            let desired_rect = egui::Rect::from_min_max(
                egui::pos2(avail.min.x, avail.max.y - self.terminal_height),
                egui::pos2(avail.max.x, avail.max.y),
            );
            ctx.data_mut(|d| {
                d.insert_persisted(
                    panel_id,
                    egui::panel::PanelState { rect: desired_rect },
                )
            });

            let rect_before = ctx.available_rect();
            egui::TopBottomPanel::bottom("terminal_panel")
                .resizable(true)
                .min_height(120.0)
                .default_height(self.terminal_height)
                .frame(
                    egui::Frame::default()
                        .fill(egui::Color32::from_rgb(18, 18, 18))
                        .inner_margin(egui::Margin::same(6.0)),
                )
                .show(ctx, |ui| {
                    // パネルのタイトルバー
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("⌨ Terminal")
                                .color(egui::Color32::LIGHT_GRAY)
                                .small(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button("✕")
                                .on_hover_text("ターミナルを閉じる")
                                .clicked()
                            {
                                self.show_terminal = false;
                            }
                        });
                    });
                    ui.separator();
                    terminal_panel::show(self, ui);
                    // 残りスペースを消費してコンテンツ rect をパネル全体に広げる
                    let remaining = ui.available_size();
                    if remaining.y > 0.0 {
                        ui.allocate_space(remaining);
                    }
                });
            // ユーザーがリサイズした高さを測定して保存する
            let rect_after = ctx.available_rect();
            let consumed = rect_before.height() - rect_after.height();
            if consumed > 0.0 {
                self.terminal_height = consumed;
            }
        }

        // 下部ステータスバー
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.add_space(4.0);
            status_bar::show(self, ui);
            ui.add_space(4.0);
        });

        match self.view_mode {
            ViewMode::Code => {
                // 左ペイン: ファイル一覧
                egui::SidePanel::left("file_panel")
                    .default_width(200.0)
                    .width_range(120.0..=400.0)
                    .show(ctx, |ui| {
                        file_panel::show(self, ui);
                    });

                // 中央: コードビュー
                egui::CentralPanel::default().show(ctx, |ui| {
                    code_panel::show(self, ui);
                });
            }
            ViewMode::Table => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    table_panel::show(self, ui);
                });
            }
            ViewMode::BatchReport => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    batch_report_panel::show(self, ui);
                });
            }
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, APP_STATE_KEY, &self.persisted_state());
    }
}
