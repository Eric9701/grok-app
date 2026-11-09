//! Atlas Fleet leader.
//!
//! Queues tasks, watches remote workers that dial `/ws`, and runs local tasks
//! with `atlas agent stdio`. Desktop sessions, Relay, and `agent serve` stay
//! on their own paths.

mod local_agent;
mod protocol;
mod schedule;
mod store;
mod ws_server;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::ws::WebSocket;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot, Notify};

use protocol::{
    dispatch_agent_id, initialize_line, parse_incoming, permission_result_line, preview_text,
    remote_cwd_ok, session_cancel_line, session_id_from_result, session_new_line,
    session_prompt_line, validate_listen, worker_command, InFrame, LineEvent, Step,
    DEFAULT_BIND, LOCAL_AGENT,
};
use schedule::{is_terminal, TaskRecord, GRACE_MS, MAX_TERMINAL};

const MAX_WORKERS: usize = 32;
static RPC: AtomicU64 = AtomicU64::new(1);
static STARTED: AtomicBool = AtomicBool::new(false);

struct WorkerSlot {
    gen: u64,
    stop: Arc<AtomicBool>,
}

struct Hub {
    dir: PathBuf,
    tasks: Vec<TaskRecord>,
    steps: HashMap<String, Vec<Step>>,
    dirty: HashSet<String>,
    cancel: HashMap<String, Arc<AtomicBool>>,
    kick: Arc<Notify>,
    workers: HashMap<String, WorkerSlot>,
    next_gen: u64,
    enabled: bool,
    saved_bind: String,
    token: String,
    listening: bool,
    listen_error: Option<String>,
    listen_shutdown: Option<oneshot::Sender<()>>,
    local_running: bool,
}

impl Hub {
    fn new() -> Self {
        Self {
            dir: store::tasks_dir(),
            tasks: Vec::new(),
            steps: HashMap::new(),
            dirty: HashSet::new(),
            cancel: HashMap::new(),
            kick: Arc::new(Notify::new()),
            workers: HashMap::new(),
            next_gen: 0,
            enabled: false,
            saved_bind: DEFAULT_BIND.into(),
            token: String::new(),
            listening: false,
            listen_error: None,
            listen_shutdown: None,
            local_running: false,
        }
    }
}

fn hub() -> &'static Mutex<Hub> {
    static HUB: OnceLock<Mutex<Hub>> = OnceLock::new();
    HUB.get_or_init(|| Mutex::new(Hub::new()))
}

