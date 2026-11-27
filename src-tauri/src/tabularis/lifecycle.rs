//! Spawn and keep the headless Tabularis web process.

use std::fs::{self, File};
use std::io::Read;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use super::approvals::{self, STATUS_EVENT};
use super::binary;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabularisStatus {
    pub state: String,
    pub origin: Option<String>,
    pub bootstrap_url: Option<String>,
    pub binary: Option<String>,
    pub data_dir_mode: String,
    pub error: Option<String>,
    pub pending_approvals: usize,
}

struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    mode: String,
    origin: String,
    bootstrap_url: String,
    binary: String,
}

struct Handle {
    running: Option<Running>,
    error: Option<String>,
    mode: String,
}

static HANDLE: std::sync::LazyLock<Mutex<Handle>> = std::sync::LazyLock::new(|| {
    Mutex::new(Handle {
        running: None,
        error: None,
        mode: "shared".into(),
    })
});

pub fn status_snapshot() -> TabularisStatus {
    let guard = HANDLE.lock();
    snapshot(&guard)
}

fn snapshot(guard: &Handle) -> TabularisStatus {
    if let Some(running) = &guard.running {
        return TabularisStatus {
            state: "running".into(),
            origin: Some(running.origin.clone()),
            bootstrap_url: Some(running.bootstrap_url.clone()),
            binary: Some(running.binary.clone()),
            data_dir_mode: running.mode.clone(),
            error: None,
            pending_approvals: approvals::count(),
        };
    }
    TabularisStatus {
        state: if guard.error.is_some() {
            "error"
        } else {
            "stopped"
        }
        .into(),
        origin: None,
        bootstrap_url: None,
        binary: None,
        data_dir_mode: guard.mode.clone(),
        error: guard.error.clone(),
        pending_approvals: approvals::count(),
    }
}

pub fn ensure_blocking(app: &AppHandle) -> Result<TabularisStatus, String> {
    let settings = crate::store::load_settings();
    let mode = normalize_mode(settings.tabularis_data_dir_mode.as_deref());
    let already = {
        let guard = HANDLE.lock();
        guard.running.as_ref().map(|running| running.mode.clone())
    };
    if already.as_deref() == Some(mode.as_str()) {
        return Ok(status_snapshot());
    }
    if already.is_some() {
        stop_blocking(app);
    }
    start_blocking(app, &mode, settings.tabularis_binary_path.as_deref())
}

pub fn restart_blocking(app: &AppHandle) -> Result<TabularisStatus, String> {
    stop_blocking(app);
    ensure_blocking(app)
}

pub fn stop_blocking(app: &AppHandle) -> TabularisStatus {
    let running = HANDLE.lock().running.take();
    if let Some(running) = running {
        shutdown(running);
    }
    let status = status_snapshot();
    let _ = app.emit(STATUS_EVENT, &status);
    status
}

fn start_blocking(
    app: &AppHandle,
    mode: &str,
    explicit_binary: Option<&str>,
) -> Result<TabularisStatus, String> {
    let resource_dir = app.path().resource_dir().ok();
    let binary = match binary::resolve(explicit_binary, resource_dir.as_deref()) {
        Some(path) => path,
        None => {
            let message = "tabularis binary was not found".to_string();
            HANDLE.lock().error = Some(message.clone());
            HANDLE.lock().mode = mode.to_string();
            let status = status_snapshot();
            let _ = app.emit(STATUS_EVENT, &status);
            return Err(message);
        }
    };
    let ready = std::env::temp_dir().join(format!(
        "tabularis-ready-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_millis())
            .unwrap_or(0)
    ));
    let _ = fs::remove_file(&ready);
    let log_path = crate::paths::app_data_root().join("tabularis-host.log");
    if let Some(parent) = log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let stderr = File::create(&log_path)
        .map(Stdio::from)
        .unwrap_or_else(|_| Stdio::null());
    let mut command = Command::new(&binary);
    command
        .arg("web")
        .arg("--embedded")
        .arg("--port")
        .arg("0")
        .arg("--no-open")
        .arg("--ready-file")
        .arg(&ready)
        .arg("--exit-on-stdin-eof")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(stderr);
    if mode == "isolated" {
        let dir = crate::paths::app_data_root().join("tabularis");
        let _ = fs::create_dir_all(&dir);
        command.env("TABULARIS_DATA_DIR", &dir);
    }
    if let Some(endpoint) = crate::session_api::read_endpoint_file() {
        let callback = format!(
            "{}/v1/tabularis/events",
            endpoint.url.trim_end_matches('/')
        );
        command
            .env("TABULARIS_HOST_CALLBACK_TOKEN", endpoint.token)
            .arg("--host-callback-url")
            .arg(callback);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let message = format!("start tabularis: {error}");
            HANDLE.lock().error = Some(message.clone());
            let status = status_snapshot();
            let _ = app.emit(STATUS_EVENT, &status);
            return Err(message);
        }
    };
    let stdin = child.stdin.take();
    match wait_ready(&ready, &mut child) {
        Ok((origin, bootstrap_url)) => {
            let _ = fs::remove_file(&ready);
            let mut guard = HANDLE.lock();
            guard.error = None;
            guard.mode = mode.to_string();
            guard.running = Some(Running {
                child,
                stdin,
                mode: mode.to_string(),
                origin: origin.clone(),
                bootstrap_url: bootstrap_url.clone(),
                binary: binary.display().to_string(),
            });
            let status = snapshot(&guard);
            drop(guard);
            let _ = app.emit(STATUS_EVENT, &status);
            Ok(status)
        }
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_file(&ready);
            HANDLE.lock().error = Some(error.clone());
            let status = status_snapshot();
            let _ = app.emit(STATUS_EVENT, &status);
            Err(error)
        }
    }
}

