//! Resumable payload download into `{app_data}/updates/{version}/`.
//!
//! Bytes land in `<file>.part` (HTTP `Range` resume), and only become `<file>`
//! after size and SHA-256 both match the feed. A mismatch deletes the bytes.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tracing::{info, warn};

use super::feed::FeedPlatform;

pub const PROGRESS_EVENT: &str = "self-update://progress";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const IDLE_READ_TIMEOUT: Duration = Duration::from_secs(60);
const RETRY_BACKOFF: [Duration; 3] = [
    Duration::from_secs(2),
    Duration::from_secs(8),
    Duration::from_secs(30),
];
const PROGRESS_THROTTLE: Duration = Duration::from_millis(250);
const STALE_PART_AGE: Duration = Duration::from_secs(7 * 24 * 3600);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub version: String,
    pub downloaded: u64,
    pub total: u64,
}

pub fn updates_root() -> PathBuf {
    crate::paths::app_data_root().join("updates")
}

pub fn version_dir(version: &str) -> PathBuf {
    updates_root().join(version)
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("open for hash: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| format!("hash read: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// `true` when `path` exists with the expected size and digest.
pub fn verify_file(path: &Path, size: u64, sha256: &str) -> Result<bool, String> {
    match fs::metadata(path) {
        Ok(m) if m.is_file() && m.len() == size => {}
        _ => return Ok(false),
    }
    Ok(sha256_file(path)?.eq_ignore_ascii_case(sha256))
}

/// Remove other versions' folders and `.part` files older than a week.
pub fn sweep_stale(keep_version: Option<&str>) {
    let root = updates_root();
    let Ok(entries) = fs::read_dir(&root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if Some(name.as_str()) != keep_version {
            let _ = fs::remove_dir_all(&path);
            continue;
        }
        if let Ok(files) = fs::read_dir(&path) {
            for f in files.flatten() {
                let p = f.path();
                let old = f
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age > STALE_PART_AGE);
                if old && p.extension().is_some_and(|e| e == "part") {
                    let _ = fs::remove_file(p);
                }
            }
        }
    }
}

fn http_client() -> Result<reqwest::Client, String> {
    crate::proxy::apply_to_reqwest(reqwest::Client::builder())
        .connect_timeout(CONNECT_TIMEOUT)
        .user_agent(format!("AtlasBuddy/{}", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())
}

pub async fn fetch_feed(feed_url: &str) -> Result<super::feed::Feed, String> {
    let client = http_client()?;
    let resp = tokio::time::timeout(
        IDLE_READ_TIMEOUT,
        client
            .get(feed_url)
            .header("Cache-Control", "no-cache")
            .send(),
    )
    .await
    .map_err(|_| "network: update feed timed out".to_string())?
    .map_err(|e| format!("network: {e}"))?;
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err("feed_not_published".into());
    }
    if !status.is_success() {
        return Err(format!("network: update feed HTTP {}", status.as_u16()));
    }
    let body = tokio::time::timeout(IDLE_READ_TIMEOUT, resp.text())
        .await
        .map_err(|_| "network: update feed timed out".to_string())?
        .map_err(|e| format!("network: {e}"))?;
    serde_json::from_str(&body).map_err(|e| format!("invalid update feed: {e}"))
}

fn free_space_ok(dir: &Path, need: u64) -> bool {
    fs2::available_space(dir).map_or(true, |free| free >= need)
}

/// Download (or reuse) the payload. Returns the verified file path.
pub async fn download_payload(
    app: &AppHandle,
    version: &str,
    file_name: &str,
    platform: &FeedPlatform,
) -> Result<PathBuf, String> {
    let dir = version_dir(version);
    fs::create_dir_all(&dir).map_err(|e| format!("create update dir: {e}"))?;
    let final_path = dir.join(file_name);
    if verify_file(&final_path, platform.size, &platform.sha256)? {
        emit(app, version, platform.size, platform.size, &mut None);
        return Ok(final_path);
    }
    let _ = fs::remove_file(&final_path);
    if !free_space_ok(&dir, platform.size.saturating_mul(2)) {
        return Err("disk_space".into());
    }

    let part = dir.join(format!("{file_name}.part"));
    let client = http_client()?;
    let mut attempt = 0usize;
    loop {
        match fetch_into_part(app, &client, version, platform, &part).await {
            Ok(()) => break,
            Err(e) if attempt < RETRY_BACKOFF.len() && !e.starts_with("checksum") => {
                warn!(target: "grok_app::self_update", error = %e, attempt, "download retry");
                tokio::time::sleep(RETRY_BACKOFF[attempt]).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }

    let part_len = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if part_len != platform.size {
        let _ = fs::remove_file(&part);
        return Err(format!("checksum: size {part_len} != {}", platform.size));
    }
    let got = sha256_file(&part)?;
    if !got.eq_ignore_ascii_case(&platform.sha256) {
        let _ = fs::remove_file(&part);
        return Err("checksum: sha256 mismatch".into());
    }
    fs::rename(&part, &final_path).map_err(|e| format!("stage payload: {e}"))?;
    info!(target: "grok_app::self_update", %version, "payload verified");
    Ok(final_path)
}

async fn fetch_into_part(
    app: &AppHandle,
    client: &reqwest::Client,
    version: &str,
    platform: &FeedPlatform,
    part: &Path,
) -> Result<(), String> {
    let mut have = fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    if have > platform.size {
        let _ = fs::remove_file(part);
        have = 0;
    }
    if have == platform.size {
        return Ok(());
    }

    let mut req = client.get(&platform.url);
    if have > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let resp = tokio::time::timeout(IDLE_READ_TIMEOUT, req.send())
        .await
        .map_err(|_| "network: download timed out".to_string())?
        .map_err(|e| format!("network: {e}"))?;
    let status = resp.status();
    let mut file = if status == reqwest::StatusCode::PARTIAL_CONTENT && have > 0 {
        fs::OpenOptions::new()
            .append(true)
            .open(part)
            .map_err(|e| format!("open part: {e}"))?
    } else if status.is_success() {
        have = 0;
        fs::File::create(part).map_err(|e| format!("create part: {e}"))?
    } else if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        let _ = fs::remove_file(part);
        return Err("network: range rejected, restarting".into());
    } else {
        return Err(format!("network: download HTTP {}", status.as_u16()));
    };

    let mut stream = resp.bytes_stream();
    let mut last_emit: Option<Instant> = None;
    loop {
        let next = tokio::time::timeout(IDLE_READ_TIMEOUT, stream.next())
            .await
            .map_err(|_| "network: download stalled".to_string())?;
        let Some(chunk) = next else {
            break;
        };
        let chunk = chunk.map_err(|e| format!("network: {e}"))?;
        have += chunk.len() as u64;
        if have > platform.size {
            drop(file);
            let _ = fs::remove_file(part);
            return Err("checksum: payload larger than manifest size".into());
        }
        file.write_all(&chunk)
            .map_err(|e| format!("write part: {e}"))?;
        emit(app, version, have, platform.size, &mut last_emit);
    }
    file.flush().map_err(|e| format!("flush part: {e}"))?;
    emit(app, version, have, platform.size, &mut None);
    if have < platform.size {
        return Err("network: connection closed early".into());
    }
    Ok(())
}

fn emit(app: &AppHandle, version: &str, downloaded: u64, total: u64, last: &mut Option<Instant>) {
    if let Some(t) = last {
        if t.elapsed() < PROGRESS_THROTTLE {
            return;
        }
    }
    *last = Some(Instant::now());
    let _ = app.emit(
        PROGRESS_EVENT,
        Progress {
            version: version.to_string(),
            downloaded,
            total,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_file_checks_size_and_digest() {
        let dir = std::env::temp_dir().join(format!("self-update-verify-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("blob.bin");
        fs::write(&path, b"abc").unwrap();
        // echo -n abc | sha256sum
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify_file(&path, 3, abc).unwrap());
        assert!(verify_file(&path, 3, &abc.to_uppercase()).unwrap());
        assert!(!verify_file(&path, 4, abc).unwrap());
        assert!(!verify_file(&path, 3, &"0".repeat(64)).unwrap());
        assert!(!verify_file(&dir.join("missing"), 3, abc).unwrap());
        let _ = fs::remove_dir_all(&dir);
    }
}