fn with_hub<T>(f: impl FnOnce(&mut Hub) -> T) -> T {
    let mut guard = hub().lock().unwrap_or_else(|err| err.into_inner());
    f(&mut guard)
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn next_rpc() -> u64 {
    RPC.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn hub_token() -> String {
    with_hub(|h| h.token.clone())
}

pub(crate) fn can_take_worker(agent_id: &str) -> bool {
    with_hub(|h| h.workers.contains_key(agent_id) || h.workers.len() < MAX_WORKERS)
}

fn token_last4(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() < 8 {
        return String::new();
    }
    chars[chars.len() - 4..].iter().collect()
}

fn new_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

#[derive(Debug)]
enum DriveEnd {
    Completed { session_id: String },
    Failed { reason: String, message: String },
    Cancelled,
    Disconnected,
}

enum Seen {
    Result(Value),
    Error(String),
    Cancelled,
    Disconnected,
    TimedOut,
}

async fn next_matching(
    out: &mpsc::UnboundedSender<String>,
    lines: &mut mpsc::UnboundedReceiver<LineEvent>,
    cancel: &AtomicBool,
    stop: &AtomicBool,
    want: u64,
    deadline: Option<Instant>,
    on_step: &mut impl FnMut(Step),
) -> Seen {
    loop {
        if stop.load(Ordering::Relaxed) {
            return Seen::Disconnected;
        }
        if cancel.load(Ordering::Relaxed) {
            return Seen::Cancelled;
        }
        let slice = match deadline {
            Some(deadline) => {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Seen::TimedOut;
                }
                left.min(Duration::from_millis(200))
            }
            None => Duration::from_millis(200),
        };
        tokio::select! {
            _ = tokio::time::sleep(slice) => {}
            ev = lines.recv() => {
                match ev {
                    Some(LineEvent::Line(line)) => match parse_incoming(&line) {
                        InFrame::Result { id, result } if id == want => return Seen::Result(result),
                        InFrame::Error { id, message } if id == want => return Seen::Error(message),
                        InFrame::Update(step) => on_step(step),
                        InFrame::Permission { id, option_id } => {
                            let _ = out.send(permission_result_line(id, &option_id));
                        }
                        _ => {}
                    },
                    Some(LineEvent::Closed) | None => return Seen::Disconnected,
                }
            }
        }
    }
}

fn env_failed(message: impl Into<String>) -> DriveEnd {
    DriveEnd::Failed {
        reason: "environment_prepare_failed".into(),
        message: message.into(),
    }
}

async fn drive(
    task: &TaskRecord,
    out: &mpsc::UnboundedSender<String>,
    lines: &mut mpsc::UnboundedReceiver<LineEvent>,
    cancel: &AtomicBool,
    stop: &AtomicBool,
    initialize: bool,
    on_step: &mut impl FnMut(Step),
    on_session: &mut impl FnMut(&str, &str),
) -> DriveEnd {
    if initialize {
        let id = next_rpc();
        if out.send(initialize_line(id)).is_err() {
            return DriveEnd::Disconnected;
        }
        match next_matching(out, lines, cancel, stop, id, Some(Instant::now() + Duration::from_secs(30)), on_step).await {
            Seen::Result(_) => {}
            Seen::Error(message) => return env_failed(message),
            Seen::Cancelled => return DriveEnd::Cancelled,
            Seen::Disconnected => return DriveEnd::Disconnected,
            Seen::TimedOut => return env_failed("initialize timed out"),
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return DriveEnd::Cancelled;
    }
    let new_id = next_rpc();
    if out.send(session_new_line(new_id, &task.cwd)).is_err() {
        return DriveEnd::Disconnected;
    }
    let created = match next_matching(
        out,
        lines,
        cancel,
        stop,
        new_id,
        Some(Instant::now() + Duration::from_secs(30)),
        on_step,
    )
    .await
    {
        Seen::Result(result) => result,
        Seen::Error(message) => return env_failed(message),
        Seen::Cancelled => return DriveEnd::Cancelled,
        Seen::Disconnected => return DriveEnd::Disconnected,
        Seen::TimedOut => return env_failed("session/new timed out"),
    };
    let session_id = session_id_from_result(&created);
    if session_id.is_empty() {
        return env_failed("missing session id");
    }
    on_session(&session_id, &new_id.to_string());
    if cancel.load(Ordering::Relaxed) {
        let _ = out.send(session_cancel_line(&session_id));
        return DriveEnd::Cancelled;
    }
    let prompt_id = next_rpc();
    if out
        .send(session_prompt_line(prompt_id, &session_id, &task.prompt))
        .is_err()
    {
        return DriveEnd::Disconnected;
    }
    loop {
        match next_matching(out, lines, cancel, stop, prompt_id, None, on_step).await {
            Seen::Result(_) => {
                return DriveEnd::Completed {
                    session_id: session_id.clone(),
                };
            }
            Seen::Error(message) => {
                return DriveEnd::Failed {
                    reason: "prompt_failed".into(),
                    message,
                };
            }
            Seen::Cancelled => {
                let _ = out.send(session_cancel_line(&session_id));
                return DriveEnd::Cancelled;
            }
            Seen::Disconnected => return DriveEnd::Disconnected,
            Seen::TimedOut => {}
        }
    }
}

fn note_step(task_id: &str, step: Step) {
    with_hub(|h| {
        let steps = h.steps.entry(task_id.to_string()).or_default();
        protocol::push_step(steps, step);
        h.dirty.insert(task_id.to_string());
    });
}

fn mark_task_running(id: &str, session_id: &str, rpc: &str) {
    with_hub(|h| {
        let Some(task) = h.tasks.iter_mut().find(|task| task.id == id) else {
            return;
        };
        schedule::mark_running(task, session_id, rpc);
        let snap = task.clone();
        let _ = store::save_task(&h.dir, &snap);
    });
}

fn claim_for(agent_id: &str) -> Option<TaskRecord> {
    with_hub(|h| {
        let index = schedule::claim_next(&mut h.tasks, agent_id, now_ms())?;
        let task = h.tasks[index].clone();
        h.cancel
            .entry(task.id.clone())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)));
        let _ = store::save_task(&h.dir, &task);
        Some(task)
    })
}

