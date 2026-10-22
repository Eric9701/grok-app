//! Atlas Relay **Agent** bridge.
//!
//! Outbound WebSocket to `atlas-relay-demo` `/ws?agent_id=&token=`. Each ACP JSON
//! frame is written as one line to a local `atlas agent --always-approve stdio`
//! process, and each stdout line is sent back as one WebSocket text frame.
//! The desktop session client is not involved.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, Notify};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_BACKOFF_SECS: u64 = 15;
const MAX_TASKS: usize = 40;
const MAX_TASK_TEXT: usize = 240;
const MAX_TRACE_STEPS: usize = 200;
const MAX_STEP_TEXT: usize = 4_000;
const DEFAULT_HEALTH_SECS: u64 = 15;
const MIN_HEALTH_SECS: u64 = 5;
const MAX_HEALTH_SECS: u64 = 300;
pub const MAX_RELAY_AGENTS: usize = 4;

static HEALTH_SECS: AtomicU64 = AtomicU64::new(DEFAULT_HEALTH_SECS);
static INJECT_SEQ: AtomicU64 = AtomicU64::new(1);

const RELAY_MODEL_INJECT_PREFIX: &str = "relay-model-";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AtlasRelayTask {
    pub id: String,
    pub session_id: String,
    pub text: String,
    /// `running` | `done` | `failed` | `cancelled`
    pub status: String,
    pub error: Option<String>,
    pub at_ms: u64,
    /// Which relay agent recorded this prompt.
    #[serde(default)]
    pub agent_id: String,
    /// Working directory from `session/new` or this prompt. Empty when unknown.
    #[serde(default)]
    pub cwd: String,
    /// App project id when `cwd` matched or created a project.
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub project_name: String,
    #[serde(default)]
    pub project_path: String,
    /// Folded `session/update` rows. Omitted from the fleet status snapshot.
    #[serde(skip)]
    pub steps: Vec<AtlasRelayTraceStep>,
}

/// One folded row of a relay task's agent run.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AtlasRelayTraceStep {
    /// `thought` | `message` | `tool`
    pub kind: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// One task's run, including folded steps. `found` is false when the id is gone.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AtlasRelayTaskTrace {
    pub found: bool,
    pub id: String,
    pub agent_id: String,
    pub text: String,
    pub status: String,
    pub error: Option<String>,
    pub steps: Vec<AtlasRelayTraceStep>,
    /// CLI agent session id for this prompt.
    #[serde(default)]
    pub session_id: String,
    /// App project id when the task is hanging under a project.
    #[serde(default)]
    pub project_id: String,
    /// True when this task has finished and no running task shares its session.
    #[serde(default)]
    pub open_session: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AtlasRelayAgentLive {
    /// `stopped` | `connecting` | `online` | `reconnecting`
    pub phase: String,
    pub error: Option<String>,
    pub cli_alive: bool,
    pub agent_id: String,
    /// Redacted socket address.
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AtlasRelayFleetStatus {
    /// `online` when any agent is online. Otherwise the busiest other phase.
    pub phase: String,
    pub agents: Vec<AtlasRelayAgentLive>,
    /// Cloud prompts from every agent, newest first.
    pub tasks: Vec<AtlasRelayTask>,
}

struct Slot {
    gen: u64,
    pid: u32,
    phase: String,
    error: Option<String>,
    cli_alive: bool,
    agent_id: String,
    /// Real socket URL, including the token. Status redacts it.
    url: String,
    /// Catalog id applied to this process. Empty means do not rewrite frames.
    model_id: String,
    /// Sessions this process has seen. Cleared when the CLI is replaced.
    sessions: HashSet<String>,
    /// `session/new` or `session/load` RPC id → cwd, until the result arrives.
    pending_cwd: HashMap<String, String>,
    /// Agent session id → cwd. Cleared with `sessions` when the CLI is replaced.
    session_cwd: HashMap<String, String>,
    /// Local `session/set_model` lines. `None` until the bridge is in the read loop.
    inject_tx: Option<mpsc::UnboundedSender<String>>,
    tasks: Vec<AtlasRelayTask>,
    cancel: Arc<Notify>,
}

struct Hub {
    next_gen: u64,
    slots: HashMap<String, Slot>,
}

fn hub() -> &'static Mutex<Hub> {
    static H: OnceLock<Mutex<Hub>> = OnceLock::new();
    H.get_or_init(|| {
        Mutex::new(Hub {
            next_gen: 1,
            slots: HashMap::new(),
        })
    })
}

fn with_hub<T>(f: impl FnOnce(&mut Hub) -> T) -> T {
    let mut guard = hub().lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

fn fleet_phase(agents: &[AtlasRelayAgentLive]) -> String {
    if agents.iter().any(|a| a.phase == "online") {
        "online".into()
    } else if agents.iter().any(|a| a.phase == "connecting") {
        "connecting".into()
    } else if agents.iter().any(|a| a.phase == "reconnecting") {
        "reconnecting".into()
    } else {
        "stopped".into()
    }
}

fn snapshot() -> AtlasRelayFleetStatus {
    with_hub(|h| {
        let mut agents: Vec<AtlasRelayAgentLive> = h
            .slots
            .values()
            .map(|s| AtlasRelayAgentLive {
                phase: s.phase.clone(),
                error: s.error.clone(),
                cli_alive: s.cli_alive,
                agent_id: s.agent_id.clone(),
                url: redact_relay_url(&s.url),
            })
            .collect();
        agents.sort_by(|a, b| a.agent_id.cmp(&b.agent_id));
        let mut tasks = Vec::new();
        for slot in h.slots.values() {
            tasks.extend(slot.tasks.iter().cloned());
        }
        tasks.sort_by(|a, b| b.at_ms.cmp(&a.at_ms));
        let phase = fleet_phase(&agents);
        AtlasRelayFleetStatus {
            phase,
            agents,
            tasks,
        }
    })
}

fn unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn clip_text(raw: &str) -> String {
    raw.trim().chars().take(MAX_TASK_TEXT).collect()
}

/// Catalog id for one relay connection. Empty means the cloud frame is left alone.
pub fn sanitize_model_id(raw: &str) -> String {
    raw.replace(['\r', '\n'], "")
        .trim()
        .chars()
        .take(128)
        .collect()
}

pub fn relay_model_inject_id(n: u64) -> String {
    format!("{RELAY_MODEL_INJECT_PREFIX}{n}")
}

/// CLI replies to a locally injected `session/set_model` must not be forwarded.
pub fn is_relay_model_inject_frame(frame: &str) -> bool {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(frame) else {
        return false;
    };
    rpc_id(&v).is_some_and(|id| id.starts_with(RELAY_MODEL_INJECT_PREFIX))
}

fn session_set_model_line(inject_id: &str, session_id: &str, model_id: &str) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": inject_id,
        "method": "session/set_model",
        "params": {
            "sessionId": session_id,
            "modelId": model_id,
        }
    })
    .to_string()
}

/// Force this connection's catalog id onto `session/new` and `session/set_model`.
/// An empty model returns the frame unchanged.
pub fn apply_connection_model(frame: &str, model_id: &str) -> String {
    let model_id = sanitize_model_id(model_id);
    if model_id.is_empty() {
        return frame.to_string();
    }
    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(frame) else {
        return frame.to_string();
    };
    let method = v
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();
    let Some(params) = v.get_mut("params").and_then(|p| p.as_object_mut()) else {
        return frame.to_string();
    };
    match method.as_str() {
        "session/new" => {
            let meta = params
                .entry("_meta")
                .or_insert_with(|| serde_json::json!({}));
            if !meta.is_object() {
                *meta = serde_json::json!({});
            }
            if let Some(obj) = meta.as_object_mut() {
                obj.insert("modelId".into(), serde_json::Value::String(model_id));
            }
        }
        "session/set_model" => {
            params.insert("modelId".into(), serde_json::Value::String(model_id));
        }
        _ => return frame.to_string(),
    }
    serde_json::to_string(&v).unwrap_or_else(|_| frame.to_string())
}

/// Session ids this bridge can switch without restarting the CLI.
pub fn session_id_from_frame(frame: &str, inbound: bool) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(frame).ok()?;
    let raw = if inbound {
        let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
        match method {
            "session/load" | "session/prompt" => v.pointer("/params/sessionId"),
            "session/new" => v.pointer("/params/_meta/sessionId"),
            _ => None,
        }
    } else {
        v.pointer("/result/sessionId")
    }?;
    let id = raw.as_str().unwrap_or("").trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

fn rpc_id(v: &serde_json::Value) -> Option<String> {
    match v.get("id")? {
        serde_json::Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn cwd_text(raw: &str) -> String {
    raw.trim().to_string()
}

/// `params.cwd`, otherwise `params._meta.cwd`.
fn params_cwd(params: &serde_json::Value) -> String {
    let direct = params.get("cwd").and_then(|v| v.as_str()).unwrap_or("");
    if !direct.trim().is_empty() {
        return cwd_text(direct);
    }
    let meta = params
        .pointer("/_meta/cwd")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    cwd_text(meta)
}

/// Remember cwd from `session/new` / `session/load` until the result names a session.
/// Returns `(sessionId, cwd)` when an outbound result binds a pending directory.
fn note_cwd_frame(
    pending: &mut HashMap<String, String>,
    by_session: &mut HashMap<String, String>,
    frame: &str,
    inbound: bool,
) -> Option<(String, String)> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(frame) else {
        return None;
    };
    if !inbound {
        let Some(id) = rpc_id(&v) else {
            return None;
        };
        let Some(session_id) = v
            .pointer("/result/sessionId")
            .and_then(|s| s.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            return None;
        };
        if let Some(cwd) = pending.remove(&id) {
            by_session.insert(session_id.to_string(), cwd.clone());
            return Some((session_id.to_string(), cwd));
        }
        return None;
    }
    let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
    if method != "session/new" && method != "session/load" {
        return None;
    }
    let Some(id) = rpc_id(&v) else {
        return None;
    };
    let cwd = params_cwd(v.get("params").unwrap_or(&serde_json::Value::Null));
    if !cwd.is_empty() {
        pending.insert(id, cwd);
    }
    None
}

/// Prompt cwd, or the directory remembered for its session.
fn prompt_cwd(frame: &str, by_session: &HashMap<String, String>) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(frame) else {
        return String::new();
    };
    if v.get("method").and_then(|m| m.as_str()) != Some("session/prompt") {
        return String::new();
    }
    let params = v.get("params").cloned().unwrap_or(serde_json::Value::Null);
    let direct = params_cwd(&params);
    if !direct.is_empty() {
        return direct;
    }
    let sid = params
        .get("sessionId")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .trim();
    by_session.get(sid).cloned().unwrap_or_default()
}

