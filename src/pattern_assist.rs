//! コード片から ast-grep パターンを提案するモジュール
//!
//! 各言語ごとに「プローブパターン」のリストを持ち、
//! 入力スニペットに実際にマッチするものだけを候補として返す。

use ast_grep_core::Pattern;
use ast_grep_language::LanguageExt;

use crate::i18n::{tr_pair, UiLanguage};
use crate::lang::SupportedLanguage;

fn probe_l(
    lang: UiLanguage,
    pattern: &str,
    ja: &'static str,
    en: &'static str,
) -> (String, String) {
    (pattern.to_string(), tr_pair(lang, ja, en))
}

/// 提案パターン1件
#[derive(Debug, Clone)]
pub struct PatternSuggestion {
    /// ast-grep に渡せるパターン文字列
    pub pattern: String,
    /// パターンの意味・用途の説明
    pub description: String,
    /// スニペット内でのマッチ数
    pub match_count: usize,
}

/// スニペットに実際にマッチするパターン候補を返す
pub fn generate_patterns(
    snippet: &str,
    lang: SupportedLanguage,
    ui_lang: UiLanguage,
) -> Vec<PatternSuggestion> {
    let snippet = snippet.trim();
    if snippet.is_empty() {
        return vec![];
    }

    // Auto のときはスニペット解析に Rust を使い、プローブは generic のみ（build_probes 側）
    let lang_for_parse = match lang {
        SupportedLanguage::Auto => SupportedLanguage::Rust,
        x => x,
    };
    let ast_lang = lang_for_parse.to_support_lang().expect("resolved language");
    let root = ast_lang.ast_grep(snippet);

    // 候補プローブ（パターン文字列, 説明）のリスト
    let probes = build_probes(snippet, lang, ui_lang);

    let mut results: Vec<PatternSuggestion> = probes
        .into_iter()
        .filter_map(|(pat, desc)| {
            // Pattern::try_new で事前検証し、MultipleNode 等の無効パターンを静かにスキップ
            let compiled = Pattern::try_new(pat.as_str(), ast_lang).ok()?;
            let count = root.root().find_all(compiled).count();
            if count > 0 {
                Some(PatternSuggestion {
                    pattern: pat,
                    description: desc,
                    match_count: count,
                })
            } else {
                None
            }
        })
        .collect();

    // 重複除去（パターン文字列が同じものは先勝ち）
    let mut seen = std::collections::HashSet::new();
    results.retain(|s| seen.insert(s.pattern.clone()));

    results
}

// ─── プローブリスト構築 ───────────────────────────────────────────────────

fn build_probes(
    snippet: &str,
    lang: SupportedLanguage,
    ui_lang: UiLanguage,
) -> Vec<(String, String)> {
    let mut probes: Vec<(String, String)> = Vec::new();

    // 完全一致は常に先頭
    probes.push((
        snippet.to_string(),
        tr_pair(ui_lang, "完全一致", "Exact match"),
    ));

    // 言語別の構造パターン
    let lang_probes = match lang {
        SupportedLanguage::Auto => generic_probes(ui_lang),
        SupportedLanguage::Rust => rust_probes(ui_lang),
        SupportedLanguage::Java => java_probes(ui_lang),
        SupportedLanguage::Python => python_probes(ui_lang),
        SupportedLanguage::JavaScript => js_probes(ui_lang),
        SupportedLanguage::TypeScript => ts_probes(ui_lang),
        SupportedLanguage::Go => go_probes(ui_lang),
        SupportedLanguage::C => c_probes(ui_lang),
        SupportedLanguage::Cpp => cpp_probes(ui_lang),
        SupportedLanguage::CSharp => csharp_probes(ui_lang),
    };
    probes.extend(lang_probes);

    // 全言語共通の汎用候補も追加
    probes.extend(generic_probes(ui_lang));

    probes
}

// ─── Rust ────────────────────────────────────────────────────────────────

