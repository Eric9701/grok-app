//! Atlas Relay **Agent** bridge.
//!
//! Outbound WebSocket to `atlas-relay-demo` `/ws?agent_id=`. Each ACP JSON
//! frame is written as one line to a local `atlas agent --always-approve stdio`
//! process, and each stdout line is sent back as one WebSocket text frame.
//! The desktop session client is not involved.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
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

static GEN: AtomicU64 = AtomicU64::new(0);
static PID: AtomicU32 = AtomicU32::new(0);
static CANCEL: Notify = Notify::const_new();
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AtlasRelayAgentStatus {
    /// `stopped` | `connecting` | `online` | `reconnecting`
    pub phase: String,
    pub error: Option<String>,
    pub cli_alive: bool,
    pub agent_id: String,
    pub url: String,
    /// Cloud `session/prompt` rows, newest first. Kept after disconnect.
    pub tasks: Vec<AtlasRelayTask>,
}

struct State {
    phase: String,
    error: Option<String>,
    cli_alive: bool,
    agent_id: String,
    url: String,
    gen: u64,
    tasks: Vec<AtlasRelayTask>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            phase: "stopped".into(),
            error: None,
            cli_alive: false,
            agent_id: String::new(),
            url: String::new(),
            gen: 0,
            tasks: Vec::new(),
        }
    }
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(State::default()))
}

