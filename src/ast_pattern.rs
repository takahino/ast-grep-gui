use ast_grep_core::{MatchStrictness, Pattern};
use ast_grep_language::Language;

use crate::lang::SupportedLanguage;

/// パターン文字列を「複数のコンパイル戦略」でコンパイルして返す。
///
/// 戦略#1 は ast-grep / Playground と同じ `Pattern::try_new`（標準機構）。
/// ただし C/C++ では tree-sitter の構文上の曖昧性により、`try_new` だけでは
/// 通らない（コンパイルは成功するがマッチ0になる）パターンが存在する。
/// それらを救済するための contextual 戦略を後段に追加する。
///
/// 呼び出し側（検索・置換）は、得られた戦略を順に試し、最初にヒットしたものを採用する。
pub fn compile_strategies<L: Language + Clone>(
    pattern: &str,
    lang: SupportedLanguage,
    ast_lang: L,
) -> Vec<Pattern> {
    let mut compiled = Vec::new();

    // 戦略#1: ast-grep 標準（CLI / Playground と同一）
    if let Ok(pat) = Pattern::try_new(pattern, ast_lang.clone()) {
        compiled.push(pat);
    }

    // 戦略#2: C/C++ の関数・メソッド「定義」救済。
    // `T n() { $$$BODY }` は tree-sitter-cpp では `{...}` が initializer_list と
    // 解釈され `declaration` になってしまい、`function_definition` にマッチしない
    // （ast-grep 本体でも既知の制約: ast-grep/ast-grep#946）。
    // `function_declarator` を選択することで、定義・メソッドを確実に発見できる。
    if let Some(pat) = compile_contextual_function_declarator(pattern, lang, ast_lang.clone()) {
        compiled.push(pat);
    }

    // 戦略#3: C/C++ のスコープ解決付き呼び出し `$CLASS::$METHOD($$$ARGS)` 救済。
    if let Some(pat) = compile_contextual_call(pattern, lang, ast_lang) {
        compiled.push(pat);
    }

    compiled
}

/// C/C++ の関数定義型パターンを `function_declarator` 文脈でコンパイルする。
///
/// 例: `void CApModel00Dlg::OnPaint() { $$$BODY }` /
///     `$RET $CLASS::$METHOD($$$ARGS) { $$$BODY }` /
///     `$RET $NAME($$$ARGS) { $$$BODY }`
///
/// 注意: `function_declarator` ノードを選択するため、戻り値型(`$RET`)と本体(`$$$BODY`)は
/// マッチ対象から外れる。修飾付きメソッド（`Class::method`）では正確に一致するが、
/// 非修飾の汎用シグネチャでは関数プロトタイプ宣言にも一致しうる。これは
/// tree-sitter-cpp の構文上の曖昧性に起因する制約である。
fn compile_contextual_function_declarator<L: Language>(
    pattern: &str,
    lang: SupportedLanguage,
    ast_lang: L,
) -> Option<Pattern> {
    if !matches!(lang, SupportedLanguage::C | SupportedLanguage::Cpp) {
        return None;
    }
    if !looks_like_function_definition_pattern(pattern) {
        return None;
    }

    Pattern::contextual(pattern.trim(), "function_declarator", ast_lang)
        .ok()
        .map(|pat| pat.with_strictness(MatchStrictness::Ast))
}

/// `<シグネチャ>(<引数>) <指定子?> { <本体> }` の形をした C/C++ 関数・メソッド定義パターンか判定する。
fn looks_like_function_definition_pattern(pattern: &str) -> bool {
    let trimmed = pattern.trim();
    if trimmed.is_empty() || !trimmed.ends_with('}') {
        return false;
    }

    // 制御構文・例外構文は関数定義ではない（`try_new` 側で正しく処理される）。
    if [
        "if ", "if(", "for ", "for(", "while ", "while(", "switch ", "switch(", "catch ", "catch(",
        "do ", "do{", "else ", "return ", "throw ",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix))
    {
        return false;
    }

    let open_paren = match trimmed.find('(') {
        Some(i) => i,
        None => return false,
    };
    let open_brace = match trimmed.find('{') {
        Some(i) => i,
        None => return false,
    };
    // シグネチャの括弧が本体の波括弧より前にあること
    if open_paren >= open_brace {
        return false;
    }
    // 括弧が閉じてから本体に入ること
    trimmed[open_paren..open_brace].contains(')')
}

fn compile_contextual_call<L: Language>(
    pattern: &str,
    lang: SupportedLanguage,
    ast_lang: L,
) -> Option<Pattern> {
    if !matches!(lang, SupportedLanguage::C | SupportedLanguage::Cpp) {
        return None;
    }
    if !looks_like_qualified_call_pattern(pattern) {
        return None;
    }

    Pattern::contextual(&format!("{pattern};"), "call_expression", ast_lang)
        .ok()
        .map(|pat| pat.with_strictness(MatchStrictness::Ast))
}