fn rust_probes(lang: UiLanguage) -> Vec<(String, String)> {
    vec![
        probe_l(lang, "fn $NAME($$$ARGS)", "関数シグネチャ（名前と引数）", "Function signature (name and args)"),
        probe_l(lang, "fn $NAME($$$ARGS) { $$$BODY }", "関数定義（戻り値なし）", "Function (no return type)"),
        probe_l(lang, "fn $NAME($$$ARGS) -> $RET { $$$BODY }", "関数定義（戻り値あり）", "Function (with return type)"),
        probe_l(lang, "pub fn $NAME($$$ARGS)", "pub関数シグネチャ", "pub fn signature"),
        probe_l(lang, "pub fn $NAME($$$ARGS) { $$$BODY }", "pub関数定義", "pub fn definition"),
        probe_l(lang, "pub fn $NAME($$$ARGS) -> $RET { $$$BODY }", "pub関数定義（戻り値あり）", "pub fn with return"),
        probe_l(lang, "async fn $NAME($$$ARGS) { $$$BODY }", "async関数定義", "async fn"),
        probe_l(lang, "async fn $NAME($$$ARGS) -> $RET { $$$BODY }", "async関数定義（戻り値あり）", "async fn with return"),
        probe_l(lang, "impl $TYPE { $$$BODY }", "impl ブロック", "impl block"),
        probe_l(lang, "impl $TRAIT for $TYPE { $$$BODY }", "トレイト実装", "trait impl"),
        probe_l(lang, "trait $NAME { $$$BODY }", "trait定義", "trait definition"),
        probe_l(lang, "struct $NAME { $$$FIELDS }", "構造体定義", "struct definition"),
        probe_l(lang, "struct $NAME($$$FIELDS);", "タプル構造体定義", "tuple struct"),
        probe_l(lang, "enum $NAME { $$$VARIANTS }", "enum定義", "enum definition"),
        probe_l(lang, "type $NAME = $TYPE", "typeエイリアス", "type alias"),
        probe_l(lang, "$FUNC($$$ARGS)", "関数呼び出し", "function call"),
        probe_l(lang, "$RECV.$METHOD($$$ARGS)", "メソッド呼び出し", "method call"),
        probe_l(lang, "$MACRO!($$$ARGS)", "マクロ呼び出し", "macro invocation"),
        probe_l(lang, "let $VAR = $EXPR", "let バインディング", "let binding"),
        probe_l(lang, "let mut $VAR = $EXPR", "let mut バインディング", "let mut binding"),
        probe_l(lang, "let $VAR: $TYPE = $EXPR", "型アノテーション付き let", "let with type annotation"),
        probe_l(lang, "$VAR = $EXPR", "再代入", "assignment"),
        probe_l(lang, "if $COND { $$$BODY }", "if 式", "if expression"),
        probe_l(lang, "if $COND { $$$THEN } else { $$$ELSE }", "if-else 式", "if-else"),
        probe_l(lang, "match $EXPR { $$$ARMS }", "match 式", "match"),
        probe_l(lang, "for $PAT in $ITER { $$$BODY }", "for ループ", "for loop"),
        probe_l(lang, "while $COND { $$$BODY }", "while ループ", "while loop"),
        probe_l(lang, "loop { $$$BODY }", "loop", "loop"),
        probe_l(lang, "return $EXPR", "return文", "return"),
        probe_l(lang, "break", "break", "break"),
        probe_l(lang, "continue", "continue", "continue"),
        probe_l(lang, "$EXPR?", "? 演算子（エラー伝播）", "? operator"),
        probe_l(lang, "$VAR.unwrap()", "unwrap()", "unwrap()"),
        probe_l(lang, "$VAR.expect($MSG)", "expect()", "expect()"),
        probe_l(lang, "$VAR.clone()", "clone()", "clone()"),
        probe_l(lang, "$VAR.to_string()", "to_string()", "to_string()"),
        probe_l(lang, "$VAR.into()", "into()", "into()"),
        probe_l(lang, "$A == $B", "等値比較", "equality"),
        probe_l(lang, "$A != $B", "非等値比較", "inequality"),
        probe_l(lang, "$A && $B", "論理AND", "logical AND"),
        probe_l(lang, "$A || $B", "論理OR", "logical OR"),
        probe_l(lang, "|$$$ARGS| $BODY", "クロージャ", "closure"),
        probe_l(lang, "|$$$ARGS| { $$$BODY }", "クロージャ（ブロック）", "closure (block)"),
        probe_l(lang, "use $PATH", "use 宣言", "use declaration"),
        probe_l(lang, "$EXPR.await", ".await", ".await"),
    ]
}

