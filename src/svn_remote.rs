//! SVN リモート URL の取得（外部 `svn` コマンド非依存）

use std::path::Path;

use anyhow::{anyhow, Context, Result};

/// キャッシュディレクトリへ指定 revision のツリーを export する
pub fn fetch_svn_remote(url: &str, cache_dir: &Path, revision: &str) -> Result<()> {
    let url = url.trim();
    if url.is_empty() {
        anyhow::bail!("SVN URL is empty");
    }

    if cache_dir.exists() {
        std::fs::remove_dir_all(cache_dir).context("clear svn cache")?;
    }
    std::fs::create_dir_all(cache_dir)?;

    let lower = url.to_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        export_http_svn(url, cache_dir, revision)?;
    } else if lower.starts_with("svn://")
        || lower.starts_with("svn+ssh://")
        || lower.starts_with("svn+")
    {
        export_ra_svn(url, cache_dir, revision)?;
    } else {
        anyhow::bail!("unsupported SVN URL scheme: {url}");
    }

    Ok(())
}

fn parse_revision(revision: &str) -> Option<u64> {
    let t = revision.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("head") {
        return None;
    }
    t.parse().ok()
}

/// `svn://` / `svn+ssh://` — `svn` crate の export API
fn export_ra_svn(url: &str, cache_dir: &Path, revision: &str) -> Result<()> {
    let url = url.to_string();
    let cache = cache_dir.to_path_buf();
    let rev = parse_revision(revision);
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("tokio runtime")?;
        rt.block_on(async { export_ra_svn_async(&url, &cache, rev).await })
    })
    .join()
    .map_err(|_| anyhow!("svn export thread panicked"))?
}

async fn export_ra_svn_async(url: &str, cache_dir: &Path, revision: Option<u64>) -> Result<()> {
    use svn::{Depth, RaSvnClient, SvnUrl, UpdateOptions};

    let parsed = SvnUrl::parse(url).map_err(|e| anyhow!("invalid SVN URL: {e}"))?;
    let client = RaSvnClient::new(parsed, None, None);
    let mut options = UpdateOptions::new("", Depth::Infinity);
    options.rev = revision;
    client.export_to_dir(&options, cache_dir).await?;
    Ok(())
}

/// HTTP(S) SVN — WebDAV PROPFIND + GET で export
fn export_http_svn(url: &str, cache_dir: &Path, revision: &str) -> Result<()> {
    let rev = parse_revision(revision);
    let base = url.trim_end_matches('/');
    let client = reqwest::blocking::Client::builder()
        .user_agent("ast-grep-gui")
        .build()
        .context("http client")?;

    let target_rev = match rev {
        Some(r) => r,
        None => discover_head_revision(&client, base)?,
    };

    export_http_dir(&client, base, "", cache_dir, target_rev)
}

fn discover_head_revision(client: &reqwest::blocking::Client, base: &str) -> Result<u64> {
    let resp = client
        .request(
            reqwest::Method::from_bytes(b"PROPFIND").unwrap(),
            base,
        )
        .header("Depth", "0")
        .header("Content-Type", "text/xml; charset=utf-8")
        .body(propfind_body())
        .send()
        .context("PROPFIND")?;
    let text = resp.text().context("PROPFIND body")?;
    parse_version_from_propfind(&text).ok_or_else(|| anyhow!("could not determine SVN HEAD revision"))
}

fn propfind_body() -> &'static str {
    r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:" xmlns:S="svn:">
  <D:prop>
    <D:resourcetype/>
    <D:getcontentlength/>
    <S:version-name/>
  </D:prop>
</D:propfind>"#
}

fn parse_version_from_propfind(xml: &str) -> Option<u64> {
    for needle in ["version-name>", "S:version-name>"] {
        if let Some(pos) = xml.find(needle) {
            let rest = &xml[pos + needle.len()..];
            if let Some(end) = rest.find('<') {
                let s = rest[..end].trim();
                if let Ok(n) = s.parse::<u64>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

fn export_http_dir(
    client: &reqwest::blocking::Client,
    base: &str,
    subpath: &str,
    local_dir: &Path,
    rev: u64,
) -> Result<()> {
    std::fs::create_dir_all(local_dir)?;
    let url = if subpath.is_empty() {
        base.to_string()
    } else {
        format!(
            "{}/{}",
            base.trim_end_matches('/'),
            subpath.trim_start_matches('/')
        )
    };

    let resp = client
        .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &url)
        .header("Depth", "1")
        .header("Content-Type", "text/xml; charset=utf-8")
        .body(propfind_body())
        .send()
        .context("PROPFIND list")?;
    let text = resp.text().context("PROPFIND response")?;
    let entries = parse_propfind_entries(&text, &url)?;

    for entry in entries {
        if entry.is_dir {
            let name = entry.name.trim_end_matches('/');
            if name.is_empty() || name == "." {
                continue;
            }
            let child_sub = if subpath.is_empty() {
                name.to_string()
            } else {
                format!("{}/{}", subpath.trim_end_matches('/'), name)
            };
            export_http_dir(client, base, &child_sub, &local_dir.join(name), rev)?;
        } else if entry.is_file {
            let name = &entry.name;
            if name.is_empty() {
                continue;
            }
            let file_url = if subpath.is_empty() {
                format!("{}/{}?p={}", base.trim_end_matches('/'), name, rev)
            } else {
                format!(
                    "{}/{}/{}?p={}",
                    base.trim_end_matches('/'),
                    subpath.trim_start_matches('/'),
                    name,
                    rev
                )
            };
            let data = client.get(&file_url).send().context("GET file")?.bytes()?;
            std::fs::write(local_dir.join(name), data)?;
        }
    }
    Ok(())
}

struct DavEntry {
    name: String,
    is_dir: bool,
    is_file: bool,
}

fn parse_propfind_entries(xml: &str, base_url: &str) -> Result<Vec<DavEntry>> {
    let href_re = regex::Regex::new(r"<(?:D:|d:)?href[^>]*>([^<]+)</").unwrap();
    let mut entries = Vec::new();
    let base_path = reqwest::Url::parse(base_url)
        .ok()
        .map(|u| u.path().trim_end_matches('/').to_string())
        .unwrap_or_default();

    for cap in href_re.captures_iter(xml) {
        let href = cap.get(1).unwrap().as_str().trim().to_string();
        if href.is_empty() {
            continue;
        }
        let name = href_to_name(&href, &base_path);
        if name.is_empty() || name == "." {
            continue;
        }
        let window = xml
            .find(&href)
            .map(|p| &xml[p.saturating_sub(80)..(p + href.len() + 200).min(xml.len())])
            .unwrap_or(xml);
        let is_dir = window.contains("collection") || href.ends_with('/');
        entries.push(DavEntry {
            name: name.clone(),
            is_dir,
            is_file: !is_dir && window.contains("getcontentlength"),
        });
    }
    Ok(entries)
}

fn href_to_name(href: &str, base_path: &str) -> String {
    let p = href.trim_end_matches('/');
    if let Some(rest) = p.strip_prefix(base_path) {
        return rest
            .trim_start_matches('/')
            .split('/')
            .next_back()
            .unwrap_or("")
            .to_string();
    }
    p.split('/').next_back().unwrap_or("").to_string()
}
