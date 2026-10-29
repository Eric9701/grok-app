//! Multi-CLI runtime. Desktop chats and the Atlas relay speak ACP.
//! Atlas and Cursor Agent are byte pipes. Claude Code is translated from
//! stream-json. Config lives in `agent-runtimes.json`.

mod translate;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use translate::{push_acp_line, push_cli_line, BridgeState, Effect};

static STORE_LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    static STORE_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

fn default_permission() -> String {
    "yolo".into()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeProfile {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub binary: String,
    #[serde(default)]
    pub home: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_permission")]
    pub permission: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionBinding {
    pub session_id: String,
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RelayBinding {
    pub relay_agent_id: String,
    pub profile_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentRuntimesFile {
    #[serde(default)]
    pub profiles: Vec<RuntimeProfile>,
    #[serde(default)]
    pub desktop_default_profile_id: String,
    #[serde(default)]
    pub session_bindings: Vec<SessionBinding>,
    #[serde(default)]
    pub relay_bindings: Vec<RelayBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeChoice {
    Legacy,
    Ready(RuntimeProfile),
    Broken(String),
}

/// ACP stdio plus the real CLI child (for kill / wait).
pub struct AcpPipe {
    pub child: Child,
    pub pid: u32,
    pub stdin: Box<dyn tokio::io::AsyncWrite + Unpin + Send>,
    pub stdout: Box<dyn tokio::io::AsyncRead + Unpin + Send>,
    pub stderr: Option<Box<dyn tokio::io::AsyncRead + Unpin + Send>>,
    pub cli_path: PathBuf,
    pub profile_id: String,
    pub profile_kind: String,
}

pub fn store_path() -> PathBuf {
    let overridden = STORE_OVERRIDE.with(|slot| slot.borrow().clone());
    if let Some(path) = overridden {
        return path;
    }
    crate::paths::app_data_root().join("agent-runtimes.json")
}

pub fn load() -> AgentRuntimesFile {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    load_unlocked()
}

fn load_unlocked() -> AgentRuntimesFile {
    let path = store_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return AgentRuntimesFile::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save(file: &AgentRuntimesFile) -> Result<(), String> {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    save_unlocked(&normalize(file.clone()))
}

fn save_unlocked(file: &AgentRuntimesFile) -> Result<(), String> {
    let path = store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

pub fn normalize(mut file: AgentRuntimesFile) -> AgentRuntimesFile {
    let mut profiles = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for mut profile in file.profiles.drain(..) {
        profile.id = profile.id.trim().to_string();
        if profile.id.is_empty() || profile.id.len() > 64 || !seen.insert(profile.id.clone()) {
            continue;
        }
        profile.kind = match profile.kind.trim() {
            "claude" => "claude".into(),
            "cursor" => "cursor".into(),
            _ => "atlas".into(),
        };
        profile.label = profile.label.trim().to_string();
        if profile.label.is_empty() {
            profile.label = profile.kind.clone();
        }
        profile.binary = profile.binary.trim().to_string();
        profile.home = profile.home.trim().to_string();
        profile.model = profile.model.trim().to_string();
        profile.permission = if profile.permission.trim() == "ask" {
            "ask".into()
        } else {
            "yolo".into()
        };
        profiles.push(profile);
    }
    file.profiles = profiles;
    let known = |id: &str| file.profiles.iter().any(|p| p.id == id);
    file.desktop_default_profile_id = file.desktop_default_profile_id.trim().to_string();
    if !known(&file.desktop_default_profile_id) {
        file.desktop_default_profile_id.clear();
    }
    file.session_bindings.retain(|b| {
        !b.session_id.trim().is_empty() && known(b.profile_id.trim())
    });
    file.relay_bindings.retain(|b| {
        !b.relay_agent_id.trim().is_empty() && known(b.profile_id.trim())
    });
    for binding in &mut file.session_bindings {
        binding.session_id = binding.session_id.trim().to_string();
        binding.profile_id = binding.profile_id.trim().to_string();
    }
    for binding in &mut file.relay_bindings {
        binding.relay_agent_id = binding.relay_agent_id.trim().to_string();
        binding.profile_id = binding.profile_id.trim().to_string();
    }
    file
}

pub fn choice_for_profile(file: &AgentRuntimesFile, profile_id: &str) -> RuntimeChoice {
    let id = profile_id.trim();
    if id.is_empty() {
        return RuntimeChoice::Legacy;
    }
    match file.profiles.iter().find(|p| p.id == id) {
        Some(profile) if profile.enabled => RuntimeChoice::Ready(profile.clone()),
        Some(_) => RuntimeChoice::Broken(format!("runtime profile {id} is disabled")),
        None => RuntimeChoice::Broken(format!("runtime profile {id} is missing")),
    }
}

pub fn session_runtime(session_id: &str) -> RuntimeChoice {
    let file = load();
    session_runtime_in(&file, session_id)
}

pub fn session_runtime_in(file: &AgentRuntimesFile, session_id: &str) -> RuntimeChoice {
    let Some(binding) = file
        .session_bindings
        .iter()
        .find(|b| b.session_id == session_id)
    else {
        return RuntimeChoice::Legacy;
    };
    choice_for_profile(file, &binding.profile_id)
}

pub fn relay_runtime(agent_id: &str) -> RuntimeChoice {
    let file = load();
    let Some(binding) = file
        .relay_bindings
        .iter()
        .find(|b| b.relay_agent_id == agent_id)
    else {
        return RuntimeChoice::Legacy;
    };
    choice_for_profile(&file, &binding.profile_id)
}

pub fn profile_kind(profile_id: &str) -> Option<String> {
    let id = profile_id.trim();
    if id.is_empty() {
        return None;
    }
    load()
        .profiles
        .into_iter()
        .find(|profile| profile.id == id)
        .map(|profile| profile.kind)
}

/// Model configured on a runtime profile. Empty means the CLI keeps its default.
pub fn profile_model(profile_id: &str) -> Option<String> {
    let id = profile_id.trim();
    if id.is_empty() {
        return None;
    }
    load()
        .profiles
        .into_iter()
        .find(|profile| profile.id == id)
        .map(|profile| profile.model.trim().to_string())
        .filter(|model| !model.is_empty())
}

pub fn session_profile_id(session_id: &str) -> Option<String> {
    match session_runtime(session_id) {
        RuntimeChoice::Ready(profile) => Some(profile.id),
        RuntimeChoice::Broken(_) => Some("__missing__".into()),
        RuntimeChoice::Legacy => None,
    }
}

pub fn hides_meta_for_session(session_id: &str) -> bool {
    matches!(
        session_runtime(session_id),
        RuntimeChoice::Ready(profile) if profile.kind != "atlas"
    )
}

pub fn apply_desktop_default(session_id: &str) {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut file = normalize(load_unlocked());
    let default_id = file.desktop_default_profile_id.clone();
    if default_id.is_empty() {
        return;
    }
    if file.session_bindings.iter().any(|b| b.session_id == session_id) {
        return;
    }
    if !file.profiles.iter().any(|p| p.id == default_id && p.enabled) {
        return;
    }
    file.session_bindings.push(SessionBinding {
        session_id: session_id.to_string(),
        profile_id: default_id,
        cli_session_id: None,
    });
    let _ = save_unlocked(&file);
}

pub fn set_session_binding(session_id: &str, profile_id: &str) -> Result<AgentRuntimesFile, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("empty session id".into());
    }
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut file = normalize(load_unlocked());
    file.session_bindings.retain(|b| b.session_id != session_id);
    let profile_id = profile_id.trim();
    if !profile_id.is_empty() {
        if !file.profiles.iter().any(|p| p.id == profile_id && p.enabled) {
            return Err("runtime profile is not available".into());
        }
        file.session_bindings.push(SessionBinding {
            session_id: session_id.to_string(),
            profile_id: profile_id.to_string(),
            cli_session_id: None,
        });
    }
    save_unlocked(&file)?;
    Ok(file)
}

pub fn set_relay_binding(agent_id: &str, profile_id: &str) -> Result<AgentRuntimesFile, String> {
    let agent_id = agent_id.trim();
    if agent_id.is_empty() {
        return Err("empty relay agent id".into());
    }
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut file = normalize(load_unlocked());
    file.relay_bindings.retain(|b| b.relay_agent_id != agent_id);
    let profile_id = profile_id.trim();
    if !profile_id.is_empty() {
        if !file.profiles.iter().any(|p| p.id == profile_id && p.enabled) {
            return Err("runtime profile is not available".into());
        }
        file.relay_bindings.push(RelayBinding {
            relay_agent_id: agent_id.to_string(),
            profile_id: profile_id.to_string(),
        });
    }
    save_unlocked(&file)?;
    Ok(file)
}

pub fn remember_cli_session(app_session_id: &str, cli_session_id: &str) {
    let app_session_id = app_session_id.trim();
    let cli_session_id = cli_session_id.trim();
    if app_session_id.is_empty() || cli_session_id.is_empty() {
        return;
    }
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut file = load_unlocked();
    let Some(binding) = file
        .session_bindings
        .iter_mut()
        .find(|b| b.session_id == app_session_id)
    else {
        return;
    };
    if binding.cli_session_id.as_deref() == Some(cli_session_id) {
        return;
    }
    binding.cli_session_id = Some(cli_session_id.to_string());
    let _ = save_unlocked(&file);
}

pub fn cli_session_for(app_session_id: &str) -> Option<String> {
    load()
        .session_bindings
        .into_iter()
        .find(|b| b.session_id == app_session_id)
        .and_then(|b| b.cli_session_id)
        .filter(|s| !s.is_empty())
}

/// Names this kind looks up on `PATH`. Hand-entered `binary` wins at spawn.
pub fn probe_kind(kind: &str) -> Vec<String> {
    let names: &[&str] = match kind {
        "claude" => &["claude"],
        "cursor" => &["cursor-agent"],
        _ => &["atlas", "grok"],
    };
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    let mut hits = Vec::new();
    for dir in std::env::split_paths(&path_var) {
        for name in names {
            for candidate in exe_candidates(name) {
                let path = dir.join(&candidate);
                if path.is_file() {
                    hits.push(path.to_string_lossy().to_string());
                }
            }
        }
    }
    hits.sort();
    hits.dedup();
    hits
}

fn exe_candidates(name: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        vec![
            format!("{name}.exe"),
            format!("{name}.cmd"),
            format!("{name}.bat"),
            name.to_string(),
        ]
    }
    #[cfg(not(windows))]
    {
        vec![name.to_string()]
    }
}

pub fn atlas_stdio_args(model: &str) -> Vec<String> {
    let mut args = vec!["agent".to_string(), "--always-approve".to_string()];
    if !model.is_empty() {
        args.push("--model".into());
        args.push(model.to_string());
    }
    args.push("stdio".into());
    args
}

/// Cursor Agent exposes a native ACP server (`cursor-agent acp`).
/// Its print mode rejects `--input-format` / `--verbose`.
pub fn cursor_acp_args() -> Vec<String> {
    vec!["acp".into()]
}

fn stream_json_args(profile: &RuntimeProfile, resume: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "-p".into(),
        "--output-format".into(),
        "stream-json".into(),
        "--input-format".into(),
        "stream-json".into(),
        "--verbose".into(),
    ];
    if profile.permission != "ask" {
        args.push("--permission-mode".into());
        args.push("bypassPermissions".into());
    } else {
        args.push("--permission-mode".into());
        args.push("default".into());
    }
    if !profile.model.is_empty() {
        args.push("--model".into());
        args.push(profile.model.clone());
    }
    if let Some(id) = resume.map(str::trim).filter(|s| !s.is_empty()) {
        args.push("--resume".into());
        args.push(id.to_string());
    }
    args
}

pub async fn spawn_acp_pipe(
    profile: &RuntimeProfile,
    cwd: &Path,
    app_session_id: Option<&str>,
) -> Result<AcpPipe, String> {
    let binary = resolve_binary(profile)?;
    let resume = app_session_id.and_then(cli_session_for);
    let args = match profile.kind.as_str() {
        "atlas" => atlas_stdio_args(&profile.model),
        "cursor" => cursor_acp_args(),
        _ => stream_json_args(profile, resume.as_deref()),
    };
    let mut cmd = Command::new(&binary);
    cmd.args(&args);
    if profile.kind == "atlas" {
        if !profile.home.is_empty() {
            cmd.env("GROK_HOME", &profile.home);
        }
    } else if profile.kind == "claude" && !profile.home.is_empty() {
        cmd.env("CLAUDE_CONFIG_DIR", &profile.home);
    }
    // Chat folder wins. Home is only GROK_HOME / CLAUDE_CONFIG_DIR.
    // The relay passes the profile home when it has no chat folder yet.
    let work_dir = if cwd.is_dir() {
        Some(cwd.to_path_buf())
    } else if !profile.home.is_empty() && Path::new(&profile.home).is_dir() {
        Some(PathBuf::from(&profile.home))
    } else {
        None
    };
    if let Some(dir) = &work_dir {
        cmd.current_dir(dir);
    }
    crate::process_util::apply_process_group_no_window_tokio(&mut cmd);
    crate::process_util::ensure_home_env_tokio(&mut cmd);
    if let Some(path) = crate::process_util::enriched_path_env() {
        cmd.env("PATH", path);
    }
    crate::proxy::apply_to_tokio_command(&mut cmd);
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| {
        tracing::warn!(
            target: "session",
            session = app_session_id.unwrap_or("-"),
            profile_id = %profile.id,
            kind = %profile.kind,
            label = %profile.label,
            binary = %binary,
            args = %args.join(" "),
            error = %e,
            "session cli spawn failed"
        );
        format!("failed to spawn {} CLI: {e}", profile.kind)
    })?;
    let pid = child.id().unwrap_or(0);
    tracing::info!(
        target: "session",
        session = app_session_id.unwrap_or("-"),
        profile_id = %profile.id,
        kind = %profile.kind,
        label = %profile.label,
        binary = %binary,
        args = %args.join(" "),
        cwd = %work_dir
            .as_ref()
            .map(|dir| dir.display().to_string())
            .unwrap_or_else(|| "-".into()),
        pid,
        "session cli spawn"
    );
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "CLI stdin missing".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "CLI stdout missing".to_string())?;
    let stderr = child.stderr.take();
    if profile.kind == "atlas" || profile.kind == "cursor" {
        return Ok(AcpPipe {
            child,
            pid,
            stdin: Box::new(stdin),
            stdout: Box::new(stdout),
            stderr: stderr.map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
            cli_path: PathBuf::from(&binary),
            profile_id: profile.id.clone(),
            profile_kind: profile.kind.clone(),
        });
    }
    let (host, bridge) = tokio::io::duplex(1024 * 1024);
    let (host_read, host_write) = tokio::io::split(host);
    let (bridge_read, bridge_write) = tokio::io::split(bridge);
    let kind = profile.kind.clone();
    let permission = profile.permission.clone();
    let resume_id = resume.clone();
    let app_session = app_session_id.map(str::to_string);
    tokio::spawn(async move {
        translate_loop(
            kind,
            permission,
            resume_id,
            app_session,
            bridge_read,
            bridge_write,
            stdin,
            stdout,
        )
        .await;
    });
    Ok(AcpPipe {
        child,
        pid,
        stdin: Box::new(host_write),
        stdout: Box::new(host_read),
        stderr: stderr.map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
        cli_path: PathBuf::from(binary),
        profile_id: profile.id.clone(),
        profile_kind: profile.kind.clone(),
    })
}

fn resolve_binary(profile: &RuntimeProfile) -> Result<String, String> {
    if !profile.binary.trim().is_empty() {
        return Ok(profile.binary.trim().to_string());
    }
    probe_kind(&profile.kind)
        .into_iter()
        .next()
        .ok_or_else(|| format!("{} CLI not found", profile.kind))
}

async fn translate_loop(
    kind: String,
    permission: String,
    resume_id: Option<String>,
    app_session: Option<String>,
    bridge_read: tokio::io::ReadHalf<tokio::io::DuplexStream>,
    mut bridge_write: tokio::io::WriteHalf<tokio::io::DuplexStream>,
    mut cli_stdin: tokio::process::ChildStdin,
    cli_stdout: tokio::process::ChildStdout,
) {
    let mut state = BridgeState::new(&kind, &permission, resume_id.as_deref());
    let mut acp_lines = BufReader::new(bridge_read);
    let mut cli_lines = BufReader::new(cli_stdout);
    let mut acp_line = String::new();
    let mut cli_line = String::new();
    loop {
        tokio::select! {
            read = acp_lines.read_line(&mut acp_line) => {
                match read {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let effects = push_acp_line(&mut state, &acp_line);
                        acp_line.clear();
                        if apply_effects(&mut state, &effects, &mut cli_stdin, &mut bridge_write, app_session.as_deref()).await.is_err() {
                            break;
                        }
                    }
                }
            }
            read = cli_lines.read_line(&mut cli_line) => {
                match read {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let effects = push_cli_line(&mut state, &cli_line);
                        cli_line.clear();
                        if apply_effects(&mut state, &effects, &mut cli_stdin, &mut bridge_write, app_session.as_deref()).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }
}

async fn apply_effects(
    _state: &mut BridgeState,
    effects: &[Effect],
    cli_stdin: &mut tokio::process::ChildStdin,
    host_write: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>,
    app_session: Option<&str>,
) -> std::io::Result<()> {
    for effect in effects {
        match effect {
            Effect::ToCli(line) => {
                cli_stdin.write_all(line.as_bytes()).await?;
                cli_stdin.write_all(b"\n").await?;
                cli_stdin.flush().await?;
            }
            Effect::ToHost(line) => {
                host_write.write_all(line.as_bytes()).await?;
                host_write.write_all(b"\n").await?;
                host_write.flush().await?;
            }
            Effect::RememberCliSession(id) => {
                if let Some(session_id) = app_session {
                    remember_cli_session(session_id, id);
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn agent_runtimes_get() -> AgentRuntimesFile {
    normalize(load())
}

#[tauri::command]
pub fn agent_runtimes_save(file: AgentRuntimesFile) -> Result<AgentRuntimesFile, String> {
    let file = normalize(file);
    save(&file)?;
    Ok(file)
}

#[tauri::command]
pub fn agent_runtimes_probe(kind: String) -> Vec<String> {
    probe_kind(&kind)
}

#[tauri::command]
pub fn agent_runtimes_set_session_binding(
    session_id: String,
    profile_id: String,
) -> Result<AgentRuntimesFile, String> {
    set_session_binding(&session_id, &profile_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated(f: impl FnOnce()) {
        let dir = std::env::temp_dir().join(format!(
            "agent-runtimes-test-{}",
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("agent-runtimes.json");
        STORE_OVERRIDE.with(|slot| *slot.borrow_mut() = Some(path));
        f();
        STORE_OVERRIDE.with(|slot| *slot.borrow_mut() = None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn empty_file_is_legacy_for_desktop_and_relay() {
        isolated(|| {
            assert!(matches!(session_runtime("s1"), RuntimeChoice::Legacy));
            assert!(matches!(relay_runtime("agent-1"), RuntimeChoice::Legacy));
            assert!(session_profile_id("s1").is_none());
        });
    }

    #[test]
    fn default_profile_is_copied_onto_a_new_session() {
        isolated(|| {
            let file = AgentRuntimesFile {
                profiles: vec![RuntimeProfile {
                    id: "claude-1".into(),
                    kind: "claude".into(),
                    label: "Claude".into(),
                    binary: "C:/claude.exe".into(),
                    home: "C:/claude-home".into(),
                    model: "sonnet".into(),
                    permission: "ask".into(),
                    enabled: true,
                }],
                desktop_default_profile_id: "claude-1".into(),
                session_bindings: Vec::new(),
                relay_bindings: Vec::new(),
            };
            save(&file).unwrap();
            apply_desktop_default("chat-1");
            match session_runtime("chat-1") {
                RuntimeChoice::Ready(profile) => {
                    assert_eq!(profile.kind, "claude");
                    assert_eq!(profile.home, "C:/claude-home");
                }
                other => panic!("expected profile, got {other:?}"),
            }
            assert!(hides_meta_for_session("chat-1"));
        });
    }

    #[test]
    fn relay_binding_does_not_touch_an_unbound_agent() {
        isolated(|| {
            let file = AgentRuntimesFile {
                profiles: vec![RuntimeProfile {
                    id: "cursor-1".into(),
                    kind: "cursor".into(),
                    label: "Cursor".into(),
                    binary: String::new(),
                    home: String::new(),
                    model: String::new(),
                    permission: "yolo".into(),
                    enabled: true,
                }],
                desktop_default_profile_id: String::new(),
                session_bindings: Vec::new(),
                relay_bindings: vec![RelayBinding {
                    relay_agent_id: "relay-a".into(),
                    profile_id: "cursor-1".into(),
                }],
            };
            save(&file).unwrap();
            assert!(matches!(relay_runtime("relay-b"), RuntimeChoice::Legacy));
            assert!(matches!(relay_runtime("relay-a"), RuntimeChoice::Ready(_)));
        });
    }

    #[test]
    fn cursor_uses_native_acp_not_stream_json() {
        assert_eq!(cursor_acp_args(), vec!["acp".to_string()]);
    }

    #[test]
    fn atlas_args_match_the_relay_stdio_shape() {
        assert_eq!(
            atlas_stdio_args(""),
            vec!["agent", "--always-approve", "stdio"]
        );
        assert_eq!(
            atlas_stdio_args("grok-4.6"),
            vec!["agent", "--always-approve", "--model", "grok-4.6", "stdio"]
        );
    }

    #[test]
    fn switching_a_session_drops_the_cli_resume_id() {
        isolated(|| {
            let file = AgentRuntimesFile {
                profiles: vec![
                    RuntimeProfile {
                        id: "c".into(),
                        kind: "claude".into(),
                        label: "C".into(),
                        binary: "claude".into(),
                        home: String::new(),
                        model: String::new(),
                        permission: "yolo".into(),
                        enabled: true,
                    },
                    RuntimeProfile {
                        id: "k".into(),
                        kind: "cursor".into(),
                        label: "K".into(),
                        binary: "cursor-agent".into(),
                        home: String::new(),
                        model: String::new(),
                        permission: "yolo".into(),
                        enabled: true,
                    },
                ],
                desktop_default_profile_id: String::new(),
                session_bindings: vec![SessionBinding {
                    session_id: "s".into(),
                    profile_id: "c".into(),
                    cli_session_id: Some("old".into()),
                }],
                relay_bindings: Vec::new(),
            };
            save(&file).unwrap();
            set_session_binding("s", "k").unwrap();
            let saved = load();
            assert_eq!(saved.session_bindings[0].profile_id, "k");
            assert!(saved.session_bindings[0].cli_session_id.is_none());
        });
    }
}