// ─── Java ────────────────────────────────────────────────────────────────

fn java_probes(lang: UiLanguage) -> Vec<(String, String)> {
    vec![
        probe_l(lang, "$RET $NAME($$$ARGS) { $$$BODY }", "メソッド定義", "method definition"),
        probe_l(lang, "$MODS $RET $NAME($$$ARGS) { $$$BODY }", "修飾子付きメソッド定義", "method with modifiers"),
        probe_l(lang, "class $NAME { $$$BODY }", "クラス定義", "class definition"),
        probe_l(lang, "class $NAME extends $PARENT { $$$BODY }", "継承クラス定義", "class extends"),
        probe_l(lang, "interface $NAME { $$$BODY }", "インターフェース定義", "interface"),
        probe_l(lang, "enum $NAME { $$$BODY }", "enum定義", "enum"),
        probe_l(lang, "@$ANNOTATION", "アノテーション", "annotation"),
        probe_l(lang, "$TYPE $VAR = $EXPR", "変数宣言", "variable declaration"),
        probe_l(lang, "$TYPE $NAME($$$ARGS)", "メソッドシグネチャ", "method signature"),
        probe_l(lang, "$RECV.$METHOD($$$ARGS)", "メソッド呼び出し", "method call"),
        probe_l(lang, "new $TYPE($$$ARGS)", "コンストラクタ呼び出し", "constructor call"),
        probe_l(lang, "if ($COND) { $$$BODY }", "if 文", "if statement"),
        probe_l(lang, "if ($COND) { $$$THEN } else { $$$ELSE }", "if-else 文", "if-else"),
        probe_l(lang, "for ($INIT; $COND; $UPDATE) { $$$BODY }", "for ループ", "for loop"),
        probe_l(lang, "for ($VAR : $ITER) { $$$BODY }", "拡張 for ループ", "enhanced for"),
        probe_l(lang, "while ($COND) { $$$BODY }", "while ループ", "while loop"),
        probe_l(lang, "switch ($EXPR) { $$$BODY }", "switch 文", "switch"),
        probe_l(lang, "try { $$$BODY } catch ($EX) { $$$CATCH }", "try-catch", "try-catch"),
        probe_l(lang, "try { $$$BODY } finally { $$$FINALLY }", "try-finally", "try-finally"),
        probe_l(lang, "return $EXPR;", "return文", "return"),
        probe_l(lang, "throw new $TYPE($$$ARGS)", "例外スロー", "throw"),
        probe_l(lang, "$VAR == null", "null チェック", "null check"),
        probe_l(lang, "$VAR != null", "null 非チェック", "not-null check"),
        probe_l(lang, "System.out.println($$$ARGS)", "System.out.println", "System.out.println"),
    ]
}

// ─── Python ──────────────────────────────────────────────────────────────

