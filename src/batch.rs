//! 複数パターンのバッチ検索用のデータモデル

use std::path::Path;

use crate::cli_config::BatchCommonOptions;
use crate::file_encoding::FileEncodingPreference;
use crate::lang::SupportedLanguage;
use crate::search::{PlainTextSearchOptions, SearchConditions, SearchMode, SearchStats};
use crate::search_target::{RemoteTargetConfig, SearchTargetMode};

/// 単一検索で使う予約 `job_id`（バッチジョブは 1 から採番）
pub const SINGLE_SEARCH_JOB_ID: usize = 0;

/// バッチに登録する 1 件の検索ジョブ（パターンと条件を個別に保持）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PatternJob {
    pub id: usize,
    pub label: String,
    pub enabled: bool,
    pub pattern: String,
    pub search_dir: String,
    #[serde(default)]
    pub search_target_mode: SearchTargetMode,
    #[serde(default)]
    pub remote_target: RemoteTargetConfig,
    pub selected_lang: SupportedLanguage,
    pub context_lines: usize,
    pub file_filter: String,
    pub file_encoding_preference: FileEncodingPreference,
    pub max_file_size_mb: u64,
    pub max_search_hits: usize,
    pub skip_dirs: String,
    pub search_mode: SearchMode,
    #[serde(default)]
    pub plain_text_options: PlainTextSearchOptions,
    /// C++ 型ヒント用（`-I` 相当、`;` 区切り）
    #[serde(default)]
    pub cpp_include_dirs: String,
    /// メタ変数の型ヒント推定を行う
    #[serde(default = "crate::search::default_type_hints_enabled")]
    pub type_hints_enabled: bool,
}

impl PatternJob {
    pub fn to_conditions(&self) -> SearchConditions {
        SearchConditions {
            search_dir: self.search_dir.clone(),
            search_target_mode: self.search_target_mode,
            remote_target: self.remote_target.clone(),
            pattern: self.pattern.clone(),
            selected_lang: self.selected_lang,
            context_lines: self.context_lines,
            file_filter: self.file_filter.clone(),
            file_encoding_preference: self.file_encoding_preference,
            max_file_size_mb: self.max_file_size_mb,
            max_search_hits: self.max_search_hits,
            skip_dirs: self.skip_dirs.clone(),
            search_mode: self.search_mode,
            plain_text_options: self.plain_text_options,
            cpp_include_dirs: self.cpp_include_dirs.clone(),
            type_hints_enabled: self.type_hints_enabled,
        }
    }

    /// メイン画面の現在設定から新規ジョブを作る（`id` は呼び出し側で設定）
    pub fn from_app_snapshot(
        id: usize,
        label: String,
        pattern: String,
        search_dir: String,
        search_target_mode: SearchTargetMode,
        remote_target: RemoteTargetConfig,
        selected_lang: SupportedLanguage,
        context_lines: usize,
        file_filter: String,
        file_encoding_preference: FileEncodingPreference,
        max_file_size_mb: u64,
        max_search_hits: usize,
        skip_dirs: String,
        search_mode: SearchMode,
        plain_text_options: PlainTextSearchOptions,
        cpp_include_dirs: String,
        type_hints_enabled: bool,
    ) -> Self {
        Self {
            id,
            label,
            enabled: true,
            pattern,
            search_dir,
            search_target_mode,
            remote_target,
            selected_lang,
            context_lines,
            file_filter,
            file_encoding_preference,
            max_file_size_mb,
            max_search_hits,
            skip_dirs,
            search_mode,
            plain_text_options,
            cpp_include_dirs,
            type_hints_enabled,
        }
    }

    pub fn is_runnable(&self) -> bool {
        if !self.enabled || self.pattern.trim().is_empty() {
            return false;
        }
        match self.search_target_mode {
            SearchTargetMode::Directory => !self.search_dir.trim().is_empty(),
            mode => self.remote_target.is_remote_ready(mode),
        }
    }

