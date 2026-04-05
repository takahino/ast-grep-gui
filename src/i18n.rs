//! UI 表示言語（日本語 / 英語）。検索対象言語 `SupportedLanguage` とは独立。

/// 実際に UI に使う言語
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiLanguage {
    Japanese,
    English,
}

/// 永続化する UI 言語設定（OS に従う / 固定）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UiLanguagePreference {
    /// OS ロケールから推定
    Auto,
    Japanese,
    English,
}

impl Default for UiLanguagePreference {
    fn default() -> Self {
        Self::Auto
    }
}

impl UiLanguagePreference {
    pub fn effective(self) -> UiLanguage {
        match self {
            Self::Auto => detect_os_ui_language(),
            Self::Japanese => UiLanguage::Japanese,
            Self::English => UiLanguage::English,
        }
    }

    pub fn display_label(self, lang: UiLanguage) -> &'static str {
        match (self, lang) {
            (Self::Auto, UiLanguage::Japanese) => "自動 (OS)",
            (Self::Auto, UiLanguage::English) => "Auto (OS)",
            (Self::Japanese, UiLanguage::Japanese) => "日本語",
            (Self::Japanese, UiLanguage::English) => "Japanese",
            (Self::English, _) => "English",
        }
    }
}

/// OS ロケールから UI 言語を推定（日本語系なら日本語、それ以外は英語）
pub fn detect_os_ui_language() -> UiLanguage {
    if let Some(locale) = sys_locale::get_locale() {
        let lower = locale.to_lowercase();
        if lower.starts_with("ja") {
            return UiLanguage::Japanese;
        }
    }
    UiLanguage::English
}

/// パターン支援などで `(ja, en)` を 1 行にまとめる
#[inline]
pub fn tr_pair(lang: UiLanguage, ja: &'static str, en: &'static str) -> String {
    match lang {
        UiLanguage::Japanese => ja.to_string(),
        UiLanguage::English => en.to_string(),
    }
}

/// 翻訳アクセサ（`app.tr()` から利用）
#[derive(Clone, Copy)]
pub struct Tr(pub UiLanguage);

impl Tr {
    // ─── main / window ─────────────────────────────────────────────────

