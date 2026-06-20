//! VCS リモート取得結果のローカルキャッシュ管理

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::search_target::SearchTargetMode;

/// キャッシュキー生成用の仕様
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VcsCacheSpec {
    pub mode: SearchTargetMode,
    pub url: String,
    pub ref_or_revision: String,
}

impl VcsCacheSpec {
    pub fn cache_key(&self) -> String {
        let mut hasher = DefaultHasher::new();
        format!("{:?}", self.mode).hash(&mut hasher);
        self.url.trim().hash(&mut hasher);
        self.ref_or_revision.trim().hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }
}

/// アプリ用キャッシュのルート（`%LOCALAPPDATA%/ast-grep-gui/vcs-cache` 等）
pub fn cache_root_dir() -> PathBuf {
    if let Some(base) = dirs_fallback() {
        return base.join("ast-grep-gui").join("vcs-cache");
    }
    std::env::temp_dir().join("ast-grep-gui-vcs-cache")
}

fn dirs_fallback() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache"))
            })
    }
}

/// 仕様に対応するキャッシュディレクトリ（存在しなくても返す）
pub fn cache_dir_for(spec: &VcsCacheSpec) -> PathBuf {
    let kind = match spec.mode {
        SearchTargetMode::GitRemote => "git",
        SearchTargetMode::SvnRemote => "svn",
        SearchTargetMode::Directory => "local",
    };
    cache_root_dir()
        .join(kind)
        .join(spec.cache_key())
}

/// キャッシュが利用可能か（`.ready` マーカーと中身があること）
pub fn cache_is_ready(path: &Path) -> bool {
    path.join(".ready").is_file() && path.is_dir()
}

/// 取得完了をマークする
pub fn mark_cache_ready(path: &Path) -> std::io::Result<()> {
    std::fs::write(path.join(".ready"), b"ok")
}

/// キャッシュを削除する
pub fn invalidate_cache(path: &Path) -> std::io::Result<()> {
    if path.exists() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_stable() {
        let a = VcsCacheSpec {
            mode: SearchTargetMode::GitRemote,
            url: "https://example.com/repo.git".into(),
            ref_or_revision: "main".into(),
        };
        let b = a.clone();
        assert_eq!(a.cache_key(), b.cache_key());
        assert_ne!(
            a.cache_key(),
            VcsCacheSpec {
                ref_or_revision: "dev".into(),
                ..a
            }
            .cache_key()
        );
    }
}
