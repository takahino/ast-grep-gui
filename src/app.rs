use std::path::PathBuf;

use crossbeam_channel::Receiver;
use eframe::egui;

use crate::file_encoding::{FileEncoding, FileEncodingPreference};
use crate::highlight::Highlighter;
use crate::i18n::{Tr, UiLanguage, UiLanguagePreference};
use crate::lang::SupportedLanguage;
use crate::search::{
    refresh_match_contexts, spawn_search, FileResult, MatchItem, SearchConditions, SearchMessage,
    SearchMode, SearchStats,
};
use crate::pattern_assist::PatternSuggestion;
use crate::terminal::TerminalState;
use crate::ui::{
    code_panel, file_panel, help_popup, pattern_assist_popup, status_bar, table_panel,
    table_preview_popup, terminal_panel, toolbar,
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
}

#[derive(Debug, Clone, Copy)]
pub struct TableRowRef {
    pub file_idx: usize,
    pub match_idx: usize,
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
    /// パターンサジェストポップアップで現在選択中の候補インデックス
    pub pattern_suggest_idx: Option<usize>,
    /// ターミナルパネルの表示フラグ
    pub show_terminal: bool,
    /// ターミナルパネルの状態（初回表示時に初期化）
    pub terminal: Option<TerminalState>,
    /// ターミナルパネルの高さ（0.0 = 未設定）
    pub terminal_height: f32,

    // バックグラウンド検索チャンネル
    result_rx: Option<Receiver<SearchMessage>>,
    // repaintのためにContextをキャッシュ
    cached_ctx: Option<egui::Context>,
    /// 表示に反映済みのコンテキスト行数（変更時に結果の前後行だけ再計算する）
    last_context_lines_applied: usize,
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
            pattern_suggest_idx: None,
            show_terminal: false,
            terminal: None,
            terminal_height: 0.0,
            result_rx: None,
            cached_ctx: None,
            last_context_lines_applied: persisted.context_lines,
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
        }
    }

    /// 検索を開始する
    pub fn start_search(&mut self) {
        if self.search_dir.is_empty() || self.pattern.is_empty() {
            return;
        }

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
            tx,
            ctx,
        );
    }

    /// 検索を停止する（チャンネルをドロップ）
    pub fn stop_search(&mut self) {
        self.result_rx = None;
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

    /// バックグラウンド検索からのメッセージを処理する
    fn drain_messages(&mut self) {
        let Some(rx) = &self.result_rx else { return };

        let messages: Vec<SearchMessage> = rx.try_iter().collect();

        for msg in messages {
            match msg {
                SearchMessage::FileResult(file_result) => {
                    let file_idx = self.results.len();
                    self.stats.total_matches += file_result.matches.len();
                    self.stats.total_files += 1;
                    for (match_idx, m) in file_result.matches.iter().enumerate() {
                        self.push_table_row(TableRowRef { file_idx, match_idx }, m);
                    }
                    self.results.push(file_result);
                }
                SearchMessage::Progress { scanned } => {
                    self.stats.scanned = scanned;
                }
                SearchMessage::Done {
                    elapsed_ms,
                    hit_limit_reached,
                } => {
                    self.stats.elapsed_ms = elapsed_ms;
                    self.stats.hit_limit_reached = hit_limit_reached;
                    self.search_state = SearchState::Done;
                    self.result_rx = None;
                    // 検索中にスライダーが変わった場合も含め、現在の設定でコンテキストを揃える
                    refresh_match_contexts(&mut self.results, self.context_lines);
                    self.rebuild_table_row_metrics();
                    self.last_context_lines_applied = self.context_lines;
                }
                SearchMessage::Error(msg) => {
                    self.search_state = SearchState::Error(msg);
                    self.result_rx = None;
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
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, APP_STATE_KEY, &self.persisted_state());
    }
}
