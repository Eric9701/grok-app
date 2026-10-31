//! Send a local PNG, JPEG, or GIF back on the WeCom long connection.
//!
//! The socket uploads the bytes in 512 KiB chunks, then replies with
//! `aibot_respond_msg` / `msgtype=image`. Planning and the upload state
//! machine stay pure so they can be tested without a socket.

use serde_json::{json, Value};
use std::path::Path;

const CHUNK_BYTES: usize = 512 * 1024;
const MAX_BYTES: usize = 10 * 1024 * 1024;
const MIN_BYTES: usize = 5;
const MAX_CHUNKS: usize = 100;

#[derive(Clone)]
pub struct ImageUploadPlan {
    pub filename: String,
    pub total_size: u64,
    pub chunks_b64: Vec<String>,
}

pub enum UploadStep {
    /// This frame belongs to someone else.
    Ignore,
    /// Send the next command.
    Send(Value),
    /// The image reply was acknowledged.
    Finished,
}

pub struct UploadMachine {
    callback_req_id: String,
    plan: ImageUploadPlan,
    upload_id: String,
    next_chunk: usize,
    phase: Phase,
    last_req: String,
}

enum Phase {
    Init,
    Chunks,
    Finish,
    Reply,
}

pub fn image_paths_in_reply(text: &str, exclude: &[String], work_dir: Option<&Path>) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let lower = text.to_ascii_lowercase();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &lower[i..];
        let Some(rel) = [".png", ".jpg", ".jpeg", ".gif"]
            .iter()
            .filter_map(|ext| rest.find(ext).map(|at| (at, ext.len())))
            .min_by_key(|(at, _)| *at)
        else {
            break;
        };
        let end = i + rel.0 + rel.1;
        let boundary = text[end..]
            .chars()
            .next()
            .is_none_or(|c| !is_path_char(c));
        if boundary {
            if let Some(start) = path_start(text, end) {
                let raw = text[start..end].trim();
                let Some(path) = located_image(raw, work_dir) else {
                    i = end;
                    continue;
                };
                let already = found.iter().any(|existing| same_path(existing, &path));
                if !excluded(&path, exclude) && !already {
                    found.push(path);
                }
            }
        }
        i = end;
    }
    found.truncate(4);
    found
}

pub fn plan_image_bytes(filename: &str, bytes: &[u8]) -> Result<ImageUploadPlan, String> {
    let filename = crate::remote_im::wecom_media::sanitize_filename(filename, "image");
    if !is_local_image(&filename) {
        return Err("wecom image: type".into());
    }
    if bytes.len() < MIN_BYTES {
        return Err("wecom image: too small".into());
    }
    if bytes.len() > MAX_BYTES {
        return Err("wecom image: too large".into());
    }
    let chunks_b64: Vec<String> = bytes
        .chunks(CHUNK_BYTES)
        .map(|chunk| base64::Engine::encode(&base64::engine::general_purpose::STANDARD, chunk))
        .collect();
    if chunks_b64.is_empty() || chunks_b64.len() > MAX_CHUNKS {
        return Err("wecom image: chunks".into());
    }
    Ok(ImageUploadPlan {
        filename,
        total_size: bytes.len() as u64,
        chunks_b64,
    })
}

pub fn plan_image_file(path: &Path) -> Result<ImageUploadPlan, String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("image.png");
    let bytes = std::fs::read(path).map_err(|_| "wecom image: read".to_string())?;
    plan_image_bytes(name, &bytes)
}

pub fn begin_upload(callback_req_id: &str, plan: ImageUploadPlan, req_id: &str) -> (UploadMachine, Value) {
    let frame = json!({
        "cmd": "aibot_upload_media_init",
        "headers": { "req_id": req_id },
        "body": {
            "type": "image",
            "filename": plan.filename,
            "total_size": plan.total_size,
            "total_chunks": plan.chunks_b64.len()
        }
    });
    let machine = UploadMachine {
        callback_req_id: callback_req_id.to_string(),
        plan,
        upload_id: String::new(),
        next_chunk: 0,
        phase: Phase::Init,
        last_req: req_id.to_string(),
    };
    (machine, frame)
}