fn normalize_cwd_key(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}

fn cwd_paths_match(a: &str, b: &str) -> bool {
    normalize_cwd_key(a).eq_ignore_ascii_case(&normalize_cwd_key(b))
}

/// Absolute cwd that is a directory. A missing path is created.
/// Relative paths and paths that already exist as files are left alone.
fn ensure_absolute_dir(cwd: &str) -> bool {
    let cwd = cwd.trim();
    if cwd.is_empty() {
        return false;
    }
    let path = Path::new(cwd);
    if !path.is_absolute() {
        return false;
    }
    if path.is_dir() {
        return true;
    }
    if path.exists() {
        return false;
    }
    std::fs::create_dir_all(path).is_ok() && path.is_dir()
}

struct ResolvedRelayProject {
    id: String,
    name: String,
    path: String,
}

/// Match an absolute existing directory to a project, or add an untrusted one.
fn resolve_relay_project(cwd: &str) -> Option<ResolvedRelayProject> {
    if !ensure_absolute_dir(cwd) {
        return None;
    }
    let cwd = cwd.trim();
    let projects = crate::store::load_projects();
    if let Some(hit) = projects.iter().find(|p| cwd_paths_match(&p.path, cwd)) {
        return Some(ResolvedRelayProject {
            id: hit.id.clone(),
            name: hit.name.clone(),
            path: hit.path.clone(),
        });
    }
    let created_project = crate::store::add_project(cwd.to_string(), false).ok()?;
    let created = !projects.iter().any(|old| old.id == created_project.id);
    if created {
        crate::remote_im::emit_projects_changed();
    }
    Some(ResolvedRelayProject {
        id: created_project.id,
        name: created_project.name,
        path: created_project.path,
    })
}

fn clear_cli_memory(slot: &mut Slot) {
    slot.sessions.clear();
    slot.pending_cwd.clear();
    slot.session_cwd.clear();
}

fn prompt_text(params: &serde_json::Value) -> String {
    let Some(blocks) = params.get("prompt").and_then(|p| p.as_array()) else {
        return String::new();
    };
    let mut parts = Vec::new();
    for block in blocks {
        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
            let text = text.trim();
            if !text.is_empty() {
                parts.push(text);
            }
        }
    }
    clip_text(&parts.join("\n"))
}

fn push_task(tasks: &mut Vec<AtlasRelayTask>, task: AtlasRelayTask) {
    tasks.insert(0, task);
    if tasks.len() > MAX_TASKS {
        tasks.truncate(MAX_TASKS);
    }
}

fn append_chars(buf: &mut String, extra: &str, max_chars: usize) {
    let used = buf.chars().count();
    if used >= max_chars {
        return;
    }
    buf.extend(extra.chars().take(max_chars - used));
}

fn one_line(raw: &str) -> String {
    let line = raw
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();
    clip_text(line)
}

fn content_to_text(content: &serde_json::Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_string();
    }
    if let Some(text) = content.get("text").and_then(|t| t.as_str()) {
        return text.to_string();
    }
    let Some(blocks) = content.as_array() else {
        return String::new();
    };
    let mut out = String::new();
    for block in blocks {
        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
            out.push_str(text);
        } else if let Some(text) = block.as_str() {
            out.push_str(text);
        }
    }
    out
}

fn chunk_text(update: &serde_json::Value) -> String {
    update
        .get("content")
        .map(content_to_text)
        .unwrap_or_default()
}