fn cancel_flag(id: &str) -> Arc<AtomicBool> {
    with_hub(|h| {
        h.cancel
            .entry(id.to_string())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone()
    })
}

fn apply_end(id: &str, end: DriveEnd) {
    let (dir, dropped) = with_hub(|h| {
        let Some(task) = h.tasks.iter_mut().find(|task| task.id == id) else {
            return (h.dir.clone(), Vec::new());
        };
        if is_terminal(&task.status) {
            return (h.dir.clone(), Vec::new());
        }
        let now = now_ms();
        match &end {
            DriveEnd::Completed { session_id } => {
                task.status = "completed".into();
                if !session_id.is_empty() {
                    task.session_id = session_id.clone();
                }
                task.finished_at_ms = Some(now);
                task.unreachable = false;
            }
            DriveEnd::Failed { reason, message } => {
                task.status = "failed".into();
                task.failure_reason = Some(reason.clone());
                task.error = Some(message.clone());
                task.finished_at_ms = Some(now);
                task.unreachable = false;
            }
            DriveEnd::Cancelled => {
                task.status = "cancelled".into();
                task.finished_at_ms = Some(now);
                task.unreachable = false;
            }
            DriveEnd::Disconnected => {
                task.unreachable = true;
                task.unreachable_at_ms = Some(now);
            }
        }
        let snap = task.clone();
        let dir = h.dir.clone();
        let _ = store::save_task(&dir, &snap);
        if let Some(steps) = h.steps.get(id) {
            let _ = store::write_steps(&dir, id, steps);
            h.dirty.remove(id);
        }
        let dropped = if is_terminal(&snap.status) {
            schedule::prune_terminal(&mut h.tasks, MAX_TERMINAL)
        } else {
            Vec::new()
        };
        for dropped_id in &dropped {
            h.steps.remove(dropped_id);
            h.dirty.remove(dropped_id);
            h.cancel.remove(dropped_id);
        }
        h.kick.notify_waiters();
        (dir, dropped)
    });
    for dropped_id in dropped {
        store::delete_task_files(&dir, &dropped_id);
    }
}

async fn local_loop(stop: Arc<AtomicBool>) {
    let kick = with_hub(|h| h.kick.clone());
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if let Some(task) = claim_for(LOCAL_AGENT) {
            let cancel = cancel_flag(&task.id);
            let task_id = task.id.clone();
            let end = match local_agent::open().await {
                Err(reason) => DriveEnd::Failed {
                    reason: reason.clone(),
                    message: reason,
                },
                Ok(mut pipe) => {
                    let mut on_step = |step| note_step(&task_id, step);
                    let mut on_session = |sid: &str, rpc: &str| mark_task_running(&task_id, sid, rpc);
                    let end = drive(
                        &task,
                        &pipe.out_tx,
                        &mut pipe.line_rx,
                        &cancel,
                        &stop,
                        true,
                        &mut on_step,
                        &mut on_session,
                    )
                    .await;
                    local_agent::shutdown(pipe).await;
                    end
                }
            };
            apply_end(&task.id, end);
            continue;
        }
        tokio::select! {
            _ = kick.notified() => {}
            _ = tokio::time::sleep(Duration::from_millis(400)) => {}
        }
    }
    with_hub(|h| h.local_running = false);
}

async fn worker_loop(
    agent_id: &str,
    out: &mpsc::UnboundedSender<String>,
    lines: &mut mpsc::UnboundedReceiver<LineEvent>,
    stop: &AtomicBool,
) {
    let kick = with_hub(|h| h.kick.clone());
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if let Some(task) = claim_for(agent_id) {
            let cancel = cancel_flag(&task.id);
            let task_id = task.id.clone();
            let mut on_step = |step| note_step(&task_id, step);
            let mut on_session = |sid: &str, rpc: &str| mark_task_running(&task_id, sid, rpc);
            let end = drive(
                &task,
                out,
                lines,
                &cancel,
                stop,
                false,
                &mut on_step,
                &mut on_session,
            )
            .await;
            let disconnected = matches!(end, DriveEnd::Disconnected);
            apply_end(&task.id, end);
            if disconnected {
                break;
            }
            continue;
        }
        tokio::select! {
            _ = kick.notified() => {}
            _ = tokio::time::sleep(Duration::from_millis(400)) => {}
            ev = lines.recv() => {
                match ev {
                    Some(LineEvent::Closed) | None => break,
                    Some(LineEvent::Line(_)) => {}
                }
            }
        }
    }
}