impl UploadMachine {
    pub fn on_frame(&mut self, frame: &Value, next_req_id: &str) -> Result<UploadStep, String> {
        let req_id = frame
            .pointer("/headers/req_id")
            .or_else(|| frame.get("req_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if req_id != self.last_req {
            return Ok(UploadStep::Ignore);
        }
        let code = frame.get("errcode").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            let errmsg = frame
                .get("errmsg")
                .and_then(|v| v.as_str())
                .unwrap_or("wecom image: upload");
            return Err(errmsg.to_string());
        }
        let next = match self.phase {
            Phase::Init => {
                let upload_id = frame
                    .pointer("/body/upload_id")
                    .or_else(|| frame.get("upload_id"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .ok_or("wecom image: upload id")?;
                self.upload_id = upload_id.to_string();
                self.phase = Phase::Chunks;
                self.chunk_frame(next_req_id)
            }
            Phase::Chunks => {
                self.next_chunk += 1;
                if self.next_chunk < self.plan.chunks_b64.len() {
                    self.chunk_frame(next_req_id)
                } else {
                    self.phase = Phase::Finish;
                    json!({
                        "cmd": "aibot_upload_media_finish",
                        "headers": { "req_id": next_req_id },
                        "body": { "upload_id": self.upload_id }
                    })
                }
            }
            Phase::Finish => {
                let media_id = frame
                    .pointer("/body/media_id")
                    .or_else(|| frame.get("media_id"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .ok_or("wecom image: media id")?;
                self.phase = Phase::Reply;
                json!({
                    "cmd": "aibot_respond_msg",
                    "headers": { "req_id": self.callback_req_id },
                    "body": {
                        "msgtype": "image",
                        "image": { "media_id": media_id }
                    }
                })
            }
            Phase::Reply => return Ok(UploadStep::Finished),
        };
        self.last_req = next
            .pointer("/headers/req_id")
            .and_then(|v| v.as_str())
            .unwrap_or(next_req_id)
            .to_string();
        Ok(UploadStep::Send(next))
    }

    fn chunk_frame(&self, req_id: &str) -> Value {
        json!({
            "cmd": "aibot_upload_media_chunk",
            "headers": { "req_id": req_id },
            "body": {
                "upload_id": self.upload_id,
                "chunk_index": self.next_chunk,
                "base64_data": self.plan.chunks_b64[self.next_chunk]
            }
        })
    }
}

fn is_path_char(c: char) -> bool {
    !c.is_whitespace() && !matches!(c, '`' | '"' | '\'' | '<' | '>' | '|' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';')
}

fn path_start(text: &str, end: usize) -> Option<usize> {
    let mut start = end;
    for (idx, ch) in text[..end].char_indices().rev() {
        if !is_path_char(ch) {
            break;
        }
        start = idx;
    }
    let path = &text[start..end];
    if path.contains("://") {
        return None;
    }
    let bytes = path.as_bytes();
    let windows = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/');
    if windows || path.starts_with('/') || path.starts_with('\\') || is_relative_image_token(path) {
        Some(start)
    } else {
        None
    }
}

fn is_relative_image_token(path: &str) -> bool {
    is_local_image(path) && !path.contains("..") && !path.contains(':') && !path.starts_with('.')
}

fn located_image(raw: &str, work_dir: Option<&Path>) -> Option<String> {
    if !is_local_image(raw) || raw.contains("..") {
        return None;
    }
    let bytes = raw.as_bytes();
    let windows = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/');
    if windows || raw.starts_with('/') || raw.starts_with('\\') {
        return Some(raw.to_string());
    }
    let dir = work_dir?;
    let mut acc = dir.to_path_buf();
    for component in Path::new(raw).components() {
        match component {
            std::path::Component::Normal(part) => acc.push(part),
            std::path::Component::CurDir => {}
            _ => return None,
        }
    }
    Some(acc.to_string_lossy().into_owned())
}

fn is_local_image(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".gif"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

fn excluded(path: &str, exclude: &[String]) -> bool {
    exclude.iter().any(|item| same_path(item, path))
}

fn same_path(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/").to_ascii_lowercase();
    norm(a) == norm(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_paths_skip_urls_webp_and_inbound_files() {
        let text = "看 E:\\work\\out.png 和 https://cdn.example/a.png 以及 /tmp/pic.jpeg `E:\\work\\in.gif`";
        let found = image_paths_in_reply(text, &["E:/work/in.gif".into()], None);
        assert_eq!(
            found,
            vec![
                "E:\\work\\out.png".to_string(),
                "/tmp/pic.jpeg".to_string()
            ]
        );
    }

    #[test]
    fn markdown_relative_images_resolve_under_the_work_dir() {
        let text = "![近7天](model-usage-7d.png)\n![入口](model-usage-entry.png)\n`model-usage-30d.png`";
        let found = image_paths_in_reply(text, &[], Some(Path::new("E:/work/atlas-work")));
        assert_eq!(found.len(), 3);
        assert!(found[0].replace('\\', "/").ends_with("E:/work/atlas-work/model-usage-7d.png"));
        assert!(found[1].replace('\\', "/").ends_with("model-usage-entry.png"));
        assert!(found[2].replace('\\', "/").ends_with("model-usage-30d.png"));
        assert!(image_paths_in_reply(text, &[], None).is_empty());
    }

    #[test]
    fn upload_machine_reaches_an_image_reply() {
        let plan = plan_image_bytes("shot.png", b"12345").expect("plan");
        assert_eq!(plan.chunks_b64.len(), 1);
        let (mut machine, init) = begin_upload("cb-1", plan, "init-1");
        assert_eq!(init["cmd"], "aibot_upload_media_init");
        assert_eq!(init["body"]["type"], "image");
        assert_eq!(init["body"]["total_chunks"], 1);
        assert_eq!(init["body"]["filename"], "shot.png");

        let chunk = machine
            .on_frame(
                &json!({"headers": {"req_id": "init-1"}, "errcode": 0, "body": {"upload_id": "up"}}),
                "chunk-1",
            )
            .expect("chunk");
        let UploadStep::Send(chunk) = chunk else {
            panic!("expected chunk");
        };
        assert_eq!(chunk["cmd"], "aibot_upload_media_chunk");
        assert_eq!(chunk["body"]["chunk_index"], 0);
        assert!(chunk["body"]["base64_data"].as_str().unwrap().len() > 4);

        let finish = machine
            .on_frame(
                &json!({"headers": {"req_id": "chunk-1"}, "errcode": 0}),
                "finish-1",
            )
            .expect("finish");
        let UploadStep::Send(finish) = finish else {
            panic!("expected finish");
        };
        assert_eq!(finish["cmd"], "aibot_upload_media_finish");

        let image = machine
            .on_frame(
                &json!({"headers": {"req_id": "finish-1"}, "errcode": 0, "body": {"media_id": "mid"}}),
                "unused",
            )
            .expect("image");
        let UploadStep::Send(image) = image else {
            panic!("expected image");
        };
        assert_eq!(image["cmd"], "aibot_respond_msg");
        assert_eq!(image["headers"]["req_id"], "cb-1");
        assert_eq!(image["body"]["msgtype"], "image");
        assert_eq!(image["body"]["image"]["media_id"], "mid");

        let done = machine
            .on_frame(
                &json!({"headers": {"req_id": "cb-1"}, "errcode": 0}),
                "unused",
            )
            .expect("done");
        assert!(matches!(done, UploadStep::Finished));
    }

    #[test]
    fn rejects_a_tiny_or_non_image_payload() {
        assert!(plan_image_bytes("a.png", b"1234").is_err());
        assert!(plan_image_bytes("a.webp", b"12345").is_err());
    }
}