fn trimmed_field(update: &serde_json::Value, key: &str) -> Option<String> {
    let text = update
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn tool_call_id(update: &serde_json::Value) -> String {
    update
        .get("toolCallId")
        .or_else(|| update.get("tool_call_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

fn tool_name(update: &serde_json::Value) -> Option<String> {
    let from_meta = update
        .pointer("/_meta/x.ai/tool/name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if !from_meta.is_empty() {
        return Some(from_meta.to_string());
    }
    trimmed_field(update, "toolName")
        .or_else(|| trimmed_field(update, "name"))
        .or_else(|| trimmed_field(update, "kind"))
}

fn tool_summary(update: &serde_json::Value) -> Option<String> {
    if let Some(title) = trimmed_field(update, "title") {
        let line = one_line(&title);
        if !line.is_empty() {
            return Some(line);
        }
    }
    let content = one_line(&chunk_text(update));
    if !content.is_empty() {
        return Some(content);
    }
    let message = update
        .pointer("/rawOutput/message")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let line = one_line(message);
    if line.is_empty() {
        None
    } else {
        Some(line)
    }
}

fn append_text_step(task: &mut AtlasRelayTask, kind: &str, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = task.steps.last_mut() {
        if last.kind == kind && last.tool_call_id.is_none() {
            append_chars(&mut last.text, text, MAX_STEP_TEXT);
            return;
        }
    }
    if text.trim().is_empty() || task.steps.len() >= MAX_TRACE_STEPS {
        return;
    }
    let mut body = String::new();
    append_chars(&mut body, text, MAX_STEP_TEXT);
    task.steps.push(AtlasRelayTraceStep {
        kind: kind.to_string(),
        text: body,
        tool_call_id: None,
        tool_name: None,
        title: None,
        status: None,
    });
}

fn apply_tool_step(task: &mut AtlasRelayTask, update: &serde_json::Value, is_update: bool) {
    let id = tool_call_id(update);
    let name = tool_name(update);
    let title = trimmed_field(update, "title");
    let status = trimmed_field(update, "status");
    let summary = tool_summary(update);
    if is_update && !id.is_empty() {
        if let Some(step) =
            task.steps.iter_mut().rev().find(|step| {
                step.kind == "tool" && step.tool_call_id.as_deref() == Some(id.as_str())
            })
        {
            if let Some(status) = status.clone() {
                step.status = Some(status);
            }
            if let Some(summary) = summary.clone() {
                step.text = summary;
            }
            if let Some(title) = title.clone() {
                step.title = Some(title);
            }
            if step.tool_name.is_none() {
                step.tool_name = name.clone();
            }
            return;
        }
    }
    if is_update && id.is_empty() {
        return;
    }
    if task.steps.len() >= MAX_TRACE_STEPS {
        return;
    }
    task.steps.push(AtlasRelayTraceStep {
        kind: "tool".into(),
        text: summary.clone().unwrap_or_default(),
        tool_call_id: if id.is_empty() { None } else { Some(id) },
        tool_name: name,
        title: title.or(summary),
        status,
    });
}

/// Append CLI `session/update` rows onto the newest running task for that session.
fn note_session_update(tasks: &mut [AtlasRelayTask], frame: &serde_json::Value) {
    let session_id = frame
        .pointer("/params/sessionId")
        .or_else(|| frame.pointer("/params/update/sessionId"))
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if session_id.is_empty() {
        return;
    }
    let Some(update) = frame
        .pointer("/params/update")
        .or_else(|| frame.get("update"))
    else {
        return;
    };
    let kind = update
        .get("sessionUpdate")
        .or_else(|| update.get("session_update"))
        .and_then(|s| s.as_str())
        .unwrap_or("");
    let Some(task) = tasks
        .iter_mut()
        .find(|task| task.status == "running" && task.session_id == session_id)
    else {
        return;
    };
    match kind {
        "agent_thought_chunk" => append_text_step(task, "thought", &chunk_text(update)),
        "agent_message_chunk" => append_text_step(task, "message", &chunk_text(update)),
        "tool_call" => apply_tool_step(task, update, false),
        "tool_call_update" => apply_tool_step(task, update, true),
        _ => {}
    }
}

fn mark_id(tasks: &mut [AtlasRelayTask], id: &str, status: &str, error: Option<String>) {
    if let Some(task) = tasks
        .iter_mut()
        .find(|t| t.id == id && t.status == "running")
    {
        task.status = status.to_string();
        task.error = error;
        return;
    }
    if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
        task.status = status.to_string();
        task.error = error;
    }
}

/// Record a cloud `session/prompt` or the CLI reply that finishes it.
/// Parse failures are ignored so the bridge keeps forwarding the frame.
pub fn note_relay_frame(tasks: &mut Vec<AtlasRelayTask>, frame: &str, inbound: bool, at_ms: u64) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(frame) else {
        return;
    };
    if inbound {
        let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
        if method == "session/prompt" {
            let Some(id) = rpc_id(&v) else { return };
            let params = v.get("params").cloned().unwrap_or(serde_json::Value::Null);
            let session_id = params
                .get("sessionId")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            push_task(
                tasks,
                AtlasRelayTask {
                    id,
                    session_id,
                    text: prompt_text(&params),
                    status: "running".into(),
                    error: None,
                    at_ms,
                    agent_id: String::new(),
                    cwd: String::new(),
                    project_id: String::new(),
                    project_name: String::new(),
                    project_path: String::new(),
                    steps: Vec::new(),
                },
            );
            return;
        }
        if method == "session/cancel" {
            let sid = v
                .pointer("/params/sessionId")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if sid.is_empty() {
                return;
            }
            for task in tasks.iter_mut() {
                if task.status == "running" && task.session_id == sid {
                    task.status = "cancelled".into();
                }
            }
        }
        return;
    }
    let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
    if method == "session/update" {
        note_session_update(tasks, &v);
        return;
    }
    let Some(id) = rpc_id(&v) else { return };
    if v.get("error").is_some() {
        let message = v
            .pointer("/error/message")
            .and_then(|m| m.as_str())
            .map(clip_text)
            .filter(|s| !s.is_empty());
        mark_id(tasks, &id, "failed", message);
        return;
    }
    if v.get("result").is_some() {
        mark_id(tasks, &id, "done", None);
    }
}

fn write_task_project(agent_id: &str, task_id: &str, cwd: &str, project: &ResolvedRelayProject) {
    with_hub(|h| {
        let Some(slot) = h.slots.get_mut(agent_id) else {
            return;
        };
        if let Some(task) = slot.tasks.iter_mut().find(|task| task.id == task_id) {
            if task.cwd.is_empty() {
                task.cwd = cwd.to_string();
            }
            task.project_id = project.id.clone();
            task.project_name = project.name.clone();
            task.project_path = project.path.clone();
        }
    });
}

fn assign_project_to_task(agent_id: &str, task_id: &str, cwd: &str) {
    let Some(project) = resolve_relay_project(cwd) else {
        return;
    };
    write_task_project(agent_id, task_id, cwd, &project);
}

fn cwd_from_agent_session(session_id: &str, mode: &str) -> Option<String> {
    let dir = crate::paths::find_agent_session_dir(session_id, None, mode)?;
    let text = std::fs::read_to_string(dir.join("summary.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let cwd = value
        .pointer("/info/cwd")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .trim();
    if cwd.is_empty() {
        None
    } else {
        Some(cwd.to_string())
    }
}

/// Fill a finished task's project from its cwd or the CLI session summary.
fn backfill_task_project(agent_id: &str, task_id: &str, session_id: &str, mode: &str) -> String {
    let known = with_hub(|h| {
        h.slots.get(agent_id).and_then(|slot| {
            slot.tasks
                .iter()
                .find(|task| task.id == task_id)
                .map(|task| (task.project_id.clone(), task.cwd.clone()))
        })
    });
    let (project_id, cwd) = known.unwrap_or_default();
    if !project_id.is_empty() {
        return project_id;
    }
    let cwd = if cwd.is_empty() {
        cwd_from_agent_session(session_id, mode).unwrap_or_default()
    } else {
        cwd
    };
    let Some(project) = resolve_relay_project(&cwd) else {
        return String::new();
    };
    write_task_project(agent_id, task_id, &cwd, &project);
    project.id
}

/// Copy the finished CLI session into the project. Does not open or connect it.
fn schedule_finished_session_import(agent_id: String, task_id: String, session_id: String) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if with_hub(|h| session_has_running(h, &session_id)) {
            return;
        }
        let imported = tauri::async_runtime::spawn_blocking(move || {
            let mode = crate::store::load_settings().session_data_mode;
            let project_id = backfill_task_project(&agent_id, &task_id, &session_id, &mode);
            let project = if project_id.is_empty() {
                None
            } else {
                Some(project_id)
            };
            crate::cli_sessions::import_cli_session(&session_id, None, project, &mode)
        })
        .await;
        if let Ok(Ok(meta)) = imported {
            crate::remote_im::emit_session_index_changed(&meta.id);
        }
    });
}

fn observe_frame(agent_id: &str, frame: &str, inbound: bool) {
    let at = unix_ms();
    let (assign, finished) = with_hub(|h| {
        let Some(slot) = h.slots.get_mut(agent_id) else {
            return (Vec::new(), None);
        };
        let bound = note_cwd_frame(&mut slot.pending_cwd, &mut slot.session_cwd, frame, inbound);
        let mut assign = Vec::new();
        if let Some((session_id, cwd)) = bound {
            for task in slot.tasks.iter_mut() {
                if task.status == "running" && task.session_id == session_id && task.cwd.is_empty()
                {
                    task.cwd = cwd.clone();
                    assign.push((task.id.clone(), cwd.clone()));
                }
            }
        }
        let prior = if inbound {
            None
        } else {
            serde_json::from_str::<serde_json::Value>(frame)
                .ok()
                .and_then(|value| {
                    let id = rpc_id(&value)?;
                    let status = slot
                        .tasks
                        .iter()
                        .find(|task| task.id == id)
                        .map(|task| task.status.clone())?;
                    Some((id, status))
                })
        };
        let before_len = slot.tasks.len();
        note_relay_frame(&mut slot.tasks, frame, inbound, at);
        if slot.tasks.len() > before_len {
            let cwd = prompt_cwd(frame, &slot.session_cwd);
            if let Some(task) = slot.tasks.first() {
                let task_id = task.id.clone();
                let session_id = task.session_id.clone();
                if !cwd.is_empty() && !session_id.is_empty() {
                    slot.session_cwd.insert(session_id, cwd.clone());
                }
                if let Some(task) = slot.tasks.first_mut() {
                    if task.id == task_id {
                        task.agent_id = agent_id.to_string();
                        task.cwd = cwd.clone();
                    }
                }
                if !cwd.is_empty() {
                    assign.push((task_id, cwd));
                }
            }
        }
        let finished = prior.and_then(|(id, status)| {
            if status != "running" {
                return None;
            }
            let task = slot.tasks.iter().find(|task| task.id == id)?;
            if task.status != "done" && task.status != "failed" {
                return None;
            }
            let session_id = task.session_id.clone();
            if session_id.is_empty() {
                return None;
            }
            Some((task.id.clone(), session_id))
        });
        (assign, finished)
    });
    let finished =
        finished.filter(|(_, session_id)| !with_hub(|h| session_has_running(h, session_id)));
    for (task_id, cwd) in assign {
        assign_project_to_task(agent_id, &task_id, &cwd);
    }
    if let Some((task_id, session_id)) = finished {
        schedule_finished_session_import(agent_id.to_string(), task_id, session_id);
    }
}

/// Trim, drop CR/LF, cap at 128 characters.
pub fn sanitize_agent_id(raw: &str) -> String {
    let flat: String = raw.chars().filter(|c| *c != '\n' && *c != '\r').collect();
    flat.trim().chars().take(128).collect()
}

/// Drop whitespace and control characters. Empty means “no token”.
pub fn sanitize_token(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_control() && !c.is_whitespace())
        .take(512)
        .collect()
}

/// Normalize a pasted relay address into the Agent socket
/// `ws(s)://host[:port]/ws?agent_id=...&token=...`.
///
/// Page URLs (`/build`, `/m`) and a bare `host:port` become `/ws`.
/// An address that is already `/ws` or `/ws/agent` keeps that path.
/// `token` is optional. A token typed in settings wins over `token` pasted
/// on the address.
pub fn relay_agent_ws_url(raw: &str, agent_id: &str, token: &str) -> Result<String, String> {
    let id = sanitize_agent_id(agent_id);
    if id.is_empty() {
        return Err("empty agent id".into());
    }
    let s = raw.trim();
    if s.is_empty() {
        return Err("empty address".into());
    }
    let with_scheme = if s.contains("://") {
        s.to_string()
    } else {
        format!("ws://{s}")
    };
    let mut u = Url::parse(&with_scheme).map_err(|e| e.to_string())?;
    let scheme = u.scheme().to_string();
    let next_scheme = match scheme.as_str() {
        "http" => "ws",
        "https" => "wss",
        "ws" | "wss" => scheme.as_str(),
        other => return Err(format!("unsupported scheme: {other}")),
    };
    if scheme != next_scheme {
        u.set_scheme(next_scheme)
            .map_err(|_| "unsupported scheme".to_string())?;
    }
    if u.host_str().unwrap_or("").is_empty() {
        return Err("empty host".into());
    }
    let path = u.path().trim_end_matches('/').to_string();
    let keep_agent_path = path == "/ws"
        || path == "/ws/agent"
        || path.ends_with("/ws")
        || path.ends_with("/ws/agent");
    let pasted = u
        .query_pairs()
        .find(|(k, _)| k == "token")
        .map(|(_, v)| sanitize_token(&v))
        .unwrap_or_default();
    let explicit = sanitize_token(token);
    let token = if explicit.is_empty() {
        pasted
    } else {
        explicit
    };
    if !keep_agent_path {
        u.set_path("/ws");
    }
    {
        let mut pairs = u.query_pairs_mut();
        pairs.clear();
        pairs.append_pair("agent_id", &id);
        if !token.is_empty() {
            pairs.append_pair("token", &token);
        }
    }
    u.set_fragment(None);
    Ok(u.to_string())
}

/// Status and logs keep the socket address, with the token value replaced.
pub fn redact_relay_url(url: &str) -> String {
    let Ok(mut u) = Url::parse(url) else {
        return url.to_string();
    };
    let pairs: Vec<(String, String)> = u
        .query_pairs()
        .map(|(k, v)| {
            let key = k.into_owned();
            let val = if key == "token" && !v.is_empty() {
                "redacted".to_string()
            } else {
                v.into_owned()
            };
            (key, val)
        })
        .collect();
    if pairs.is_empty() {
        return u.to_string();
    }
    {
        let mut q = u.query_pairs_mut();
        q.clear();
        for (k, v) in &pairs {
            q.append_pair(k, v);
        }
    }
    u.to_string()
}

fn token_from_ws_url(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| {
            u.query_pairs()
                .find(|(k, _)| k == "token")
                .map(|(_, v)| v.into_owned())
        })
        .unwrap_or_default()
}