    pub fn effective_search_dir_display(&self) -> String {
        match self.search_target_mode {
            SearchTargetMode::Directory => self.search_dir.clone(),
            mode => {
                let mut s = self.remote_target.url.clone();
                let rev = self.remote_target.ref_or_revision_for(mode);
                if !rev.is_empty() {
                    s.push_str(&format!("@{rev}"));
                }
                if !self.remote_target.subdir.trim().is_empty() {
                    s.push_str(&format!("/{}", self.remote_target.subdir.trim()));
                }
                s
            }
        }
    }
}

/// ファイルに保存するバッチジョブ一覧（YAML で入出力）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BatchJobsFile {
    /// スキーマ版（将来の互換用）
    #[serde(default = "batch_file_version_default")]
    pub version: u32,
    pub jobs: Vec<PatternJob>,
}

fn batch_file_version_default() -> u32 {
    1
}

impl BatchJobsFile {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new(jobs: Vec<PatternJob>) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            jobs,
        }
    }

    /// ID を 1 から振り直し、次に採番すべき ID を返す
    pub fn renumber_job_ids(mut self) -> (Vec<PatternJob>, usize) {
        let mut next = 1usize;
        for j in &mut self.jobs {
            j.id = next;
            next += 1;
        }
        (self.jobs, next)
    }
}

/// バッチジョブ一覧を YAML 文字列にする（手編集しやすい形式）
pub fn batch_jobs_to_yaml_string(jobs: &[PatternJob]) -> anyhow::Result<String> {
    let file = BatchJobsFile::new(jobs.to_vec());
    Ok(serde_yaml::to_string(&file)?)
}

fn parse_batch_jobs_file_str(s: &str) -> anyhow::Result<(Vec<PatternJob>, usize)> {
    let file: BatchJobsFile = serde_yaml::from_str(s)?;
    if file.version > BatchJobsFile::CURRENT_VERSION {
        anyhow::bail!(
            "unsupported batch file version {} (max {})",
            file.version,
            BatchJobsFile::CURRENT_VERSION
        );
    }
    Ok(BatchJobsFile::new(file.jobs).renumber_job_ids())
}

/// パスにバッチ設定を書き出す（拡張子は `.yaml` / `.yml` を推奨）
pub fn write_batch_jobs_file(path: &Path, jobs: &[PatternJob]) -> anyhow::Result<()> {
    let yaml = batch_jobs_to_yaml_string(jobs)?;
    std::fs::write(path, yaml)?;
    Ok(())
}

/// パスからバッチ設定を読み込む（YAML のみ）
pub fn read_batch_jobs_file(path: &Path) -> anyhow::Result<(Vec<PatternJob>, usize)> {
    let s = std::fs::read_to_string(path)?;
    parse_batch_jobs_file_str(&s)
}

/// 1 ジョブ分の実行結果
#[derive(Debug, Clone)]
pub struct BatchRunResult {
    pub job_id: usize,
    pub label: String,
    pub conditions: SearchConditions,
    pub results: Vec<crate::search::FileResult>,
    pub stats: SearchStats,
    pub error: Option<String>,
}

/// バッチ完了後の集約レポート
#[derive(Debug, Clone)]
pub struct BatchReport {
    pub total_elapsed_ms: u64,
    pub runs: Vec<BatchRunResult>,
}

impl BatchReport {
    pub fn total_matches(&self) -> usize {
        self.runs.iter().map(|r| r.stats.total_matches).sum()
    }

    pub fn total_files(&self) -> usize {
        self.runs.iter().map(|r| r.stats.total_files).sum()
    }

    pub fn failed_count(&self) -> usize {
        self.runs.iter().filter(|r| r.error.is_some()).count()
    }
}

/// バッチ実行中の状態（メインスレッド）
pub struct BatchRunnerState {
    pub ordered_indices: Vec<usize>,
    pub active_idx: usize,
    pub runs: Vec<BatchRunResult>,
    pub started: std::time::Instant,
}

/// 1行1パターンのテキストファイルからパターン行を読み込む（空行・ `#` コメントを除外）
pub fn read_patterns_file(path: &Path) -> anyhow::Result<Vec<String>> {
    let s = std::fs::read_to_string(path)?;
    Ok(parse_pattern_lines(&s))
}

