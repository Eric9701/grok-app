//! ACP text frames and listen-address checks for Atlas Fleet.
//!
//! The leader speaks the same newline JSON-RPC the Atlas CLI already uses on
//! `--grok-ws-url`. This module does not call the desktop session client.

use serde_json::{json, Value};

pub const DEFAULT_BIND: &str = "127.0.0.1:24720";

/// One ACP line from a worker, or the socket/process closing.
#[derive(Debug)]
pub enum LineEvent {
    Line(String),
    Closed,
}
pub const LOCAL_AGENT: &str = "local";
pub const MAX_STEPS: usize = 200;
pub const MAX_STEP_CHARS: usize = 4_000;
pub const PREVIEW_CHARS: usize = 240;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindAddr {
    pub host: String,
    pub port: u16,
}

impl BindAddr {
    pub fn target(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

/// One folded execution row. Raw ACP frames are not stored.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub kind: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

pub fn preview_text(prompt: &str) -> String {
    let count = prompt.chars().count();
    let mut out: String = prompt.chars().take(PREVIEW_CHARS).collect();
    if count > PREVIEW_CHARS {
        out.push('…');
    }
    out
}

pub fn clip_text(text: &str) -> String {
    text.chars().take(MAX_STEP_CHARS).collect()
}

/// Absolute path on the worker. Does not look at this machine's disk.
pub fn remote_cwd_ok(cwd: &str) -> bool {
    let t = cwd.trim();
    if t.is_empty() || t.chars().any(|c| c == '\0' || c == '\n' || c == '\r') {
        return false;
    }
    if t.starts_with('/') || t.starts_with("\\\\") {
        return true;
    }
    let b = t.as_bytes();
    b.len() >= 3
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b[2] == b'\\' || b[2] == b'/')
}

/// Remote socket id. `local` is reserved for the leader's own CLI.
pub fn remote_agent_id(raw: &str) -> Result<String, String> {
    let id = normalize_agent_id(raw)?;
    if id == LOCAL_AGENT {
        return Err("local is reserved".into());
    }
    Ok(id)
}

/// Dispatch target. `local` is allowed.
pub fn dispatch_agent_id(raw: &str) -> Result<String, String> {
    normalize_agent_id(raw)
}

fn normalize_agent_id(raw: &str) -> Result<String, String> {
    let id = raw.trim().replace(['\r', '\n'], "");
    if id.is_empty() {
        return Err("empty agent id".into());
    }
    if id.chars().count() > 128 {
        return Err("agent id too long".into());
    }
    Ok(id)
}

pub fn token_matches(expected: &str, got: &str) -> bool {
    if expected.is_empty() {
        return false;
    }
    let a = expected.as_bytes();
    let b = got.as_bytes();
    let mut diff = a.len() ^ b.len();
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = *a.get(i).unwrap_or(&0);
        let y = *b.get(i).unwrap_or(&0);
        diff |= (x ^ y) as usize;
    }
    diff == 0
}

pub fn parse_bind(raw: &str) -> Result<BindAddr, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Ok(BindAddr {
            host: "127.0.0.1".into(),
            port: 24720,
        });
    }
    if s.contains('/') || s.contains('?') || s.contains('#') {
        return Err("invalid bind".into());
    }
    let (host, port_raw) = if let Some(rest) = s.strip_prefix('[') {
        let close = rest.find(']').ok_or_else(|| "invalid bind".to_string())?;
        let host = rest[..close].to_string();
        let after = &rest[close + 1..];
        if !after.starts_with(':') {
            return Err("missing port".into());
        }
        (host, after[1..].to_string())
    } else {
        let colon = s.rfind(':').ok_or_else(|| "missing port".to_string())?;
        if s[..colon].contains(':') {
            return Err("invalid bind".into());
        }
        (s[..colon].to_string(), s[colon + 1..].to_string())
    };
    let host = host.trim();
    if host.is_empty() {
        return Err("empty host".into());
    }
    let port: u16 = port_raw
        .trim()
        .parse()
        .map_err(|_| "invalid port".to_string())?;
    if port == 0 {
        return Err("invalid port".into());
    }
    Ok(BindAddr {
        host: host.to_string(),
        port,
    })
}

pub fn is_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// Listening always requires a token, including loopback.
pub fn validate_listen(enabled: bool, bind: &str, token: &str) -> Result<BindAddr, String> {
    let addr = parse_bind(bind)?;
    if enabled && token.trim().is_empty() {
        return Err("token required".into());
    }
    Ok(addr)
}

/// `0.0.0.0` / `::` can be bound, but a worker cannot dial them.
/// The copied command uses loopback; a remote machine replaces that host
/// with this computer's LAN address.
pub fn dial_addr(bind: &BindAddr) -> BindAddr {
    let host = match bind.host.as_str() {
        "0.0.0.0" => "127.0.0.1".to_string(),
        "::" => "::1".to_string(),
        other => other.to_string(),
    };
    BindAddr {
        host,
        port: bind.port,
    }
}

