//! CLI / GUI コマンドライン補助で共有する設定型

use std::path::PathBuf;

use crate::batch::PatternJob;
use crate::export::{OutputFormat, OutputViewSet};
use crate::file_encoding::FileEncodingPreference;
use crate::i18n::UiLanguage;
use crate::lang::SupportedLanguage;
use crate::search::{PlainTextSearchOptions, SearchMode};

/// バッチ検索の共通オプション（全パターンに適用）
#[derive(Debug, Clone)]
pub struct BatchCommonOptions {
    pub search_dir: String,
    pub selected_lang: SupportedLanguage,
    pub context_lines: usize,
    pub file_filter: String,
    pub file_encoding_preference: FileEncodingPreference,
    pub max_file_size_mb: u64,
    pub max_search_hits: usize,
    pub skip_dirs: String,
    pub search_mode: SearchMode,
    pub plain_text_options: PlainTextSearchOptions,
    pub cpp_include_dirs: String,
    pub type_hints_enabled: bool,
}

impl Default for BatchCommonOptions {
    fn default() -> Self {
        Self {
            search_dir: String::new(),
            selected_lang: SupportedLanguage::Auto,
            context_lines: 2,
            file_filter: String::new(),
            file_encoding_preference: FileEncodingPreference::default(),
            max_file_size_mb: 10,
            max_search_hits: crate::search::default_max_search_hits(),
            skip_dirs: ".git;.hg;.svn;target;node_modules;dist;build;.cache;.next;vendor;__pycache__;venv;.venv"
                .to_string(),
            search_mode: SearchMode::AstGrep,
            plain_text_options: PlainTextSearchOptions::default(),
            cpp_include_dirs: String::new(),
            type_hints_enabled: crate::search::default_type_hints_enabled(),
        }
    }
}

/// CLI / GUI 補助画面の実行リクエスト
#[derive(Debug, Clone)]
pub struct BatchRunRequest {
    pub patterns_file: PathBuf,
    pub common: BatchCommonOptions,
    pub views: OutputViewSet,
    pub format: OutputFormat,
    pub output: Option<PathBuf>,
    pub ui_lang: UiLanguage,
}

impl BatchRunRequest {
    pub fn jobs_from_patterns_file(&self) -> anyhow::Result<Vec<PatternJob>> {
        let patterns = crate::batch::read_patterns_file(&self.patterns_file)?;
        Ok(crate::batch::jobs_from_pattern_lines(
            &patterns,
            &self.common,
        ))
    }
}

/// 配布 exe 名（コマンドプレビュー用）
pub fn default_cli_exe_name() -> &'static str {
    if cfg!(windows) {
        "ast-grep-gui.exe"
    } else {
        "ast-grep-gui"
    }
}

/// バッチ CLI 用のコマンド文字列を組み立てる（`ast-grep-gui.exe --batch ...`）
pub fn format_cli_command(req: &BatchRunRequest, exe_name: &str) -> String {
    let mut parts = vec![exe_name.to_string(), "--batch".to_string()];

    parts.push("--patterns".to_string());
    parts.push(shell_quote(&req.patterns_file.display().to_string()));

    parts.push("--dir".to_string());
    parts.push(shell_quote(&req.common.search_dir));

    if req.common.selected_lang != SupportedLanguage::Auto {
        if let Some(lang) = lang_cli_arg(req.common.selected_lang) {
            parts.push("--lang".to_string());
            parts.push(lang.to_string());
        }
    }

    if !req.views.is_default_table_only() {
        parts.push("--view".to_string());
        parts.push(req.views.to_cli_arg());
    }

    if req.format != OutputFormat::Json {
        parts.push("--format".to_string());
        parts.push(req.format.cli_name().to_string());
    }

    if let Some(out) = &req.output {
        parts.push("--output".to_string());
        parts.push(shell_quote(&out.display().to_string()));
    }

    if req.common.context_lines != 2 {
        parts.push("--context".to_string());
        parts.push(req.common.context_lines.to_string());
    }

    if !req.common.file_filter.trim().is_empty() {
        parts.push("--filter".to_string());
        parts.push(shell_quote(req.common.file_filter.trim()));
    }

    if req.common.skip_dirs != BatchCommonOptions::default().skip_dirs {
        parts.push("--skip-dirs".to_string());
        parts.push(shell_quote(&req.common.skip_dirs));
    }

    if req.common.max_search_hits != crate::search::default_max_search_hits() {
        parts.push("--max-hits".to_string());
        parts.push(req.common.max_search_hits.to_string());
    }

    if req.common.max_file_size_mb != 10 {
        parts.push("--max-file-size-mb".to_string());
        parts.push(req.common.max_file_size_mb.to_string());
    }

    if !req.common.cpp_include_dirs.trim().is_empty() {
        parts.push("--include-dirs".to_string());
        parts.push(shell_quote(req.common.cpp_include_dirs.trim()));
    }

    if !req.common.type_hints_enabled {
        parts.push("--no-type-hints".to_string());
    }

    parts.join(" ")
}

fn lang_cli_arg(lang: SupportedLanguage) -> Option<&'static str> {
    match lang {
        SupportedLanguage::Auto => None,
        SupportedLanguage::Rust => Some("rust"),
        SupportedLanguage::Java => Some("java"),
        SupportedLanguage::Python => Some("python"),
        SupportedLanguage::JavaScript => Some("javascript"),
        SupportedLanguage::TypeScript => Some("typescript"),
        SupportedLanguage::Go => Some("go"),
        SupportedLanguage::C => Some("c"),
        SupportedLanguage::Cpp => Some("cpp"),
        SupportedLanguage::CSharp => Some("csharp"),
        SupportedLanguage::Kotlin => Some("kotlin"),
        SupportedLanguage::Scala => Some("scala"),
    }
}

fn shell_quote(s: &str) -> String {
    if s.contains(' ') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

/// ビュー名をパース（`,` 区切り・複数回指定）
pub fn parse_output_views(values: &[String]) -> anyhow::Result<OutputViewSet> {
    let mut set = OutputViewSet::default();
    for v in values {
        for part in v.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            set.insert(parse_output_view(part)?);
        }
    }
    if set.is_empty() {
        set.insert(crate::export::OutputView::Table);
    }
    Ok(set)
}

pub fn parse_output_view(s: &str) -> anyhow::Result<crate::export::OutputView> {
    use crate::export::OutputView;
    match s.to_lowercase().as_str() {
        "code" => Ok(OutputView::Code),
        "table" => Ok(OutputView::Table),
        "summary" => Ok(OutputView::Summary),
        other => anyhow::bail!("unknown view: {other} (expected code, table, or summary)"),
    }
}

pub fn parse_output_format(s: &str) -> anyhow::Result<OutputFormat> {
    OutputFormat::from_cli_str(s)
        .ok_or_else(|| anyhow::anyhow!("unknown format: {s}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::OutputView;

    #[test]
    fn parse_views_comma_separated() {
        let views = parse_output_views(&["code,summary".to_string()]).unwrap();
        assert!(views.contains(OutputView::Code));
        assert!(views.contains(OutputView::Summary));
        assert!(!views.contains(OutputView::Table));
    }

    #[test]
    fn parse_views_defaults_to_table() {
        let views = parse_output_views(&[]).unwrap();
        assert!(views.contains(OutputView::Table));
    }
}
