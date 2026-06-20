//! Git リモート URL の取得（外部 `git` コマンド非依存、`gix` 使用）

use std::path::Path;
use std::sync::atomic::AtomicBool;

use anyhow::{Context, Result};

static INTERRUPT: AtomicBool = AtomicBool::new(false);

/// キャッシュディレクトリへ clone し、必要なら ref を checkout する
pub fn fetch_git_remote(url: &str, cache_dir: &Path, git_ref: &str) -> Result<()> {
    let url = url.trim();
    if url.is_empty() {
        anyhow::bail!("Git URL is empty");
    }

    if cache_dir.exists() {
        std::fs::remove_dir_all(cache_dir).context("clear git cache")?;
    }
    std::fs::create_dir_all(cache_dir)?;

    let _ = unsafe { gix::interrupt::init_handler(1, || {}) };

    let parsed = gix::url::parse(url.into()).context("invalid Git URL")?;
    let mut prepare_clone = gix::prepare_clone(parsed, cache_dir).context("prepare clone")?;

    let git_ref = git_ref.trim();
    if !git_ref.is_empty() {
        prepare_clone = prepare_clone
            .with_ref_name(Some(git_ref))
            .context("invalid git ref name")?;
    }

    let (mut prepare_checkout, _) = prepare_clone
        .fetch_then_checkout(gix::progress::Discard, &INTERRUPT)
        .context("fetch then checkout")?;
    let (_repo, _) = prepare_checkout
        .main_worktree(gix::progress::Discard, &INTERRUPT)
        .context("checkout main worktree")?;

    Ok(())
}