pub fn worker_command(bind: &BindAddr, agent_id: &str, token: &str) -> String {
    let target = dial_addr(bind).target();
    let ws = format!(
        "ws://{target}/ws?agent_id={}&token={}",
        encode_query(agent_id),
        encode_query(token)
    );
    let origin = format!("http://{target}");
    format!(
        "atlas agent --always-approve headless --grok-ws-url '{ws}' --grok-ws-origin '{origin}'"
    )
}

fn encode_query(raw: &str) -> String {
    let mut out = String::new();
    for b in raw.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn inbound_frame_to_line(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("ping") {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn query_param(query: &str, key: &str) -> String {
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let k = parts.next().unwrap_or("");
        let v = parts.next().unwrap_or("");
        if decode_query(k) == key {
            return decode_query(v);
        }
    }
    String::new()
}

fn decode_query(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = &raw[i + 1..i + 3];
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn initialize_line(id: u64) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientInfo": { "name": "atlas-fleet", "version": "0.1.0" },
            "capabilities": {}
        }
    })
    .to_string()
}

pub fn session_new_line(id: u64, cwd: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session/new",
        "params": { "cwd": cwd, "mcpServers": [] }
    })
    .to_string()
}

pub fn session_prompt_line(id: u64, session_id: &str, text: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session/prompt",
        "params": {
            "sessionId": session_id,
            "prompt": [{ "type": "text", "text": text }]
        }
    })
    .to_string()
}

pub fn session_cancel_line(session_id: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "method": "session/cancel",
        "params": { "sessionId": session_id }
    })
    .to_string()
}

pub fn permission_result_line(id: u64, option_id: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": { "outcome": { "outcome": "selected", "optionId": option_id } }
    })
    .to_string()
}

#[derive(Debug)]
pub enum InFrame {
    Result { id: u64, result: Value },
    Error { id: u64, message: String },
    Update(Step),
    Permission { id: u64, option_id: String },
    Ignore,
}

pub fn parse_incoming(line: &str) -> InFrame {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return InFrame::Ignore;
    };
    if let Some(method) = v.get("method").and_then(|m| m.as_str()) {
        if method == "session/request_permission" {
            let id = v.get("id").and_then(|x| x.as_u64()).unwrap_or(0);
            let option = v
                .pointer("/params/options/0/optionId")
                .and_then(|s| s.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("allow");
            return InFrame::Permission {
                id,
                option_id: option.to_string(),
            };
        }
        if method == "session/update" {
            if let Some(step) = fold_update(&v) {
                return InFrame::Update(step);
            }
            return InFrame::Ignore;
        }
    }
    let Some(id) = v.get("id").and_then(|x| x.as_u64()) else {
        return InFrame::Ignore;
    };
    if v.get("error").is_some() {
        let message = v
            .pointer("/error/message")
            .and_then(|m| m.as_str())
            .unwrap_or("request failed");
        return InFrame::Error {
            id,
            message: clip_text(message),
        };
    }
    if let Some(result) = v.get("result").cloned() {
        return InFrame::Result { id, result };
    }
    InFrame::Ignore
}