async fn handshake(
    out: &mpsc::UnboundedSender<String>,
    lines: &mut mpsc::UnboundedReceiver<LineEvent>,
    stop: &AtomicBool,
) -> bool {
    let id = next_rpc();
    if out.send(initialize_line(id)).is_err() {
        return false;
    }
    let cancel = AtomicBool::new(false);
    let mut on_step = |_: Step| {};
    matches!(
        next_matching(
            out,
            lines,
            &cancel,
            stop,
            id,
            Some(Instant::now() + Duration::from_secs(30)),
            &mut on_step,
        )
        .await,
        Seen::Result(_)
    )
}

pub(crate) async fn accept_worker(agent_id: String, socket: WebSocket) {
    let (out_tx, out_rx) = mpsc::unbounded_channel();
    let (line_tx, mut line_rx) = mpsc::unbounded_channel();
    let stop = Arc::new(AtomicBool::new(false));
    let gen = with_hub(|h| {
        if !h.workers.contains_key(&agent_id) && h.workers.len() >= MAX_WORKERS {
            return None;
        }
        h.next_gen = h.next_gen.saturating_add(1);
        let gen = h.next_gen;
        if let Some(old) = h.workers.insert(
            agent_id.clone(),
            WorkerSlot {
                gen,
                stop: stop.clone(),
            },
        ) {
            old.stop.store(true, Ordering::Relaxed);
        }
        h.kick.notify_waiters();
        Some(gen)
    });
    let Some(gen) = gen else {
        return;
    };
    tracing::info!(target: "fleet", agent = %agent_id, "worker connected");
    let stop_pump = stop.clone();
    let pump = tokio::spawn(async move {
        ws_server::pump(socket, out_rx, line_tx, stop_pump).await;
    });
    if handshake(&out_tx, &mut line_rx, &stop).await {
        worker_loop(&agent_id, &out_tx, &mut line_rx, &stop).await;
    }
    stop.store(true, Ordering::Relaxed);
    drop(out_tx);
    with_hub(|h| {
        if h.workers.get(&agent_id).map(|slot| slot.gen) == Some(gen) {
            h.workers.remove(&agent_id);
        }
    });
    let _ = pump.await;
    tracing::info!(target: "fleet", agent = %agent_id, "worker disconnected");
}

fn flush_steps() {
    let batch = with_hub(|h| {
        let dir = h.dir.clone();
        let ids: Vec<String> = h.dirty.drain().collect();
        ids.into_iter()
            .map(|id| {
                let steps = h.steps.get(&id).cloned().unwrap_or_default();
                (dir.clone(), id, steps)
            })
            .collect::<Vec<_>>()
    });
    for (dir, id, steps) in batch {
        let _ = store::write_steps(&dir, &id, &steps);
    }
}

fn expire_now() {
    let (dir, saves, dropped, wake) = with_hub(|h| {
        let before: HashMap<String, String> = h
            .tasks
            .iter()
            .map(|task| (task.id.clone(), format!("{}:{}", task.status, task.unreachable)))
            .collect();
        let now = now_ms();
        let created = schedule::expire_unreachable(&mut h.tasks, now, GRACE_MS);
        let dropped = schedule::prune_terminal(&mut h.tasks, MAX_TERMINAL);
        let saves: Vec<TaskRecord> = h
            .tasks
            .iter()
            .filter(|task| {
                created.iter().any(|id| id == &task.id)
                    || before.get(&task.id).map(String::as_str)
                        != Some(format!("{}:{}", task.status, task.unreachable)).as_deref()
            })
            .cloned()
            .collect();
        for id in &dropped {
            h.steps.remove(id);
            h.dirty.remove(id);
            h.cancel.remove(id);
        }
        (h.dir.clone(), saves, dropped, !created.is_empty())
    });
    for task in saves {
        let _ = store::save_task(&dir, &task);
    }
    for id in dropped {
        store::delete_task_files(&dir, &id);
    }
    if wake {
        with_hub(|h| h.kick.notify_waiters());
    }
}