fn python_probes(lang: UiLanguage) -> Vec<(String, String)> {
    vec![
        probe_l(lang, "def $NAME($$$ARGS): ...", "関数定義", "function def"),
        probe_l(lang, "def $NAME($$$ARGS):\n    $$$BODY", "関数定義（本体あり）", "function with body"),
        probe_l(lang, "async def $NAME($$$ARGS): ...", "async 関数定義", "async def"),
        probe_l(lang, "class $NAME: ...", "クラス定義", "class"),
        probe_l(lang, "class $NAME($PARENT): ...", "継承クラス定義", "class inheritance"),
        probe_l(lang, "$FUNC($$$ARGS)", "関数呼び出し", "function call"),
        probe_l(lang, "$RECV.$METHOD($$$ARGS)", "メソッド呼び出し", "method call"),
        probe_l(lang, "if $COND: ...", "if 文", "if"),
        probe_l(lang, "if $COND:\n    $$$THEN\nelse:\n    $$$ELSE", "if-else 文", "if-else"),
        probe_l(lang, "for $VAR in $ITER: ...", "for ループ", "for loop"),
        probe_l(lang, "while $COND: ...", "while ループ", "while loop"),
        probe_l(lang, "with $EXPR as $VAR: ...", "with 文", "with statement"),
        probe_l(lang, "lambda $$$ARGS: $BODY", "lambda", "lambda"),
        probe_l(lang, "import $MODULE", "import 文", "import"),
        probe_l(lang, "from $MODULE import $NAME", "from import 文", "from import"),
        probe_l(lang, "print($$$ARGS)", "print 呼び出し", "print()"),
        probe_l(lang, "$VAR = $EXPR", "代入", "assignment"),
        probe_l(lang, "return $EXPR", "return文", "return"),
        probe_l(lang, "raise $EXPR", "例外送出", "raise"),
        probe_l(lang, "try: ...\nexcept $EX: ...", "try-except", "try-except"),
        probe_l(lang, "try: ...\nfinally: ...", "try-finally", "try-finally"),
        probe_l(lang, "$A == $B", "等値比較", "equality"),
        probe_l(lang, "$A != $B", "非等値比較", "inequality"),
    ]
}

// ─── JavaScript / TypeScript ──────────────────────────────────────────────

fn js_probes(lang: UiLanguage) -> Vec<(String, String)> {
    vec![
        probe_l(lang, "function $NAME($$$ARGS) { $$$BODY }", "function 宣言", "function declaration"),
        probe_l(lang, "const $NAME = function($$$ARGS) { $$$BODY }", "function 式", "function expression"),
        probe_l(lang, "const $NAME = ($$$ARGS) => $BODY", "アロー関数（式）", "arrow function (expr)"),
        probe_l(lang, "const $NAME = ($$$ARGS) => { $$$BODY }", "アロー関数（ブロック）", "arrow function (block)"),
        probe_l(lang, "async function $NAME($$$ARGS) { $$$BODY }", "async function 宣言", "async function"),
        probe_l(lang, "class $NAME { $$$BODY }", "class 宣言", "class"),
        probe_l(lang, "class $NAME extends $PARENT { $$$BODY }", "継承 class 宣言", "class extends"),
        probe_l(lang, "$FUNC($$$ARGS)", "関数呼び出し", "function call"),
        probe_l(lang, "$RECV.$METHOD($$$ARGS)", "メソッド呼び出し", "method call"),
        probe_l(lang, "await $EXPR", "await 式", "await"),
        probe_l(lang, "if ($COND) { $$$BODY }", "if 文", "if"),
        probe_l(lang, "if ($COND) { $$$THEN } else { $$$ELSE }", "if-else 文", "if-else"),
        probe_l(lang, "for (let $VAR = $INIT; $COND; $UPDATE) { $$$BODY }", "for ループ", "for loop"),
        probe_l(lang, "for (const $VAR of $ITER) { $$$BODY }", "for...of ループ", "for...of"),
        probe_l(lang, "for (const $KEY in $OBJ) { $$$BODY }", "for...in ループ", "for...in"),
        probe_l(lang, "while ($COND) { $$$BODY }", "while ループ", "while loop"),
        probe_l(lang, "try { $$$BODY } catch ($ERR) { $$$CATCH }", "try-catch", "try-catch"),
        probe_l(lang, "return $EXPR", "return文", "return"),
        probe_l(lang, "console.log($$$ARGS)", "console.log", "console.log"),
        probe_l(lang, "const $VAR = $EXPR", "const 宣言", "const"),
        probe_l(lang, "let $VAR = $EXPR", "let 宣言", "let"),
        probe_l(lang, "var $VAR = $EXPR", "var 宣言", "var"),
        probe_l(lang, "$EXPR ?? $DEFAULT", "Nullish Coalescing", "nullish coalescing"),
        probe_l(lang, "$OBJ?.$PROP", "Optional Chaining", "optional chaining"),
        probe_l(lang, "throw new $TYPE($$$ARGS)", "例外スロー", "throw"),
        probe_l(lang, "$PROMISE.then($$$ARGS)", ".then() チェーン", ".then()"),
        probe_l(lang, "import $NAME from $MODULE", "default import", "default import"),
        probe_l(lang, "import { $$$NAMES } from $MODULE", "named import", "named import"),
        probe_l(lang, "export default $EXPR", "default export", "default export"),
    ]
}