fn looks_like_qualified_call_pattern(pattern: &str) -> bool {
    let trimmed = pattern.trim();
    if trimmed.is_empty() || !trimmed.contains("::") {
        return false;
    }
    if !trimmed.contains('(') || !trimmed.ends_with(')') {
        return false;
    }
    if trimmed.contains('{') || trimmed.contains('=') || trimmed.ends_with(';') {
        return false;
    }

    ![
        "if ", "for ", "while ", "switch ", "return ", "catch ", "throw ",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ast_grep_language::{LanguageExt, SupportLang};

    /// `compile_strategies` でコンパイルした全戦略のうち、対象コードに 1 件でも当たるか。
    fn matches_any(pattern: &str, lang: SupportedLanguage, source: &str) -> usize {
        let ast_lang = lang.to_support_lang().expect("fixed language");
        let root = ast_lang.ast_grep(source);
        for pat in compile_strategies(pattern, lang, ast_lang) {
            let count = root.root().find_all(&pat).count();
            if count > 0 {
                return count;
            }
        }
        0
    }

    const CPP_DLG: &str = r#"
void CApModel00Dlg::OnPaint()
{
    if (IsIconic())
    {
        CPaintDC dc(this);
        SendMessage(WM_ICONERASEBKGND, (WPARAM) dc.GetSafeHdc(), 0);
        int cxIcon = GetSystemMetrics(SM_CXICON);
        dc.DrawIcon(x, y, m_hIcon);
    }
    else
    {
        CDialog::OnPaint();
    }
}
"#;

    #[test]
    fn looks_like_function_definition_basic() {
        assert!(looks_like_function_definition_pattern(
            "void CApModel00Dlg::OnPaint() { $$$BODY }"
        ));
        assert!(looks_like_function_definition_pattern(
            "$RET $NAME() { $$$BODY }"
        ));
        assert!(looks_like_function_definition_pattern(
            "$RET $CLASS::$METHOD($$$ARGS) { $$$BODY }"
        ));
        // 制御構文・呼び出し・宣言は関数定義扱いしない
        assert!(!looks_like_function_definition_pattern(
            "if ($C) { $$$BODY }"
        ));
        assert!(!looks_like_function_definition_pattern(
            "while ($C) { $$$BODY }"
        ));
        assert!(!looks_like_function_definition_pattern(
            "$CLASS::$METHOD($$$ARGS)"
        ));
        assert!(!looks_like_function_definition_pattern(
            "class $N { $$$BODY }"
        ));
        assert!(!looks_like_function_definition_pattern("$VAR"));
        assert!(!looks_like_function_definition_pattern("foo({})"));
    }

    /// 退行確認: 報告にあった「ast-grep では通るがこのプロジェクトでは通らない」C++ 関数定義パターン群。
    #[test]
    fn cpp_function_definition_patterns_match() {
        for pat in [
            "void CApModel00Dlg::OnPaint() { $$$BODY }",
            "$RET CApModel00Dlg::OnPaint() { $$$BODY }",
            "$RET $NAME() { $$$BODY }",
            "$RET $NAME($$$ARGS) { $$$BODY }",
            "$RET $CLASS::$METHOD($$$ARGS) { $$$BODY }",
        ] {
            assert!(
                matches_any(pat, SupportedLanguage::Cpp, CPP_DLG) >= 1,
                "C++ 関数定義パターンがマッチしない: {pat:?}"
            );
        }
    }

    /// 既存挙動: 単純なメタ変数・呼び出し・スコープ解決付き呼び出しが引き続きマッチすること。
    #[test]
    fn cpp_existing_patterns_still_match() {
        assert!(matches_any("$VAR", SupportedLanguage::Cpp, CPP_DLG) >= 1);
        assert!(matches_any("IsIconic()", SupportedLanguage::Cpp, CPP_DLG) >= 1);
        assert!(matches_any("$CLASS::$METHOD($$$ARGS)", SupportedLanguage::Cpp, CPP_DLG) >= 1);
    }

    /// 関数定義パターンのマッチノードは `function_declarator` だが、その親が
    /// `function_definition` であり、本体（`{ $$$BODY }`）を含む全体に範囲拡張できること。
    /// （検索側で親へ拡張して表示するための前提を担保する。）
    #[test]
    fn cpp_function_match_parent_is_definition_with_body() {
        let ast_lang = SupportLang::Cpp;
        let root = ast_lang.ast_grep(CPP_DLG);
        let pat = compile_contextual_function_declarator(
            "void CApModel00Dlg::OnPaint() { $$$BODY }",
            SupportedLanguage::Cpp,
            ast_lang,
        )
        .expect("function_declarator pattern compiles");
        let m = root.root().find_all(&pat).next().expect("one match");
        let node = m.get_node();
        assert_eq!(node.kind(), "function_declarator");
        let parent = node.parent().expect("has parent");
        assert_eq!(parent.kind(), "function_definition");
        // 親（関数定義全体）には本体の中身が含まれる
        assert!(
            parent.text().contains("DrawIcon"),
            "関数定義全体に本体が含まれていない: {}",
            parent.text()
        );
    }

    /// C でも関数定義が発見できること。
    #[test]
    fn c_function_definition_matches() {
        let src = "int add(int a, int b) {\n    return a + b;\n}\n";
        assert!(matches_any("$RET $NAME($$$ARGS) { $$$BODY }", SupportedLanguage::C, src) >= 1);
    }

    /// 関数定義救済は C/C++ 限定であること（他言語では function_declarator 戦略を足さない）。
    #[test]
    fn function_declarator_strategy_is_c_cpp_only() {
        assert!(compile_contextual_function_declarator(
            "$RET $NAME($$$ARGS) { $$$BODY }",
            SupportedLanguage::Rust,
            SupportLang::Rust,
        )
        .is_none());
        assert!(compile_contextual_function_declarator(
            "$RET $NAME($$$ARGS) { $$$BODY }",
            SupportedLanguage::Cpp,
            SupportLang::Cpp,
        )
        .is_some());
    }
}