async fn housekeeping() {
    let mut tick = 0u32;
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        flush_steps();
        tick = tick.wrapping_add(1);
        if tick % 15 == 0 {
            expire_now();
        }
    }
}

fn stop_listener() {
    with_hub(|h| {
        if let Some(tx) = h.listen_shutdown.take() {
            let _ = tx.send(());
        }
        h.listening = false;
        for slot in h.workers.values() {
            slot.stop.store(true, Ordering::Relaxed);
        }
        h.workers.clear();
    });
}

async fn restart_listener(enabled: bool, bind: String, token: String, sample_agent: Option<String>) -> ListenApply {
    stop_listener();
    if !enabled {
        with_hub(|h| {
            h.enabled = false;
            h.saved_bind = bind.clone();
            h.token = token;
            h.listening = false;
            h.listen_error = None;
        });
        return ListenApply {
            listening: false,
            bind,
            error: None,
            connection_cli: None,
        };
    }
    let addr = match validate_listen(true, &bind, &token) {
        Ok(addr) => addr,
        Err(err) => {
            return ListenApply {
                listening: false,
                bind,
                error: Some(err),
                connection_cli: None,
            };
        }
    };
    let target = addr.target();
    match tokio::net::TcpListener::bind(&target).await {
        Err(err) => {
            with_hub(|h| {
                h.enabled = true;
                h.saved_bind = target.clone();
                h.token = token;
                h.listening = false;
                h.listen_error = Some(err.to_string());
            });
            ListenApply {
                listening: false,
                bind: target,
                error: Some(err.to_string()),
                connection_cli: None,
            }
        }
        Ok(listener) => {
            let (tx, rx) = oneshot::channel();
            with_hub(|h| {
                h.enabled = true;
                h.saved_bind = target.clone();
                h.token = token.clone();
                h.listening = true;
                h.listen_error = None;
                h.listen_shutdown = Some(tx);
            });
            tokio::spawn(ws_server::serve(listener, rx));
            let sample = sample_agent
                .and_then(|raw| protocol::remote_agent_id(&raw).ok())
                .unwrap_or_else(|| "worker-1".into());
            tracing::info!(target: "fleet", bind = %target, "fleet listening");
            ListenApply {
                listening: true,
                bind: target,
                error: None,
                connection_cli: Some(worker_command(&addr, &sample, &token)),
            }
        }
    }
}