fn ts_probes(lang: UiLanguage) -> Vec<(String, String)> {
    let mut probes = js_probes(lang);
    probes.extend(vec![
        probe_l(lang, "interface $NAME { $$$BODY }", "interface 宣言", "interface"),
        probe_l(lang, "type $NAME = $TYPE", "type alias", "type alias"),
        probe_l(lang, "enum $NAME { $$$BODY }", "enum 宣言", "enum"),
        probe_l(lang, "function $NAME($$$ARGS): $RET { $$$BODY }", "戻り値型付き function", "function with return type"),
        probe_l(lang, "const $NAME = ($$$ARGS): $RET => $BODY", "型付きアロー関数", "typed arrow"),
        probe_l(lang, "const $VAR: $TYPE = $EXPR", "型注釈付き const", "const with type"),
        probe_l(lang, "let $VAR: $TYPE = $EXPR", "型注釈付き let", "let with type"),
        probe_l(lang, "$EXPR as $TYPE", "型アサーション", "type assertion"),
    ]);
    probes
}

// ─── Go ──────────────────────────────────────────────────────────────────

fn go_probes(lang: UiLanguage) -> Vec<(String, String)> {
    vec![
        probe_l(lang, "func $NAME($$$ARGS) { $$$BODY }", "関数定義", "function"),
        probe_l(lang, "func $NAME($$$ARGS) $RET { $$$BODY }", "関数定義（戻り値あり）", "function with return"),
        probe_l(lang, "func ($RECV $TYPE) $NAME($$$ARGS) { $$$BODY }", "メソッド定義", "method"),
        probe_l(lang, "type $NAME struct { $$$FIELDS }", "構造体定義", "struct"),
        probe_l(lang, "type $NAME interface { $$$METHODS }", "インターフェース定義", "interface"),
        probe_l(lang, "$RECV.$METHOD($$$ARGS)", "メソッド呼び出し", "method call"),
        probe_l(lang, "$FUNC($$$ARGS)", "関数呼び出し", "function call"),
        probe_l(lang, "if $COND { $$$BODY }", "if 文", "if"),
        probe_l(lang, "if err != nil { $$$BODY }", "エラーチェック", "error check"),
        probe_l(lang, "if $EXPR; $COND { $$$BODY }", "初期化付き if", "if with init"),
        probe_l(lang, "for $COND { $$$BODY }", "for ループ", "for"),
        probe_l(lang, "for $KEY, $VAL := range $ITER { $$$BODY }", "range ループ", "range (key,val)"),
        probe_l(lang, "for $VAL := range $ITER { $$$BODY }", "range ループ（単値）", "range (val)"),
        probe_l(lang, "switch $EXPR { $$$BODY }", "switch 文", "switch"),
        probe_l(lang, "return $EXPR", "return文", "return"),
        probe_l(lang, "go $FUNC($$$ARGS)", "goroutine 起動", "go statement"),
        probe_l(lang, "$VAR, err := $EXPR", "エラー返却代入", "error return assign"),
        probe_l(lang, "$VAR := $EXPR", "短縮変数宣言", "short var decl"),
        probe_l(lang, "defer $FUNC($$$ARGS)", "defer", "defer"),
    ]
}

