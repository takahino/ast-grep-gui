//! リモート VCS 取得のオーケストレーション（バックグラウンドスレッド）

use std::path::PathBuf;

use crossbeam_channel::{Receiver, Sender};

use crate::git_remote;
use crate::search_target::{
    resolve_subdir_path, RemoteCachePolicy, RemoteFetchRequest, SearchTargetMode,
};
use crate::svn_remote;
use crate::vcs_cache::{cache_dir_for, cache_is_ready, invalidate_cache, mark_cache_ready, VcsCacheSpec};

/// リモート取得メッセージ
#[derive(Debug)]
pub enum RemoteFetchMessage {
    Progress(String),
    Done { local_path: PathBuf },
    Error(String),
}

pub fn remote_fetch_channel() -> (Sender<RemoteFetchMessage>, Receiver<RemoteFetchMessage>) {
    crossbeam_channel::unbounded()
}

/// バックグラウンドでリモートを取得し、検索可能なローカルパスを返す
pub fn spawn_remote_fetch(req: RemoteFetchRequest, tx: Sender<RemoteFetchMessage>) {
    std::thread::spawn(move || {
        let send = |msg| {
            let _ = tx.send(msg);
        };

        let spec = VcsCacheSpec {
            mode: req.mode,
            url: req.url.clone(),
            ref_or_revision: req.ref_or_revision.clone(),
        };
        let cache_dir = cache_dir_for(&spec);

        let use_cache = !req.force_refresh && cache_is_ready(&cache_dir);
        if !use_cache {
            if req.force_refresh {
                let _ = invalidate_cache(&cache_dir);
            }
            send(RemoteFetchMessage::Progress(format!(
                "fetching {} …",
                req.url
            )));
            let fetch_result = match req.mode {
                SearchTargetMode::GitRemote => {
                    git_remote::fetch_git_remote(&req.url, &cache_dir, &req.ref_or_revision)
                }
                SearchTargetMode::SvnRemote => {
                    svn_remote::fetch_svn_remote(&req.url, &cache_dir, &req.ref_or_revision)
                }
                SearchTargetMode::Directory => {
                    Err(anyhow::anyhow!("not a remote target"))
                }
            };
            if let Err(e) = fetch_result {
                send(RemoteFetchMessage::Error(e.to_string()));
                return;
            }
            if let Err(e) = mark_cache_ready(&cache_dir) {
                send(RemoteFetchMessage::Error(e.to_string()));
                return;
            }
        } else {
            send(RemoteFetchMessage::Progress("using cached copy".into()));
        }

        match resolve_subdir_path(&cache_dir, &req.subdir) {
            Ok(p) => send(RemoteFetchMessage::Done { local_path: p }),
            Err(e) => send(RemoteFetchMessage::Error(e)),
        }
    });
}

/// 検索条件から実際に走査するローカルディレクトリを解決する（CLI 同期実行用）
pub fn resolve_search_dir_from_conditions(
    cond: &crate::search::SearchConditions,
    force_refresh: bool,
) -> Result<std::path::PathBuf, String> {
    if cond.search_target_mode == SearchTargetMode::Directory {
        let dir = cond.search_dir.trim();
        if dir.is_empty() {
            return Err("search directory is empty".into());
        }
        return Ok(std::path::PathBuf::from(dir));
    }

    let req = RemoteFetchRequest {
        mode: cond.search_target_mode,
        url: cond.remote_target.url.clone(),
        ref_or_revision: cond
            .remote_target
            .ref_or_revision_for(cond.search_target_mode),
        subdir: cond.remote_target.subdir.clone(),
        force_refresh: force_refresh
            || cond.remote_target.cache_policy == RemoteCachePolicy::RefreshNext,
    };

    let spec = VcsCacheSpec {
        mode: req.mode,
        url: req.url.clone(),
        ref_or_revision: req.ref_or_revision.clone(),
    };
    let cache_dir = cache_dir_for(&spec);
    let use_cache = !req.force_refresh && cache_is_ready(&cache_dir);
    if !use_cache {
        if req.force_refresh {
            let _ = invalidate_cache(&cache_dir);
        }
        let fetch_result = match req.mode {
            SearchTargetMode::GitRemote => {
                git_remote::fetch_git_remote(&req.url, &cache_dir, &req.ref_or_revision)
            }
            SearchTargetMode::SvnRemote => {
                svn_remote::fetch_svn_remote(&req.url, &cache_dir, &req.ref_or_revision)
            }
            SearchTargetMode::Directory => Err(anyhow::anyhow!("not remote")),
        };
        fetch_result.map_err(|e| e.to_string())?;
        mark_cache_ready(&cache_dir).map_err(|e| e.to_string())?;
    }
    resolve_subdir_path(&cache_dir, &req.subdir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search_target::SearchTargetMode;

    #[test]
    fn cache_spec_key_differs_by_url() {
        let a = VcsCacheSpec {
            mode: SearchTargetMode::GitRemote,
            url: "https://a.git".into(),
            ref_or_revision: String::new(),
        };
        let b = VcsCacheSpec {
            url: "https://b.git".into(),
            ..a.clone()
        };
        assert_ne!(a.cache_key(), b.cache_key());
    }
}
