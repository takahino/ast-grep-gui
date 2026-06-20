//! 検索対象（ローカルディレクトリ / Git / SVN リモートURL）の設定型

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 検索対象の種別
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SearchTargetMode {
    /// ローカルディレクトリ
    #[default]
    Directory,
    /// Git リモート URL（未 clone でも可）
    GitRemote,
    /// SVN リモート URL（未 checkout でも可）
    SvnRemote,
}

impl SearchTargetMode {
    pub fn label(self, lang: crate::i18n::UiLanguage) -> &'static str {
        match (self, lang) {
            (Self::Directory, crate::i18n::UiLanguage::Japanese) => "ローカル",
            (Self::Directory, crate::i18n::UiLanguage::English) => "Local",
            (Self::GitRemote, crate::i18n::UiLanguage::Japanese) => "Git URL",
            (Self::GitRemote, crate::i18n::UiLanguage::English) => "Git URL",
            (Self::SvnRemote, crate::i18n::UiLanguage::Japanese) => "SVN URL",
            (Self::SvnRemote, crate::i18n::UiLanguage::English) => "SVN URL",
        }
    }
}

/// リモート取得のキャッシュ方針
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RemoteCachePolicy {
    /// キャッシュがあれば再利用
    #[default]
    UseCache,
    /// 次回検索時に再取得
    RefreshNext,
}

/// リモート VCS 検索の永続化可能な設定
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RemoteTargetConfig {
    pub url: String,
    /// Git: branch / tag / commit（空なら clone 時のデフォルト）
    #[serde(default)]
    pub git_ref: String,
    /// SVN: revision（空なら HEAD）
    #[serde(default)]
    pub svn_revision: String,
    /// リポジトリルートからの相対サブディレクトリ（空ならルート全体）
    #[serde(default)]
    pub subdir: String,
    #[serde(default)]
    pub cache_policy: RemoteCachePolicy,
}

impl RemoteTargetConfig {
    pub fn ref_or_revision_for(&self, mode: SearchTargetMode) -> String {
        match mode {
            SearchTargetMode::GitRemote => self.git_ref.trim().to_string(),
            SearchTargetMode::SvnRemote => self.svn_revision.trim().to_string(),
            SearchTargetMode::Directory => String::new(),
        }
    }

    pub fn is_remote_ready(&self, mode: SearchTargetMode) -> bool {
        mode != SearchTargetMode::Directory && !self.url.trim().is_empty()
    }
}

/// リモート取得リクエスト（実行時）
#[derive(Debug, Clone)]
pub struct RemoteFetchRequest {
    pub mode: SearchTargetMode,
    pub url: String,
    pub ref_or_revision: String,
    pub subdir: String,
    pub force_refresh: bool,
}

/// キャッシュ上のパスにサブディレクトリを適用する
pub fn resolve_subdir_path(repo_root: &Path, subdir: &str) -> Result<PathBuf, String> {
    let trimmed = subdir.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Ok(repo_root.to_path_buf());
    }
    let joined = repo_root.join(trimmed);
    if !joined.is_dir() {
        return Err(format!(
            "subdirectory not found in fetched tree: {}",
            trimmed
        ));
    }
    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn resolve_subdir_empty() {
        let dir = std::env::temp_dir().join("ast_grep_gui_subdir_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = resolve_subdir_path(&dir, "").unwrap();
        assert_eq!(p, dir);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_subdir_nested() {
        let dir = std::env::temp_dir().join("ast_grep_gui_subdir_nested");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        let p = resolve_subdir_path(&dir, "src").unwrap();
        assert!(p.ends_with("src"));
        let _ = fs::remove_dir_all(&dir);
    }
}