/// One inbound WebSocket body → one stdin line, or drop (`ping` / blank).
pub fn inbound_frame_to_line(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("ping") {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// One stdout line → one WebSocket text body, or drop blanks.
pub fn stdout_line_to_frame(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn status() -> AtlasRelayFleetStatus {
    snapshot()
}

/// Clamp a health-check interval. `0` (unset) is the 15s default; other
/// values sit in 5–300 seconds.
pub fn health_interval_secs(raw: u32) -> u64 {
    match raw as u64 {
        0 => DEFAULT_HEALTH_SECS,
        n => n.clamp(MIN_HEALTH_SECS, MAX_HEALTH_SECS),
    }
}

/// Update the live Ping interval. Does not restart the bridge.
pub fn set_health_secs(raw: u32) {
    HEALTH_SECS.store(health_interval_secs(raw), Ordering::SeqCst);
}

fn current_health_secs() -> u64 {
    let n = HEALTH_SECS.load(Ordering::SeqCst);
    if n == 0 {
        DEFAULT_HEALTH_SECS
    } else {
        n
    }
}

/// Fold a legacy single relay target into the agent list, then drop empty
/// and duplicate ids. Returns true when the settings object changed.
pub fn migrate_relay_agent_settings(settings: &mut crate::store::AppSettings) -> bool {
    let mut changed = false;
    if settings.atlas_relay_agents.is_empty() {
        let id = sanitize_agent_id(settings.atlas_relay_agent_id.as_deref().unwrap_or(""));
        if !id.is_empty() {
            settings
                .atlas_relay_agents
                .push(crate::store::AtlasRelayAgentRecord {
                    url: settings
                        .atlas_relay_agent_url
                        .clone()
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                    token: sanitize_token(
                        settings.atlas_relay_agent_token.as_deref().unwrap_or(""),
                    ),
                    id,
                    model_id: String::new(),
                    enabled: settings.atlas_relay_agent_enabled,
                });
            changed = true;
        }
    }
    let mut kept = Vec::new();
    let mut seen = HashSet::new();
    for slot in settings.atlas_relay_agents.drain(..) {
        if kept.len() >= MAX_RELAY_AGENTS {
            changed = true;
            continue;
        }
        let id = sanitize_agent_id(&slot.id);
        let url = slot.url.trim().to_string();
        let token = sanitize_token(&slot.token);
        let model_id = sanitize_model_id(&slot.model_id);
        if id.is_empty() || !seen.insert(id.clone()) {
            changed = true;
            continue;
        }
        if id != slot.id || url != slot.url || token != slot.token || model_id != slot.model_id {
            changed = true;
        }
        kept.push(crate::store::AtlasRelayAgentRecord {
            url,
            token,
            id,
            model_id,
            enabled: slot.enabled,
        });
    }
    settings.atlas_relay_agents = kept;
    if settings.atlas_relay_agent_url.is_some()
        || settings.atlas_relay_agent_id.is_some()
        || settings.atlas_relay_agent_token.is_some()
        || settings.atlas_relay_agent_enabled
    {
        settings.atlas_relay_agent_url = None;
        settings.atlas_relay_agent_id = None;
        settings.atlas_relay_agent_token = None;
        settings.atlas_relay_agent_enabled = false;
        changed = true;
    }
    changed
}

/// Reject a fifth agent and any repeated id. Empty ids are rejected too.
pub fn validate_relay_agents(agents: &[crate::store::AtlasRelayAgentRecord]) -> Result<(), String> {
    if agents.len() > MAX_RELAY_AGENTS {
        return Err("at most 4 relay agents".into());
    }
    let mut seen = HashSet::new();
    for slot in agents {
        let id = sanitize_agent_id(&slot.id);
        if id.is_empty() {
            return Err("empty agent id".into());
        }
        if !seen.insert(id) {
            return Err("duplicate agent id".into());
        }
    }
    Ok(())
}

fn live_model(agent_id: &str) -> String {
    with_hub(|h| {
        h.slots
            .get(agent_id)
            .map(|s| s.model_id.clone())
            .unwrap_or_default()
    })
}

fn remember_session(agent_id: &str, frame: &str, inbound: bool) {
    let Some(session_id) = session_id_from_frame(frame, inbound) else {
        return;
    };
    with_hub(|h| {
        if let Some(slot) = h.slots.get_mut(agent_id) {
            slot.sessions.insert(session_id);
        }
    });
}

/// Remember the catalog id and, when it changes to a real id, switch sessions
/// the current CLI already has. Does not restart the process or the socket.
fn set_live_model(agent_id: &str, model_id: &str) {
    let model_id = sanitize_model_id(model_id);
    with_hub(|h| {
        let Some(slot) = h.slots.get_mut(agent_id) else {
            return;
        };
        if slot.model_id == model_id {
            return;
        }
        slot.model_id = model_id.clone();
        if model_id.is_empty() {
            return;
        }
        let Some(tx) = slot.inject_tx.clone() else {
            return;
        };
        let sessions: Vec<String> = slot.sessions.iter().cloned().collect();
        for session_id in sessions {
            let n = INJECT_SEQ.fetch_add(1, Ordering::SeqCst);
            let frame = session_set_model_line(&relay_model_inject_id(n), &session_id, &model_id);
            let _ = tx.send(frame);
        }
    });
}

/// Start or replace one agent. Other agents keep running.
pub fn start(raw_url: &str, agent_id: &str, token: &str, model_id: &str) -> Result<(), String> {
    let saved = crate::store::load_settings();
    set_health_secs(saved.atlas_relay_agent_health_secs);
    let id = sanitize_agent_id(agent_id);
    if id.is_empty() {
        return Err("empty agent id".into());
    }
    let ws_url = relay_agent_ws_url(raw_url, &id, token)?;
    let model_id = sanitize_model_id(model_id);
    let (gen, old_pid, cancel) = with_hub(|h| {
        if !h.slots.contains_key(&id) && h.slots.len() >= MAX_RELAY_AGENTS {
            return Err("at most 4 relay agents".to_string());
        }
        let gen = h.next_gen;
        h.next_gen = h.next_gen.saturating_add(1);
        let slot = h.slots.entry(id.clone()).or_insert_with(|| Slot {
            gen: 0,
            pid: 0,
            phase: "stopped".into(),
            error: None,
            cli_alive: false,
            agent_id: id.clone(),
            url: String::new(),
            model_id: String::new(),
            sessions: HashSet::new(),
            pending_cwd: HashMap::new(),
            session_cwd: HashMap::new(),
            inject_tx: None,
            tasks: Vec::new(),
            cancel: Arc::new(Notify::new()),
        });
        let old_pid = std::mem::take(&mut slot.pid);
        slot.gen = gen;
        slot.phase = "connecting".into();
        slot.error = None;
        slot.cli_alive = false;
        slot.agent_id = id.clone();
        slot.url = ws_url.clone();
        slot.model_id = model_id.clone();
        clear_cli_memory(slot);
        slot.inject_tx = None;
        slot.cancel.notify_waiters();
        Ok((gen, old_pid, slot.cancel.clone()))
    })?;
    kill_pid(old_pid);
    tauri::async_runtime::spawn(run_forever(id, gen, ws_url, cancel));
    Ok(())
}

/// Stop one agent and keep its task log. Other agents keep running.
pub fn stop_agent(agent_id: &str) {
    let id = sanitize_agent_id(agent_id);
    if id.is_empty() {
        return;
    }
    let pid = with_hub(|h| {
        let Some(slot) = h.slots.get_mut(&id) else {
            return 0;
        };
        slot.gen = h.next_gen;
        h.next_gen = h.next_gen.saturating_add(1);
        slot.phase = "stopped".into();
        slot.error = None;
        slot.cli_alive = false;
        slot.cancel.notify_waiters();
        std::mem::take(&mut slot.pid)
    });
    kill_pid(pid);
}

fn drop_agent(agent_id: &str) {
    stop_agent(agent_id);
    let id = sanitize_agent_id(agent_id);
    with_hub(|h| {
        h.slots.remove(&id);
    });
}

/// Reconcile running bridges with the saved list. One row's address change
/// restarts only that row.
pub fn apply_settings(settings: &crate::store::AppSettings) {
    set_health_secs(settings.atlas_relay_agent_health_secs);
    let saved_ids: HashSet<String> = settings
        .atlas_relay_agents
        .iter()
        .map(|s| sanitize_agent_id(&s.id))
        .filter(|id| !id.is_empty())
        .collect();
    let running: Vec<String> = with_hub(|h| h.slots.keys().cloned().collect());
    for id in running {
        if !saved_ids.contains(&id) {
            drop_agent(&id);
        }
    }
    for slot in &settings.atlas_relay_agents {
        let id = sanitize_agent_id(&slot.id);
        if id.is_empty() {
            continue;
        }
        if !slot.enabled || slot.url.trim().is_empty() {
            stop_agent(&id);
            continue;
        }
        let Ok(ws) = relay_agent_ws_url(slot.url.trim(), &id, &slot.token) else {
            stop_agent(&id);
            continue;
        };
        let same = with_hub(|h| {
            h.slots
                .get(&id)
                .map(|s| s.phase != "stopped" && s.url == ws)
                .unwrap_or(false)
        });
        if same {
            set_live_model(&id, &sanitize_model_id(&slot.model_id));
            continue;
        }
        if let Err(e) = start(
            slot.url.trim(),
            &id,
            &slot.token,
            &sanitize_model_id(&slot.model_id),
        ) {
            with_hub(|h| {
                if let Some(s) = h.slots.get_mut(&id) {
                    s.phase = "stopped".into();
                    s.error = Some(e);
                    s.cli_alive = false;
                }
            });
        }
    }
}

pub async fn autostart_from_settings() {
    let settings = crate::store::load_settings();
    if settings.atlas_relay_agents.iter().any(|s| s.enabled) {
        apply_settings(&settings);
    }
}

fn kill_pid(pid: u32) {
    if pid != 0 {
        tauri::async_runtime::spawn(async move {
            crate::process_util::kill_process_tree_async(pid).await;
        });
    }
}

fn slot_is_current(agent_id: &str, gen: u64) -> bool {
    with_hub(|h| h.slots.get(agent_id).map(|s| s.gen == gen).unwrap_or(false))
}

async fn cancelled(agent_id: &str, gen: u64, notify: &Notify) {
    loop {
        notify.notified().await;
        if !slot_is_current(agent_id, gen) {
            return;
        }
    }
}

enum RunEnd {
    Stopped,
    Disconnected(Option<String>),
}

async fn run_forever(agent_id: String, gen: u64, ws_url: String, cancel: Arc<Notify>) {
    if !slot_is_current(&agent_id, gen) {
        return;
    }
    let mut delay_secs = 1u64;
    let mut first = true;
    loop {
        if !slot_is_current(&agent_id, gen) {
            return;
        }
        with_hub(|h| {
            if let Some(slot) = h.slots.get_mut(&agent_id) {
                if slot.gen == gen {
                    slot.phase = if first {
                        "connecting".into()
                    } else {
                        "reconnecting".into()
                    };
                    slot.cli_alive = false;
                }
            }
        });
        first = false;
        match run_once(&agent_id, gen, &ws_url, cancel.as_ref()).await {
            RunEnd::Stopped => return,
            RunEnd::Disconnected(err) => {
                if !slot_is_current(&agent_id, gen) {
                    return;
                }
                with_hub(|h| {
                    if let Some(slot) = h.slots.get_mut(&agent_id) {
                        if slot.gen == gen {
                            slot.phase = "reconnecting".into();
                            slot.cli_alive = false;
                            slot.error = err;
                        }
                    }
                });
                let sleep = tokio::time::sleep(Duration::from_secs(delay_secs));
                tokio::pin!(sleep);
                tokio::select! {
                    _ = &mut sleep => {}
                    _ = cancelled(&agent_id, gen, cancel.as_ref()) => return,
                }
                delay_secs = delay_secs.saturating_mul(2).min(MAX_BACKOFF_SECS);
            }
        }
    }
}

async fn run_once(agent_id: &str, gen: u64, ws_url: &str, cancel: &Notify) -> RunEnd {
    if !slot_is_current(agent_id, gen) {
        return RunEnd::Stopped;
    }
    let model_id = live_model(agent_id);
    let mut spawned = match spawn_stdio_agent(agent_id, &model_id).await {
        Ok(spawned) => spawned,
        Err(e) => return RunEnd::Disconnected(Some(e)),
    };
    let pid = spawned.pid;
    with_hub(|h| {
        if let Some(slot) = h.slots.get_mut(agent_id) {
            if slot.gen == gen {
                slot.pid = pid;
                slot.cli_alive = true;
            }
        }
    });
    if !slot_is_current(agent_id, gen) {
        finish_child(agent_id, &mut spawned.child, pid).await;
        return RunEnd::Stopped;
    }

    let ws = tokio::select! {
        _ = cancelled(agent_id, gen, cancel) => {
            finish_child(agent_id, &mut spawned.child, pid).await;
            return RunEnd::Stopped;
        }
        _ = tokio::time::sleep(CONNECT_TIMEOUT) => {
            finish_child(agent_id, &mut spawned.child, pid).await;
            return RunEnd::Disconnected(Some(format!(
                "connect timed out ({}s)",
                CONNECT_TIMEOUT.as_secs()
            )));
        }
        result = connect_async(ws_url) => {
            match result {
                Ok((ws, _)) => ws,
                Err(e) => {
                    finish_child(agent_id, &mut spawned.child, pid).await;
                    return RunEnd::Disconnected(Some(format!("connect failed: {e}")));
                }
            }
        }
    };
    if !slot_is_current(agent_id, gen) {
        finish_child(agent_id, &mut spawned.child, pid).await;
        return RunEnd::Stopped;
    }
    with_hub(|h| {
        if let Some(slot) = h.slots.get_mut(agent_id) {
            if slot.gen == gen {
                slot.phase = "online".into();
                slot.error = None;
                slot.cli_alive = true;
            }
        }
    });
    let safe_url = redact_relay_url(ws_url);
    tracing::info!(url = %safe_url, pid, agent_id, "atlas relay agent online");

    let stdin = spawned.stdin;
    let stdout = spawned.stdout;
    let stderr = spawned.stderr;
    let end = bridge(
        agent_id,
        gen,
        cancel,
        ws,
        stdin,
        stdout,
        stderr,
        &mut spawned.child,
    )
    .await;
    finish_child(agent_id, &mut spawned.child, pid).await;
    end
}

struct RelayProc {
    child: Child,
    pid: u32,
    stdin: Box<dyn AsyncWrite + Unpin + Send>,
    stdout: Box<dyn AsyncRead + Unpin + Send>,
    stderr: Option<Box<dyn AsyncRead + Unpin + Send>>,
}

async fn finish_child(agent_id: &str, child: &mut Child, pid: u32) {
    with_hub(|h| {
        if let Some(slot) = h.slots.get_mut(agent_id) {
            if pid != 0 && slot.pid == pid {
                slot.pid = 0;
                slot.cli_alive = false;
            }
        }
    });
    if pid != 0 {
        crate::process_util::kill_process_tree_async(pid).await;
    }
    let _ = child.start_kill();
    let _ = child.wait().await;
}

async fn spawn_stdio_agent(agent_id: &str, model_id: &str) -> Result<RelayProc, String> {
    match crate::agent_runtimes::relay_runtime(agent_id) {
        crate::agent_runtimes::RuntimeChoice::Ready(mut profile) => {
            if !model_id.is_empty() {
                profile.model = model_id.to_string();
            }
            let cwd = if profile.home.is_empty() {
                std::env::temp_dir()
            } else {
                std::path::PathBuf::from(&profile.home)
            };
            let pipe = crate::agent_runtimes::spawn_acp_pipe(&profile, &cwd, None).await?;
            return Ok(RelayProc {
                pid: pipe.pid,
                stdin: pipe.stdin,
                stdout: pipe.stdout,
                stderr: pipe.stderr,
                child: pipe.child,
            });
        }
        crate::agent_runtimes::RuntimeChoice::Broken(message) => return Err(message),
        crate::agent_runtimes::RuntimeChoice::Legacy => {}
    }
    let settings = crate::store::load_settings();
    let probe = crate::cli_probe::probe_cli(settings.manual_cli_path.as_deref());
    let cli_path = probe
        .path
        .filter(|_| probe.found)
        .ok_or_else(|| "Atlas CLI not found".to_string())?;
    let home: PathBuf = crate::paths::resolve_agent_grok_home(&settings.session_data_mode);
    let mut cmd = Command::new(&cli_path);
    cmd.arg("agent").arg("--always-approve");
    if !model_id.is_empty() {
        cmd.arg("--model").arg(model_id);
    }
    cmd.arg("stdio");
    crate::process_util::apply_process_group_no_window_tokio(&mut cmd);
    crate::process_util::apply_cli_env_tokio(&mut cmd);
    cmd.env("GROK_HOME", &home);
    cmd.env("GROK_CLAUDE_MCPS_ENABLED", "false");
    cmd.env("GROK_CURSOR_MCPS_ENABLED", "false");
    crate::proxy::apply_to_tokio_command(&mut cmd);
    if home.is_dir() {
        cmd.current_dir(&home);
    }
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn Atlas agent stdio: {e}"))?;
    let pid = child.id().unwrap_or(0);
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "CLI stdin missing".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "CLI stdout missing".to_string())?;
    let stderr = child.stderr.take();
    Ok(RelayProc {
        child,
        pid,
        stdin: Box::new(stdin),
        stdout: Box::new(stdout),
        stderr: stderr.map(|pipe| Box::new(pipe) as Box<dyn AsyncRead + Unpin + Send>),
    })
}

async fn bridge(
    agent_id: &str,
    gen: u64,
    cancel: &Notify,
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    mut stdin: Box<dyn AsyncWrite + Unpin + Send>,
    stdout: Box<dyn AsyncRead + Unpin + Send>,
    stderr: Option<Box<dyn AsyncRead + Unpin + Send>>,
    child: &mut Child,
) -> RunEnd {
    let (mut sink, mut stream) = ws.split();
    let (line_tx, mut line_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let stdout_task = tokio::spawn(async move {
        let mut lines = BufReader::with_capacity(8 * 1024 * 1024, stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match lines.read_line(&mut line).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if let Some(frame) = stdout_line_to_frame(&line) {
                        if line_tx.send(frame).is_err() {
                            break;
                        }
                    }
                }
            }
        }
    });
    let stderr_task = stderr.map(|stderr| {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match lines.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let t = line.trim();
                        if !t.is_empty() {
                            tracing::debug!(target: "atlas_relay_agent", "{t}");
                        }
                    }
                }
            }
        })
    });

    let (inject_tx, mut inject_rx) = mpsc::unbounded_channel::<String>();
    with_hub(|h| {
        if let Some(slot) = h.slots.get_mut(agent_id) {
            if slot.gen == gen {
                slot.inject_tx = Some(inject_tx);
                clear_cli_memory(slot);
            }
        }
    });

    let health_sleep = tokio::time::sleep(Duration::from_secs(current_health_secs()));
    tokio::pin!(health_sleep);
    let mut awaiting_pong = false;
    let mut inject_open = true;

    let end = loop {
        tokio::select! {
            _ = cancelled(agent_id, gen, cancel) => break RunEnd::Stopped,
            _ = health_sleep.as_mut() => {
                if awaiting_pong {
                    break RunEnd::Disconnected(Some("health check timed out".into()));
                }
                if sink
                    .send(Message::Ping(Vec::<u8>::new().into()))
                    .await
                    .is_err()
                {
                    break RunEnd::Disconnected(Some("relay closed".into()));
                }
                awaiting_pong = true;
                health_sleep.as_mut().reset(
                    tokio::time::Instant::now()
                        + Duration::from_secs(current_health_secs()),
                );
            }
            incoming = stream.next() => {
                match incoming {
                    Some(Ok(Message::Text(t))) => {
                        if let Some(body) = inbound_frame_to_line(&t) {
                            if write_cloud_frame(agent_id, &mut stdin, &body).await.is_err() {
                                break RunEnd::Disconnected(Some("CLI stdin closed".into()));
                            }
                        }
                    }
                    Some(Ok(Message::Binary(b))) => {
                        if let Ok(text) = std::str::from_utf8(&b) {
                            if let Some(body) = inbound_frame_to_line(text) {
                                if write_cloud_frame(agent_id, &mut stdin, &body).await.is_err() {
                                    break RunEnd::Disconnected(Some("CLI stdin closed".into()));
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Ping(p))) => {
                        if sink.send(Message::Pong(p)).await.is_err() {
                            break RunEnd::Disconnected(Some("relay closed".into()));
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        awaiting_pong = false;
                    }
                    Some(Ok(Message::Frame(_))) => {}
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => {
                        break RunEnd::Disconnected(Some("relay disconnected".into()));
                    }
                }
            }
            outgoing = line_rx.recv() => {
                match outgoing {
                    Some(frame) => {
                        if is_relay_model_inject_frame(&frame) {
                            continue;
                        }
                        remember_session(agent_id, &frame, false);
                        observe_frame(agent_id, &frame, false);
                        if sink.send(Message::Text(frame.into())).await.is_err() {
                            break RunEnd::Disconnected(Some("relay closed".into()));
                        }
                    }
                    None => break RunEnd::Disconnected(Some("CLI stdout closed".into())),
                }
            }
            injected = inject_rx.recv(), if inject_open => {
                match injected {
                    Some(body) => {
                        if write_line(&mut stdin, &body).await.is_err() {
                            break RunEnd::Disconnected(Some("CLI stdin closed".into()));
                        }
                    }
                    None => inject_open = false,
                }
            }
            status = child.wait() => {
                let code = status.ok().and_then(|s| s.code());
                break RunEnd::Disconnected(Some(format!("CLI exited ({code:?})")));
            }
        }
    };
    stdout_task.abort();
    if let Some(task) = stderr_task {
        task.abort();
    }
    with_hub(|h| {
        if let Some(slot) = h.slots.get_mut(agent_id) {
            if slot.gen == gen {
                slot.inject_tx = None;
                clear_cli_memory(slot);
            }
        }
    });
    let _ = stdin.shutdown().await;
    end
}

async fn write_cloud_frame(
    agent_id: &str,
    stdin: &mut (dyn AsyncWrite + Unpin + Send),
    body: &str,
) -> std::io::Result<()> {
    remember_session(agent_id, body, true);
    let body = apply_connection_model(body, &live_model(agent_id));
    observe_frame(agent_id, &body, true);
    write_line(stdin, &body).await
}

async fn write_line(
    stdin: &mut (dyn AsyncWrite + Unpin + Send),
    body: &str,
) -> std::io::Result<()> {
    stdin.write_all(body.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await
}

#[tauri::command]
pub fn atlas_relay_agent_status() -> AtlasRelayFleetStatus {
    status()
}

fn empty_task_trace(agent_id: String, task_id: String) -> AtlasRelayTaskTrace {
    AtlasRelayTaskTrace {
        found: false,
        id: task_id,
        agent_id,
        text: String::new(),
        status: String::new(),
        error: None,
        steps: Vec::new(),
        session_id: String::new(),
        project_id: String::new(),
        open_session: false,
    }
}

fn session_has_running(hub: &Hub, session_id: &str) -> bool {
    if session_id.is_empty() {
        return false;
    }
    hub.slots.values().any(|slot| {
        slot.tasks
            .iter()
            .any(|task| task.status == "running" && task.session_id == session_id)
    })
}

/// Folded run for one cloud prompt. Missing ids return an empty trace.
#[tauri::command]
pub fn atlas_relay_task_trace(agent_id: String, task_id: String) -> AtlasRelayTaskTrace {
    let agent_id = sanitize_agent_id(&agent_id);
    let task_id = task_id.trim().to_string();
    if agent_id.is_empty() || task_id.is_empty() {
        return empty_task_trace(agent_id, task_id);
    }
    with_hub(|h| {
        let snap = {
            let Some(slot) = h.slots.get(&agent_id) else {
                return empty_task_trace(agent_id.clone(), task_id.clone());
            };
            let Some(task) = slot.tasks.iter().find(|task| task.id == task_id) else {
                return empty_task_trace(agent_id.clone(), task_id.clone());
            };
            (
                task.id.clone(),
                if task.agent_id.is_empty() {
                    agent_id.clone()
                } else {
                    task.agent_id.clone()
                },
                task.text.clone(),
                task.status.clone(),
                task.error.clone(),
                task.steps.clone(),
                task.session_id.clone(),
                task.project_id.clone(),
            )
        };
        let (id, traced_agent, text, status, error, steps, session_id, project_id) = snap;
        let open_session = (status == "done" || status == "failed")
            && !session_id.is_empty()
            && !session_has_running(h, &session_id);
        AtlasRelayTaskTrace {
            found: true,
            id,
            agent_id: traced_agent,
            text,
            status,
            error,
            steps,
            session_id,
            project_id,
            open_session,
        }
    })
}

/// Persist this agent, mark it auto-connect, and open only its bridge.
#[tauri::command]
pub async fn atlas_relay_agent_connect(
    url: String,
    agent_id: String,
    token: String,
    model_id: Option<String>,
) -> Result<AtlasRelayFleetStatus, String> {
    let id = sanitize_agent_id(&agent_id);
    if id.is_empty() {
        return Err("empty agent id".into());
    }
    let model = sanitize_model_id(model_id.as_deref().unwrap_or(""));
    let given = token;
    let ws = relay_agent_ws_url(url.trim(), &id, &given)?;
    let stored = token_from_ws_url(&ws);
    let mut settings = crate::store::load_settings();
    if let Some(slot) = settings
        .atlas_relay_agents
        .iter_mut()
        .find(|s| sanitize_agent_id(&s.id) == id)
    {
        slot.url = url.trim().to_string();
        slot.token = stored.clone();
        slot.id = id.clone();
        slot.model_id = model.clone();
        slot.enabled = true;
    } else {
        if settings.atlas_relay_agents.len() >= MAX_RELAY_AGENTS {
            return Err("at most 4 relay agents".into());
        }
        settings
            .atlas_relay_agents
            .push(crate::store::AtlasRelayAgentRecord {
                url: url.trim().to_string(),
                token: stored.clone(),
                id: id.clone(),
                model_id: model.clone(),
                enabled: true,
            });
    }
    validate_relay_agents(&settings.atlas_relay_agents)?;
    crate::store::save_settings_async(&settings).await?;
    start(url.trim(), &id, &stored, &model)?;
    Ok(status())
}

/// Save this connection's catalog id. An online bridge keeps its socket and CLI.
#[tauri::command]
pub async fn atlas_relay_agent_set_model(
    agent_id: String,
    model_id: String,
) -> Result<AtlasRelayFleetStatus, String> {
    let id = sanitize_agent_id(&agent_id);
    if id.is_empty() {
        return Err("empty agent id".into());
    }
    let model = sanitize_model_id(&model_id);
    let mut settings = crate::store::load_settings();
    let Some(slot) = settings
        .atlas_relay_agents
        .iter_mut()
        .find(|s| sanitize_agent_id(&s.id) == id)
    else {
        return Err("relay agent is not saved".into());
    };
    slot.model_id = model.clone();
    crate::store::save_settings_async(&settings).await?;
    set_live_model(&id, &model);
    Ok(status())
}

/// Stop one agent and turn off its auto-connect. The row stays in the list.
#[tauri::command]
pub async fn atlas_relay_agent_disconnect(
    agent_id: String,
) -> Result<AtlasRelayFleetStatus, String> {
    let id = sanitize_agent_id(&agent_id);
    if id.is_empty() {
        return Err("empty agent id".into());
    }
    let mut settings = crate::store::load_settings();
    if let Some(slot) = settings
        .atlas_relay_agents
        .iter_mut()
        .find(|s| sanitize_agent_id(&s.id) == id)
    {
        slot.enabled = false;
    }
    crate::store::save_settings_async(&settings).await?;
    stop_agent(&id);
    Ok(status())
}

/// Point one relay agent at a runtime profile and reconnect only that agent.
#[tauri::command]
pub async fn agent_runtimes_set_relay_binding(
    agent_id: String,
    profile_id: String,
) -> Result<crate::agent_runtimes::AgentRuntimesFile, String> {
    let id = sanitize_agent_id(&agent_id);
    if id.is_empty() {
        return Err("empty agent id".into());
    }
    let file = crate::agent_runtimes::set_relay_binding(&id, &profile_id)?;
    let settings = crate::store::load_settings();
    if let Some(slot) = settings
        .atlas_relay_agents
        .iter()
        .find(|s| sanitize_agent_id(&s.id) == id)
    {
        if slot.enabled {
            start(&slot.url, &slot.id, &slot.token, &slot.model_id)?;
        }
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_url_from_page_host_and_existing_socket() {
        assert_eq!(
            relay_agent_ws_url("http://10.1.2.3:2420/build?agent=laptop-a", "laptop-a", "")
                .unwrap(),
            "ws://10.1.2.3:2420/ws?agent_id=laptop-a"
        );
        assert_eq!(
            relay_agent_ws_url("10.1.2.3:2420", "laptop-a", "").unwrap(),
            "ws://10.1.2.3:2420/ws?agent_id=laptop-a"
        );
        assert_eq!(
            relay_agent_ws_url("wss://relay.example:2420/ws/agent", "laptop-b", "").unwrap(),
            "wss://relay.example:2420/ws/agent?agent_id=laptop-b"
        );
        assert_eq!(
            relay_agent_ws_url("https://relay.example/m", "laptop-a", "").unwrap(),
            "wss://relay.example/ws?agent_id=laptop-a"
        );
        assert!(relay_agent_ws_url("ws://10.1.2.3:2420/ws", "", "").is_err());
        assert!(relay_agent_ws_url("", "laptop-a", "").is_err());
        assert!(relay_agent_ws_url("ftp://relay.example/ws", "laptop-a", "").is_err());
    }

    #[test]
    fn agent_url_appends_token_and_keeps_it_out_of_status() {
        let secret = "OGAhRfcbqFwB_4j63SpU3AadbeGakHflr5Q1MobZNVQ";
        let id = "9c7fc43585b0422e9cbfe93407f9b150-1";
        let expected = format!("ws://10.218.221.203:2420/ws?agent_id={id}&token={secret}");
        assert_eq!(
            relay_agent_ws_url("ws://10.218.221.203:2420/ws", id, secret).unwrap(),
            expected
        );
        assert_eq!(
            relay_agent_ws_url(
                &format!("ws://10.218.221.203:2420/ws?agent_id=other&token={secret}"),
                id,
                ""
            )
            .unwrap(),
            expected
        );
        assert_eq!(
            relay_agent_ws_url(
                "ws://10.218.221.203:2420/ws?token=from-url",
                id,
                "from-field"
            )
            .unwrap(),
            format!("ws://10.218.221.203:2420/ws?agent_id={id}&token=from-field")
        );
        let redacted = redact_relay_url(&expected);
        assert!(!redacted.contains(secret));
        assert!(redacted.contains("token=redacted"));
        assert!(redacted.contains(id));
    }

    #[test]
    fn agent_id_strips_newlines_and_caps() {
        assert_eq!(sanitize_agent_id("  laptop-a\n"), "laptop-a");
        assert_eq!(sanitize_agent_id(""), "");
        assert_eq!(sanitize_agent_id(&"a".repeat(200)).len(), 128);
    }

    #[test]
    fn health_interval_clamps_to_five_through_three_hundred() {
        assert_eq!(health_interval_secs(0), 15);
        assert_eq!(health_interval_secs(15), 15);
        assert_eq!(health_interval_secs(1), 5);
        assert_eq!(health_interval_secs(4), 5);
        assert_eq!(health_interval_secs(5), 5);
        assert_eq!(health_interval_secs(300), 300);
        assert_eq!(health_interval_secs(9_999), 300);
    }

    #[test]
    fn frames_are_one_json_each_and_ping_is_dropped() {
        assert!(inbound_frame_to_line("  ping  ").is_none());
        assert!(inbound_frame_to_line("").is_none());
        assert_eq!(
            inbound_frame_to_line("  {\"jsonrpc\":\"2.0\",\"method\":\"session/prompt\"}  ")
                .as_deref(),
            Some("{\"jsonrpc\":\"2.0\",\"method\":\"session/prompt\"}")
        );
        assert!(stdout_line_to_frame("\n").is_none());
        assert_eq!(
            stdout_line_to_frame("{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n").as_deref(),
            Some("{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}")
        );
    }

    #[test]
    fn prompt_tracks_running_done_failed_and_cancelled() {
        let mut tasks = Vec::new();
        note_relay_frame(
            &mut tasks,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
            true,
            1,
        );
        assert!(tasks.is_empty());

        note_relay_frame(
            &mut tasks,
            r#"{"jsonrpc":"2.0","id":5,"method":"session/prompt","params":{"sessionId":"s1","prompt":[{"type":"text","text":"看一下进度"},{"type":"text","text":"第二段"}]}}"#,
            true,
            10,
        );
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "5");
        assert_eq!(tasks[0].session_id, "s1");
        assert_eq!(tasks[0].status, "running");
        assert_eq!(tasks[0].text, "看一下进度\n第二段");

        note_relay_frame(
            &mut tasks,
            r#"{"jsonrpc":"2.0","id":5,"result":{"stopReason":"end_turn"}}"#,
            false,
            11,
        );
        assert_eq!(tasks[0].status, "done");
        assert!(tasks[0].error.is_none());

        note_relay_frame(
            &mut tasks,
            r#"{"jsonrpc":"2.0","id":"6","method":"session/prompt","params":{"sessionId":"s1","prompt":[{"type":"text","text":"second"}]}}"#,
            true,
            12,
        );
        note_relay_frame(
            &mut tasks,
            r#"{"jsonrpc":"2.0","id":"6","error":{"code":-1,"message":"boom"}}"#,
            false,
            13,
        );
        assert_eq!(tasks[0].status, "failed");
        assert_eq!(tasks[0].error.as_deref(), Some("boom"));
        assert_eq!(tasks[1].status, "done");

        note_relay_frame(
            &mut tasks,
            r#"{"jsonrpc":"2.0","id":7,"method":"session/prompt","params":{"sessionId":"s2","prompt":[{"type":"text","text":"third"}]}}"#,
            true,
            14,
        );
        note_relay_frame(
            &mut tasks,
            r#"{"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":"s2"}}"#,
            true,
            15,
        );
        assert_eq!(tasks[0].status, "cancelled");
        assert_eq!(tasks[1].status, "failed");

        let long = "字".repeat(300);
        let frame = format!(
            r#"{{"jsonrpc":"2.0","id":8,"method":"session/prompt","params":{{"sessionId":"s3","prompt":[{{"type":"text","text":"{long}"}}]}}}}"#
        );
        note_relay_frame(&mut tasks, &frame, true, 16);
        assert_eq!(tasks[0].text.chars().count(), MAX_TASK_TEXT);
        note_relay_frame(&mut tasks, "{not-json", true, 17);
        assert_eq!(tasks[0].id, "8");
    }

    #[test]
    fn session_update_folds_into_the_running_task() {
        let mut tasks = Vec::new();
        note_relay_frame(
            &mut tasks,
            r#"{"jsonrpc":"2.0","id":5,"method":"session/prompt","params":{"sessionId":"s1","prompt":[{"type":"text","text":"look"}]}}"#,
            true,
            1,
        );
        note_relay_frame(
            &mut tasks,
            r#"{"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"Think"}}}}"#,
            false,
            2,
        );
        note_relay_frame(
            &mut tasks,
            r#"{"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"ing"}}}}"#,
            false,
            3,
        );
        note_relay_frame(
            &mut tasks,
            r#"{"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Hello "}}}}"#,
            false,
            4,
        );
        note_relay_frame(
            &mut tasks,
            r#"{"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"world."}}}}"#,
            false,
            5,
        );
        note_relay_frame(
            &mut tasks,
            r#"{"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"tool_call","toolCallId":"tc-1","title":"read","kind":"read","status":"pending"}}}"#,
            false,
            6,
        );
        note_relay_frame(
            &mut tasks,
            r#"{"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"tool_call_update","toolCallId":"tc-1","title":"read notes.md","status":"completed"}}}"#,
            false,
            7,
        );
        assert_eq!(tasks[0].steps.len(), 3);
        assert_eq!(tasks[0].steps[0].kind, "thought");
        assert_eq!(tasks[0].steps[0].text, "Thinking");
        assert_eq!(tasks[0].steps[1].kind, "message");
        assert_eq!(tasks[0].steps[1].text, "Hello world.");
        assert_eq!(tasks[0].steps[2].kind, "tool");
        assert_eq!(tasks[0].steps[2].tool_call_id.as_deref(), Some("tc-1"));
        assert_eq!(tasks[0].steps[2].status.as_deref(), Some("completed"));
        assert_eq!(tasks[0].steps[2].text, "read notes.md");
        let snap = serde_json::to_value(&tasks[0]).unwrap();
        assert!(snap.get("steps").is_none());
        assert_eq!(snap.get("text").and_then(|v| v.as_str()), Some("look"));

        note_relay_frame(
            &mut tasks,
            r#"{"jsonrpc":"2.0","id":5,"result":{"stopReason":"end_turn"}}"#,
            false,
            8,
        );
        note_relay_frame(
            &mut tasks,
            r#"{"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"late"}}}}"#,
            false,
            9,
        );
        assert_eq!(tasks[0].status, "done");
        assert_eq!(tasks[0].steps[1].text, "Hello world.");

        note_relay_frame(
            &mut tasks,
            r#"{"jsonrpc":"2.0","id":6,"method":"session/prompt","params":{"sessionId":"s1","prompt":[{"type":"text","text":"next"}]}}"#,
            true,
            10,
        );
        note_relay_frame(
            &mut tasks,
            r#"{"jsonrpc":"2.0","id":7,"method":"session/prompt","params":{"sessionId":"s2","prompt":[{"type":"text","text":"other"}]}}"#,
            true,
            11,
        );
        note_relay_frame(
            &mut tasks,
            r#"{"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"only-s1"}}}}"#,
            false,
            12,
        );
        let s1 = tasks.iter().find(|t| t.id == "6").unwrap();
        let s2 = tasks.iter().find(|t| t.id == "7").unwrap();
        let done = tasks.iter().find(|t| t.id == "5").unwrap();
        assert_eq!(s1.steps.len(), 1);
        assert_eq!(s1.steps[0].text, "only-s1");
        assert!(s2.steps.is_empty());
        assert_eq!(done.steps[1].text, "Hello world.");

        let long = "a".repeat(MAX_STEP_TEXT + 40);
        let frame = format!(
            r#"{{"method":"session/update","params":{{"sessionId":"s2","update":{{"sessionUpdate":"agent_message_chunk","content":{{"type":"text","text":"{long}"}}}}}}}}"#
        );
        note_relay_frame(&mut tasks, &frame, false, 13);
        assert_eq!(
            tasks.iter().find(|t| t.id == "7").unwrap().steps[0]
                .text
                .chars()
                .count(),
            MAX_STEP_TEXT
        );
    }

    #[test]
    fn legacy_relay_target_becomes_the_first_agent() {
        let mut settings = crate::store::AppSettings::default();
        settings.atlas_relay_agent_url = Some("ws://10.218.221.203:2420".into());
        settings.atlas_relay_agent_id = Some("9c7fc43585b0422e9cbfe93407f9b150-1".into());
        settings.atlas_relay_agent_token = Some("tok".into());
        settings.atlas_relay_agent_enabled = true;
        assert!(migrate_relay_agent_settings(&mut settings));
        assert_eq!(settings.atlas_relay_agents.len(), 1);
        assert_eq!(
            settings.atlas_relay_agents[0].id,
            "9c7fc43585b0422e9cbfe93407f9b150-1"
        );
        assert_eq!(
            settings.atlas_relay_agents[0].url,
            "ws://10.218.221.203:2420"
        );
        assert_eq!(settings.atlas_relay_agents[0].token, "tok");
        assert!(settings.atlas_relay_agents[0].enabled);
        assert!(settings.atlas_relay_agent_id.is_none());
        assert!(!settings.atlas_relay_agent_enabled);
        assert!(!migrate_relay_agent_settings(&mut settings));
    }

    #[test]
    fn relay_agents_reject_a_fifth_and_a_duplicate_id() {
        let mut agents = Vec::new();
        for i in 0..4 {
            agents.push(crate::store::AtlasRelayAgentRecord {
                url: format!("ws://10.0.0.{i}:2420"),
                token: String::new(),
                id: format!("a{i}"),
                model_id: String::new(),
                enabled: false,
            });
        }
        assert!(validate_relay_agents(&agents).is_ok());
        agents.push(crate::store::AtlasRelayAgentRecord {
            url: "ws://10.9.9.9:2420".into(),
            token: String::new(),
            id: "a9".into(),
            model_id: String::new(),
            enabled: false,
        });
        assert!(validate_relay_agents(&agents).unwrap_err().contains("4"));
        agents.pop();
        agents[3].id = "a0".into();
        agents[3].url = "ws://other.example:2420".into();
        assert!(validate_relay_agents(&agents)
            .unwrap_err()
            .contains("duplicate"));
    }

    #[test]
    fn connection_model_rewrites_new_and_set_model_only() {
        let new_frame = r#"{"jsonrpc":"2.0","id":2,"method":"session/new","params":{"cwd":".","_meta":{"modelId":"cloud-model","yoloMode":true}}}"#;
        let rewritten = apply_connection_model(new_frame, "arch-kimi-code-eric");
        let v: serde_json::Value = serde_json::from_str(&rewritten).unwrap();
        assert_eq!(
            v.pointer("/params/_meta/modelId").and_then(|s| s.as_str()),
            Some("arch-kimi-code-eric")
        );
        assert_eq!(
            v.pointer("/params/_meta/yoloMode")
                .and_then(|b| b.as_bool()),
            Some(true)
        );

        let set_frame = r#"{"jsonrpc":"2.0","id":4,"method":"session/set_model","params":{"sessionId":"s1","modelId":"cloud-model"}}"#;
        let rewritten = apply_connection_model(set_frame, " arch-kimi-code-eric ");
        let v: serde_json::Value = serde_json::from_str(&rewritten).unwrap();
        assert_eq!(
            v.pointer("/params/modelId").and_then(|s| s.as_str()),
            Some("arch-kimi-code-eric")
        );
        assert_eq!(
            v.pointer("/params/sessionId").and_then(|s| s.as_str()),
            Some("s1")
        );

        let prompt =
            r#"{"jsonrpc":"2.0","id":5,"method":"session/prompt","params":{"sessionId":"s1"}}"#;
        assert_eq!(
            apply_connection_model(prompt, "arch-kimi-code-eric"),
            prompt
        );
        assert_eq!(apply_connection_model(set_frame, "  "), set_frame);
        assert_eq!(apply_connection_model(set_frame, ""), set_frame);
    }

    #[test]
    fn relay_model_inject_reply_is_recognized() {
        let id = relay_model_inject_id(3);
        let frame = session_set_model_line(&id, "s1", "arch-kimi-code-eric");
        assert!(is_relay_model_inject_frame(&frame));
        let reply = format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":{{}}}}"#);
        assert!(is_relay_model_inject_frame(&reply));
        assert!(!is_relay_model_inject_frame(
            r#"{"jsonrpc":"2.0","id":5,"result":{}}"#
        ));
    }

    #[test]
    fn session_ids_come_from_load_prompt_and_new_result() {
        assert_eq!(
            session_id_from_frame(
                r#"{"method":"session/load","params":{"sessionId":"s1"}}"#,
                true
            )
            .as_deref(),
            Some("s1")
        );
        assert_eq!(
            session_id_from_frame(
                r#"{"method":"session/prompt","params":{"sessionId":"s2"}}"#,
                true
            )
            .as_deref(),
            Some("s2")
        );
        assert_eq!(
            session_id_from_frame(
                r#"{"method":"session/new","params":{"_meta":{"sessionId":"018f"}}}"#,
                true
            )
            .as_deref(),
            Some("018f")
        );
        assert_eq!(
            session_id_from_frame(r#"{"id":2,"result":{"sessionId":"s-new"}}"#, false).as_deref(),
            Some("s-new")
        );
        assert!(
            session_id_from_frame(r#"{"method":"session/prompt","params":{}}"#, true).is_none()
        );
    }

    #[test]
    fn prompt_inherits_cwd_from_session_new_unless_it_sends_its_own() {
        let mut pending = std::collections::HashMap::new();
        let mut by_session = std::collections::HashMap::new();
        note_cwd_frame(
            &mut pending,
            &mut by_session,
            r#"{"jsonrpc":"2.0","id":1,"method":"session/new","params":{"cwd":"C:/work/app"}}"#,
            true,
        );
        assert!(by_session.is_empty());
        note_cwd_frame(
            &mut pending,
            &mut by_session,
            r#"{"jsonrpc":"2.0","id":1,"result":{"sessionId":"s1"}}"#,
            false,
        );
        let prompt = r#"{"jsonrpc":"2.0","id":5,"method":"session/prompt","params":{"sessionId":"s1","prompt":[{"type":"text","text":"look"}]}}"#;
        assert_eq!(prompt_cwd(prompt, &by_session), "C:/work/app");

        let own = r#"{"jsonrpc":"2.0","id":6,"method":"session/prompt","params":{"sessionId":"s1","cwd":"D:/other","prompt":[{"type":"text","text":"look"}]}}"#;
        assert_eq!(prompt_cwd(own, &by_session), "D:/other");

        let meta = r#"{"jsonrpc":"2.0","id":7,"method":"session/prompt","params":{"sessionId":"s1","_meta":{"cwd":"E:/meta"},"prompt":[{"type":"text","text":"look"}]}}"#;
        assert_eq!(prompt_cwd(meta, &by_session), "E:/meta");
    }

    #[test]
    fn relative_cwd_does_not_resolve_a_project() {
        assert!(resolve_relay_project(".").is_none());
        assert!(resolve_relay_project("foo").is_none());
        assert!(resolve_relay_project("").is_none());
        assert!(cwd_paths_match(r"C:\Work\App\", "c:/work/app"));
        assert!(!cwd_paths_match(r"C:\Work\App", r"C:\Work\Other"));
    }

    #[test]
    fn existing_directory_matches_once_and_is_not_added_twice() {
        struct Restore(Option<String>, std::path::PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) {
                match self.0.take() {
                    Some(v) => std::env::set_var("GROK_APP_HOME", v),
                    None => std::env::remove_var("GROK_APP_HOME"),
                }
                let _ = std::fs::remove_dir_all(&self.1);
            }
        }
        let _home_lock = crate::paths::APP_HOME_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = std::env::temp_dir().join(format!("grok-app-relay-cwd-{n}"));
        std::fs::create_dir_all(&home).unwrap();
        let prev = std::env::var("GROK_APP_HOME").ok();
        std::env::set_var("GROK_APP_HOME", &home);
        let _restore = Restore(prev, home.clone());
        let project_dir = home.join("repo");
        std::fs::create_dir_all(&project_dir).unwrap();
        let stored = project_dir.display().to_string();
        let first = resolve_relay_project(&stored).expect("create");
        assert!(!first.id.is_empty());
        let alt = normalize_cwd_key(&stored);
        let second = resolve_relay_project(&alt).expect("match");
        assert_eq!(second.id, first.id);
        assert_eq!(second.path, first.path);
        let listed = crate::store::load_projects()
            .into_iter()
            .filter(|p| cwd_paths_match(&p.path, &stored))
            .count();
        assert_eq!(listed, 1);

        let missing = home.join("atlas-work-3");
        assert!(!missing.exists());
        let missing_path = missing.display().to_string();
        let created = resolve_relay_project(&missing_path).expect("mkdir");
        assert!(missing.is_dir());
        let again = resolve_relay_project(&normalize_cwd_key(&missing_path)).expect("match");
        assert_eq!(again.id, created.id);
        let listed_missing = crate::store::load_projects()
            .into_iter()
            .filter(|p| cwd_paths_match(&p.path, &missing_path))
            .count();
        assert_eq!(listed_missing, 1);
    }
}
