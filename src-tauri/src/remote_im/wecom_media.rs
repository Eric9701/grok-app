//! Download and decrypt WeCom long-connection media into `{work_dir}/.remote`.

use super::control_plane::PendingAttachment;
use super::outbound::http_client;
use super::types::InboundMediaRef;
use aes::Aes256;
use base64::Engine;
#[cfg(test)]
use cbc::cipher::BlockEncryptMut;
use cbc::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
use cbc::Decryptor;
#[cfg(test)]
use cbc::Encryptor;
use futures_util::StreamExt;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

const IMAGE_OR_VIDEO_MAX: usize = 10 * 1024 * 1024;
const FILE_MAX: usize = 20 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(20);

pub fn max_bytes(kind: &str) -> usize {
    match kind {
        "image" | "video" => IMAGE_OR_VIDEO_MAX,
        _ => FILE_MAX,
    }
}

pub fn media_url_allowed(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "https" || !parsed.username().is_empty() || parsed.password().is_some() {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    weixin_host(&host) || wecom_cos_host(&host)
}

fn weixin_host(host: &str) -> bool {
    host == "weixin.qq.com" || host.ends_with(".weixin.qq.com")
}

/// 智能机器人文件实际落在腾讯云 COS：`ww-aibot-*.cos.<region>.myqcloud.com`。
/// 只放行这个桶名前缀，不放行任意 `*.myqcloud.com`。
fn wecom_cos_host(host: &str) -> bool {
    let Some(rest) = host.strip_suffix(".myqcloud.com") else {
        return false;
    };
    let Some((bucket, region)) = rest.rsplit_once(".cos.") else {
        return false;
    };
    !region.is_empty()
        && !region.contains('.')
        && dns_label(region)
        && bucket.starts_with("ww-aibot-")
        && bucket.len() > "ww-aibot-".len()
        && dns_label(bucket)
}

fn dns_label(value: &str) -> bool {
    value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !value.starts_with('-')
        && !value.ends_with('-')
}

pub fn media_host(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
        .unwrap_or_default()
}

pub fn decode_aes_key(raw: &str) -> Result<[u8; 32], String> {
    let raw = raw.trim();
    if raw.len() == 32 {
        let mut key = [0u8; 32];
        key.copy_from_slice(raw.as_bytes());
        return Ok(key);
    }
    let mut padded = raw.to_string();
    while !padded.len().is_multiple_of(4) {
        padded.push('=');
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(padded.as_bytes())
        .map_err(|_| "wecom media: aes key".to_string())?;
    if decoded.len() != 32 {
        return Err("wecom media: aes key length".into());
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&decoded);
    Ok(key)
}

/// AES-256-CBC. IV is the first 16 bytes of the key. PKCS#7 padding is to 32 bytes.
pub fn decrypt_media(key: &[u8; 32], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    if ciphertext.is_empty() || !ciphertext.len().is_multiple_of(16) {
        return Err("wecom media: ciphertext length".into());
    }
    let mut iv = [0u8; 16];
    iv.copy_from_slice(&key[..16]);
    let decryptor = Decryptor::<Aes256>::new(key.into(), &iv.into());
    let mut buf = ciphertext.to_vec();
    let plain = decryptor
        .decrypt_padded_mut::<NoPadding>(&mut buf)
        .map_err(|_| "wecom media: decrypt".to_string())?;
    unpad_pkcs7_32(plain)
}

fn unpad_pkcs7_32(data: &[u8]) -> Result<Vec<u8>, String> {
    let n = *data.last().ok_or("wecom media: empty")? as usize;
    if n == 0 || n > 32 || data.len() < n {
        return Err("wecom media: padding".into());
    }
    if data[data.len() - n..].iter().any(|b| *b as usize != n) {
        return Err("wecom media: padding".into());
    }
    Ok(data[..data.len() - n].to_vec())
}

pub fn push_capped(buf: &mut Vec<u8>, chunk: &[u8], max: usize) -> Result<(), ()> {
    if buf.len().saturating_add(chunk.len()) > max {
        return Err(());
    }
    buf.extend_from_slice(chunk);
    Ok(())
}

pub fn sanitize_filename(raw: &str, kind: &str) -> String {
    let base = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(raw)
        .trim()
        .trim_matches('.');
    let mut out = String::new();
    for c in base.chars() {
        if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
            out.push('_');
        } else {
            out.push(c);
        }
    }
    let out = out.trim().trim_matches('.').to_string();
    if out.is_empty() || out == "." || out == ".." {
        return default_filename(kind);
    }
    out.chars().take(120).collect()
}

fn default_filename(kind: &str) -> String {
    match kind {
        "image" => "image.jpg".into(),
        "video" => "video.mp4".into(),
        _ => "file.bin".into(),
    }
}

fn sanitize_segment(raw: &str) -> String {
    let out: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(80)
        .collect();
    if out.is_empty() {
        "msg".into()
    } else {
        out
    }
}

fn path_stays_under(path: &Path, root: &Path) -> bool {
    let mut acc = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => return false,
            Component::CurDir => {}
            other => acc.push(other),
        }
    }
    acc.starts_with(root)
}