fn with_state<T>(f: impl FnOnce(&mut State) -> T) -> T {
    let mut guard = state().lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

fn snapshot() -> AtlasRelayAgentStatus {
    with_state(|s| AtlasRelayAgentStatus {
        phase: s.phase.clone(),
        error: s.error.clone(),
        cli_alive: s.cli_alive,
        agent_id: s.agent_id.clone(),
        url: s.url.clone(),
        tasks: s.tasks.clone(),
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

fn observe_frame(frame: &str, inbound: bool) {
    let at = unix_ms();
    with_state(|s| note_relay_frame(&mut s.tasks, frame, inbound, at));
}

/// Trim, drop CR/LF, cap at 128 characters.
pub fn sanitize_agent_id(raw: &str) -> String {
    let flat: String = raw.chars().filter(|c| *c != '\n' && *c != '\r').collect();
    flat.trim().chars().take(128).collect()
}

/// Normalize a pasted relay address into the Agent socket
/// `ws(s)://host[:port]/ws?agent_id=...`.
///
/// Page URLs (`/build`, `/m`) and a bare `host:port` become `/ws`.
/// An address that is already `/ws` or `/ws/agent` keeps that path and only
/// receives `agent_id`.
pub fn relay_agent_ws_url(raw: &str, agent_id: &str) -> Result<String, String> {
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
    if !keep_agent_path {
        u.set_path("/ws");
    }
    {
        let mut pairs = u.query_pairs_mut();
        pairs.clear();
        pairs.append_pair("agent_id", &id);
    }
    u.set_fragment(None);
    Ok(u.to_string())
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

pub fn status() -> AtlasRelayAgentStatus {
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

/// Start or replace the bridge. Reconnects until [`stop`] or a newer start.
pub fn start(raw_url: &str, agent_id: &str) -> Result<(), String> {
    let saved = crate::store::load_settings();
    set_health_secs(saved.atlas_relay_agent_health_secs);
    let ws_url = relay_agent_ws_url(raw_url, agent_id)?;
    let id = sanitize_agent_id(agent_id);
    let gen = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    CANCEL.notify_waiters();
    kill_recorded_child();
    with_state(|s| {
        s.phase = "connecting".into();
        s.error = None;
        s.cli_alive = false;
        s.agent_id = id.clone();
        s.url = ws_url.clone();
        s.gen = gen;
    });
    tauri::async_runtime::spawn(run_forever(gen, ws_url));
    Ok(())
}

pub fn stop() {
    let gen = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    CANCEL.notify_waiters();
    kill_recorded_child();
    with_state(|s| {
        s.phase = "stopped".into();
        s.error = None;
        s.cli_alive = false;
        s.gen = gen;
    });
}

/// Apply persisted settings. No-op when the same target is already running.
pub fn apply_settings(settings: &crate::store::AppSettings) {
    if !settings.atlas_relay_agent_enabled {
        stop();
        return;
    }
    let url = settings
        .atlas_relay_agent_url
        .as_deref()
        .unwrap_or("")
        .trim();
    let id = settings
        .atlas_relay_agent_id
        .as_deref()
        .unwrap_or("");
    if url.is_empty() || sanitize_agent_id(id).is_empty() {
        stop();
        return;
    }
    if let Ok(ws) = relay_agent_ws_url(url, id) {
        let same = with_state(|s| s.phase != "stopped" && s.url == ws);
        if same {
            return;
        }
    }
    if let Err(e) = start(url, id) {
        with_state(|s| {
            s.phase = "stopped".into();
            s.error = Some(e);
            s.cli_alive = false;
        });
    }
}

pub async fn autostart_from_settings() {
    let settings = crate::store::load_settings();
    if settings.atlas_relay_agent_enabled {
        apply_settings(&settings);
    }
}

fn kill_recorded_child() {
    let pid = PID.swap(0, Ordering::SeqCst);
    if pid != 0 {
        tauri::async_runtime::spawn(async move {
            crate::process_util::kill_process_tree_async(pid).await;
        });
    }
}

fn current(gen: u64) -> bool {
    GEN.load(Ordering::SeqCst) == gen
}

async fn cancelled(gen: u64) {
    loop {
        CANCEL.notified().await;
        if !current(gen) {
            return;
        }
    }
}

enum RunEnd {
    Stopped,
    Disconnected(Option<String>),
}

async fn run_forever(gen: u64, ws_url: String) {
    if !current(gen) {
        return;
    }
    let mut delay_secs = 1u64;
    let mut first = true;
    loop {
        if !current(gen) {
            return;
        }
        with_state(|s| {
            if s.gen == gen {
                s.phase = if first {
                    "connecting".into()
                } else {
                    "reconnecting".into()
                };
                s.cli_alive = false;
            }
        });
        first = false;
        match run_once(gen, &ws_url).await {
            RunEnd::Stopped => return,
            RunEnd::Disconnected(err) => {
                if !current(gen) {
                    return;
                }
                with_state(|s| {
                    if s.gen == gen {
                        s.phase = "reconnecting".into();
                        s.cli_alive = false;
                        s.error = err;
                    }
                });
                let sleep = tokio::time::sleep(Duration::from_secs(delay_secs));
                tokio::pin!(sleep);
                tokio::select! {
                    _ = &mut sleep => {}
                    _ = cancelled(gen) => return,
                }
                delay_secs = delay_secs.saturating_mul(2).min(MAX_BACKOFF_SECS);
            }
        }
    }
}

async fn run_once(gen: u64, ws_url: &str) -> RunEnd {
    if !current(gen) {
        return RunEnd::Stopped;
    }
    let mut child = match spawn_stdio_agent().await {
        Ok(child) => child,
        Err(e) => return RunEnd::Disconnected(Some(e)),
    };
    let pid = child.id().unwrap_or(0);
    if pid != 0 {
        PID.store(pid, Ordering::SeqCst);
    }
    with_state(|s| {
        if s.gen == gen {
            s.cli_alive = true;
        }
    });
    if !current(gen) {
        finish_child(&mut child, pid).await;
        return RunEnd::Stopped;
    }

    let ws = tokio::select! {
        _ = cancelled(gen) => {
            finish_child(&mut child, pid).await;
            return RunEnd::Stopped;
        }
        _ = tokio::time::sleep(CONNECT_TIMEOUT) => {
            finish_child(&mut child, pid).await;
            return RunEnd::Disconnected(Some(format!(
                "connect timed out ({}s)",
                CONNECT_TIMEOUT.as_secs()
            )));
        }
        result = connect_async(ws_url) => {
            match result {
                Ok((ws, _)) => ws,
                Err(e) => {
                    finish_child(&mut child, pid).await;
                    return RunEnd::Disconnected(Some(format!("connect failed: {e}")));
                }
            }
        }
    };
    if !current(gen) {
        finish_child(&mut child, pid).await;
        return RunEnd::Stopped;
    }
    with_state(|s| {
        if s.gen == gen {
            s.phase = "online".into();
            s.error = None;
            s.cli_alive = true;
        }
    });
    tracing::info!(url = ws_url, pid, "atlas relay agent online");

    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let end = bridge(gen, ws, stdin, stdout, stderr, &mut child).await;
    finish_child(&mut child, pid).await;
    end
}

async fn finish_child(child: &mut Child, pid: u32) {
    if pid != 0 && PID.load(Ordering::SeqCst) == pid {
        PID.store(0, Ordering::SeqCst);
    }
    if pid != 0 {
        crate::process_util::kill_process_tree_async(pid).await;
    }
    let _ = child.start_kill();
    let _ = child.wait().await;
    with_state(|s| {
        if s.cli_alive && PID.load(Ordering::SeqCst) == 0 {
            s.cli_alive = false;
        }
    });
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
    gen: u64,
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
            _ = cancelled(gen) => break RunEnd::Stopped,
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
                            observe_frame(&body, true);
                            if write_line(&mut stdin, &body).await.is_err() {
                                break RunEnd::Disconnected(Some("CLI stdin closed".into()));
                            }
                        }
                    }
                    Some(Ok(Message::Binary(b))) => {
                        if let Ok(text) = std::str::from_utf8(&b) {
                            if let Some(body) = inbound_frame_to_line(text) {
                                observe_frame(&body, true);
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
                        observe_frame(&frame, false);
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
pub fn atlas_relay_agent_status() -> AtlasRelayAgentStatus {
    status()
}

/// Persist the relay target, mark auto-connect, and open the bridge.
#[tauri::command]
pub async fn atlas_relay_agent_connect(
    url: String,
    agent_id: String,
) -> Result<AtlasRelayAgentStatus, String> {
    let id = sanitize_agent_id(&agent_id);
    relay_agent_ws_url(url.trim(), &id)?;
    let mut settings = crate::store::load_settings();
    settings.atlas_relay_agent_url = Some(url.trim().to_string());
    settings.atlas_relay_agent_id = Some(id.clone());
    settings.atlas_relay_agent_enabled = true;
    crate::store::save_settings_async(&settings).await?;
    start(url.trim(), &id)?;
    Ok(status())
}

/// Stop the bridge and turn off auto-connect.
#[tauri::command]
pub async fn atlas_relay_agent_disconnect() -> Result<AtlasRelayAgentStatus, String> {
    let mut settings = crate::store::load_settings();
    settings.atlas_relay_agent_enabled = false;
    crate::store::save_settings_async(&settings).await?;
    stop();
    Ok(status())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_url_from_page_host_and_existing_socket() {
        assert_eq!(
            relay_agent_ws_url("http://10.1.2.3:2420/build?agent=laptop-a", "laptop-a").unwrap(),
            "ws://10.1.2.3:2420/ws?agent_id=laptop-a"
        );
        assert_eq!(
            relay_agent_ws_url("10.1.2.3:2420", "laptop-a").unwrap(),
            "ws://10.1.2.3:2420/ws?agent_id=laptop-a"
        );
        assert_eq!(
            relay_agent_ws_url("wss://relay.example:2420/ws/agent", "laptop-b").unwrap(),
            "wss://relay.example:2420/ws/agent?agent_id=laptop-b"
        );
        assert_eq!(
            relay_agent_ws_url("https://relay.example/m", "laptop-a").unwrap(),
            "wss://relay.example/ws?agent_id=laptop-a"
        );
        assert!(relay_agent_ws_url("ws://10.1.2.3:2420/ws", "").is_err());
        assert!(relay_agent_ws_url("", "laptop-a").is_err());
        assert!(relay_agent_ws_url("ftp://relay.example/ws", "laptop-a").is_err());
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
}
