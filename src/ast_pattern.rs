use ast_grep_core::{MatchStrictness, Pattern};
use ast_grep_language::Language;

use crate::lang::SupportedLanguage;

pub fn compile_strategies<L: Language + Clone>(
    pattern: &str,
    lang: SupportedLanguage,
    ast_lang: L,
) -> Vec<Pattern> {
    let mut compiled = Vec::new();

    if let Ok(pat) = Pattern::try_new(pattern, ast_lang.clone()) {
        compiled.push(pat);
    }

    if let Some(pat) = compile_contextual_call(pattern, lang, ast_lang) {
        compiled.push(pat);
    }

    compiled
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
        "if ",
        "for ",
        "while ",
        "switch ",
        "return ",
        "catch ",
        "throw ",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix))
}