pub fn inbound_file_path(
    work_dir: &Path,
    message_id: &str,
    filename: &str,
    kind: &str,
) -> Result<PathBuf, String> {
    let remote = work_dir.join(".remote");
    if remote.is_file() {
        return Err("wecom media: .remote is a file".into());
    }
    let dest = remote
        .join(sanitize_segment(message_id))
        .join(sanitize_filename(filename, kind));
    if !path_stays_under(&dest, &remote) {
        return Err("wecom media: path escaped".into());
    }
    Ok(dest)
}

fn filename_from_content_disposition(value: &str) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    if let Some(idx) = lower.find("filename*") {
        let rest = value[idx + "filename*".len()..].trim_start_matches(['=', ' ']);
        let token = rest.split(';').next()?.trim().trim_matches('"');
        let encoded = token.split("''").nth(1).unwrap_or(token);
        let decoded = percent_encoding::percent_decode_str(encoded)
            .decode_utf8_lossy()
            .to_string();
        let name = sanitize_filename(&decoded, "file");
        if name != "file.bin" || decoded.contains('.') {
            return Some(name);
        }
    }
    let marker = "filename=";
    let idx = lower.find(marker)?;
    if idx > 0 && lower.as_bytes().get(idx - 1) == Some(&b'*') {
        return None;
    }
    let rest = value[idx + marker.len()..].trim_start();
    let token = if let Some(stripped) = rest.strip_prefix('"') {
        stripped.split('"').next()?
    } else {
        rest.split(';').next()?.trim()
    };
    let name = sanitize_filename(token, "file");
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

pub async fn materialize(
    work_dir: &Path,
    message_id: &str,
    refs: &[InboundMediaRef],
) -> Vec<PendingAttachment> {
    let mut saved = Vec::new();
    for media in refs {
        match save_one(work_dir, message_id, media).await {
            Ok(attachment) => saved.push(attachment),
            Err(err) => {
                tracing::warn!(
                    host = %media_host(&media.url),
                    kind = %media.kind,
                    error = %err,
                    "wecom media: skipped"
                );
            }
        }
    }
    saved
}

async fn save_one(
    work_dir: &Path,
    message_id: &str,
    media: &InboundMediaRef,
) -> Result<PendingAttachment, String> {
    if !media_url_allowed(&media.url) {
        return Err("wecom media: host".into());
    }
    let key = decode_aes_key(media.aes_key.expose())?;
    let (bytes, header_name) = download_capped(&media.url, max_bytes(&media.kind)).await?;
    let plain = decrypt_media(&key, &bytes)?;
    let filename = header_name
        .or_else(|| media.filename.clone())
        .unwrap_or_else(|| default_filename(&media.kind));
    let path = write_inbound(work_dir, message_id, &filename, &media.kind, &plain)?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&filename)
        .to_string();
    Ok(PendingAttachment {
        path: path.to_string_lossy().to_string(),
        name,
    })
}

