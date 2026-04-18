//! 複数パターンのバッチ検索用のデータモデル

use std::path::Path;

use crate::file_encoding::FileEncodingPreference;
use crate::lang::SupportedLanguage;
use crate::search::{PlainTextSearchOptions, SearchConditions, SearchMode, SearchStats};

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
}

impl PatternJob {
    pub fn to_conditions(&self) -> SearchConditions {
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
            plain_text_options: self.plain_text_options,
            cpp_include_dirs: self.cpp_include_dirs.clone(),
        }
    }

    /// メイン画面の現在設定から新規ジョブを作る（`id` は呼び出し側で設定）
    pub fn from_app_snapshot(
        id: usize,
        label: String,
        pattern: String,
        search_dir: String,
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
    ) -> Self {
        Self {
            id,
            label,
            enabled: true,
            pattern,
            search_dir,
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
        }
    }

    pub fn is_runnable(&self) -> bool {
        self.enabled
            && !self.pattern.trim().is_empty()
            && !self.search_dir.trim().is_empty()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_encoding::FileEncodingPreference;
    use crate::lang::SupportedLanguage;
    use crate::search::{PlainTextSearchOptions, SearchConditions, SearchMode, SearchStats};

    fn make_job(id: usize, enabled: bool, pattern: &str, search_dir: &str) -> PatternJob {
        PatternJob {
            id,
            label: format!("job-{id}"),
            enabled,
            pattern: pattern.to_string(),
            search_dir: search_dir.to_string(),
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
        }
    }

    fn make_run_result(matches: usize, files: usize, error: Option<String>) -> BatchRunResult {
        BatchRunResult {
            job_id: 1,
            label: "test".to_string(),
            conditions: SearchConditions {
                search_dir: String::new(),
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
            },
            results: vec![],
            stats: SearchStats {
                total_matches: matches,
                total_files: files,
                elapsed_ms: 0,
                scanned: 0,
                hit_limit_reached: false,
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
    fn unsupported_version_returns_error() {
        let yaml = "version: 9999\njobs: []\n";
        assert!(parse_batch_jobs_file_str(yaml).is_err());
    }
}