    pub fn window_title(self) -> &'static str {
        "ast-grep GUI"
    }

    // ─── toolbar ───────────────────────────────────────────────────────

    pub fn directory_label(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ディレクトリ:",
            UiLanguage::English => "Directory:",
        }
    }
    pub fn directory_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ASTパターンで検索するフォルダを指定します",
            UiLanguage::English => "Folder to search with AST patterns",
        }
    }
    pub fn directory_hint(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "検索対象のディレクトリ",
            UiLanguage::English => "Directory to search",
        }
    }
    pub fn browse(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "参照...",
            UiLanguage::English => "Browse...",
        }
    }

    pub fn ui_language_label(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "表示言語:",
            UiLanguage::English => "UI language:",
        }
    }
    pub fn ui_language_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "インターフェースの表示言語を選びます（設定は保存されます）",
            UiLanguage::English => "Choose interface language (saved with settings)",
        }
    }

    pub fn mode_label(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "モード:",
            UiLanguage::English => "Mode:",
        }
    }
    pub fn mode_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "検索方法を選択します\n\
                 AST: ast-grep の AST パターン検索（マッチ範囲は本体・CLI と同じ）\n\
                 ast-grepそのまま: 検索は同じ。コードパネルに CLI 風の出力を表示\n\
                 文字列: 通常のテキスト検索（単純な部分一致）\n\
                 正規表現: 正規表現パターンで検索"
            }
            UiLanguage::English => {
                "Search method\n\
                 AST: ast-grep AST patterns (match spans match the CLI)\n\
                 ast-grep (raw): same search; code panel shows console-style output\n\
                 Text: plain substring search\n\
                 Regex: regular expression search"
            }
        }
    }
    pub fn mode_ast(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "AST",
            UiLanguage::English => "AST",
        }
    }
    pub fn mode_ast_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "ast-grep のASTパターンで検索します\n\
                 $VAR・$$$ARGS などのメタ変数が使えます\n\
                 コードの構造を理解した検索が可能です\n\
                 マッチ範囲は ast-grep 本体・CLI と同じです"
            }
            UiLanguage::English => {
                "Search with ast-grep AST patterns\n\
                 Meta-variables like $VAR, $$$ARGS\n\
                 Match spans match ast-grep / the CLI"
            }
        }
    }
    pub fn mode_ast_raw(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ast-grepそのまま",
            UiLanguage::English => "ast-grep (raw)",
        }
    }
    pub fn mode_ast_raw_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "AST モードと同じ検索結果を、コードパネルで CLI に近いテキストとして表示します\n\
                 エクスポートや表のマッチ範囲は AST モードと同じです"
            }
            UiLanguage::English => {
                "Same search as AST mode; code panel shows console-style text\n\
                 Table/export spans are identical to AST mode"
            }
        }
    }
    pub fn mode_plain(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "文字列",
            UiLanguage::English => "Text",
        }
    }
    pub fn mode_plain_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "入力した文字列を含む行を検索します\n\
                 大文字/小文字を区別します\n\
                 ファイルフィルタ未指定時は全ファイルを対象にします"
            }
            UiLanguage::English => {
                "Search for the substring (case-sensitive)\n\
                 Without a file filter, all files are searched"
            }
        }
    }
    pub fn mode_regex(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "正規表現",
            UiLanguage::English => "Regex",
        }
    }
    pub fn mode_regex_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "正規表現パターンで検索します\n\
                 例) foo\\(.*\\)  \\berror\\b  ^import\n\
                 ファイルフィルタ未指定時は全ファイルを対象にします"
            }
            UiLanguage::English => {
                "Search with a regex (Rust regex syntax)\n\
                 e.g. foo\\(.*\\)  \\berror\\b  ^import\n\
                 Without a file filter, all files are searched"
            }
        }
    }

    pub fn search_lang_label(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "言語:",
            UiLanguage::English => "Language:",
        }
    }
    pub fn search_lang_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "検索対象のプログラミング言語を選択します\n\
                 自動: 拡張子から言語を判定し、混在プロジェクトではファイルごとに別言語で解析します\n\
                 手動: その言語の拡張子のみを対象にします"
            }
            UiLanguage::English => {
                "Language for AST search\n\
                 Auto: detect language from each file extension (mixed projects supported)\n\
                 Manual: only extensions for that language are searched"
            }
        }
    }

    /// AST モード: 拡張子と解析言語の対応表の見出し
    pub fn ext_mapping_title(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "拡張子 → 解析言語（grep）",
            UiLanguage::English => "Extension → parser (AST grep)",
        }
    }

    pub fn ext_mapping_file_filter_note(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "※ ファイル名フィルタを指定している場合、一覧以外の拡張子のファイルも対象になることがあります"
            }
            UiLanguage::English => {
                "※ With a file name filter, files with other extensions may be searched too"
            }
        }
    }

    pub fn ext_mapping_plain_regex(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "文字列／正規表現モードでは拡張子による言語分類は使いません（対象はファイル名フィルタ・詳細設定に従います）"
            }
            UiLanguage::English => {
                "Text/Regex mode does not map extensions to a parser (files follow the name filter and skip rules)"
            }
        }
    }
    pub fn all_files_note(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "（全ファイル対象）",
            UiLanguage::English => "(all files)",
        }
    }
    pub fn all_files_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ファイルフィルタを設定することで対象を絞り込めます",
            UiLanguage::English => "Use the file filter to narrow targets",
        }
    }

    pub fn context_lines_label(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "コンテキスト行数:",
            UiLanguage::English => "Context lines:",
        }
    }
    pub fn context_lines_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "マッチした行の前後に何行表示するかを指定します\n\
                 0 = マッチ行のみ\n\
                 2 = マッチ行の前後2行ずつ表示（デフォルト）\n\
                 エクスポート時の出力範囲にも影響します\n\
                 Shift+Page Up / Shift+Page Down で1ずつ増減（いつでも利用可）"
            }
            UiLanguage::English => {
                "Lines of context before/after each match\n\
                 0 = match line only\n\
                 2 = two lines each side (default)\n\
                 Affects exports too\n\
                 Shift+Page Up / Shift+Page Down: ±1 (always available)"
            }
        }
    }
    pub fn context_drag_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ドラッグまたは数値を直接入力して変更できます",
            UiLanguage::English => "Drag or type a number",
        }
    }
    pub fn context_lines_decrease_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "前後コンテキスト行を1つ減らす",
            UiLanguage::English => "Decrease context lines by 1",
        }
    }
    pub fn context_lines_increase_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "前後コンテキスト行を1つ増やす",
            UiLanguage::English => "Increase context lines by 1",
        }
    }

    pub fn file_filter_label(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ファイルフィルタ:",
            UiLanguage::English => "File filter:",
        }
    }
    pub fn file_filter_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "検索するファイル名を絞り込みます\n\
                 空のままだと「言語」設定の拡張子が使われます\n\
                 複数指定する場合は ; で区切ります"
            }
            UiLanguage::English => {
                "Filter by file name pattern\n\
                 Empty = use language default extensions\n\
                 Multiple patterns: separate with ;"
            }
        }
    }
    pub fn file_filter_hint(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "*.rs;*.toml  (空=言語デフォルト)",
            UiLanguage::English => "*.rs;*.toml  (empty = language default)",
        }
    }
    pub fn file_filter_hover(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "ファイル名パターンを ; 区切りで指定します\n\
                 ─────────────────────────\n\
                 * … 任意の文字列にマッチ\n\
                 .  … ドット（エスケープ不要）\n\
                 正規表現も使えます\n\
                 ─────────────────────────\n\
                 例）*.rs;*.toml\n\
                 例）test_.*\\.java\n\
                 例）Main.java"
            }
            UiLanguage::English => {
                "File name patterns separated by ;\n\
                 ─────────────────────────\n\
                 * … matches any substring\n\
                 .  … literal dot (no escape needed)\n\
                 Regex features allowed\n\
                 ─────────────────────────\n\
                 e.g. *.rs;*.toml\n\
                 e.g. test_.*\\.java\n\
                 e.g. Main.java"
            }
        }
    }

    pub fn pattern_label_tooltip_ast(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "ASTパターンを入力します\n\
                 ─────────────────────────\n\
                 $VAR   … 任意の単一ノードにマッチ\n\
                 $$$VAR … 0個以上の複数ノードにマッチ\n\
                 $_     … 何にでもマッチ（キャプチャしない）\n\
                 ─────────────────────────\n\
                 例）fn $NAME($$$ARGS)\n\
                 例）$X.unwrap()\n\
                 Enter キーでも検索を開始できます\n\
                 「ℹ ヘルプ」でプリセット一覧を表示"
            }
            UiLanguage::English => {
                "Enter an AST pattern\n\
                 ─────────────────────────\n\
                 $VAR   … single node\n\
                 $$$VAR … zero or more nodes\n\
                 $_     … match without capture\n\
                 ─────────────────────────\n\
                 e.g. fn $NAME($$$ARGS)\n\
                 Press Enter to search\n\
                 See Help for presets"
            }
        }
    }
    pub fn pattern_label_tooltip_ast_raw(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "ast-grep 本体にそのまま渡す AST パターンを入力します\n\
                 ─────────────────────────\n\
                 $VAR   … 任意の単一ノードにマッチ\n\
                 $$$VAR … 0個以上の複数ノードにマッチ\n\
                 $_     … 何にでもマッチ（キャプチャしない）\n\
                 ─────────────────────────\n\
                 AST モードと同じ（マッチ範囲は本体・CLI と一致）"
            }
            UiLanguage::English => {
                "AST pattern passed to ast-grep as-is\n\
                 Same match spans as AST mode / the CLI"
            }
        }
    }
    pub fn pattern_label_tooltip_plain(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "検索したい文字列をそのまま入力します\n\
                 大文字/小文字を区別します\n\
                 例）TODO  println!  System.out.println"
            }
            UiLanguage::English => {
                "Plain text to find (case-sensitive)\n\
                 e.g. TODO  println!  unwrap()"
            }
        }
    }
    pub fn pattern_label_tooltip_regex(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "正規表現パターンを入力します（Rust の regex クレート構文）\n\
                 ─────────────────────────\n\
                 .     … 任意の1文字\n\
                 .*    … 任意の文字列\n\
                 \\b    … 単語境界\n\
                 [A-Z] … 文字クラス\n\
                 ─────────────────────────\n\
                 例）TODO:.*  \\bpanic!\\b  ^import\\s"
            }
            UiLanguage::English => {
                "Regular expression (Rust regex syntax)\n\
                 .  *  \\b  character classes, etc."
            }
        }
    }
    pub fn pattern_hint_ast(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "例: fn $NAME($$$ARGS)",
            UiLanguage::English => "e.g. fn $NAME($$$ARGS)",
        }
    }
    pub fn pattern_hint_ast_raw(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "例: for ($$$ARGS)",
            UiLanguage::English => "e.g. for ($$$ARGS)",
        }
    }
    pub fn pattern_hint_plain(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "例: TODO  println!  unwrap()",
            UiLanguage::English => "e.g. TODO  println!  unwrap()",
        }
    }
    pub fn pattern_hint_regex(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "例: TODO:.*  \\bpanic!\\b",
            UiLanguage::English => "e.g. TODO:.*  \\bpanic!\\b",
        }
    }

    pub fn pattern_colon(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "パターン:",
            UiLanguage::English => "Pattern:",
        }
    }

    pub fn stop(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "停止",
            UiLanguage::English => "Stop",
        }
    }
    pub fn stop_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "検索を中断します",
            UiLanguage::English => "Cancel search",
        }
    }
    pub fn search_btn(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "検索",
            UiLanguage::English => "Search",
        }
    }
    pub fn search_tooltip_ast(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ASTパターン検索を開始します（マッチ範囲は ast-grep 本体と同じ）",
            UiLanguage::English => "Start AST search (same spans as ast-grep CLI)",
        }
    }
    pub fn search_tooltip_ast_raw(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "AST モードと同じ検索を開始します（コードパネルは CLI 風表示）",
            UiLanguage::English => "Same search as AST mode (console-style code panel)",
        }
    }
    pub fn search_tooltip_plain(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "文字列検索を開始します（Enter キーでも可）",
            UiLanguage::English => "Start text search (Enter also works)",
        }
    }
    pub fn search_tooltip_regex(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "正規表現検索を開始します（Enter キーでも可）",
            UiLanguage::English => "Start regex search (Enter also works)",
        }
    }

    pub fn clear_results(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "クリア",
            UiLanguage::English => "Clear",
        }
    }
    pub fn clear_results_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "検索結果をすべて消去します",
            UiLanguage::English => "Clear all results",
        }
    }

    pub fn help_btn(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ℹ ヘルプ",
            UiLanguage::English => "ℹ Help",
        }
    }
    pub fn help_btn_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "パターンの書き方とプリセット一覧を表示します",
            UiLanguage::English => "Pattern help and presets",
        }
    }
    pub fn pattern_assist_btn(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "✨ パターン支援",
            UiLanguage::English => "✨ Pattern assist",
        }
    }
    pub fn pattern_assist_btn_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "コード片を入力すると、マッチするパターン候補を自動生成します",
            UiLanguage::English => "Generate pattern candidates from a code snippet",
        }
    }
    pub fn regex_visualizer_btn(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "🧩 Regex 補助",
            UiLanguage::English => "🧩 Regex helper",
        }
    }
    pub fn regex_visualizer_btn_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "正規表現の構造を見やすく分解し、コンパイル可否も確認できます"
            }
            UiLanguage::English => {
                "Break down the regex structure and check whether it compiles"
            }
        }
    }
    pub fn regex_visualizer_window_title(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "Regex visualiser",
            UiLanguage::English => "Regex visualiser",
        }
    }
    pub fn regex_visualizer_intro(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "現在の正規表現をライブで分解表示します。量指定、分岐、グループ、文字クラスの位置が追いやすくなります。"
            }
            UiLanguage::English => {
                "Live breakdown of the current regex so groups, alternations, character classes, and quantifiers are easier to follow."
            }
        }
    }
    pub fn regex_visualizer_pattern_label(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "対象の正規表現:",
            UiLanguage::English => "Regex pattern:",
        }
    }
    pub fn regex_visualizer_status_ok(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "コンパイル成功",
            UiLanguage::English => "Compiles successfully",
        }
    }
    pub fn regex_visualizer_status_error(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "コンパイルエラー",
            UiLanguage::English => "Compile error",
        }
    }
    pub fn regex_visualizer_summary(
        self,
        groups: usize,
        alternations: usize,
        char_classes: usize,
        quantifiers: usize,
    ) -> String {
        match self.0 {
            UiLanguage::Japanese => format!(
                "グループ: {groups} / 分岐: {alternations} / 文字クラス: {char_classes} / 量指定: {quantifiers}"
            ),
            UiLanguage::English => format!(
                "Groups: {groups} / Alternations: {alternations} / Character classes: {char_classes} / Quantifiers: {quantifiers}"
            ),
        }
    }
    pub fn regex_visualizer_empty(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "正規表現を入力すると、ここに分解結果が表示されます。",
            UiLanguage::English => "Enter a regex pattern to see the breakdown here.",
        }
    }
    pub fn regex_visualizer_automaton_heading(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "簡易オートマトン表示",
            UiLanguage::English => "Simplified automaton view",
        }
    }
    pub fn regex_visualizer_test_label(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "テスト文字列:",
            UiLanguage::English => "Test text:",
        }
    }
    pub fn regex_visualizer_test_hint(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "ここに文字列を入力すると、上の正規表現でマッチ箇所を試せます"
            }
            UiLanguage::English => {
                "Type sample text here to try matches against the pattern above"
            }
        }
    }
    pub fn regex_visualizer_test_matches_heading(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "マッチ結果",
            UiLanguage::English => "Matches",
        }
    }
    pub fn regex_visualizer_test_no_matches(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "マッチなし",
            UiLanguage::English => "No matches",
        }
    }
    pub fn regex_visualizer_test_match_truncated(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "（先頭のみ表示）",
            UiLanguage::English => "(showing first matches only)",
        }
    }
    pub fn regex_visualizer_test_count(self, n: usize) -> String {
        match self.0 {
            UiLanguage::Japanese => format!("ヒット: {n} 件"),
            UiLanguage::English => format!("Hits: {n}"),
        }
    }

    pub fn view_code(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "📄 コード",
            UiLanguage::English => "📄 Code",
        }
    }
    pub fn view_code_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "シンタックスハイライト付きコードビューで結果を表示します",
            UiLanguage::English => "Syntax-highlighted code view",
        }
    }
    pub fn view_table(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "📊 表",
            UiLanguage::English => "📊 Table",
        }
    }
    pub fn view_table_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "ファイル・行・列・マッチ範囲の原文・該当行付近（前後コンテキスト）を一覧表示します"
            }
            UiLanguage::English => {
                "Table: file, line, col, matched span, and surrounding source lines"
            }
        }
    }

    pub fn advanced_settings(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "⚙ 詳細設定",
            UiLanguage::English => "⚙ Advanced",
        }
    }
    pub fn file_encoding_label(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ファイル文字コード:",
            UiLanguage::English => "File encoding:",
        }
    }
    pub fn file_encoding_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "テキストファイルをどの文字コードで読むかを指定します\n\
                 自動判定: UTF-8 / UTF-16 / Shift_JIS / EUC-JP / JIS / GBK / Big5 / EUC-KR / Latin1 系を推定します\n\
                 手動指定の既定候補: UTF-8 / UTF-16 LE / UTF-16 BE / Shift_JIS / EUC-JP / JIS / GBK / Big5 / EUC-KR / Latin1 系\n\
                 選択ファイルの判定結果も横に反映します"
            }
            UiLanguage::English => {
                "Choose how text files are decoded\n\
                 Auto detects UTF-8 / UTF-16 / Shift_JIS / EUC-JP / JIS / GBK / Big5 / EUC-KR / Latin1 families\n\
                 Default manual choices: UTF-8 / UTF-16 LE / UTF-16 BE / Shift_JIS / EUC-JP / JIS / GBK / Big5 / EUC-KR / Latin1 families\n\
                 Detected results are also reflected beside the selected file"
            }
        }
    }
    pub fn max_file_size_label(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "最大ファイルサイズ (MB):",
            UiLanguage::English => "Max file size (MB):",
        }
    }
    pub fn max_file_size_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "このサイズを超えるファイルは検索対象から除外されます\n\
                 巨大なバイナリや自動生成ファイルのスキップに使えます"
            }
            UiLanguage::English => "Skip files larger than this (binaries, generated files)",
        }
    }
    pub fn max_file_size_drag_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ドラッグまたは直接入力で変更できます（1〜500 MB）",
            UiLanguage::English => "1–500 MB",
        }
    }
    pub fn max_search_hits_label(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ヒット上限:",
            UiLanguage::English => "Max hits:",
        }
    }
    pub fn max_search_hits_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "収集するマッチ件数の上限です。多すぎるとメモリを大量に使うため、上限で打ち切ります\n\
                 0 を指定すると無制限です"
            }
            UiLanguage::English => {
                "Maximum number of matches to collect (memory). Use 0 for unlimited"
            }
        }
    }
    pub fn max_search_hits_drag_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "0 = 無制限、1 以上で件数上限（例: 100000）",
            UiLanguage::English => "0 = unlimited; 1+ caps collected hits",
        }
    }
    pub fn skip_dirs_label(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "スキップディレクトリ:",
            UiLanguage::English => "Skip directories:",
        }
    }
    pub fn skip_dirs_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "走査をスキップするディレクトリ名を ; 区切りで指定します\n\
                 ディレクトリ名が一致した場合、そのフォルダ配下を丸ごとスキップします\n\
                 ─────────────────────────\n\
                 例）.git;target;node_modules"
            }
            UiLanguage::English => {
                "Directory names to skip (semicolon-separated)\n\
                 e.g. .git;target;node_modules"
            }
        }
    }
    pub fn skip_dirs_hint(self) -> &'static str {
        ".git;target;node_modules;…"
    }
    pub fn skip_dirs_hover(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "ディレクトリ名を ; 区切りで入力します\n\
                 フルパスではなくディレクトリ名のみで指定します"
            }
            UiLanguage::English => "Names only (not full paths), separated by ;",
        }
    }

    pub fn footer_hint_ast(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "$VAR=単一ノード  $$$ARGS=複数ノード  $_=無視  例: fn $NAME($$$ARGS)  $X.unwrap()  if ($COND) { $$$BODY }"
            }
            UiLanguage::English => {
                "$VAR=single node  $$$ARGS=multiple nodes  $_=ignore  e.g. fn $NAME($$$ARGS)  $X.unwrap()  if ($COND) { $$$BODY }"
            }
        }
    }
    pub fn footer_hint_non_ast(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ファイルフィルタと詳細設定で対象を絞り込めます",
            UiLanguage::English => "Use file filter and advanced settings to narrow targets",
        }
    }

    pub fn terminal_input_hint(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "コマンドを入力… (Enter: 実行 / Shift+Enter: 改行)",
            UiLanguage::English => "Enter a command... (Enter: run / Shift+Enter: newline)",
        }
    }

    // ─── status bar ────────────────────────────────────────────────────

    pub fn status_idle(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "待機中",
            UiLanguage::English => "Idle",
        }
    }
    pub fn status_searching(self, scanned: usize) -> String {
        match self.0 {
            UiLanguage::Japanese => format!("検索中... {} ファイルスキャン済み", scanned),
            UiLanguage::English => format!("Searching... {} files scanned", scanned),
        }
    }
    pub fn status_done(self, matches: usize, files: usize, ms: u64, hit_limit_reached: bool) -> String {
        let base = match self.0 {
            UiLanguage::Japanese => format!("{}件 / {}ファイル / {}ms", matches, files, ms),
            UiLanguage::English => format!("{} matches / {} files / {}ms", matches, files, ms),
        };
        if hit_limit_reached {
            match self.0 {
                UiLanguage::Japanese => format!("{}（ヒット上限）", base),
                UiLanguage::English => format!("{} (hit limit)", base),
            }
        } else {
            base
        }
    }
    pub fn status_error(self, msg: &str) -> String {
        match self.0 {
            UiLanguage::Japanese => format!("エラー: {msg}"),
            UiLanguage::English => format!("Error: {msg}"),
        }
    }

    pub fn export_excel(self) -> &'static str {
        "Excel"
    }
    pub fn export_excel_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "検索結果を Excel (.xlsx) ファイルに出力します\n\
                 ファイル名・行・列・マッチテキストを表形式で保存します"
            }
            UiLanguage::English => {
                "Export results to .xlsx\n\
                 Columns: file, line, column, match text"
            }
        }
    }
    pub fn export_html(self) -> &'static str {
        "HTML"
    }
    pub fn export_html_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "検索結果を HTML テーブル形式で出力します",
            UiLanguage::English => "Export as HTML table",
        }
    }
    pub fn export_md(self) -> &'static str {
        "Markdown"
    }
    pub fn export_md_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "検索結果を Markdown テーブル形式で出力します",
            UiLanguage::English => "Export as Markdown table",
        }
    }
    pub fn export_json(self) -> &'static str {
        "JSON"
    }
    pub fn export_json_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "検索結果を JSON 形式で出力します",
            UiLanguage::English => "Export as JSON",
        }
    }
    pub fn export_txt(self) -> &'static str {
        "TXT"
    }
    pub fn export_txt_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "検索結果をプレーンテキストで出力します",
            UiLanguage::English => "Export as plain text",
        }
    }
    pub fn copy_results(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "コピー",
            UiLanguage::English => "Copy",
        }
    }
    pub fn copy_results_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "検索結果をクリップボードにコピーします",
            UiLanguage::English => "Copy results to clipboard",
        }
    }

    pub fn err_export_excel(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "Excelエクスポートエラー:",
            UiLanguage::English => "Excel export error:",
        }
    }
    pub fn err_export_html(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "HTML書き出しエラー:",
            UiLanguage::English => "HTML export error:",
        }
    }
    pub fn err_export_md(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "Markdown書き出しエラー:",
            UiLanguage::English => "Markdown export error:",
        }
    }
    pub fn err_export_json(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "JSON書き出しエラー:",
            UiLanguage::English => "JSON export error:",
        }
    }
    pub fn err_export_txt(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "テキスト書き出しエラー:",
            UiLanguage::English => "Text export error:",
        }
    }
    pub fn err_clipboard(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "クリップボードコピーエラー:",
            UiLanguage::English => "Clipboard error:",
        }
    }

    pub fn file_filter_txt(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "テキスト",
            UiLanguage::English => "Text",
        }
    }

    // ─── help popup ────────────────────────────────────────────────────

    pub fn help_window_title(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "パターンヘルプ",
            UiLanguage::English => "Pattern help",
        }
    }
    pub fn help_meta_heading(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "メタ変数の使い方",
            UiLanguage::English => "Meta-variables",
        }
    }
    pub fn help_meta_var_single(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "任意の単一ノード（式・識別子・型など）にマッチ  例: $NAME  $EXPR  $TYPE"
            }
            UiLanguage::English => {
                "Matches a single node (expression, identifier, type, ...)  e.g. $NAME  $EXPR  $TYPE"
            }
        }
    }
    pub fn help_meta_multi(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "0個以上の複数ノードにマッチ  例: $$$ARGS  $$$BODY  $$$ITEMS"
            }
            UiLanguage::English => {
                "Matches zero or more nodes  e.g. $$$ARGS  $$$BODY  $$$ITEMS"
            }
        }
    }
    pub fn help_meta_ignore(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "何にでもマッチするが、キャプチャしない  例: foo($_)  if ($_) { $$$BODY }"
            }
            UiLanguage::English => {
                "Matches anything without capturing  e.g. foo($_)  if ($_) { $$$BODY }"
            }
        }
    }
    pub fn help_meta_same(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "同じ変数名は同じノードにマッチ  例: $A == $A",
            UiLanguage::English => "Same name -> same node  e.g. $A == $A",
        }
    }
    pub fn help_meta_same_var_key(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "$A (同じ変数名)",
            UiLanguage::English => "$A (same name)",
        }
    }
    pub fn help_presets_heading(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "プリセットパターン",
            UiLanguage::English => "Presets",
        }
    }
    pub fn help_examples_heading(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "使用例 (クリックで適用)",
            UiLanguage::English => "Examples (click to apply)",
        }
    }
    pub fn help_example_1_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "すべての関数定義を検索 (Rust)",
            UiLanguage::English => "All function definitions (Rust)",
        }
    }
    pub fn help_example_2_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "unwrap()の呼び出し箇所 (Rust)",
            UiLanguage::English => "unwrap() calls (Rust)",
        }
    }
    pub fn help_example_3_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "console.logの使用箇所 (JS/TS)",
            UiLanguage::English => "console.log (JS/TS)",
        }
    }
    pub fn help_popup_browser_blurb(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "下記は要点のみです。記法の詳細・言語別の注意・トラブルシューティングはブラウザの詳細ヘルプを参照してください。"
            }
            UiLanguage::English => {
                "Below is a short summary. Full syntax, per-language notes, and troubleshooting are in the detailed HTML help (opens in your browser)."
            }
        }
    }
    pub fn help_open_browser_btn(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "詳細ヘルプをブラウザで開く",
            UiLanguage::English => "Open full help in browser",
        }
    }
    pub fn help_open_browser_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "埋め込みの HTML ドキュメントを既定ブラウザで表示します",
            UiLanguage::English => "Shows the bundled HTML documentation in your default browser",
        }
    }
    pub fn help_err_write_temp(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ヘルプ一時ファイルの書き込みに失敗:",
            UiLanguage::English => "Failed to write help temp file:",
        }
    }
    pub fn help_err_open_browser(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ブラウザでヘルプを開けませんでした:",
            UiLanguage::English => "Could not open help in browser:",
        }
    }
    pub fn help_tips_heading(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "クイックヒント",
            UiLanguage::English => "Quick tips",
        }
    }
    pub fn help_tip_1(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "同じ名前のメタ変数（例: $A, $A）は、同一のコード片にマッチする必要があります",
            UiLanguage::English => "Same meta-variable name (e.g. $A, $A) must match the same code",
        }
    }
    pub fn help_tip_2(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "$$$ は「複数ノード」用。引数リストや文の並びに使います",
            UiLanguage::English => "$$$ matches zero or more nodes—use for arg lists or statement sequences",
        }
    }
    pub fn help_tip_3(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "マッチしないときは、ツールバーの言語が対象ファイルの構文解析に合っているか確認してください",
            UiLanguage::English => "If nothing matches, check the toolbar language matches how files are parsed",
        }
    }

    // ─── pattern assist popup ─────────────────────────────────────────

    pub fn pa_window_title(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "パターン支援",
            UiLanguage::English => "Pattern assist",
        }
    }
    pub fn pa_intro(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "コード片を入力すると、マッチするパターン候補を列挙します。\n\
                 「使用」ボタンで検索欄に反映できます。"
            }
            UiLanguage::English => {
                "Enter a snippet to list matching patterns.\n\
                 Use \"Apply\" to copy a pattern to the search field."
            }
        }
    }
    pub fn pa_snippet_label(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "コード片:",
            UiLanguage::English => "Snippet:",
        }
    }
    pub fn pa_snippet_hint(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "ここにコード片を貼り付けてください\n例: fn foo(x: i32) -> String { ... }"
            }
            UiLanguage::English => {
                "Paste a code snippet here\ne.g. fn foo(x: i32) -> String { ... }"
            }
        }
    }
    pub fn pa_generate(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "🔍 パターン生成",
            UiLanguage::English => "🔍 Generate",
        }
    }
    pub fn pa_generate_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "入力したコード片に実際にマッチするパターン候補を列挙します",
            UiLanguage::English => "List patterns that match the snippet",
        }
    }
    pub fn pa_clear(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "クリア",
            UiLanguage::English => "Clear",
        }
    }
    pub fn pa_lang_line(self, lang_name: &str) -> String {
        match self.0 {
            UiLanguage::Japanese => format!("言語: {}", lang_name),
            UiLanguage::English => format!("Language: {}", lang_name),
        }
    }
    pub fn pa_no_candidates(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "パターン候補が見つかりませんでした（スニペットを確認してください）",
            UiLanguage::English => "No pattern candidates (check your snippet)",
        }
    }
    pub fn pa_candidates_count(self, n: usize) -> String {
        match self.0 {
            UiLanguage::Japanese => format!(
                "{}件のパターン候補（スニペットで実際にマッチすることを確認済み）",
                n
            ),
            UiLanguage::English => format!("{} pattern candidate(s) (verified on snippet)", n),
        }
    }
    pub fn pa_col_pattern(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "パターン",
            UiLanguage::English => "Pattern",
        }
    }
    pub fn pa_col_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "説明",
            UiLanguage::English => "Description",
        }
    }
    pub fn pa_col_count(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "マッチ数",
            UiLanguage::English => "Matches",
        }
    }
    pub fn pa_col_action(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "操作",
            UiLanguage::English => "Action",
        }
    }
    pub fn pa_apply(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "使用",
            UiLanguage::English => "Apply",
        }
    }
    pub fn pa_apply_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "このパターンを検索欄に設定します",
            UiLanguage::English => "Set this pattern in the search field",
        }
    }
    pub fn pa_copy(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "コピー",
            UiLanguage::English => "Copy",
        }
    }
    pub fn pa_copy_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "クリップボードにコピーします",
            UiLanguage::English => "Copy to clipboard",
        }
    }
    pub fn pa_pat_hover(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "クリックで選択できます",
            UiLanguage::English => "Click to select",
        }
    }

    // ─── code / table / file panels ────────────────────────────────────

    pub fn code_select_file(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "← ファイルを選択してください",
            UiLanguage::English => "← Select a file",
        }
    }
    pub fn code_read_error_fmt(self, e: impl std::fmt::Display) -> String {
        match self.0 {
            UiLanguage::Japanese => format!("ファイル読み込みエラー: {e}"),
            UiLanguage::English => format!("Failed to read file: {e}"),
        }
    }
    pub fn code_match_count(self, n: usize) -> String {
        match self.0 {
            UiLanguage::Japanese => format!("{} マッチ", n),
            UiLanguage::English => format!("{} match(es)", n),
        }
    }
    pub fn code_match_list_header(self, n: usize) -> String {
        match self.0 {
            UiLanguage::Japanese => format!("マッチ一覧 ({} 件)", n),
            UiLanguage::English => format!("Matches ({})", n),
        }
    }
    pub fn to_assist(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "→支援",
            UiLanguage::English => "→Assist",
        }
    }
    pub fn to_assist_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "このマッチテキストをパターン支援に送ります\n\
                 パターン支援ウィンドウが開き、コード片として自動入力されます"
            }
            UiLanguage::English => {
                "Send this match text to Pattern assist\n\
                 Opens the window and fills the snippet field"
            }
        }
    }

    pub fn table_empty(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "検索結果がありません",
            UiLanguage::English => "No results",
        }
    }
    pub fn table_col_file(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ファイル",
            UiLanguage::English => "File",
        }
    }
    pub fn table_col_line(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "行",
            UiLanguage::English => "Line",
        }
    }
    pub fn table_col_col(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "列",
            UiLanguage::English => "Col",
        }
    }
    pub fn table_col_text(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "マッチしたテキスト",
            UiLanguage::English => "Matched text",
        }
    }
    /// 表: 該当行の全文＋前後コンテキスト（マッチ列とは別）
    pub fn table_col_source_context(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "元コード（前後）",
            UiLanguage::English => "Source (context)",
        }
    }
    pub fn table_col_action(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "操作",
            UiLanguage::English => "Action",
        }
    }

    /// 表: `$RECV` から推定した型（表示専用）
    pub fn table_col_recv_hint(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "推定型 ($RECV)",
            UiLanguage::English => "Inferred ($RECV)",
        }
    }

    /// 表: 推定型が無いときのツールチップ
    pub fn table_recv_hint_none_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "パターンに $RECV が無いか、構文から型を推定できませんでした"
            }
            UiLanguage::English => {
                "No $RECV in pattern, or type could not be inferred from syntax"
            }
        }
    }

    /// コードビュー: マッチ一覧の推定型ツールチップ用プレフィックス
    pub fn code_recv_hint_prefix(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "推定型: ",
            UiLanguage::English => "Inferred: ",
        }
    }

    /// 表: ダブルクリックで開くコードプレビューウィンドウのタイトル
    pub fn table_preview_window_title(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ファイルプレビュー",
            UiLanguage::English => "File preview",
        }
    }
    pub fn table_preview_subtitle(self, path: &str, line: usize, col: usize) -> String {
        match self.0 {
            UiLanguage::Japanese => format!("{path}  · 行{line} / 列{col}"),
            UiLanguage::English => format!("{path}  · line {line}, col {col}"),
        }
    }
    /// 表ビュー: 行をダブルクリックでプレビューできる旨
    pub fn table_double_click_hint(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "行をダブルクリックすると、コードビューと同じ表示でファイル全文を開き、該当行へスクロールします"
            }
            UiLanguage::English => {
                "Double-click a row to open the full file with the same highlighting as the code view, scrolled to the match"
            }
        }
    }

    pub fn file_list_heading(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ファイル一覧",
            UiLanguage::English => "Files",
        }
    }
    pub fn file_list_empty(self) -> &'static str {
        self.table_empty()
    }

    // ─── search errors (thread) ────────────────────────────────────────

    pub fn err_regex_compile(self, e: impl std::fmt::Display) -> String {
        match self.0 {
            UiLanguage::Japanese => format!("正規表現エラー: {e}"),
            UiLanguage::English => format!("Regex error: {e}"),
        }
    }

    // ─── export strings ────────────────────────────────────────────────

    pub fn export_text_title(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ast-grep 検索結果",
            UiLanguage::English => "ast-grep search results",
        }
    }
    /// ヒット上限に達した場合の言語別サフィックスを返す
    fn hit_limit_suffix(self, hit_limit_reached: bool) -> &'static str {
        if !hit_limit_reached {
            return "";
        }
        match self.0 {
            UiLanguage::Japanese => "（ヒット上限により打ち切り）",
            UiLanguage::English => " (truncated at hit limit)",
        }
    }

    pub fn export_text_total(self, m: usize, f: usize, ms: u64, hit_limit_reached: bool) -> String {
        match self.0 {
            UiLanguage::Japanese => format!(
                "合計: {}件 / {}ファイル / {}ms{}\n\n",
                m, f, ms, self.hit_limit_suffix(hit_limit_reached)
            ),
            UiLanguage::English => format!(
                "Total: {} matches / {} files / {}ms{}\n\n",
                m, f, ms, self.hit_limit_suffix(hit_limit_reached)
            ),
        }
    }
    /// プレーンテキスト出力で、マッチ位置の見出し行（続けて text_with_context の行をインデント出力）
    pub fn export_line_match_header(self, line: usize, col: usize) -> String {
        match self.0 {
            UiLanguage::Japanese => format!("  行 {}:{} \n", line, col),
            UiLanguage::English => format!("  line {}:{} \n", line, col),
        }
    }

    pub fn export_console_header(self, m: usize, f: usize, ms: u64, hit_limit_reached: bool) -> String {
        match self.0 {
            UiLanguage::Japanese => format!(
                "ast-grep console\n{} 件 / {} ファイル ({}ms){}\n\n",
                m, f, ms, self.hit_limit_suffix(hit_limit_reached)
            ),
            UiLanguage::English => {
                // 英語版はサフィックスが丸括弧の内側に入る形式のため個別処理
                let note = if hit_limit_reached { ", truncated at hit limit" } else { "" };
                format!("ast-grep console\n{} matches in {} files ({}ms{})\n\n", m, f, ms, note)
            }
        }
    }

    pub fn export_md_heading(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ast-grep 検索結果",
            UiLanguage::English => "ast-grep search results",
        }
    }
    pub fn export_md_stats(self, m: usize, f: usize, ms: u64, hit_limit_reached: bool) -> String {
        match self.0 {
            UiLanguage::Japanese => format!(
                "\n\n合計: **{}件** / **{}ファイル** / {}ms{}\n\n",
                m, f, ms, self.hit_limit_suffix(hit_limit_reached)
            ),
            UiLanguage::English => format!(
                "\n\nTotal: **{}** matches / **{}** files / {}ms{}\n\n",
                m, f, ms, self.hit_limit_suffix(hit_limit_reached)
            ),
        }
    }
    pub fn export_md_table_header(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "| ファイル | 行 | 列 | マッチテキスト | 元コード（前後） |\n"
            }
            UiLanguage::English => "| File | Line | Col | Matched text | Source (context) |\n",
        }
    }

    /// Markdown テーブル: `$RECV` 推定型列付き（パターンに `$RECV` があるとき）
    pub fn export_md_table_header_with_recv(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "| ファイル | 行 | 列 | マッチテキスト | 元コード（前後） | 推定型 ($RECV) |\n"
            }
            UiLanguage::English => {
                "| File | Line | Col | Matched text | Source (context) | Inferred ($RECV) |\n"
            }
        }
    }

    pub fn export_html_lang(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ja",
            UiLanguage::English => "en",
        }
    }
    pub fn export_html_title(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ast-grep 検索結果",
            UiLanguage::English => "ast-grep search results",
        }
    }
    pub fn export_html_h1(self) -> &'static str {
        self.export_html_title()
    }
    pub fn export_html_stats(self, m: usize, f: usize, ms: u64, hit_limit_reached: bool) -> String {
        match self.0 {
            UiLanguage::Japanese => format!(
                "<p class=\"stats\">合計: <b>{}件</b> / <b>{}ファイル</b> / {}ms{}</p>\n",
                m, f, ms, self.hit_limit_suffix(hit_limit_reached)
            ),
            UiLanguage::English => format!(
                "<p class=\"stats\">Total: <b>{}</b> matches / <b>{}</b> files / {}ms{}</p>\n",
                m, f, ms, self.hit_limit_suffix(hit_limit_reached)
            ),
        }
    }
    pub fn export_html_th_file(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ファイル",
            UiLanguage::English => "File",
        }
    }
    pub fn export_html_th_line(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "行",
            UiLanguage::English => "Line",
        }
    }
    pub fn export_html_th_col(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "列",
            UiLanguage::English => "Col",
        }
    }
    pub fn export_html_th_match(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "マッチテキスト",
            UiLanguage::English => "Match text",
        }
    }
    pub fn export_html_th_source_context(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "元コード（前後）",
            UiLanguage::English => "Source (context)",
        }
    }

    /// HTML テーブル: `$RECV` 推定型列（パターンに `$RECV` があるとき）
    pub fn export_html_th_recv_hint(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "推定型 ($RECV)",
            UiLanguage::English => "Inferred ($RECV)",
        }
    }

    pub fn export_xlsx_sheet_results(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "検索結果",
            UiLanguage::English => "Results",
        }
    }
    pub fn export_xlsx_sheet_stats(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "統計",
            UiLanguage::English => "Stats",
        }
    }
    pub fn export_xlsx_col_file(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ファイル",
            UiLanguage::English => "File",
        }
    }
    pub fn export_xlsx_col_line(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "行",
            UiLanguage::English => "Line",
        }
    }
    pub fn export_xlsx_col_col(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "列",
            UiLanguage::English => "Col",
        }
    }
    pub fn export_xlsx_col_match(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "マッチテキスト",
            UiLanguage::English => "Match text",
        }
    }
    pub fn export_xlsx_col_source_context(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "元コード（前後）",
            UiLanguage::English => "Source (context)",
        }
    }

    /// Excel: `$RECV` 推定型列（パターンに `$RECV` があるとき）
    pub fn export_xlsx_col_recv_hint(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "推定型 ($RECV)",
            UiLanguage::English => "Inferred ($RECV)",
        }
    }

    pub fn export_xlsx_total_matches(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "総マッチ数",
            UiLanguage::English => "Total matches",
        }
    }
    pub fn export_xlsx_file_count(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "対象ファイル数",
            UiLanguage::English => "Files",
        }
    }
    pub fn export_xlsx_elapsed(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "経過時間 (ms)",
            UiLanguage::English => "Elapsed (ms)",
        }
    }
    pub fn export_xlsx_hit_limit_note(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "備考",
            UiLanguage::English => "Note",
        }
    }
    pub fn export_xlsx_hit_limit_truncated(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ヒット上限により打ち切り",
            UiLanguage::English => "Truncated at hit limit",
        }
    }

    pub fn export_conditions_title(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "検索条件",
            UiLanguage::English => "Search conditions",
        }
    }
    pub fn export_cond_root(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "検索ルート",
            UiLanguage::English => "Root directory",
        }
    }
    pub fn export_cond_pattern(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "パターン",
            UiLanguage::English => "Pattern",
        }
    }
    pub fn export_cond_lang(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "言語",
            UiLanguage::English => "Language",
        }
    }
    pub fn export_cond_context_lines(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "前後コンテキスト行数",
            UiLanguage::English => "Context lines",
        }
    }
    pub fn export_cond_file_filter(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ファイルフィルタ",
            UiLanguage::English => "File filter",
        }
    }
    pub fn export_cond_file_filter_default(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "（空: 言語デフォルトの拡張子）",
            UiLanguage::English => "(empty: language default extensions)",
        }
    }
    pub fn export_cond_file_encoding(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ファイル文字コード",
            UiLanguage::English => "File encoding",
        }
    }
    pub fn export_cond_max_file_mb(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "最大ファイルサイズ (MB)",
            UiLanguage::English => "Max file size (MB)",
        }
    }
    pub fn export_cond_max_search_hits(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ヒット上限（0=無制限）",
            UiLanguage::English => "Max hits (0=unlimited)",
        }
    }
    pub fn export_cond_skip_dirs(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "スキップディレクトリ",
            UiLanguage::English => "Skip directories",
        }
    }
    pub fn export_cond_search_mode(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "検索モード",
            UiLanguage::English => "Search mode",
        }
    }

    pub fn export_html_conditions_heading(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "検索条件",
            UiLanguage::English => "Search conditions",
        }
    }

    // ─── lang.rs presets ─────────────────────────────────────────────

    pub fn preset_rust_fn(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "関数定義",
            UiLanguage::English => "Function definition",
        }
    }
    pub fn preset_rust_fn_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "すべての関数定義",
            UiLanguage::English => "All function definitions",
        }
    }
    pub fn preset_rust_trait_impl(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "トレイト実装",
            UiLanguage::English => "Trait implementation",
        }
    }
    pub fn preset_rust_trait_impl_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "トレイト実装",
            UiLanguage::English => "Trait impl blocks",
        }
    }
    pub fn preset_rust_unwrap(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "unwrap呼び出し",
            UiLanguage::English => "unwrap() calls",
        }
    }
    pub fn preset_rust_unwrap_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "unwrap()の呼び出し箇所",
            UiLanguage::English => "Calls to unwrap()",
        }
    }
    pub fn preset_rust_clone(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "clone呼び出し",
            UiLanguage::English => "clone() calls",
        }
    }
    pub fn preset_rust_clone_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "clone()の呼び出し箇所",
            UiLanguage::English => "Calls to clone()",
        }
    }
    pub fn preset_rust_panic(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "panicマクロ",
            UiLanguage::English => "panic! macro",
        }
    }
    pub fn preset_rust_panic_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "panic!マクロの使用箇所",
            UiLanguage::English => "Uses of panic!",
        }
    }
    pub fn preset_java_null(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "null チェック",
            UiLanguage::English => "null check",
        }
    }
    pub fn preset_java_null_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "nullとの比較",
            UiLanguage::English => "Comparison to null",
        }
    }
    pub fn preset_java_println(self) -> &'static str {
        "System.out.println"
    }
    pub fn preset_java_println_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "デバッグ出力",
            UiLanguage::English => "Debug output",
        }
    }
    pub fn preset_py_print(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "print文",
            UiLanguage::English => "print()",
        }
    }
    pub fn preset_py_print_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "print()の使用箇所",
            UiLanguage::English => "Calls to print()",
        }
    }
    pub fn preset_py_import(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "import文",
            UiLanguage::English => "import",
        }
    }
    pub fn preset_py_import_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "importの使用箇所",
            UiLanguage::English => "import statements",
        }
    }
    pub fn preset_js_console(self) -> &'static str {
        "console.log"
    }
    pub fn preset_js_console_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "console.log()の使用箇所",
            UiLanguage::English => "console.log() calls",
        }
    }
    pub fn preset_go_err(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "エラーチェック",
            UiLanguage::English => "Error check",
        }
    }
    pub fn preset_go_err_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "エラーチェック",
            UiLanguage::English => "if err != nil { ... }",
        }
    }
    pub fn preset_kotlin_println(self) -> &'static str {
        "println"
    }
    pub fn preset_kotlin_println_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "println()の使用箇所",
            UiLanguage::English => "println() calls",
        }
    }
    pub fn preset_kotlin_fun(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "fun 定義",
            UiLanguage::English => "fun definition",
        }
    }
    pub fn preset_kotlin_fun_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "トップレベルまたはメンバの関数定義",
            UiLanguage::English => "Top-level or member function",
        }
    }
    pub fn preset_scala_println(self) -> &'static str {
        "println"
    }
    pub fn preset_scala_println_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "println()の使用箇所",
            UiLanguage::English => "println() calls",
        }
    }
    pub fn preset_scala_def(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "def 定義",
            UiLanguage::English => "def definition",
        }
    }
    pub fn preset_scala_def_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "def で = 右辺を持つ定義",
            UiLanguage::English => "def with = body",
        }
    }
    pub fn preset_generic_any(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "任意のパターン",
            UiLanguage::English => "Any pattern",
        }
    }
    pub fn preset_generic_any_desc(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "任意の式にマッチ",
            UiLanguage::English => "Match any expression",
        }
    }
}

impl From<UiLanguage> for Tr {
    fn from(lang: UiLanguage) -> Self {
        Tr(lang)
    }
}
