//! 複数パターンのバッチ検索用のデータモデル

use std::path::Path;

use crate::file_encoding::FileEncodingPreference;
use crate::lang::SupportedLanguage;
use crate::search::{SearchConditions, SearchMode, SearchStats};

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
