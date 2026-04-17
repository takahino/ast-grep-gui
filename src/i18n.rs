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
                 トークン: スペース区切りのトークンを順序通りに検索（空白の有無は問わない）\n\
                 文字列: 通常のテキスト検索（単純な部分一致）\n\
                 正規表現: 正規表現パターンで検索"
            }
            UiLanguage::English => {
                "Search method\n\
                 AST: ast-grep AST patterns (match spans match the CLI)\n\
                 Token: search by space-separated tokens in order (whitespace-flexible)\n\
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
    pub fn mode_token(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "トークン",
            UiLanguage::English => "Token",
        }
    }
    pub fn mode_token_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "スペースで区切ったトークンを順序通りに検索します\n\
                 空白の有無は問いません\n\
                 例: `method ( int` は `method(int` にもマッチします\n\
                 ファイルフィルタ未指定時は全ファイルが対象です"
            }
            UiLanguage::English => {
                "Search by space-separated tokens in order (whitespace-flexible)\n\
                 e.g. `method ( int` matches `method(int`\n\
                 Without a file filter, all files are searched"
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
                 ツールバーで「大文字小文字を区別しない」「単語単位」を選べます（正規表現の知識は不要）\n\
                 ファイルフィルタ未指定時は全ファイルを対象にします"
            }
            UiLanguage::English => {
                "Search for the substring in each line\n\
                 Use the checkboxes for ignore case and whole word (no regex knowledge needed)\n\
                 Without a file filter, all files are searched"
            }
        }
    }

    pub fn plain_text_ignore_case(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "大文字小文字を区別しない",
            UiLanguage::English => "Ignore case",
        }
    }
    pub fn plain_text_ignore_case_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "オンにすると、検索語と行内の表記が大文字・小文字違いでも一致します\n\
                 （例: foo と Foo）"
            }
            UiLanguage::English => {
                "When enabled, matches ignore ASCII/Unicode case\n\
                 (e.g. foo matches Foo)"
            }
        }
    }
    pub fn plain_text_whole_word(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "単語単位で一致",
            UiLanguage::English => "Whole word",
        }
    }
    pub fn plain_text_whole_word_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "検索語の前後が、空白（スペース・タブ・改行など）か行頭／行末で区切られているときだけヒットします。\n\
                 ─────────────────────────\n\
                 例: cat を探すとき\n\
                 • ヒットする … 「 cat 」のように前後が空白、または行の先頭／末尾の cat\n\
                 • ヒットしない … catch の cat、int* の int（* の直前は空白ではない）\n\
                 ─────────────────────────\n\
                 foo(int) の int のように、記号のすぐ内側にある語はヒットしません。コード全体を探すときは単語単位をオフにしてください。"
            }
            UiLanguage::English => {
                "Matches only when the search text is delimited by whitespace (spaces, tabs, line breaks) or line start/end.\n\
                 ─────────────────────────\n\
                 Example: searching for cat\n\
                 • Matches … when surrounded by whitespace (e.g. “ cat ”)\n\
                 • Does not match … cat inside catch, or int before * in int* (no space before *)\n\
                 ─────────────────────────\n\
                 Tokens glued to punctuation such as foo(int) won’t match int. Turn off whole word to search more broadly."
            }
        }
    }
    pub fn incremental_search_label(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "入力で即時検索",
            UiLanguage::English => "Auto-search",
        }
    }
    pub fn incremental_search_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "パターン入力後 0.5 秒で自動的に検索を開始します",
            UiLanguage::English => "Automatically start search 0.5 s after typing stops",
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
    pub fn pattern_label_tooltip_token(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "スペースで区切ったトークンを入力します\n\
                 ─────────────────────────\n\
                 各トークンはリテラル文字列として扱われます\n\
                 トークン間の空白は「0個以上の空白」としてマッチします\n\
                 ─────────────────────────\n\
                 例: method ( int i = 0 ) は method(int i=0) にもマッチ"
            }
            UiLanguage::English => {
                "Enter tokens separated by spaces\n\
                 Each token is matched literally\n\
                 Whitespace between tokens matches zero or more spaces\n\
                 e.g. method ( int matches method(int"
            }
        }
    }
    pub fn pattern_label_tooltip_plain(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "検索したい文字列をそのまま入力します\n\
                 大文字/小文字・単語単位はツールバーのチェックで指定できます\n\
                 例）TODO  println!  System.out.println"
            }
            UiLanguage::English => {
                "Plain text to find (use toolbar for ignore case / whole word)\n\
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
    pub fn pattern_hint_token(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "例: method ( int i = 0 )",
            UiLanguage::English => "e.g. method ( int i = 0 )",
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
    pub fn search_tooltip_token(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "トークン検索を開始します（スペース区切りで空白柔軟マッチ）",
            UiLanguage::English => "Start token search (space-separated, whitespace-flexible)",
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

    pub fn view_batch_report(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "📑 バッチレポート",
            UiLanguage::English => "📑 Batch report",
        }
    }
    pub fn view_batch_report_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "複数パターンのバッチ検索結果をまとめて表示します",
            UiLanguage::English => "Aggregated results from multi-pattern batch search",
        }
    }

    pub fn view_summary(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "📈 サマリー",
            UiLanguage::English => "📈 Summary",
        }
    }
    pub fn view_summary_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "受信側の型と、引数個数・各引数の型の組み合わせを集計します（パターンにメソッド用の単一メタがある場合はその列も表示）"
            }
            UiLanguage::English => {
                "Count hits by receiver type, arity, and per-argument types (adds a method column if the pattern has a second single metavar)"
            }
        }
    }

    pub fn summary_title(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "型バリエーションのサマリー",
            UiLanguage::English => "Type variation summary",
        }
    }

    /// `args_multi` が `Some` のときは `$$$` 引数列。`None` のときは `arg_singles`（`$RECV.Format($A)` や `$RECV.Format()`）
    pub fn summary_keys_explanation(
        self,
        recv: &str,
        method: Option<&str>,
        args_multi: Option<&str>,
        arg_singles: &[String],
    ) -> String {
        if let Some(multi) = args_multi {
            return match (self.0, method) {
                (UiLanguage::Japanese, None) => format!(
                    "単一メタ ${}（受信側）と $$$ {} の #arity / #i（各引数の型）を集計しています。",
                    recv, multi
                ),
                (UiLanguage::English, None) => format!(
                    "Single ${} is the receiver; $$$ {} uses #arity / #i for each argument type.",
                    recv, multi
                ),
                (UiLanguage::Japanese, Some(m)) => format!(
                    "単一メタ ${} / ${} を受信・メソッド、$$${} の #arity / #i を引数として集計しています。",
                    recv, m, multi
                ),
                (UiLanguage::English, Some(m)) => format!(
                    "Singles ${} / ${} are receiver and method; $$$ {} uses #arity / #i for arguments.",
                    recv, m, multi
                ),
            };
        }

        match self.0 {
            UiLanguage::Japanese => {
                if arg_singles.is_empty() {
                    format!("単一メタ ${}（受信側）のみ。引数メタはありません。", recv)
                } else if method.is_none() && arg_singles.len() == 1 {
                    format!(
                        "単一メタ ${}（受信）と ${}（1引数）の型を集計しています。",
                        recv, arg_singles[0]
                    )
                } else if method.is_none() && arg_singles.len() > 1 {
                    let joined = arg_singles
                        .iter()
                        .map(|s| format!("${}", s))
                        .collect::<Vec<_>>()
                        .join("、");
                    format!(
                        "単一メタ ${}（受信）と {} の型を集計しています。",
                        recv, joined
                    )
                } else if let Some(met) = method {
                    let joined = arg_singles
                        .iter()
                        .map(|s| format!("${}", s))
                        .collect::<Vec<_>>()
                        .join("、");
                    format!(
                        "単一メタ ${} / ${} を受信・メソッド、{} の型を集計しています。",
                        recv, met, joined
                    )
                } else {
                    format!("単一メタ ${}（受信側）の型を集計しています。", recv)
                }
            }
            UiLanguage::English => {
                if arg_singles.is_empty() {
                    format!(
                        "Only single metavar ${} (receiver); no argument metavariables.",
                        recv
                    )
                } else if method.is_none() && arg_singles.len() == 1 {
                    format!(
                        "Single metavars ${} (receiver) and ${} (one argument).",
                        recv, arg_singles[0]
                    )
                } else if method.is_none() && arg_singles.len() > 1 {
                    let joined = arg_singles
                        .iter()
                        .map(|s| format!("${}", s))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "Single metavar ${} (receiver); argument types: {}.",
                        recv, joined
                    )
                } else if let Some(met) = method {
                    let joined = arg_singles
                        .iter()
                        .map(|s| format!("${}", s))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "Singles ${} / ${} (receiver / method), argument types: {}.",
                        recv, met, joined
                    )
                } else {
                    format!("Aggregating types for single metavar ${} (receiver).", recv)
                }
            }
        }
    }

    pub fn summary_empty_results(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "検索結果がありません。先に検索を実行してください。",
            UiLanguage::English => "No search results. Run a search first.",
        }
    }

    pub fn summary_no_match_rows(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "マッチ行がありません。",
            UiLanguage::English => "No match rows.",
        }
    }

    pub fn summary_pattern_ineligible(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "このサマリーには、パターンに単一メタ変数が1つ以上（受信側）必要です（例: $RECV、$RECV.Format($A)、$RECV.Format()）。"
            }
            UiLanguage::English => {
                "This summary needs at least one single metavar for the receiver (e.g. $RECV, $RECV.Format($A), $RECV.Format())."
            }
        }
    }

    pub fn summary_col_count(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "件数",
            UiLanguage::English => "Count",
        }
    }

    pub fn summary_col_receiver(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "受信側",
            UiLanguage::English => "Receiver",
        }
    }

    pub fn summary_col_method(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "メソッド側",
            UiLanguage::English => "Method",
        }
    }

    pub fn summary_col_arity(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "引数数",
            UiLanguage::English => "Arity",
        }
    }

    pub fn summary_col_arg(self, index: usize) -> String {
        match self.0 {
            UiLanguage::Japanese => format!("引数{index}"),
            UiLanguage::English => format!("Arg {index}"),
        }
    }

    pub fn batch_jobs_header(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "バッチ検索ジョブ",
            UiLanguage::English => "Batch search jobs",
        }
    }
    pub fn batch_add_job(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "＋ 現在設定を追加",
            UiLanguage::English => "+ Add current",
        }
    }
    pub fn batch_add_job_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ツールバーのパターンと検索条件を 1 件として一覧に追加します",
            UiLanguage::English => "Add current pattern and search settings as one job",
        }
    }
    pub fn batch_run_all(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "▶ バッチ実行",
            UiLanguage::English => "▶ Run batch",
        }
    }
    pub fn batch_run_all_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "有効なジョブを上から順に実行し、完了後にレポートを表示します",
            UiLanguage::English => "Run enabled jobs in order, then show the report",
        }
    }
    pub fn batch_save_config(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "設定を保存",
            UiLanguage::English => "Save jobs",
        }
    }
    pub fn batch_save_config_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "バッチジョブ一覧を YAML ファイルに保存します（テキストエディタで編集しやすい形式です）"
            }
            UiLanguage::English => {
                "Save the batch job list as a YAML file (easy to edit in a text editor)"
            }
        }
    }
    pub fn batch_load_config(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "設定を読込",
            UiLanguage::English => "Load jobs",
        }
    }
    pub fn batch_load_config_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "YAML ファイルからバッチジョブ一覧を読み込み、現在の一覧を置き換えます"
            }
            UiLanguage::English => {
                "Load batch jobs from a YAML file, replacing the current list"
            }
        }
    }
    pub fn err_batch_save(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "バッチ設定の保存に失敗:",
            UiLanguage::English => "Failed to save batch config:",
        }
    }
    pub fn err_batch_load(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "バッチ設定の読み込みに失敗:",
            UiLanguage::English => "Failed to load batch config:",
        }
    }
    pub fn batch_jobs_empty_hint(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ジョブがありません。「現在設定を追加」で登録してください。",
            UiLanguage::English => "No jobs. Use “Add current” to register search jobs.",
        }
    }
    pub fn batch_col_label(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ラベル",
            UiLanguage::English => "Label",
        }
    }
    pub fn batch_col_pattern(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "パターン",
            UiLanguage::English => "Pattern",
        }
    }
    pub fn batch_col_enabled(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "有効",
            UiLanguage::English => "On",
        }
    }
    pub fn batch_col_actions(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "操作",
            UiLanguage::English => "Actions",
        }
    }
    pub fn batch_edit(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "編集",
            UiLanguage::English => "Edit",
        }
    }
    pub fn batch_remove(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "削除",
            UiLanguage::English => "Del",
        }
    }
    pub fn batch_move_up_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "上へ",
            UiLanguage::English => "Move up",
        }
    }
    pub fn batch_move_down_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "下へ",
            UiLanguage::English => "Move down",
        }
    }
    pub fn batch_edit_window_title(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ジョブの編集",
            UiLanguage::English => "Edit job",
        }
    }
    pub fn batch_job_default_label_prefix(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ジョブ",
            UiLanguage::English => "Job",
        }
    }
    pub fn batch_no_runnable_jobs(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "実行できるジョブがありません（有効かつパターンとディレクトリが必要です）"
            }
            UiLanguage::English => {
                "No runnable jobs (need enabled jobs with pattern and directory)"
            }
        }
    }

    pub fn batch_report_title(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "バッチ検索レポート",
            UiLanguage::English => "Batch search report",
        }
    }
    pub fn batch_report_empty(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "レポートがありません。バッチ実行を完了するとここに結果が表示されます。",
            UiLanguage::English => "No report yet. Run a batch search to see results here.",
        }
    }
    pub fn batch_report_summary(
        self,
        total_ms: u64,
        total_matches: usize,
        total_files: usize,
        job_count: usize,
        failed: usize,
    ) -> String {
        match self.0 {
            UiLanguage::Japanese => format!(
                "合計時間: {total_ms} ms ／ 合計マッチ: {total_matches} ／ 合計ファイル: {total_files} ／ ジョブ数: {job_count} ／ 失敗: {failed}"
            ),
            UiLanguage::English => format!(
                "Total time: {total_ms} ms / Matches: {total_matches} / Files: {total_files} / Jobs: {job_count} / Failed: {failed}"
            ),
        }
    }
    pub fn batch_report_error(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "エラー",
            UiLanguage::English => "Error",
        }
    }
    pub fn batch_report_job_stats(
        self,
        matches: usize,
        files: usize,
        elapsed_ms: u64,
        hit_limit: bool,
    ) -> String {
        match self.0 {
            UiLanguage::Japanese => format!(
                "マッチ: {matches} ／ ファイル: {files} ／ 時間: {elapsed_ms} ms ／ 上限打切: {hit_limit}"
            ),
            UiLanguage::English => format!(
                "Matches: {matches} / Files: {files} / Time: {elapsed_ms} ms / Hit cap: {hit_limit}"
            ),
        }
    }
    pub fn batch_report_conditions(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "検索条件",
            UiLanguage::English => "Search conditions",
        }
    }
    pub fn batch_report_matches(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "マッチ一覧",
            UiLanguage::English => "Matches",
        }
    }

    pub fn status_batch_running(self, cur: usize, total: usize, scanned: usize) -> String {
        match self.0 {
            UiLanguage::Japanese => format!("バッチ {cur}/{total} ・ 走査 {scanned} ファイル"),
            UiLanguage::English => format!("Batch {cur}/{total} · scanned {scanned} files"),
        }
    }

    pub fn export_batch_json_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "バッチレポートを JSON で保存",
            UiLanguage::English => "Save batch report as JSON",
        }
    }
    pub fn export_batch_md_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "バッチレポートを Markdown で保存",
            UiLanguage::English => "Save batch report as Markdown",
        }
    }
    pub fn export_batch_html_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "バッチレポートを HTML で保存",
            UiLanguage::English => "Save batch report as HTML",
        }
    }
    pub fn export_batch_xlsx_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "バッチレポートを Excel で保存（ジョブごとシート）",
            UiLanguage::English => "Save batch report as Excel (one sheet per job)",
        }
    }
    pub fn export_batch_txt_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "バッチレポートをテキストで保存",
            UiLanguage::English => "Save batch report as plain text",
        }
    }
    pub fn err_export_batch(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "バッチエクスポート失敗:",
            UiLanguage::English => "Batch export failed:",
        }
    }
    pub fn copy_batch_report_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "バッチレポートをテキストとしてコピー",
            UiLanguage::English => "Copy batch report as text",
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

    // ─── rewrite (AST) ─────────────────────────────────────────────────

    pub fn rewrite_template_label(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "置換:",
            UiLanguage::English => "Rewrite:",
        }
    }
    pub fn rewrite_template_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "ast-grep の --rewrite に相当する置換テンプレートです。\n\
                 検索パターンのメタ変数（$A など）を参照できます。"
            }
            UiLanguage::English => {
                "Replacement template (like ast-grep --rewrite).\n\
                 Meta-variables from the search pattern can be used."
            },
        }
    }
    pub fn rewrite_template_hint(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "例: new_name($A)",
            UiLanguage::English => "e.g. new_name($A)",
        }
    }
    pub fn rewrite_preview_btn(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "置換プレビュー",
            UiLanguage::English => "Preview rewrite",
        }
    }
    pub fn rewrite_preview_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "ヒットした各ファイルに対し、置換後のソースを生成して差分を確認します（ディスクは変更しません）"
            }
            UiLanguage::English => {
                "Generate rewritten source per hit file and show a diff (does not write to disk)"
            },
        }
    }
    pub fn rewrite_window_title(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "置換プレビュー",
            UiLanguage::English => "Rewrite preview",
        }
    }
    pub fn rewrite_preview_summary(self, files: usize, elapsed_ms: u64) -> String {
        match self.0 {
            UiLanguage::Japanese => {
                format!("変更のあるファイル: {files} / 所要時間: {elapsed_ms} ms")
            }
            UiLanguage::English => {
                format!("Files with changes: {files} / Time: {elapsed_ms} ms")
            }
        }
    }
    pub fn rewrite_no_changes(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "置換の結果、検索ヒットしたファイルに変更はありませんでした（テンプレートが元と同じか、マッチと整合しません）。"
            }
            UiLanguage::English => {
                "No changes after rewrite (template may be identical or incompatible with matches)."
            },
        }
    }
    pub fn rewrite_close(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "閉じる",
            UiLanguage::English => "Close",
        }
    }
    pub fn rewrite_file_list_label(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ファイル:",
            UiLanguage::English => "Files:",
        }
    }
    pub fn rewrite_replacements_in_file(self, count: usize) -> String {
        match self.0 {
            UiLanguage::Japanese => format!("このファイルでの置換回数: {count}"),
            UiLanguage::English => format!("Replacements in this file: {count}"),
        }
    }
    pub fn rewrite_apply(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ファイルに書き戻す",
            UiLanguage::English => "Apply to files",
        }
    }
    pub fn rewrite_apply_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "プレビュー内容を元の文字コードでディスクに保存し、その後検索を再実行します"
            }
            UiLanguage::English => {
                "Save preview using each file's encoding, then run search again"
            },
        }
    }
    pub fn rewrite_status_previewing(self, done: usize, total: usize) -> String {
        match self.0 {
            UiLanguage::Japanese => format!("置換プレビュー生成中… {done}/{total}"),
            UiLanguage::English => format!("Building rewrite preview… {done}/{total}"),
        }
    }
    pub fn rewrite_status_applying(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "ファイルへ書き込み中…",
            UiLanguage::English => "Writing files…",
        }
    }
    pub fn rewrite_applied_ok(self, n: usize) -> String {
        match self.0 {
            UiLanguage::Japanese => format!("{n} 件のファイルを書き換えました。検索を更新しました。"),
            UiLanguage::English => format!("Updated {n} file(s). Search refreshed."),
        }
    }
    pub fn rewrite_compare_hint(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => {
                "unified diff 形式です。コンテキストは通常色、削除行（-）は赤系、追加行（+）は緑系で表示します。"
            }
            UiLanguage::English => {
                "Unified diff: context lines are neutral, removals (-) are reddish, additions (+) are greenish."
            },
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
            UiLanguage::Japanese => "→パターン支援",
            UiLanguage::English => "→Pattern assist",
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

    /// 表: あるメタ変数列で型が推定できなかったときのツールチップ
    pub fn table_type_hint_column_empty_tooltip(self, metavar: &str) -> String {
        match self.0 {
            UiLanguage::Japanese => format!("`${metavar}` の型を推定できませんでした"),
            UiLanguage::English => format!("Could not infer type for `${metavar}`"),
        }
    }

    /// 表: `NAME#arity` 列（`$$$NAME` のキャプチャ個数）で値が無いときのツールチップ
    pub fn table_type_hint_arity_empty_tooltip(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "複数ノードキャプチャの引数個数を取得できませんでした",
            UiLanguage::English => "Could not get captured node count for this multi metavar",
        }
    }

    /// 表: このマッチでは該当スロットがないとき（列は他行の最大キャプチャ数に合わせた空き）のツールチップ
    pub fn table_type_hint_no_slot_tooltip(self, column_key: &str) -> String {
        match self.0 {
            UiLanguage::Japanese => format!(
                "このマッチでは列 `{column_key}` に相当するキャプチャがありません（列幅は他の検索結果の最大に合わせています）"
            ),
            UiLanguage::English => format!(
                "This match has no capture for `{column_key}` (the column exists to align with wider matches elsewhere)"
            ),
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

    /// Markdown テーブル: ベース列のあとにメタ変数列（`$NAME`）を並べる
    pub fn export_md_table_header_with_metavars(self, metavars: &[String]) -> String {
        let mut line = match self.0 {
            UiLanguage::Japanese => {
                String::from("| ファイル | 行 | 列 | マッチテキスト | 元コード（前後） |")
            }
            UiLanguage::English => {
                String::from("| File | Line | Col | Matched text | Source (context) |")
            }
        };
        for mv in metavars {
            line.push_str(&format!(" ${} |", mv));
        }
        line.push('\n');
        line
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

    pub fn export_xlsx_sheet_results(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "検索結果",
            UiLanguage::English => "Results",
        }
    }
    pub fn export_xlsx_sheet_summary(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "サマリー",
            UiLanguage::English => "Summary",
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
    pub fn export_cond_plain_text_options(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "文字列検索のオプション",
            UiLanguage::English => "Text search options",
        }
    }
    pub fn export_plain_text_options_not_applicable(self) -> &'static str {
        match self.0 {
            UiLanguage::Japanese => "（文字列モード以外では該当なし）",
            UiLanguage::English => "(n/a except in Text mode)",
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