fn c_probes(lang: UiLanguage) -> Vec<(String, String)> {
    vec![
        probe_l(lang, "$RET $NAME($$$ARGS) { $$$BODY }", "関数定義", "function"),
        probe_l(lang, "$TYPE $VAR = $EXPR;", "変数定義", "variable def"),
        probe_l(lang, "$VAR = $EXPR;", "代入", "assignment"),
        probe_l(lang, "$FUNC($$$ARGS)", "関数呼び出し", "function call"),
        probe_l(lang, "if ($COND) { $$$BODY }", "if 文", "if"),
        probe_l(lang, "if ($COND) { $$$THEN } else { $$$ELSE }", "if-else 文", "if-else"),
        probe_l(lang, "for ($INIT; $COND; $UPDATE) { $$$BODY }", "for ループ", "for loop"),
        probe_l(lang, "while ($COND) { $$$BODY }", "while ループ", "while loop"),
        probe_l(lang, "switch ($EXPR) { $$$BODY }", "switch 文", "switch"),
        probe_l(lang, "return $EXPR;", "return文", "return"),
        probe_l(lang, "$PTR == NULL", "NULL チェック", "NULL check"),
        probe_l(lang, "$PTR != NULL", "NULL 非チェック", "not NULL check"),
        probe_l(lang, "#include $HEADER", "include", "#include"),
        probe_l(lang, "$TYPE *$VAR", "ポインタ宣言", "pointer decl"),
        probe_l(lang, "$TYPE $NAME[$SIZE]", "配列宣言", "array decl"),
    ]
}

