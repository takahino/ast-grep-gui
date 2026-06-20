//! `ast-grep-gui.exe --batch ...` による CLI バッチ実行

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;

use crate::batch::run_batch_sync;
use crate::cli_config::{
    parse_output_format, parse_output_views, BatchCommonOptions, BatchRunRequest,
};
use crate::export::{export_batch_report_with_views, OutputViewSet};
use crate::file_encoding::FileEncodingPreference;
use crate::i18n::UiLanguage;
use crate::lang::SupportedLanguage;
use crate::search::SearchMode;
use crate::search_target::{RemoteCachePolicy, RemoteTargetConfig, SearchTargetMode};

/// コマンドライン引数に `--batch` が含まれるか
pub fn is_batch_mode(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().any(|a| a == "--batch")
}

/// 環境変数 `std::env::args()` からバッチ CLI を実行する
pub fn run_from_env() -> anyhow::Result<()> {
    let cli = BatchCli::parse();
    if !cli.batch {
        anyhow::bail!("--batch flag is required for CLI batch mode");
    }
    run_batch(cli)
}

#[derive(Parser)]
#[command(
    name = "ast-grep-gui",
    about = "ast-grep GUI and batch search from a pattern file"
)]
struct BatchCli {
    /// Run batch search without opening the GUI
    #[arg(long)]
    batch: bool,

    /// Pattern file (one pattern per line; empty lines and # comments ignored)
    #[arg(long)]
    patterns: PathBuf,

    /// Search root directory (local)
    #[arg(long)]
    dir: Option<PathBuf>,

    /// Git remote URL to fetch before search
    #[arg(long)]
    git_url: Option<String>,

    /// SVN remote URL to fetch before search
    #[arg(long)]
    svn_url: Option<String>,

    /// Git branch/tag/commit (with --git-url)
    #[arg(long)]
    git_ref: Option<String>,

    /// SVN revision (with --svn-url)
    #[arg(long)]
    revision: Option<String>,

    /// Subdirectory within fetched repository
    #[arg(long)]
    subdir: Option<String>,

    /// Force refresh remote cache
    #[arg(long)]
    refresh_cache: bool,

    /// Target language (default: auto)
    #[arg(long, default_value = "auto")]
    lang: String,

    /// Output view(s): code, table, summary (comma-separated or repeat flag)
    #[arg(long = "view", value_delimiter = ',', default_value = "table")]
    view: Vec<String>,

    /// Output format
    #[arg(long, default_value = "json")]
    format: String,

    /// Output file path (required in batch mode)
    #[arg(long)]
    output: Option<PathBuf>,

    /// Context lines around each match
    #[arg(long, default_value_t = 2)]
    context: usize,

    /// File name filter (semicolon-separated globs)
    #[arg(long, default_value = "")]
    filter: String,

    /// Directories to skip (semicolon-separated names)
    #[arg(long)]
    skip_dirs: Option<String>,

    /// Max hits per pattern (0 = unlimited)
    #[arg(long)]
    max_hits: Option<usize>,

    /// Max file size in MB
    #[arg(long, default_value_t = 10)]
    max_file_size_mb: u64,

    /// C++ include dirs for type hints (; separated)
    #[arg(long, default_value = "")]
    include_dirs: String,

    /// Disable type hint inference
    #[arg(long)]
    no_type_hints: bool,
}

fn run_batch(args: BatchCli) -> anyhow::Result<()> {
    let output = args
        .output
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--output is required for batch mode"))?;

    let views: OutputViewSet = parse_output_views(&args.view)?;
    let format = parse_output_format(&args.format)?;

    if format.requires_output_file() && args.output.is_none() {
        anyhow::bail!("--output is required for xlsx format");
    }

    let lang = parse_lang(&args.lang)?;
    let skip_dirs = args.skip_dirs.unwrap_or_else(|| {
        ".git;.hg;.svn;target;node_modules;dist;build;.cache;.next;vendor;__pycache__;venv;.venv"
            .to_string()
    });

    let (search_target_mode, search_dir, remote_target) = if let Some(url) = args.git_url {
        (
            SearchTargetMode::GitRemote,
            String::new(),
            RemoteTargetConfig {
                url,
                git_ref: args.git_ref.unwrap_or_default(),
                svn_revision: String::new(),
                subdir: args.subdir.unwrap_or_default(),
                cache_policy: if args.refresh_cache {
                    RemoteCachePolicy::RefreshNext
                } else {
                    RemoteCachePolicy::UseCache
                },
            },
        )
    } else if let Some(url) = args.svn_url {
        (
            SearchTargetMode::SvnRemote,
            String::new(),
            RemoteTargetConfig {
                url,
                git_ref: String::new(),
                svn_revision: args.revision.unwrap_or_default(),
                subdir: args.subdir.unwrap_or_default(),
                cache_policy: if args.refresh_cache {
                    RemoteCachePolicy::RefreshNext
                } else {
                    RemoteCachePolicy::UseCache
                },
            },
        )
    } else {
        let dir = args
            .dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--dir, --git-url, or --svn-url is required"))?;
        (
            SearchTargetMode::Directory,
            dir.display().to_string(),
            RemoteTargetConfig::default(),
        )
    };

    let common = BatchCommonOptions {
        search_dir,
        search_target_mode,
        remote_target,
        selected_lang: lang,
        context_lines: args.context,
        file_filter: args.filter,
        file_encoding_preference: FileEncodingPreference::default(),
        max_file_size_mb: args.max_file_size_mb,
        max_search_hits: args
            .max_hits
            .unwrap_or_else(crate::search::default_max_search_hits),
        skip_dirs,
        search_mode: SearchMode::AstGrep,
        plain_text_options: Default::default(),
        cpp_include_dirs: args.include_dirs,
        type_hints_enabled: !args.no_type_hints,
    };

    let req = BatchRunRequest {
        patterns_file: args.patterns,
        common,
        views: views.clone(),
        format,
        output: args.output.clone(),
        ui_lang: UiLanguage::Japanese,
    };

    let jobs = req.jobs_from_patterns_file()?;
    if jobs.is_empty() {
        anyhow::bail!("no patterns found in pattern file");
    }

    let report = run_batch_sync(&jobs, req.ui_lang, Arc::new(crate::type_hint_config::TypeHintConfig::default()));

    export_batch_report_with_views(
        &report,
        &views,
        format,
        req.ui_lang,
        Some(output.as_path()),
    )?;

    if report.failed_count() > 0 {
        std::process::exit(1);
    }

    Ok(())
}

fn parse_lang(s: &str) -> anyhow::Result<SupportedLanguage> {
    match s.to_lowercase().as_str() {
        "auto" => Ok(SupportedLanguage::Auto),
        other => SupportedLanguage::from_cli_str(other)
            .ok_or_else(|| anyhow::anyhow!("unknown language: {other}")),
    }
}