fn wait_ready(path: &std::path::Path, child: &mut Child) -> Result<(String, String), String> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(20) {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            return Err(format!("tabularis exited before it was ready ({status})"));
        }
        if let Ok(mut file) = File::open(path) {
            let mut raw = String::new();
            file.read_to_string(&mut raw).map_err(|error| error.to_string())?;
            if let Ok(value) = serde_json::from_str::<Value>(&raw) {
                let origin = value
                    .get("origin")
                    .and_then(|item| item.as_str())
                    .unwrap_or("")
                    .to_string();
                let bootstrap = value
                    .get("bootstrapUrl")
                    .and_then(|item| item.as_str())
                    .unwrap_or("")
                    .to_string();
                if !origin.is_empty() && !bootstrap.is_empty() {
                    return Ok((origin, bootstrap));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("tabularis did not write its ready file".into())
}

fn shutdown(mut running: Running) {
    drop(running.stdin.take());
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(3) {
        match running.child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(_) => break,
        }
    }
    let _ = running.child.kill();
    let _ = running.child.wait();
}

pub fn normalize_mode(mode: Option<&str>) -> String {
    if mode == Some("isolated") {
        "isolated".into()
    } else {
        "shared".into()
    }
}

#[tauri::command]
pub async fn tabularis_status() -> TabularisStatus {
    status_snapshot()
}

#[tauri::command]
pub async fn tabularis_ensure_started(app: AppHandle) -> Result<TabularisStatus, String> {
    tauri::async_runtime::spawn_blocking(move || ensure_blocking(&app))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn tabularis_restart(app: AppHandle) -> Result<TabularisStatus, String> {
    tauri::async_runtime::spawn_blocking(move || restart_blocking(&app))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn tabularis_stop(app: AppHandle) -> Result<TabularisStatus, String> {
    Ok(tauri::async_runtime::spawn_blocking(move || stop_blocking(&app))
        .await
        .map_err(|error| error.to_string())?)
}

#[tauri::command]
pub async fn tabularis_host_script() -> Result<String, String> {
    let endpoint = crate::session_api::read_endpoint_file()
        .ok_or_else(|| "session api is not listening".to_string())?;
    Ok(super::bridge::host_script(&endpoint.url, &endpoint.token))
}

#[tauri::command]
pub async fn tabularis_pending_approvals() -> Vec<super::approvals::ApprovalNotice> {
    approvals::list()
}

#[tauri::command]
pub async fn tabularis_decide_approval(
    id: String,
    decision: String,
) -> Result<(), String> {
    let id = id.trim().to_string();
    if id.is_empty() || id.contains(['/', '\\', '.']) {
        return Err("invalid approval id".into());
    }
    let decision = match decision.as_str() {
        "approve" | "deny" => decision,
        _ => return Err("decision must be approve or deny".into()),
    };
    let status = status_snapshot();
    let origin = status.origin.ok_or_else(|| "tabularis is not running".to_string())?;
    let endpoint = crate::session_api::read_endpoint_file()
        .ok_or_else(|| "session api is not listening".to_string())?;
    let url = format!(
        "{}/api/v1/embedded/approvals/{id}/decision",
        origin.trim_end_matches('/')
    );
    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .header("x-tabularis-host-token", endpoint.token)
        .json(&serde_json::json!({ "decision": decision }))
        .send()
        .await
        .map_err(|error| format!("approval decision: {error}"))?;
    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("approval decision failed: {body}"));
    }
    approvals::dismiss(&id);
    Ok(())
}