fn cpp_probes(lang: UiLanguage) -> Vec<(String, String)> {
    let mut probes = c_probes(lang);
    probes.extend(vec![
        probe_l(lang, "class $NAME { $$$BODY }", "class 定義", "class"),
        probe_l(lang, "struct $NAME { $$$BODY }", "struct 定義", "struct"),
        probe_l(lang, "namespace $NAME { $$$BODY }", "namespace", "namespace"),
        probe_l(lang, "template <$$$ARGS> $DECL", "template 宣言", "template"),
        probe_l(lang, "$RET $NAME($$$ARGS) const { $$$BODY }", "const メンバ関数", "const member fn"),
        probe_l(lang, "for ($TYPE $VAR : $ITER) { $$$BODY }", "range-based for", "range-for"),
        probe_l(lang, "try { $$$BODY } catch ($EX) { $$$CATCH }", "try-catch", "try-catch"),
        probe_l(lang, "throw $EXPR;", "throw", "throw"),
        probe_l(lang, "std::cout << $EXPR", "std::cout", "std::cout"),
        probe_l(lang, "auto $VAR = $EXPR;", "auto 変数宣言", "auto"),
        probe_l(lang, "$PTR == nullptr", "nullptr チェック", "nullptr check"),
        probe_l(lang, "$PTR != nullptr", "nullptr 非チェック", "not nullptr check"),
        probe_l(lang, "#include <$HEADER>", "include <...>", "#include <>"),
        // ─── C++ 追加（継承・特殊メンバ・モダン構文）────────────────
        probe_l(
            lang,
            "class $NAME : public $BASE { $$$BODY }",
            "クラス（public 継承）",
            "class (public inheritance)",
        ),
        probe_l(
            lang,
            "struct $NAME : public $BASE { $$$BODY }",
            "struct（public 継承）",
            "struct (public inheritance)",
        ),
        probe_l(
            lang,
            "class $NAME : private $BASE { $$$BODY }",
            "クラス（private 継承）",
            "class (private inheritance)",
        ),
        probe_l(
            lang,
            "virtual $RET $NAME($$$ARGS) = 0;",
            "純粋仮想関数宣言",
            "pure virtual declaration",
        ),
        probe_l(
            lang,
            "$RET $NAME($$$ARGS) override { $$$BODY }",
            "override メンバ関数",
            "override member function",
        ),
        probe_l(
            lang,
            "$RET $NAME($$$ARGS) final { $$$BODY }",
            "final メンバ関数",
            "final member function",
        ),
        probe_l(lang, "~$NAME() { $$$BODY }", "デストラクタ定義", "destructor definition"),
        probe_l(lang, "~$NAME() = default;", "デストラクタ（= default）", "destructor (= default)"),
        probe_l(
            lang,
            "explicit $NAME($$$ARGS) { $$$BODY }",
            "explicit コンストラクタ",
            "explicit constructor",
        ),
        probe_l(lang, "enum class $NAME { $$$BODY }", "enum class 定義", "enum class"),
        probe_l(lang, "using $NAME = $TYPE;", "using 型エイリアス", "using type alias"),
        probe_l(lang, "typedef $TYPE $NAME;", "typedef", "typedef"),
        probe_l(lang, "static_cast<$TYPE>($EXPR)", "static_cast", "static_cast"),
        probe_l(lang, "dynamic_cast<$TYPE>($EXPR)", "dynamic_cast", "dynamic_cast"),
        probe_l(lang, "const_cast<$TYPE>($EXPR)", "const_cast", "const_cast"),
        probe_l(lang, "reinterpret_cast<$TYPE>($EXPR)", "reinterpret_cast", "reinterpret_cast"),
        probe_l(lang, "std::move($EXPR)", "std::move", "std::move"),
        probe_l(lang, "std::forward<$TYPE>($EXPR)", "std::forward", "std::forward"),
        probe_l(lang, "std::swap($A, $B)", "std::swap", "std::swap"),
        probe_l(lang, "std::max($A, $B)", "std::max", "std::max"),
        probe_l(lang, "std::min($A, $B)", "std::min", "std::min"),
        probe_l(lang, "std::make_unique<$TYPE>($$$ARGS)", "std::make_unique", "std::make_unique"),
        probe_l(lang, "std::make_shared<$TYPE>($$$ARGS)", "std::make_shared", "std::make_shared"),
        probe_l(lang, "std::unique_ptr<$TYPE> $VAR", "std::unique_ptr 変数", "std::unique_ptr var"),
        probe_l(lang, "std::shared_ptr<$TYPE> $VAR", "std::shared_ptr 変数", "std::shared_ptr var"),
        probe_l(lang, "std::optional<$TYPE> $VAR", "std::optional 変数", "std::optional var"),
        probe_l(lang, "std::vector<$TYPE> $VAR", "std::vector 変数", "std::vector var"),
        probe_l(lang, "std::string $VAR", "std::string 変数", "std::string var"),
        probe_l(lang, "std::cerr << $EXPR", "std::cerr", "std::cerr"),
        probe_l(lang, "std::cin >> $VAR", "std::cin >>", "std::cin >>"),
        probe_l(lang, "constexpr $VAR = $EXPR;", "constexpr 変数", "constexpr variable"),
        probe_l(
            lang,
            "constexpr $RET $NAME($$$ARGS) { $$$BODY }",
            "constexpr 関数",
            "constexpr function",
        ),
        probe_l(lang, "noexcept", "noexcept 指定子", "noexcept specifier"),
        probe_l(lang, "noexcept($EXPR)", "noexcept(式)", "noexcept(expression)"),
        probe_l(lang, "static_assert($COND, $MSG);", "static_assert", "static_assert"),
        probe_l(lang, "friend class $NAME;", "friend class", "friend class"),
        probe_l(lang, "this->$FIELD", "this-> メンバ", "this-> member"),
        probe_l(lang, "delete $PTR;", "delete", "delete"),
        probe_l(lang, "delete[] $PTR;", "delete[]", "delete[]"),
        probe_l(lang, "new $TYPE($$$ARGS)", "new 式", "new expression"),
        probe_l(lang, "[]($$$ARGS) { $$$BODY }", "ラムダ（キャプチャなし）", "lambda (no capture)"),
        probe_l(lang, "[=]($$$ARGS) { $$$BODY }", "ラムダ（[=]）", "lambda ([=])"),
        probe_l(lang, "[&]($$$ARGS) { $$$BODY }", "ラムダ（[&]）", "lambda ([&])"),
        probe_l(
            lang,
            "std::lock_guard<$TYPE> $VAR($MUTEX);",
            "std::lock_guard",
            "std::lock_guard",
        ),
        probe_l(
            lang,
            "std::unique_lock<$TYPE> $VAR($MUTEX);",
            "std::unique_lock",
            "std::unique_lock",
        ),
        probe_l(lang, "std::mutex $VAR;", "std::mutex 変数", "std::mutex var"),
        probe_l(lang, "std::thread $VAR($$$ARGS);", "std::thread", "std::thread"),
        probe_l(lang, "std::async($$$ARGS)", "std::async", "std::async"),
        probe_l(lang, "co_await $EXPR", "co_await", "co_await"),
        probe_l(lang, "co_return $EXPR;", "co_return", "co_return"),
        probe_l(lang, "co_yield $EXPR;", "co_yield", "co_yield"),
        probe_l(lang, "concept $NAME = $EXPR;", "concept 定義", "concept"),
        probe_l(
            lang,
            "requires $EXPR { $$$BODY }",
            "requires 句（ブロック）",
            "requires clause (block)",
        ),
        probe_l(lang, "if constexpr ($COND) { $$$BODY }", "if constexpr", "if constexpr"),
    ]);
    probes
}