pub fn session_id_from_result(result: &Value) -> String {
    result
        .get("sessionId")
        .or_else(|| result.pointer("/session/sessionId"))
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

fn fold_update(frame: &Value) -> Option<Step> {
    let update = frame.pointer("/params/update").or_else(|| frame.get("update"))?;
    let kind = update
        .get("sessionUpdate")
        .or_else(|| update.get("session_update"))
        .and_then(|s| s.as_str())
        .unwrap_or("");
    match kind {
        "agent_thought_chunk" => Some(Step {
            kind: "thought".into(),
            text: chunk_text(update),
            tool_call_id: None,
            tool_name: None,
            title: None,
            status: None,
        }),
        "agent_message_chunk" => Some(Step {
            kind: "message".into(),
            text: chunk_text(update),
            tool_call_id: None,
            tool_name: None,
            title: None,
            status: None,
        }),
        "tool_call" | "tool_call_update" => {
            let id = update
                .get("toolCallId")
                .or_else(|| update.get("tool_call_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let name = update
                .get("title")
                .or_else(|| update.get("toolName"))
                .or_else(|| update.get("name"))
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let status = update
                .get("status")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            Some(Step {
                kind: "tool".into(),
                text: clip_text(&chunk_text(update)),
                tool_call_id: if id.is_empty() { None } else { Some(id) },
                tool_name: name.clone(),
                title: name,
                status,
            })
        }
        _ => None,
    }
}

fn chunk_text(update: &Value) -> String {
    if let Some(s) = update.get("text").and_then(|v| v.as_str()) {
        return clip_text(s);
    }
    if let Some(s) = update.get("content").and_then(|v| v.as_str()) {
        return clip_text(s);
    }
    if let Some(obj) = update.get("content").and_then(|v| v.as_object()) {
        if let Some(t) = obj.get("text").and_then(|v| v.as_str()) {
            return clip_text(t);
        }
    }
    if let Some(arr) = update.get("content").and_then(|v| v.as_array()) {
        let mut out = String::new();
        for item in arr {
            if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                out.push_str(t);
            } else if let Some(t) = item.as_str() {
                out.push_str(t);
            }
        }
        return clip_text(&out);
    }
    String::new()
}

/// Merge consecutive text and tool updates. Stops at 200 steps.
pub fn push_step(steps: &mut Vec<Step>, step: Step) {
    if step.kind == "thought" || step.kind == "message" {
        if let Some(last) = steps.last_mut() {
            if last.kind == step.kind && last.text.chars().count() < MAX_STEP_CHARS {
                let room = MAX_STEP_CHARS.saturating_sub(last.text.chars().count());
                let extra: String = step.text.chars().take(room).collect();
                last.text.push_str(&extra);
                return;
            }
        }
    }
    if step.kind == "tool" {
        if let Some(id) = step.tool_call_id.clone() {
            if let Some(existing) = steps
                .iter_mut()
                .rev()
                .find(|s| s.tool_call_id.as_deref() == Some(id.as_str()))
            {
                if !step.text.is_empty() {
                    existing.text = clip_text(&step.text);
                }
                if step.status.is_some() {
                    existing.status = step.status;
                }
                if step.title.is_some() {
                    existing.title = step.title;
                }
                if step.tool_name.is_some() {
                    existing.tool_name = step.tool_name;
                }
                return;
            }
        }
    }
    if steps.len() >= MAX_STEPS {
        return;
    }
    let mut step = step;
    step.text = clip_text(&step.text);
    steps.push(step);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_cwd_accepts_absolute_paths_without_local_disk() {
        assert!(remote_cwd_ok("/data/proj"));
        assert!(remote_cwd_ok("C:\\work"));
        assert!(remote_cwd_ok("D:/work"));
        assert!(remote_cwd_ok("\\\\server\\share"));
        assert!(!remote_cwd_ok("relative"));
        assert!(!remote_cwd_ok(""));
        assert!(!remote_cwd_ok("/tmp/a\nb"));
    }

    #[test]
    fn token_compare_rejects_empty_and_mismatch() {
        assert!(!token_matches("", "abc"));
        assert!(!token_matches("abc", "abd"));
        assert!(!token_matches("abc", "abcd"));
        assert!(token_matches("abc", "abc"));
    }

    #[test]
    fn listen_requires_token_on_loopback_too() {
        assert!(validate_listen(true, "127.0.0.1:24720", "").is_err());
        assert!(validate_listen(true, "10.0.0.8:24720", "").is_err());
        assert!(validate_listen(true, "127.0.0.1:24720", "tok").is_ok());
        assert!(validate_listen(false, "not a bind", "").is_err());
        assert!(is_loopback("localhost"));
        assert!(!is_loopback("10.0.0.8"));
        let any = parse_bind("0.0.0.0:24720").unwrap();
        let cmd = worker_command(&any, "worker-2", "tok");
        assert!(cmd.contains("ws://127.0.0.1:24720/ws?"));
        assert!(!cmd.contains("0.0.0.0"));
        assert!(cmd.contains("http://127.0.0.1:24720"));
    }

    #[test]
    fn frame_drops_blank_and_literal_ping() {
        assert_eq!(inbound_frame_to_line("  "), None);
        assert_eq!(inbound_frame_to_line("PING"), None);
        assert_eq!(
            inbound_frame_to_line("  {\"jsonrpc\":\"2.0\"}  ").as_deref(),
            Some("{\"jsonrpc\":\"2.0\"}")
        );
    }

    #[test]
    fn steps_merge_text_and_cap() {
        let mut steps = Vec::new();
        push_step(
            &mut steps,
            Step {
                kind: "thought".into(),
                text: "a".into(),
                tool_call_id: None,
                tool_name: None,
                title: None,
                status: None,
            },
        );
        push_step(
            &mut steps,
            Step {
                kind: "thought".into(),
                text: "b".into(),
                tool_call_id: None,
                tool_name: None,
                title: None,
                status: None,
            },
        );
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].text, "ab");
        for i in 0..MAX_STEPS + 5 {
            push_step(
                &mut steps,
                Step {
                    kind: "message".into(),
                    text: format!("m{i}"),
                    tool_call_id: None,
                    tool_name: None,
                    title: None,
                    status: None,
                },
            );
        }
        assert!(steps.len() <= MAX_STEPS);
    }

    #[test]
    fn query_reads_agent_and_token() {
        let q = "agent_id=box-a&token=a%2Fb";
        assert_eq!(query_param(q, "agent_id"), "box-a");
        assert_eq!(query_param(q, "token"), "a/b");
    }

    #[test]
    fn local_id_cannot_register_as_remote() {
        assert!(remote_agent_id("local").is_err());
        assert_eq!(dispatch_agent_id("local").unwrap(), "local");
    }
}