async fn download_capped(url: &str, max: usize) -> Result<(Vec<u8>, Option<String>), String> {
    let client = http_client()?;
    let response = client
        .get(url)
        .timeout(DOWNLOAD_TIMEOUT)
        .send()
        .await
        .map_err(|_| "wecom media: download".to_string())?;
    if !response.status().is_success() {
        return Err("wecom media: download status".into());
    }
    if !media_url_allowed(response.url().as_str()) {
        return Err("wecom media: redirect host".into());
    }
    if response
        .content_length()
        .is_some_and(|len| len > max as u64)
    {
        return Err("wecom media: too large".into());
    }
    let header_name = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .and_then(filename_from_content_disposition);
    let mut buf = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "wecom media: download".to_string())?;
        push_capped(&mut buf, &chunk, max).map_err(|_| "wecom media: too large".to_string())?;
    }
    Ok((buf, header_name))
}

fn write_inbound(
    work_dir: &Path,
    message_id: &str,
    filename: &str,
    kind: &str,
    bytes: &[u8],
) -> Result<PathBuf, String> {
    let dest = inbound_file_path(work_dir, message_id, filename, kind)?;
    let remote = work_dir.join(".remote");
    std::fs::create_dir_all(&remote).map_err(|e| e.to_string())?;
    let canon_root = work_dir
        .canonicalize()
        .map_err(|_| "wecom media: work dir".to_string())?;
    let canon_remote = remote
        .canonicalize()
        .map_err(|_| "wecom media: .remote".to_string())?;
    if !canon_remote.starts_with(&canon_root) {
        return Err("wecom media: .remote escaped".into());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let canon_parent = parent
            .canonicalize()
            .map_err(|_| "wecom media: dest".to_string())?;
        if !canon_parent.starts_with(&canon_remote) {
            return Err("wecom media: path escaped".into());
        }
    }
    std::fs::write(&dest, bytes).map_err(|e| e.to_string())?;
    Ok(dest)
}

#[cfg(test)]
fn encrypt_for_test(key: &[u8; 32], plain: &[u8]) -> Vec<u8> {
    let mut padded = plain.to_vec();
    let n = 32 - (padded.len() % 32);
    padded.extend(std::iter::repeat(n as u8).take(n));
    let mut iv = [0u8; 16];
    iv.copy_from_slice(&key[..16]);
    let len = padded.len();
    Encryptor::<Aes256>::new(key.into(), &iv.into())
        .encrypt_padded_mut::<NoPadding>(&mut padded, len)
        .expect("test encrypt")
        .to_vec()
}

#[cfg(test)]
mod tests {
    use super::super::types::RedactedSecret;
    use super::*;

    #[test]
    fn decrypt_roundtrip_uses_32_byte_padding() {
        let key = [9u8; 32];
        let plain = b"aa.doc contents";
        let ciphertext = encrypt_for_test(&key, plain);
        let out = decrypt_media(&key, &ciphertext).expect("decrypt");
        assert_eq!(out, plain);
    }

    #[test]
    fn decode_aes_key_accepts_raw_32_and_base64() {
        let raw = "0123456789abcdef0123456789abcdef";
        assert_eq!(raw.len(), 32);
        let key = decode_aes_key(raw).expect("raw");
        assert_eq!(&key, raw.as_bytes());
        let encoded = base64::engine::general_purpose::STANDARD.encode(key);
        assert_ne!(encoded.len(), 32);
        assert_eq!(decode_aes_key(&encoded).expect("b64"), key);
        let unpadded = encoded.trim_end_matches('=');
        assert_eq!(decode_aes_key(unpadded).expect("unpadded"), key);
        assert!(decode_aes_key("short").is_err());
    }