/// 文字列からパターン行を抽出
pub fn parse_pattern_lines(s: &str) -> Vec<String> {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// 共通オプションとパターン列からジョブ一覧を作る
pub fn jobs_from_pattern_lines(
    patterns: &[String],
    common: &BatchCommonOptions,
) -> Vec<PatternJob> {
    patterns
        .iter()
        .enumerate()
        .map(|(i, pattern)| {
            let id = i + 1;
            PatternJob {
                id,
                label: format!("pattern-{id}"),
                enabled: true,
                pattern: pattern.clone(),
                search_dir: common.search_dir.clone(),
                search_target_mode: common.search_target_mode,
                remote_target: common.remote_target.clone(),
                selected_lang: common.selected_lang,
                context_lines: common.context_lines,
                file_filter: common.file_filter.clone(),
                file_encoding_preference: common.file_encoding_preference,
                max_file_size_mb: common.max_file_size_mb,
                max_search_hits: common.max_search_hits,
                skip_dirs: common.skip_dirs.clone(),
                search_mode: common.search_mode,
                plain_text_options: common.plain_text_options,
                cpp_include_dirs: common.cpp_include_dirs.clone(),
                type_hints_enabled: common.type_hints_enabled,
            }
        })
        .collect()
}

/// 有効ジョブを逐次実行してバッチレポートを返す（CLI 同期実行用）
pub fn run_batch_sync(
    jobs: &[PatternJob],
    ui_lang: crate::i18n::UiLanguage,
) -> BatchReport {
    let started = std::time::Instant::now();
    let runs: Vec<BatchRunResult> = jobs
        .iter()
        .filter(|j| j.is_runnable())
        .map(|job| {
            crate::search::run_search_sync(
                &job.to_conditions(),
                job.id,
                job.label.clone(),
                ui_lang,
            )
        })
        .collect();
    BatchReport {
        total_elapsed_ms: started.elapsed().as_millis() as u64,
        runs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_encoding::FileEncodingPreference;
    use crate::lang::SupportedLanguage;
    use crate::search::{PlainTextSearchOptions, SearchConditions, SearchMode, SearchStats};
    use crate::search_target::{RemoteTargetConfig, SearchTargetMode};

    fn make_job(id: usize, enabled: bool, pattern: &str, search_dir: &str) -> PatternJob {
        PatternJob {
            id,
            label: format!("job-{id}"),
            enabled,
            pattern: pattern.to_string(),
            search_dir: search_dir.to_string(),
            search_target_mode: SearchTargetMode::default(),
            remote_target: RemoteTargetConfig::default(),
            selected_lang: SupportedLanguage::Rust,
            context_lines: 0,
            file_filter: String::new(),
            file_encoding_preference: FileEncodingPreference::Auto,
            max_file_size_mb: 10,
            max_search_hits: 1000,
            skip_dirs: String::new(),
            search_mode: SearchMode::AstGrep,
            plain_text_options: PlainTextSearchOptions::default(),
            cpp_include_dirs: String::new(),
            type_hints_enabled: crate::search::default_type_hints_enabled(),
        }
    }

    fn make_run_result(matches: usize, files: usize, error: Option<String>) -> BatchRunResult {
        BatchRunResult {
            job_id: 1,
            label: "test".to_string(),
            conditions: SearchConditions {
                search_dir: String::new(),
                search_target_mode: SearchTargetMode::default(),
                remote_target: RemoteTargetConfig::default(),
                pattern: String::new(),
                selected_lang: SupportedLanguage::Rust,
                context_lines: 0,
                file_filter: String::new(),
                file_encoding_preference: FileEncodingPreference::Auto,
                max_file_size_mb: 10,
                max_search_hits: 100,
                skip_dirs: String::new(),
                search_mode: SearchMode::AstGrep,
                plain_text_options: PlainTextSearchOptions::default(),
                cpp_include_dirs: String::new(),
                type_hints_enabled: crate::search::default_type_hints_enabled(),
            },
            results: vec![],
            stats: SearchStats {
                total_matches: matches,
                total_files: files,
                elapsed_ms: 0,
                scanned: 0,
                hit_limit_reached: false,
                ..SearchStats::default()
            },
            error,
        }
    }

    #[test]
    fn is_runnable_enabled_with_content() {
        assert!(make_job(1, true, "fn $NAME()", "/src").is_runnable());
    }

    #[test]
    fn is_runnable_false_when_disabled() {
        assert!(!make_job(1, false, "fn $NAME()", "/src").is_runnable());
    }

    #[test]
    fn is_runnable_false_when_pattern_blank() {
        assert!(!make_job(1, true, "   ", "/src").is_runnable());
    }

    #[test]
    fn is_runnable_true_for_git_remote_url() {
        let mut job = make_job(1, true, "fn main", "");
        job.search_target_mode = SearchTargetMode::GitRemote;
        job.remote_target.url = "https://example.com/repo.git".into();
        assert!(job.is_runnable());
    }

    #[test]
    fn is_runnable_false_when_dir_blank() {
        assert!(!make_job(1, true, "fn $NAME()", "  ").is_runnable());
    }

    #[test]
    fn renumber_ids_assigns_sequential_from_one() {
        let jobs = vec![
            make_job(99, true, "p1", "/a"),
            make_job(42, true, "p2", "/b"),
            make_job(7, true, "p3", "/c"),
        ];
        let (renumbered, next_id) = BatchJobsFile::new(jobs).renumber_job_ids();
        assert_eq!(renumbered[0].id, 1);
        assert_eq!(renumbered[1].id, 2);
        assert_eq!(renumbered[2].id, 3);
        assert_eq!(next_id, 4);
    }

    #[test]
    fn renumber_ids_empty_returns_next_one() {
        let (jobs, next_id) = BatchJobsFile::new(vec![]).renumber_job_ids();
        assert!(jobs.is_empty());
        assert_eq!(next_id, 1);
    }

    #[test]
    fn batch_report_aggregates_totals() {
        let report = BatchReport {
            total_elapsed_ms: 500,
            runs: vec![make_run_result(5, 2, None), make_run_result(3, 1, None)],
        };
        assert_eq!(report.total_matches(), 8);
        assert_eq!(report.total_files(), 3);
        assert_eq!(report.failed_count(), 0);
    }

    #[test]
    fn batch_report_failed_count() {
        let report = BatchReport {
            total_elapsed_ms: 100,
            runs: vec![
                make_run_result(0, 0, Some("error".to_string())),
                make_run_result(5, 1, None),
                make_run_result(0, 0, Some("another error".to_string())),
            ],
        };
        assert_eq!(report.failed_count(), 2);
        assert_eq!(report.total_matches(), 5);
    }

    #[test]
    fn yaml_round_trip_preserves_fields() {
        let jobs = vec![make_job(1, true, "fn $NAME($$$ARGS)", "/my/src")];
        let yaml = batch_jobs_to_yaml_string(&jobs).unwrap();
        let (parsed, next_id) = parse_batch_jobs_file_str(&yaml).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].pattern, "fn $NAME($$$ARGS)");
        assert_eq!(parsed[0].search_dir, "/my/src");
        assert!(parsed[0].enabled);
        assert_eq!(next_id, 2);
    }

    #[test]
    fn yaml_round_trip_multiple_jobs() {
        let jobs = vec![
            make_job(1, true, "pattern1", "/a"),
            make_job(2, false, "pattern2", "/b"),
        ];
        let yaml = batch_jobs_to_yaml_string(&jobs).unwrap();
        let (parsed, next_id) = parse_batch_jobs_file_str(&yaml).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].pattern, "pattern1");
        assert!(!parsed[1].enabled);
        assert_eq!(next_id, 3);
    }

    #[test]
    fn parse_pattern_lines_skips_comments_and_blanks() {
        let text = "# comment\n\nfn $NAME()\nconsole.log($$$ARGS)\n";
        let lines = parse_pattern_lines(text);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "fn $NAME()");
        assert_eq!(lines[1], "console.log($$$ARGS)");
    }

    #[test]
    fn unsupported_version_returns_error() {
        let yaml = "version: 9999\njobs: []\n";
        assert!(parse_batch_jobs_file_str(yaml).is_err());
    }
}