fn csharp_probes(lang: UiLanguage) -> Vec<(String, String)> {
    vec![
        probe_l(lang, "class $NAME { $$$BODY }", "class 定義", "class"),
        probe_l(lang, "interface $NAME { $$$BODY }", "interface 定義", "interface"),
        probe_l(lang, "enum $NAME { $$$BODY }", "enum 定義", "enum"),
        probe_l(lang, "namespace $NAME { $$$BODY }", "namespace", "namespace"),
        probe_l(lang, "$MODS $RET $NAME($$$ARGS) { $$$BODY }", "メソッド定義", "method"),
        probe_l(lang, "$TYPE $VAR = $EXPR;", "変数宣言", "variable"),
        probe_l(lang, "var $VAR = $EXPR;", "var 宣言", "var"),
        probe_l(lang, "$FUNC($$$ARGS)", "関数呼び出し", "function call"),
        probe_l(lang, "$RECV.$METHOD($$$ARGS)", "メソッド呼び出し", "method call"),
        probe_l(lang, "if ($COND) { $$$BODY }", "if 文", "if"),
        probe_l(lang, "if ($COND) { $$$THEN } else { $$$ELSE }", "if-else 文", "if-else"),
        probe_l(lang, "foreach ($TYPE $VAR in $ITER) { $$$BODY }", "foreach", "foreach"),
        probe_l(lang, "for ($INIT; $COND; $UPDATE) { $$$BODY }", "for ループ", "for"),
        probe_l(lang, "while ($COND) { $$$BODY }", "while ループ", "while"),
        probe_l(lang, "using $NAME = $TYPE;", "using alias", "using alias"),
        probe_l(lang, "using ($EXPR) { $$$BODY }", "using 文", "using statement"),
        probe_l(lang, "return $EXPR;", "return文", "return"),
        probe_l(lang, "throw new $TYPE($$$ARGS);", "例外スロー", "throw"),
        probe_l(lang, "try { $$$BODY } catch ($EX) { $$$CATCH }", "try-catch", "try-catch"),
        probe_l(lang, "$VAR == null", "null チェック", "null check"),
        probe_l(lang, "$VAR != null", "null 非チェック", "not-null check"),
        probe_l(lang, "Console.WriteLine($$$ARGS)", "Console.WriteLine", "Console.WriteLine"),
    ]
}

// ─── 汎用（全言語共通） ───────────────────────────────────────────────────

fn generic_probes(lang: UiLanguage) -> Vec<(String, String)> {
    vec![
        probe_l(lang, "$VAR", "任意の単一ノード", "any single node"),
        probe_l(lang, "$FUNC($$$ARGS)", "関数/メソッド呼び出し（汎用）", "call (generic)"),
        probe_l(lang, "$RECV.$METHOD($$$ARGS)", "メソッド呼び出し（汎用）", "method call (generic)"),
        probe_l(lang, "$VAR = $EXPR", "代入（汎用）", "assignment (generic)"),
        probe_l(lang, "return $EXPR", "return（汎用）", "return (generic)"),
        probe_l(lang, "$A == $B", "等値比較（汎用）", "equality (generic)"),
        probe_l(lang, "$A != $B", "非等値比較（汎用）", "inequality (generic)"),
        probe_l(lang, "$A && $B", "論理AND", "logical AND"),
        probe_l(lang, "$A || $B", "論理OR", "logical OR"),
        probe_l(lang, "if ($COND) { $$$BODY }", "if 文（汎用）", "if (generic)"),
    ]
}