/// Read task files and, when listening is enabled, open the socket.
pub async fn restore_and_listen() {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let dir = store::tasks_dir();
    let mut tasks = store::load_tasks(&dir);
    schedule::mark_restored_unreachable(&mut tasks, now_ms());
    for task in &tasks {
        let _ = store::save_task(&dir, task);
    }
    with_hub(|h| {
        h.dir = dir;
        h.tasks = tasks;
        h.local_running = true;
    });
    let stop = Arc::new(AtomicBool::new(false));
    tokio::spawn(local_loop(stop));
    tokio::spawn(housekeeping());
    let settings = crate::store::load_settings_async().await;
    let bind = if settings.fleet_bind.trim().is_empty() {
        DEFAULT_BIND.to_string()
    } else {
        settings.fleet_bind
    };
    if settings.fleet_listen_enabled {
        let _ = restart_listener(true, bind, settings.fleet_token, None).await;
    } else {
        with_hub(|h| {
            h.enabled = false;
            h.saved_bind = bind;
            h.token = settings.fleet_token;
            h.listening = false;
        });
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineDto {
    pub agent_id: String,
    pub online: bool,
    pub local: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetStatus {
    pub enabled: bool,
    pub saved_bind: String,
    pub listening: bool,
    pub listen_error: Option<String>,
    pub token_last4: String,
    pub machines: Vec<MachineDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDto {
    pub id: String,
    pub agent_id: String,
    pub cwd: String,
    pub preview: String,
    pub status: String,
    pub error: Option<String>,
    pub failure_reason: Option<String>,
    pub created_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub session_id: String,
    pub retry_of: Option<String>,
    pub unreachable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceDto {
    pub id: String,
    pub agent_id: String,
    pub cwd: String,
    pub prompt: String,
    pub status: String,
    pub error: Option<String>,
    pub failure_reason: Option<String>,
    pub session_id: String,
    pub unreachable: bool,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenApply {
    pub listening: bool,
    pub bind: String,
    pub error: Option<String>,
    pub connection_cli: Option<String>,
}

fn task_dto(task: &TaskRecord) -> TaskDto {
    TaskDto {
        id: task.id.clone(),
        agent_id: task.agent_id.clone(),
        cwd: task.cwd.clone(),
        preview: preview_text(&task.prompt),
        status: task.status.clone(),
        error: task.error.clone(),
        failure_reason: task.failure_reason.clone(),
        created_at_ms: task.created_at_ms,
        finished_at_ms: task.finished_at_ms,
        session_id: task.session_id.clone(),
        retry_of: task.retry_of.clone(),
        unreachable: task.unreachable,
    }
}

fn check_dispatch(agent_id: &str, cwd: &str, prompt: &str) -> Result<(String, String, String), String> {
    let agent = dispatch_agent_id(agent_id).map_err(|_| "empty_agent".to_string())?;
    if !remote_cwd_ok(cwd) {
        return Err("cwd_not_absolute".into());
    }
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("empty_prompt".into());
    }
    Ok((agent, cwd.trim().to_string(), prompt.to_string()))
}

#[tauri::command]
pub fn fleet_status() -> FleetStatus {
    with_hub(|h| {
        let mut ids: Vec<String> = Vec::new();
        ids.push(LOCAL_AGENT.into());
        for task in &h.tasks {
            if !ids.iter().any(|id| id == &task.agent_id) {
                ids.push(task.agent_id.clone());
            }
        }
        for id in h.workers.keys() {
            if !ids.iter().any(|have| have == id) {
                ids.push(id.clone());
            }
        }
        let machines = ids
            .into_iter()
            .map(|id| MachineDto {
                online: if id == LOCAL_AGENT {
                    h.local_running
                } else {
                    h.workers.contains_key(&id)
                },
                local: id == LOCAL_AGENT,
                agent_id: id,
            })
            .collect();
        FleetStatus {
            enabled: h.enabled,
            saved_bind: h.saved_bind.clone(),
            listening: h.listening,
            listen_error: h.listen_error.clone(),
            token_last4: token_last4(&h.token),
            machines,
        }
    })
}

#[tauri::command]
pub fn fleet_tasks() -> Vec<TaskDto> {
    with_hub(|h| {
        let mut tasks = h.tasks.clone();
        tasks.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));
        tasks.iter().map(task_dto).collect()
    })
}

#[tauri::command]
pub fn fleet_task_trace(id: String) -> Result<TraceDto, String> {
    let (task, steps, dir) = with_hub(|h| {
        let task = h.tasks.iter().find(|task| task.id == id)?.clone();
        let steps = h.steps.get(&id).cloned();
        Some((task, steps, h.dir.clone()))
    })
    .ok_or_else(|| "not_found".to_string())?;
    let steps = steps.unwrap_or_else(|| store::load_steps(&dir, &task.id));
    Ok(TraceDto {
        id: task.id,
        agent_id: task.agent_id,
        cwd: task.cwd,
        prompt: task.prompt,
        status: task.status,
        error: task.error,
        failure_reason: task.failure_reason,
        session_id: task.session_id,
        unreachable: task.unreachable,
        steps,
    })
}

#[tauri::command]
pub fn fleet_dispatch(agent_id: String, cwd: String, prompt: String) -> Result<TaskDto, String> {
    let (agent, cwd, prompt) = check_dispatch(&agent_id, &cwd, &prompt)?;
    let task = schedule::new_task(
        &uuid::Uuid::new_v4().simple().to_string(),
        &agent,
        &cwd,
        &prompt,
        now_ms(),
    );
    let dto = task_dto(&task);
    with_hub(|h| {
        let _ = store::save_task(&h.dir, &task);
        h.tasks.push(task);
        h.kick.notify_waiters();
    });
    Ok(dto)
}

#[tauri::command]
pub fn fleet_cancel(id: String) -> Result<(), String> {
    let dropped = with_hub(|h| {
        let task = h
            .tasks
            .iter_mut()
            .find(|task| task.id == id)
            .ok_or_else(|| "not_found".to_string())?;
        if is_terminal(&task.status) {
            return Err("already_finished".to_string());
        }
        if task.status == "queued" {
            task.status = "cancelled".into();
            task.finished_at_ms = Some(now_ms());
            let snap = task.clone();
            let _ = store::save_task(&h.dir, &snap);
            let dropped = schedule::prune_terminal(&mut h.tasks, MAX_TERMINAL);
            for dropped_id in &dropped {
                h.steps.remove(dropped_id);
                h.cancel.remove(dropped_id);
            }
            return Ok(dropped);
        }
        h.cancel
            .entry(id)
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .store(true, Ordering::Relaxed);
        Ok(Vec::new())
    })?;
    let dir = with_hub(|h| h.dir.clone());
    for dropped_id in dropped {
        store::delete_task_files(&dir, &dropped_id);
    }
    Ok(())
}

#[tauri::command]
pub async fn fleet_listen_apply(
    enabled: bool,
    bind: String,
    token: String,
    sample_agent_id: Option<String>,
) -> Result<ListenApply, String> {
    let mut token = token.trim().to_string();
    if enabled && token.is_empty() {
        token = new_token();
    }
    let addr = validate_listen(enabled, bind.trim(), &token)?;
    let bind = addr.target();
    let mut settings = crate::store::load_settings_async().await;
    settings.fleet_listen_enabled = enabled;
    settings.fleet_bind = bind.clone();
    settings.fleet_token = token.clone();
    crate::store::save_settings_async(&settings).await?;
    Ok(restart_listener(enabled, bind, token, sample_agent_id).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dispatch_rejects_relative_cwd_without_touching_disk() {
        let err = check_dispatch("box", "relative/path", "hello").unwrap_err();
        assert_eq!(err, "cwd_not_absolute");
        assert!(check_dispatch("box", "/data/proj", "hello").is_ok());
        assert!(super::protocol::token_matches("secret-token", "secret-token"));
        assert!(token_last4("secret-token").len() <= 4);
        assert_ne!(token_last4("secret-token"), "secret-token");
    }

    #[tokio::test]
    async fn fake_cli_runs_a_task_to_completed() {
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
        let (line_tx, mut line_rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(line) = out_rx.recv().await {
                let value: Value = serde_json::from_str(&line).unwrap();
                let method = value.get("method").and_then(|m| m.as_str()).unwrap_or("");
                let id = value.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                if method == "session/prompt" {
                    let _ = line_tx.send(LineEvent::Line(
                        json!({
                            "jsonrpc": "2.0",
                            "method": "session/update",
                            "params": {
                                "update": {
                                    "sessionUpdate": "agent_message_chunk",
                                    "content": { "type": "text", "text": "done-step" }
                                }
                            }
                        })
                        .to_string(),
                    ));
                }
                let result = if method == "session/new" {
                    json!({ "sessionId": "sess-1" })
                } else {
                    json!({ "ok": true })
                };
                let _ = line_tx.send(LineEvent::Line(
                    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string(),
                ));
            }
        });
        let task = schedule::new_task("t", "local", "/data/proj", "say hi", 1);
        let steps = Arc::new(Mutex::new(Vec::new()));
        let steps_bg = steps.clone();
        let mut on_step = move |step| {
            steps_bg.lock().unwrap().push(step);
        };
        let mut on_session = |_: &str, _: &str| {};
        let end = drive(
            &task,
            &out_tx,
            &mut line_rx,
            &AtomicBool::new(false),
            &AtomicBool::new(false),
            true,
            &mut on_step,
            &mut on_session,
        )
        .await;
        match end {
            DriveEnd::Completed { session_id } => assert_eq!(session_id, "sess-1"),
            other => panic!("unexpected {other:?}"),
        }
        let saved = steps.lock().unwrap();
        assert!(saved.iter().any(|step| step.text.contains("done-step")));
        assert!(!saved.iter().any(|step| step.text.contains("jsonrpc")));
    }
}