    #[test]
    fn media_host_allowlist() {
        assert!(media_url_allowed("https://wwcdn.weixin.qq.com/a/b?x=1"));
        assert!(media_url_allowed("https://weixin.qq.com/a"));
        assert!(!media_url_allowed("http://wwcdn.weixin.qq.com/a"));
        assert!(!media_url_allowed("https://evil.com/weixin.qq.com"));
        assert!(!media_url_allowed("https://weixin.qq.com.evil.com/a"));
        assert!(!media_url_allowed("https://user:pw@wwcdn.weixin.qq.com/a"));
        assert!(media_url_allowed(
            "https://ww-aibot-img-1258476243.cos.ap-guangzhou.myqcloud.com/file?q=1"
        ));
        assert!(!media_url_allowed(
            "https://other-bucket.cos.ap-guangzhou.myqcloud.com/file"
        ));
        assert!(!media_url_allowed("https://ww-aibot-img.evil.myqcloud.com/file"));
        assert_eq!(
            media_host("https://wwcdn.weixin.qq.com/a?q=1"),
            "wwcdn.weixin.qq.com"
        );
    }

    #[test]
    fn size_cap_stops_before_the_limit() {
        let mut buf = vec![1u8; 8];
        assert!(push_capped(&mut buf, &[2, 3], 10).is_ok());
        assert!(push_capped(&mut buf, &[4], 10).is_err());
        assert_eq!(max_bytes("image"), IMAGE_OR_VIDEO_MAX);
        assert_eq!(max_bytes("video"), IMAGE_OR_VIDEO_MAX);
        assert_eq!(max_bytes("file"), FILE_MAX);
    }

    #[test]
    fn filename_is_a_single_segment() {
        assert_eq!(sanitize_filename("../aa.doc", "file"), "aa.doc");
        assert_eq!(sanitize_filename("..", "image"), "image.jpg");
        assert_eq!(sanitize_filename("", "video"), "video.mp4");
        assert_eq!(sanitize_filename("a<b>.txt", "file"), "a_b_.txt");
        let dir = std::env::temp_dir().join(format!("wecom-media-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let dest = inbound_file_path(&dir, "msg/../x", "../aa.doc", "file").unwrap();
        assert!(dest.starts_with(dir.join(".remote")));
        assert!(dest.ends_with("aa.doc"));
        assert!(!dest.components().any(|c| matches!(c, Component::ParentDir)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remote_file_blocks_the_directory() {
        let dir = std::env::temp_dir().join(format!("wecom-remote-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".remote"), b"not a dir").unwrap();
        assert!(inbound_file_path(&dir, "m", "aa.doc", "file").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_inbound_stays_under_remote() {
        let dir = std::env::temp_dir().join(format!("wecom-write-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let _key = [3u8; 32];
        let plain = b"hello doc";
        let path = write_inbound(&dir, "mid", "aa.doc", "file", plain).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), plain);
        assert!(path.starts_with(dir.join(".remote")));
        let shown = format!(
            "{:?}",
            InboundMediaRef {
                kind: "file".into(),
                url: "https://wwcdn.weixin.qq.com/secret-path?q=1".into(),
                aes_key: RedactedSecret::new("0123456789abcdef0123456789abcdef"),
                filename: Some("aa.doc".into()),
            }
        );
        assert!(!shown.contains("0123456789abcdef"));
        assert!(!shown.contains("secret-path"));
        assert!(shown.contains("redacted"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn content_disposition_filename() {
        assert_eq!(
            filename_from_content_disposition("attachment; filename=\"aa.doc\"").as_deref(),
            Some("aa.doc")
        );
        assert_eq!(
            filename_from_content_disposition("attachment; filename*=UTF-8''bb%20c.png").as_deref(),
            Some("bb c.png")
        );
    }
}
