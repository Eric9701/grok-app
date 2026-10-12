//! Atlas Relay **Agent** bridge.
//!
//! Outbound WebSocket to `atlas-relay-demo` `/ws?agent_id=&token=`. Each ACP JSON
//! frame is written as one line to a local `atlas agent --always-approve stdio`
//! process, and each stdout line is sent back as one WebSocket text frame.
//! The desktop session client is not involved.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Notify;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_BACKOFF_SECS: u64 = 15;
const MAX_TASKS: usize = 40;
const MAX_TASK_TEXT: usize = 240;
const DEFAULT_HEALTH_SECS: u64 = 15;
const MIN_HEALTH_SECS: u64 = 5;
const MAX_HEALTH_SECS: u64 = 300;
pub const MAX_RELAY_AGENTS: usize = 4;

static HEALTH_SECS: AtomicU64 = AtomicU64::new(DEFAULT_HEALTH_SECS);

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

fn mark_id(tasks: &mut [AtlasRelayTask], id: &str, status: &str, error: Option<String>) {
    if let Some(task) = tasks.iter_mut().find(|t| t.id == id && t.status == "running") {
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

fn observe_frame(agent_id: &str, frame: &str, inbound: bool) {
    let at = unix_ms();
    with_hub(|h| {
        let Some(slot) = h.slots.get_mut(agent_id) else {
            return;
        };
        let before = slot.tasks.len();
        note_relay_frame(&mut slot.tasks, frame, inbound, at);
        if slot.tasks.len() > before {
            if let Some(task) = slot.tasks.first_mut() {
                task.agent_id = agent_id.to_string();
            }
        }
    });
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
    let token = if explicit.is_empty() { pasted } else { explicit };
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
        if id.is_empty() || !seen.insert(id.clone()) {
            changed = true;
            continue;
        }
        if id != slot.id || url != slot.url || token != slot.token {
            changed = true;
        }
        kept.push(crate::store::AtlasRelayAgentRecord {
            url,
            token,
            id,
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

/// Start or replace one agent. Other agents keep running.
pub fn start(raw_url: &str, agent_id: &str, token: &str) -> Result<(), String> {
    let saved = crate::store::load_settings();
    set_health_secs(saved.atlas_relay_agent_health_secs);
    let id = sanitize_agent_id(agent_id);
    if id.is_empty() {
        return Err("empty agent id".into());
    }
    let ws_url = relay_agent_ws_url(raw_url, &id, token)?;
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
            continue;
        }
        if let Err(e) = start(slot.url.trim(), &id, &slot.token) {
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
    let mut child = match spawn_stdio_agent().await {
        Ok(child) => child,
        Err(e) => return RunEnd::Disconnected(Some(e)),
    };
    let pid = child.id().unwrap_or(0);
    with_hub(|h| {
        if let Some(slot) = h.slots.get_mut(agent_id) {
            if slot.gen == gen {
                slot.pid = pid;
                slot.cli_alive = true;
            }
        }
    });
    if !slot_is_current(agent_id, gen) {
        finish_child(agent_id, &mut child, pid).await;
        return RunEnd::Stopped;
    }

    let ws = tokio::select! {
        _ = cancelled(agent_id, gen, cancel) => {
            finish_child(agent_id, &mut child, pid).await;
            return RunEnd::Stopped;
        }
        _ = tokio::time::sleep(CONNECT_TIMEOUT) => {
            finish_child(agent_id, &mut child, pid).await;
            return RunEnd::Disconnected(Some(format!(
                "connect timed out ({}s)",
                CONNECT_TIMEOUT.as_secs()
            )));
        }
        result = connect_async(ws_url) => {
            match result {
                Ok((ws, _)) => ws,
                Err(e) => {
                    finish_child(agent_id, &mut child, pid).await;
                    return RunEnd::Disconnected(Some(format!("connect failed: {e}")));
                }
            }
        }
    };
    if !slot_is_current(agent_id, gen) {
        finish_child(agent_id, &mut child, pid).await;
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

    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let end = bridge(agent_id, gen, cancel, ws, stdin, stdout, stderr, &mut child).await;
    finish_child(agent_id, &mut child, pid).await;
    end
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

async fn spawn_stdio_agent() -> Result<Child, String> {
    let settings = crate::store::load_settings();
    let probe = crate::cli_probe::probe_cli(settings.manual_cli_path.as_deref());
    let cli_path = probe
        .path
        .filter(|_| probe.found)
        .ok_or_else(|| "Atlas CLI not found".to_string())?;
    let home: PathBuf = crate::paths::resolve_agent_grok_home(&settings.session_data_mode);
    let mut cmd = Command::new(&cli_path);
    cmd.arg("agent").arg("--always-approve").arg("stdio");
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
    cmd.spawn()
        .map_err(|e| format!("failed to spawn Atlas agent stdio: {e}"))
}

async fn bridge(
    agent_id: &str,
    gen: u64,
    cancel: &Notify,
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    stdin: Option<tokio::process::ChildStdin>,
    stdout: Option<tokio::process::ChildStdout>,
    stderr: Option<tokio::process::ChildStderr>,
    child: &mut Child,
) -> RunEnd {
    let Some(mut stdin) = stdin else {
        return RunEnd::Disconnected(Some("CLI stdin missing".into()));
    };
    let Some(stdout) = stdout else {
        return RunEnd::Disconnected(Some("CLI stdout missing".into()));
    };
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

    let health_sleep = tokio::time::sleep(Duration::from_secs(current_health_secs()));
    tokio::pin!(health_sleep);
    let mut awaiting_pong = false;

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
                            observe_frame(agent_id, &body, true);
                            if write_line(&mut stdin, &body).await.is_err() {
                                break RunEnd::Disconnected(Some("CLI stdin closed".into()));
                            }
                        }
                    }
                    Some(Ok(Message::Binary(b))) => {
                        if let Ok(text) = std::str::from_utf8(&b) {
                            if let Some(body) = inbound_frame_to_line(text) {
                                observe_frame(agent_id, &body, true);
                                if write_line(&mut stdin, &body).await.is_err() {
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
                        observe_frame(agent_id, &frame, false);
                        if sink.send(Message::Text(frame.into())).await.is_err() {
                            break RunEnd::Disconnected(Some("relay closed".into()));
                        }
                    }
                    None => break RunEnd::Disconnected(Some("CLI stdout closed".into())),
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
    let _ = stdin.shutdown().await;
    end
}

async fn write_line(stdin: &mut tokio::process::ChildStdin, body: &str) -> std::io::Result<()> {
    stdin.write_all(body.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await
}

#[tauri::command]
pub fn atlas_relay_agent_status() -> AtlasRelayFleetStatus {
    status()
}

/// Persist this agent, mark it auto-connect, and open only its bridge.
#[tauri::command]
pub async fn atlas_relay_agent_connect(
    url: String,
    agent_id: String,
    token: String,
) -> Result<AtlasRelayFleetStatus, String> {
    let id = sanitize_agent_id(&agent_id);
    if id.is_empty() {
        return Err("empty agent id".into());
    }
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
                enabled: true,
            });
    }
    validate_relay_agents(&settings.atlas_relay_agents)?;
    crate::store::save_settings_async(&settings).await?;
    start(url.trim(), &id, &stored)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_url_from_page_host_and_existing_socket() {
        assert_eq!(
            relay_agent_ws_url("http://10.1.2.3:2420/build?agent=laptop-a", "laptop-a", "").unwrap(),
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
                enabled: false,
            });
        }
        assert!(validate_relay_agents(&agents).is_ok());
        agents.push(crate::store::AtlasRelayAgentRecord {
            url: "ws://10.9.9.9:2420".into(),
            token: String::new(),
            id: "a9".into(),
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
}
