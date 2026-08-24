//! SSH hosts from OpenSSH config: list, test, probe remote Grok CLI.
//!
//! Wave 1 of remote workspaces. The App does **not** spawn a remote agent here.
//! Transport is the system `ssh` binary so `~/.ssh/config` (ProxyJump, keys,
//! ssh-agent) keeps working. Aliases are argv, never interpolated into a shell.

use std::collections::{HashMap, HashSet};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::time::timeout;

use crate::process_util;

const SSH_CONNECT_TIMEOUT_SECS: u64 = 8;
const SSH_OVERALL_TIMEOUT_SECS: u64 = 15;
const SSH_INSPECT_TIMEOUT_SECS: u64 = 25;
const INCLUDE_DEPTH_MAX: u32 = 8;
const ERROR_CHARS: usize = 240;

/// Remote POSIX snippet. Constant — no host alias inside.
const REMOTE_PROBE: &str = r#"BIN=""
if command -v grok >/dev/null 2>&1; then
  BIN=$(command -v grok)
elif [ -x "$HOME/.grok/bin/grok" ]; then
  BIN="$HOME/.grok/bin/grok"
fi
echo GROK_APP_PROBE
if [ -z "$BIN" ]; then
  echo CLI_MISSING
  echo AUTH_MISSING
  echo
  echo
  exit 0
fi
echo CLI_OK
if [ -f "$HOME/.grok/auth.json" ]; then
  echo AUTH_OK
else
  echo AUTH_MISSING
fi
echo "$BIN"
"$BIN" --version 2>/dev/null | head -n 1
exit 0
"#;

const INSTALL_REMOTE: &str = "curl -fsSL https://x.ai/cli/install.sh | bash";
const LOGIN_REMOTE: &str = "grok login --device-auth";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SshHostDto {
    pub alias: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshListResult {
    pub hosts: Vec<SshHostDto>,
    pub config_path: String,
    pub config_exists: bool,
    pub ssh_found: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshProbeResult {
    pub alias: String,
    /// True only when SSH connected **and** the probe marker came back.
    pub ok: bool,
    pub ssh_ok: bool,
    /// `ok` | `missing` | `unknown`
    pub cli: String,
    /// `ok` | `missing` | `unknown`
    pub auth: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    pub install_cmd: String,
    pub login_cmd: String,
    pub install_remote_cmd: String,
    pub login_remote_cmd: String,
}

/// Concrete Host alias: no glob, no leading hyphen.
pub fn is_safe_ssh_alias(alias: &str) -> bool {
    let b = alias.as_bytes();
    if b.is_empty() || b.len() > 255 {
        return false;
    }
    if matches!(b[0], b'-' | b'.') {
        return false;
    }
    b.iter()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
}

/// True when this project lives on an OpenSSH host. Do not treat its path as
/// local `std::fs`. Wave 3 spawns `grok agent stdio` through `ssh`, never as a
/// local child with the remote path as cwd.
pub fn should_skip_local_acp_spawn(ssh_alias: Option<&str>) -> bool {
    ssh_alias
        .map(str::trim)
        .is_some_and(|s| !s.is_empty() && is_safe_ssh_alias(s))
}

/// First safe alias wins: explicit connect arg, bound project, path match.
pub fn pick_ssh_alias(
    explicit: Option<&str>,
    bound_project_alias: Option<&str>,
    path_project_alias: Option<&str>,
) -> Option<String> {
    for raw in [explicit, bound_project_alias, path_project_alias] {
        if let Some(a) = raw.map(str::trim).filter(|s| is_safe_ssh_alias(s)) {
            return Some(a.to_string());
        }
    }
    None
}

/// Local `grok agent stdio` needs a real directory on this machine.
/// SSH aliases and missing paths must not go through local spawn (ENOENT
/// used to be mislabeled `CLI_NOT_FOUND`).
pub fn local_acp_cwd_ok(ssh_alias: Option<&str>, cwd: &str) -> bool {
    if should_skip_local_acp_spawn(ssh_alias) {
        return false;
    }
    let t = cwd.trim();
    !t.is_empty() && std::path::Path::new(t).is_dir()
}

/// Whether ACP `session/new` may use this cwd.
///
/// SSH: grok checks the path on the host. A local `is_dir` miss used to abort
/// connect with `AGENT_CRASHED` after a successful remote handshake.
/// Local: must be a directory on this machine.
pub fn acp_session_cwd_ok(ssh_alias: Option<&str>, cwd: &str) -> bool {
    let t = cwd.trim();
    if t.is_empty() || t.contains('\0') {
        return false;
    }
    if should_skip_local_acp_spawn(ssh_alias) {
        return true;
    }
    std::path::Path::new(t).is_dir()
}

/// Same gate as `grok sessions list` / TUI `/resume` for a cwd.
///
/// Disk under `~/.grok/sessions` also stores subagent children and empty
/// shells that only have `chat_history.jsonl`. Those are not resumable
/// parent chats. Grok's list uses `summary.json` `session_kind` plus the
/// `updates.jsonl` restore log — not every directory.
pub fn remote_session_is_listable(
    session_kind: Option<&str>,
    title: &str,
    has_updates: bool,
) -> bool {
    let kind = session_kind.unwrap_or("").trim().to_ascii_lowercase();
    if kind.starts_with("subagent") {
        return false;
    }
    has_updates || !title.trim().is_empty()
}

pub fn is_pattern_token(tok: &str) -> bool {
    tok.contains('*') || tok.contains('?') || tok.contains('!')
}

fn default_ssh_config_path() -> PathBuf {
    process_util::user_home().join(".ssh").join("config")
}

fn find_ssh_binary() -> Option<PathBuf> {
    which::which("ssh").ok().filter(|p| p.is_file())
}

fn truncate_err(s: &str) -> String {
    let t = s.trim();
    if t.chars().count() <= ERROR_CHARS {
        t.to_string()
    } else {
        format!("{}…", t.chars().take(ERROR_CHARS).collect::<String>())
    }
}

fn classify_ssh_stderr(stderr: &str) -> (&'static str, String) {
    let l = stderr.to_ascii_lowercase();
    if l.contains("host key") || l.contains("known_hosts") || l.contains("authenticity of host") {
        (
            "host_key",
            truncate_err(stderr)
                .if_empty("Host key not in known_hosts. Run ssh <alias> once in a terminal."),
        )
    } else if l.contains("permission denied") {
        ("auth", truncate_err(stderr).if_empty("Permission denied"))
    } else if l.contains("timed out") || l.contains("timeout") || l.contains("connection timed out")
    {
        (
            "timeout",
            truncate_err(stderr).if_empty("Connection timed out"),
        )
    } else if l.contains("could not resolve")
        || l.contains("name or service not known")
        || l.contains("nodename nor servname")
    {
        (
            "connect",
            truncate_err(stderr).if_empty("Could not resolve host"),
        )
    } else if l.contains("connection refused") {
        (
            "connect",
            truncate_err(stderr).if_empty("Connection refused"),
        )
    } else if l.contains("connection reset") {
        ("connect", truncate_err(stderr).if_empty("Connection reset"))
    } else if stderr.trim().is_empty() {
        ("other", "SSH failed with no stderr".into())
    } else {
        ("other", truncate_err(stderr))
    }
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.trim().is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

pub fn commands_for_alias(alias: &str) -> (String, String, String, String) {
    let install = format!("ssh {alias} '{INSTALL_REMOTE}'");
    let login = format!("ssh -t {alias} '{LOGIN_REMOTE}'");
    (
        install,
        login,
        INSTALL_REMOTE.to_string(),
        LOGIN_REMOTE.to_string(),
    )
}

fn empty_probe(alias: &str, code: &str, err: impl Into<String>) -> SshProbeResult {
    let (install_cmd, login_cmd, install_remote_cmd, login_remote_cmd) = commands_for_alias(alias);
    SshProbeResult {
        alias: alias.to_string(),
        ok: false,
        ssh_ok: false,
        cli: "unknown".into(),
        auth: "unknown".into(),
        cli_path: None,
        cli_version: None,
        error: Some(err.into()),
        error_code: Some(code.into()),
        latency_ms: None,
        install_cmd,
        login_cmd,
        install_remote_cmd,
        login_remote_cmd,
    }
}

/// Parse OpenSSH config text. `Include` is resolved via `read_file`.
pub fn parse_ssh_config(
    text: &str,
    base_dir: &Path,
    read_file: &dyn Fn(&Path) -> Option<String>,
) -> Vec<SshHostDto> {
    let mut visited = HashSet::new();
    let mut out = Vec::new();
    parse_ssh_config_inner(text, base_dir, read_file, 0, &mut visited, &mut out);
    out
}

fn parse_ssh_config_inner(
    text: &str,
    base_dir: &Path,
    read_file: &dyn Fn(&Path) -> Option<String>,
    depth: u32,
    visited: &mut HashSet<String>,
    out: &mut Vec<SshHostDto>,
) {
    let mut current_aliases: Vec<String> = Vec::new();
    let mut hostname: Option<String> = None;
    let mut user: Option<String> = None;
    let mut port: Option<u16> = None;
    let mut identity: Option<String> = None;
    let mut in_host = false;

    let flush = |aliases: &mut Vec<String>,
                 hostname: &mut Option<String>,
                 user: &mut Option<String>,
                 port: &mut Option<u16>,
                 identity: &mut Option<String>,
                 in_host: &mut bool,
                 out: &mut Vec<SshHostDto>| {
        if *in_host {
            for alias in aliases.drain(..) {
                if !is_safe_ssh_alias(&alias) {
                    continue;
                }
                if out.iter().any(|h| h.alias == alias) {
                    continue;
                }
                out.push(SshHostDto {
                    alias: alias.clone(),
                    hostname: hostname.clone(),
                    user: user.clone(),
                    port: *port,
                    identity_file: identity.clone(),
                });
            }
        } else {
            aliases.clear();
        }
        *hostname = None;
        *user = None;
        *port = None;
        *identity = None;
        *in_host = false;
    };

    for raw in text.lines() {
        let line = strip_ssh_comment(raw).trim().to_string();
        if line.is_empty() {
            continue;
        }
        let (kw, rest) = split_keyword(&line);
        let kw_l = kw.to_ascii_lowercase();
        if kw_l == "host" {
            flush(
                &mut current_aliases,
                &mut hostname,
                &mut user,
                &mut port,
                &mut identity,
                &mut in_host,
                out,
            );
            let tokens = split_ws(&rest);
            let concrete: Vec<String> = tokens
                .into_iter()
                .filter(|t| !is_pattern_token(t))
                .collect();
            if concrete.is_empty() {
                in_host = false;
                current_aliases.clear();
            } else {
                in_host = true;
                current_aliases = concrete;
            }
            continue;
        }
        if kw_l == "match" {
            flush(
                &mut current_aliases,
                &mut hostname,
                &mut user,
                &mut port,
                &mut identity,
                &mut in_host,
                out,
            );
            continue;
        }
        if kw_l == "include" && depth < INCLUDE_DEPTH_MAX {
            for spec in split_ws(&rest) {
                include_spec(&spec, base_dir, read_file, depth, visited, out);
            }
            continue;
        }
        if !in_host {
            continue;
        }
        match kw_l.as_str() {
            "hostname" => {
                if hostname.is_none() {
                    hostname = unquote(&rest);
                }
            }
            "user" => {
                if user.is_none() {
                    user = unquote(&rest);
                }
            }
            "port" => {
                if port.is_none() {
                    if let Some(v) = unquote(&rest).and_then(|s| s.parse::<u16>().ok()) {
                        if v > 0 {
                            port = Some(v);
                        }
                    }
                }
            }
            "identityfile" => {
                if identity.is_none() {
                    identity = unquote(&rest);
                }
            }
            _ => {}
        }
    }
    flush(
        &mut current_aliases,
        &mut hostname,
        &mut user,
        &mut port,
        &mut identity,
        &mut in_host,
        out,
    );
}

fn include_spec(
    spec: &str,
    base_dir: &Path,
    read_file: &dyn Fn(&Path) -> Option<String>,
    depth: u32,
    visited: &mut HashSet<String>,
    out: &mut Vec<SshHostDto>,
) {
    for path in expand_include_paths(spec, base_dir) {
        let key = path.to_string_lossy().to_string();
        if !visited.insert(key) {
            continue;
        }
        let Some(text) = read_file(&path) else {
            continue;
        };
        let next_base = path.parent().unwrap_or(base_dir);
        parse_ssh_config_inner(&text, next_base, read_file, depth + 1, visited, out);
    }
}

fn expand_include_paths(spec: &str, base_dir: &Path) -> Vec<PathBuf> {
    let expanded = expand_tilde(spec);
    let path = if Path::new(&expanded).is_absolute() {
        PathBuf::from(&expanded)
    } else {
        base_dir.join(&expanded)
    };
    let os = path.to_string_lossy();
    if let Some(star) = os.find('*') {
        if os[star + 1..].contains('*') || os[star + 1..].contains('/') {
            return Vec::new();
        }
        let parent = Path::new(&os[..star]).to_path_buf();
        let prefix = Path::new(&os[..star])
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let suffix = os[star + 1..].to_string();
        let dir = if os[..star].ends_with('/') {
            parent
        } else {
            parent.parent().unwrap_or(base_dir).to_path_buf()
        };
        let Ok(rd) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .filter(|p| {
                let name = p.file_name().map(|s| s.to_string_lossy().to_string());
                let Some(name) = name else {
                    return false;
                };
                name.starts_with(&prefix) && name.ends_with(&suffix)
            })
            .collect();
        files.sort();
        files
    } else {
        vec![path]
    }
}

fn expand_tilde(s: &str) -> String {
    if let Some(rest) = s.strip_prefix("~/") {
        process_util::user_home()
            .join(rest)
            .to_string_lossy()
            .into_owned()
    } else if s == "~" {
        process_util::user_home().to_string_lossy().into_owned()
    } else {
        s.to_string()
    }
}

fn strip_ssh_comment(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        if quote.is_none() && c == '#' {
            break;
        }
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(c);
                out.push(n);
            }
            continue;
        }
        if c == '"' || c == '\'' {
            if quote == Some(c) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(c);
            }
        }
        out.push(c);
    }
    out
}

fn split_keyword(line: &str) -> (String, String) {
    let line = line.trim();
    if let Some(eq) = line.find('=') {
        let (a, b) = line.split_at(eq);
        if !a.trim().contains(char::is_whitespace) {
            return (a.trim().to_string(), b[1..].trim().to_string());
        }
    }
    match line.split_once(char::is_whitespace) {
        Some((k, rest)) => (k.to_string(), rest.trim().to_string()),
        None => (line.to_string(), String::new()),
    }
}

fn split_ws(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = s.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        if quote.is_none() && c.is_whitespace() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if c == '"' || c == '\'' {
            if quote == Some(c) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(c);
            } else {
                cur.push(c);
            }
            continue;
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn unquote(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let t = if (t.starts_with('"') && t.ends_with('"') && t.len() >= 2)
        || (t.starts_with('\'') && t.ends_with('\'') && t.len() >= 2)
    {
        &t[1..t.len() - 1]
    } else {
        t
    };
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

pub fn parse_probe_stdout(stdout: &str) -> Option<ParsedProbe> {
    let mut lines = stdout.lines().map(|l| l.trim_end_matches('\r'));
    while let Some(line) = lines.next() {
        if line.trim() == "GROK_APP_PROBE" {
            let cli = lines.next().unwrap_or("").trim();
            let auth = lines.next().unwrap_or("").trim();
            let path = lines.next().unwrap_or("").trim();
            let version = lines.next().unwrap_or("").trim();
            let cli = match cli {
                "CLI_OK" => "ok",
                "CLI_MISSING" => "missing",
                _ => return None,
            };
            let auth = match auth {
                "AUTH_OK" => "ok",
                "AUTH_MISSING" => "missing",
                _ => return None,
            };
            return Some(ParsedProbe {
                cli: cli.into(),
                auth: auth.into(),
                path: if path.is_empty() {
                    None
                } else {
                    Some(path.to_string())
                },
                version: if version.is_empty() {
                    None
                } else {
                    Some(version.to_string())
                },
            });
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedProbe {
    pub cli: String,
    pub auth: String,
    pub path: Option<String>,
    pub version: Option<String>,
}

fn fs_read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

#[tauri::command]
pub async fn ssh_list_hosts() -> Result<SshListResult, String> {
    let config_path = default_ssh_config_path();
    let config_exists = config_path.is_file();
    let ssh_found = find_ssh_binary().is_some();
    let text = if config_exists {
        std::fs::read_to_string(&config_path).unwrap_or_default()
    } else {
        String::new()
    };
    let base = config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(process_util::user_home);
    let hosts = if config_exists {
        parse_ssh_config(&text, &base, &fs_read)
    } else {
        Vec::new()
    };
    Ok(SshListResult {
        hosts,
        config_path: config_path.to_string_lossy().into_owned(),
        config_exists,
        ssh_found,
        error: None,
    })
}

#[tauri::command]
pub async fn ssh_test_host(alias: String) -> Result<SshProbeResult, String> {
    let alias = alias.trim().to_string();
    if !is_safe_ssh_alias(&alias) {
        return Ok(empty_probe(
            &alias,
            "invalid_alias",
            "Host alias is not a concrete OpenSSH Host name",
        ));
    }
    let Some(ssh) = find_ssh_binary() else {
        return Ok(empty_probe(
            &alias,
            "ssh_missing",
            "OpenSSH client (ssh) was not found on this machine",
        ));
    };

    let (install_cmd, login_cmd, install_remote_cmd, login_remote_cmd) = commands_for_alias(&alias);

    let mut cmd = Command::new(&ssh);
    process_util::apply_no_window_tokio(&mut cmd);
    cmd.arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg(format!("ConnectTimeout={SSH_CONNECT_TIMEOUT_SECS}"))
        .arg("-o")
        .arg("PasswordAuthentication=no")
        .arg("-o")
        .arg("KbdInteractiveAuthentication=no")
        .arg("-o")
        .arg("StrictHostKeyChecking=yes")
        .arg(&alias)
        .arg(REMOTE_PROBE)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let started = Instant::now();
    let joined = timeout(Duration::from_secs(SSH_OVERALL_TIMEOUT_SECS), cmd.output()).await;
    let latency_ms = started.elapsed().as_millis() as u64;

    let output = match joined {
        Err(_) => {
            return Ok(SshProbeResult {
                alias,
                ok: false,
                ssh_ok: false,
                cli: "unknown".into(),
                auth: "unknown".into(),
                cli_path: None,
                cli_version: None,
                error: Some("Connection timed out".into()),
                error_code: Some("timeout".into()),
                latency_ms: Some(latency_ms),
                install_cmd,
                login_cmd,
                install_remote_cmd,
                login_remote_cmd,
            });
        }
        Ok(Err(e)) => {
            return Ok(SshProbeResult {
                alias,
                ok: false,
                ssh_ok: false,
                cli: "unknown".into(),
                auth: "unknown".into(),
                cli_path: None,
                cli_version: None,
                error: Some(truncate_err(&e.to_string())),
                error_code: Some("other".into()),
                latency_ms: Some(latency_ms),
                install_cmd,
                login_cmd,
                install_remote_cmd,
                login_remote_cmd,
            });
        }
        Ok(Ok(o)) => o,
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        let (code, msg) = classify_ssh_stderr(&stderr);
        return Ok(SshProbeResult {
            alias,
            ok: false,
            ssh_ok: false,
            cli: "unknown".into(),
            auth: "unknown".into(),
            cli_path: None,
            cli_version: None,
            error: Some(msg),
            error_code: Some(code.into()),
            latency_ms: Some(latency_ms),
            install_cmd,
            login_cmd,
            install_remote_cmd,
            login_remote_cmd,
        });
    }

    match parse_probe_stdout(&stdout) {
        Some(p) => Ok(SshProbeResult {
            alias,
            ok: true,
            ssh_ok: true,
            cli: p.cli,
            auth: p.auth,
            cli_path: p.path,
            cli_version: p.version,
            error: None,
            error_code: None,
            latency_ms: Some(latency_ms),
            install_cmd,
            login_cmd,
            install_remote_cmd,
            login_remote_cmd,
        }),
        None => Ok(SshProbeResult {
            alias,
            ok: false,
            ssh_ok: true,
            cli: "unknown".into(),
            auth: "unknown".into(),
            cli_path: None,
            cli_version: None,
            error: Some(
                "SSH connected but the remote probe did not return GROK_APP_PROBE (need a POSIX login shell)"
                    .into(),
            ),
            error_code: Some("probe_parse".into()),
            latency_ms: Some(latency_ms),
            install_cmd,
            login_cmd,
            install_remote_cmd,
            login_remote_cmd,
        }),
    }
}

pub fn posix_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Join a remote POSIX project root with a relative path. Rejects `..`.
pub fn join_remote_rel(root: &str, relative: &str) -> Result<String, String> {
    let root = root.trim().trim_end_matches('/');
    if root.is_empty() || !root.starts_with('/') || root.contains('\0') {
        return Err("invalid remote project root".into());
    }
    if relative.contains('\0') {
        return Err("invalid path".into());
    }
    let rel = relative
        .trim()
        .trim_start_matches("./")
        .trim_start_matches('/');
    let mut parts: Vec<&str> = root.split('/').filter(|s| !s.is_empty()).collect();
    if !rel.is_empty() && rel != "." {
        for c in rel.split('/') {
            if c.is_empty() || c == "." {
                continue;
            }
            if c == ".." {
                return Err("path escapes project root".into());
            }
            parts.push(c);
        }
    }
    Ok(format!("/{}", parts.join("/")))
}

fn remote_file_kind(name: &str) -> &'static str {
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "md" | "mdx" | "markdown" => "markdown",
        "json" | "jsonc" => "json",
        "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java" | "toml" | "yml" | "yaml"
        | "css" | "html" | "sh" => "code",
        _ => "text",
    }
}

fn remote_file_mime(kind: &str) -> &'static str {
    match kind {
        "markdown" => "text/markdown",
        "json" => "application/json",
        "code" => "text/plain",
        _ => "text/plain",
    }
}

pub fn percent_decode_path(enc: &str) -> String {
    let bytes = enc.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn ensure_control_dir() -> Result<PathBuf, String> {
    let dir = control_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create SSH control dir: {e}"))?;
    Ok(dir)
}

fn control_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("GROK_APP_SSH_CM") {
        return PathBuf::from(custom);
    }
    // macOS data_dir is `Library/Application Support/...` (spaces). OpenSSH
    // parses `-o` as a config line, so an unquoted ControlPath there becomes
    // `keyword controlpath extra arguments at end of line`. cache_dir is
    // `Library/Caches/...` and shorter for AF_UNIX sun_path.
    directories::ProjectDirs::from("com", "grokapp", "grok-app")
        .map(|p| p.cache_dir().join("ssh-cm"))
        .unwrap_or_else(|| std::env::temp_dir().join("grok-app-ssh-cm"))
}

fn control_path(alias: &str) -> PathBuf {
    control_dir().join(format!("{alias}.sock"))
}

/// `-o KEY=VALUE` is a ssh_config line. Quote values that contain spaces.
fn ssh_config_assignment(key: &str, value: &str) -> String {
    if ssh_config_value_needs_quotes(value) {
        format!("{key}=\"{}\"", escape_ssh_config_value(value))
    } else {
        format!("{key}={value}")
    }
}

fn ssh_config_value_needs_quotes(value: &str) -> bool {
    value.is_empty()
        || value.bytes().any(|b| {
            matches!(
                b,
                b' ' | b'\t' | b'"' | b'\'' | b'#' | b'\\' | b'=' | b'\n' | b'\r'
            )
        })
}

fn escape_ssh_config_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn push_ssh_opt(cmd: &mut Command, key: &str, value: impl AsRef<str>) {
    cmd.arg("-o")
        .arg(ssh_config_assignment(key, value.as_ref()));
}

fn apply_base_ssh_opts(cmd: &mut Command) {
    push_ssh_opt(cmd, "BatchMode", "yes");
    push_ssh_opt(cmd, "ConnectTimeout", SSH_CONNECT_TIMEOUT_SECS.to_string());
    push_ssh_opt(cmd, "PasswordAuthentication", "no");
    push_ssh_opt(cmd, "KbdInteractiveAuthentication", "no");
    push_ssh_opt(cmd, "StrictHostKeyChecking", "yes");
}

fn apply_control_opts(cmd: &mut Command, alias: &str, master: &str) {
    push_ssh_opt(cmd, "ControlMaster", master);
    push_ssh_opt(
        cmd,
        "ControlPath",
        control_path(alias).to_string_lossy().as_ref(),
    );
    push_ssh_opt(cmd, "ControlPersist", "yes");
}

fn apply_common_ssh_opts(cmd: &mut Command, alias: &str, mux: bool) {
    apply_base_ssh_opts(cmd);
    if mux {
        apply_control_opts(cmd, alias, "auto");
    }
}

fn push_ssh_opt_argv(args: &mut Vec<String>, key: &str, value: &str) {
    args.push("-o".into());
    args.push(ssh_config_assignment(key, value));
}

/// `ssh -tt` argv: ControlMaster + remote login shell in the project cwd.
/// Alias is its own argv word. Remote cwd is POSIX-quoted inside the remote snippet.
pub fn ssh_pty_argv(alias: &str, remote_cwd: Option<&str>) -> Result<Vec<String>, String> {
    if !is_safe_ssh_alias(alias) {
        return Err("invalid SSH host alias".into());
    }
    let ssh = find_ssh_binary()
        .ok_or_else(|| "OpenSSH client (ssh) was not found on this machine".to_string())?;
    let _ = ensure_control_dir();
    let mut args = vec![ssh.to_string_lossy().into_owned(), "-tt".to_string()];
    push_ssh_opt_argv(&mut args, "BatchMode", "yes");
    push_ssh_opt_argv(
        &mut args,
        "ConnectTimeout",
        &SSH_CONNECT_TIMEOUT_SECS.to_string(),
    );
    push_ssh_opt_argv(&mut args, "PasswordAuthentication", "no");
    push_ssh_opt_argv(&mut args, "KbdInteractiveAuthentication", "no");
    push_ssh_opt_argv(&mut args, "StrictHostKeyChecking", "yes");
    push_ssh_opt_argv(&mut args, "RequestTTY", "yes");
    push_ssh_opt_argv(&mut args, "ControlMaster", "auto");
    push_ssh_opt_argv(
        &mut args,
        "ControlPath",
        control_path(alias).to_string_lossy().as_ref(),
    );
    push_ssh_opt_argv(&mut args, "ControlPersist", "yes");
    args.push(alias.to_string());
    args.push(ssh_pty_remote_cmd(remote_cwd));
    Ok(args)
}

/// Remote launch header for ACP stdio. Cwd and grok flags are appended as
/// POSIX-quoted words. OpenSSH joins the remote command into one `-c` string,
/// so extra argv after `bash -lc` is not a real argv array.
const REMOTE_ACP_HEADER: &str = r#"export PATH="$HOME/.grok/bin:$PATH"
BIN=$(command -v grok 2>/dev/null || true)
if [ -z "$BIN" ] && [ -x "$HOME/.grok/bin/grok" ]; then BIN="$HOME/.grok/bin/grok"; fi
if [ -z "$BIN" ]; then echo GROK_APP_CLI_MISSING >&2; exit 127; fi
"#;

/// One remote `-c` script: cd + `exec grok <quoted flags>`. Never interpolates the alias.
pub fn ssh_acp_remote_command(remote_cwd: &str, grok_args: &[String]) -> Result<String, String> {
    if remote_cwd.contains('\0') || grok_args.iter().any(|a| a.contains('\0')) {
        return Err("invalid remote ACP command".into());
    }
    let mut script = REMOTE_ACP_HEADER.to_string();
    let dir = remote_cwd.trim();
    if !dir.is_empty() {
        let q = posix_single_quote(dir);
        script.push_str(&format!(
            "DIR={q}\ncase \"$DIR\" in ~*) DIR=\"$HOME${{DIR#~}}\" ;; esac\nif [ -d \"$DIR\" ]; then cd \"$DIR\" || exit 1; fi\n"
        ));
    }
    script.push_str("exec \"$BIN\"");
    for a in grok_args {
        script.push(' ');
        script.push_str(&posix_single_quote(a));
    }
    script.push('\n');
    Ok(script)
}

/// `ssh -T` argv: ControlMaster + a single remote script (cwd and grok flags inside).
pub fn ssh_acp_argv(
    alias: &str,
    remote_cwd: &str,
    grok_args: &[String],
) -> Result<Vec<String>, String> {
    if !is_safe_ssh_alias(alias) {
        return Err("invalid SSH host alias".into());
    }
    let ssh = find_ssh_binary()
        .ok_or_else(|| "OpenSSH client (ssh) was not found on this machine".to_string())?;
    let _ = ensure_control_dir();
    let mut args = vec![ssh.to_string_lossy().into_owned(), "-T".to_string()];
    push_ssh_opt_argv(&mut args, "BatchMode", "yes");
    push_ssh_opt_argv(
        &mut args,
        "ConnectTimeout",
        &SSH_CONNECT_TIMEOUT_SECS.to_string(),
    );
    push_ssh_opt_argv(&mut args, "PasswordAuthentication", "no");
    push_ssh_opt_argv(&mut args, "KbdInteractiveAuthentication", "no");
    push_ssh_opt_argv(&mut args, "StrictHostKeyChecking", "yes");
    push_ssh_opt_argv(&mut args, "RequestTTY", "no");
    push_ssh_opt_argv(&mut args, "ControlMaster", "auto");
    push_ssh_opt_argv(
        &mut args,
        "ControlPath",
        control_path(alias).to_string_lossy().as_ref(),
    );
    push_ssh_opt_argv(&mut args, "ControlPersist", "yes");
    args.push(alias.to_string());
    args.push(ssh_acp_remote_command(remote_cwd, grok_args)?);
    Ok(args)
}

/// Local `ssh` process whose remote side execs `grok agent stdio` in `remote_cwd`.
pub fn start_ssh_acp_command(
    alias: &str,
    remote_cwd: &str,
    grok_args: &[String],
) -> Result<tokio::process::Command, String> {
    let argv = ssh_acp_argv(alias, remote_cwd, grok_args)?;
    let mut cmd = tokio::process::Command::new(&argv[0]);
    crate::process_util::apply_no_window_tokio(&mut cmd);
    for a in argv.iter().skip(1) {
        cmd.arg(a);
    }
    Ok(cmd)
}

/// Remote snippet for an interactive PTY. Never interpolates the alias.
pub fn ssh_pty_remote_cmd(remote_cwd: Option<&str>) -> String {
    let dir = remote_cwd
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.contains('\0'))
        .unwrap_or("");
    if dir.is_empty() {
        return "exec ${SHELL:-bash} -l".to_string();
    }
    let q = posix_single_quote(dir);
    format!(
        "DIR={q}; case \"$DIR\" in ~*) DIR=\"$HOME${{DIR#~}}\" ;; esac; if [ -d \"$DIR\" ]; then cd \"$DIR\" || true; fi; exec ${{SHELL:-bash}} -l"
    )
}

struct SshRun {
    success: bool,
    stdout: String,
    stderr: String,
    latency_ms: u64,
}

enum SshRunErr {
    Missing,
    Timeout { latency_ms: u64 },
    Spawn(String),
}

async fn run_ssh(alias: &str, remote: &str, mux: bool, secs: u64) -> Result<SshRun, SshRunErr> {
    run_ssh_io(alias, remote, mux, secs, None).await
}

async fn run_ssh_io(
    alias: &str,
    remote: &str,
    mux: bool,
    secs: u64,
    stdin: Option<&[u8]>,
) -> Result<SshRun, SshRunErr> {
    let ssh = find_ssh_binary().ok_or(SshRunErr::Missing)?;
    let mut cmd = Command::new(&ssh);
    process_util::apply_no_window_tokio(&mut cmd);
    apply_common_ssh_opts(&mut cmd, alias, mux);
    cmd.arg(alias).arg(remote);
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let started = Instant::now();
    let fut = async {
        if let Some(bytes) = stdin {
            let mut child = cmd.spawn().map_err(|e| e.to_string())?;
            if let Some(mut sin) = child.stdin.take() {
                use tokio::io::AsyncWriteExt;
                sin.write_all(bytes).await.map_err(|e| e.to_string())?;
                drop(sin);
            }
            child.wait_with_output().await.map_err(|e| e.to_string())
        } else {
            cmd.output().await.map_err(|e| e.to_string())
        }
    };
    match timeout(Duration::from_secs(secs), fut).await {
        Err(_) => Err(SshRunErr::Timeout {
            latency_ms: started.elapsed().as_millis() as u64,
        }),
        Ok(Err(e)) => Err(SshRunErr::Spawn(e)),
        Ok(Ok(o)) => Ok(SshRun {
            success: o.status.success(),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
            latency_ms: started.elapsed().as_millis() as u64,
        }),
    }
}

fn remote_ls_script(dir: &str) -> String {
    let q = posix_single_quote(dir);
    format!(
        r#"DIR={q}
if [ -z "$DIR" ]; then DIR="$HOME"; fi
case "$DIR" in
  ~*) DIR="$HOME${{DIR#~}}" ;;
esac
if [ ! -d "$DIR" ]; then
  echo GROK_APP_LS_ERR
  echo not_a_dir
  exit 0
fi
cd "$DIR" || {{ echo GROK_APP_LS_ERR; echo cd_fail; exit 0; }}
echo GROK_APP_LS
pwd
ls -1p 2>/dev/null | head -n 400
exit 0
"#
    )
}

fn remote_sess_script(offset: u32, limit: u32) -> String {
    let offset = offset.min(50_000);
    let limit = limit.clamp(1, 50);
    format!(
        r#"OFFSET={offset} LIMIT={limit}
SESS="$HOME/.grok/sessions"
if command -v python3 >/dev/null 2>&1; then
  OFFSET="$OFFSET" LIMIT="$LIMIT" python3 -c '
import json, os
root = os.path.expanduser("~/.grok/sessions")
off = int(os.environ.get("OFFSET", "0"))
lim = int(os.environ.get("LIMIT", "20"))
rows = []
if os.path.isdir(root):
    for enc in os.listdir(root):
        base = os.path.join(root, enc)
        if not os.path.isdir(base):
            continue
        for sid in os.listdir(base):
            d = os.path.join(base, sid)
            if not os.path.isdir(d) or sid.startswith("."):
                continue
            sp = os.path.join(d, "summary.json")
            kind = ""
            title = ""
            if os.path.isfile(sp):
                try:
                    s = json.load(open(sp))
                    kind = str(s.get("session_kind") or "").strip().lower()
                    title = (s.get("generated_title") or s.get("session_summary") or s.get("title") or "").strip()
                except Exception:
                    pass
            if kind.startswith("subagent"):
                continue
            up = os.path.join(d, "updates.jsonl")
            has_up = os.path.isfile(up) and os.path.getsize(up) > 0
            if not has_up and not title:
                continue
            try:
                mt = os.path.getmtime(d)
            except OSError:
                continue
            rows.append((mt, sid, enc, d, title))
rows.sort(reverse=True)
print("GROK_APP_SESS")
print("TOTAL\t%d" % len(rows))
for mt, sid, enc, d, title in rows[off:off+lim]:
    if not title:
        hp = os.path.join(d, "chat_history.jsonl")
        if os.path.isfile(hp):
            try:
                f = open(hp)
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        o = json.loads(line)
                    except Exception:
                        continue
                    role = o.get("type") or o.get("role") or ""
                    if role != "user":
                        continue
                    c = o.get("content")
                    text = ""
                    if isinstance(c, str):
                        text = c
                    elif isinstance(c, list):
                        bits = []
                        for p in c:
                            if isinstance(p, str):
                                bits.append(p)
                            elif isinstance(p, dict):
                                bits.append(str(p.get("text") or ""))
                        text = " ".join(bits)
                    text = text.strip()
                    if not text:
                        continue
                    if "<user_query>" in text:
                        a = text.find("<user_query>") + 12
                        b = text.find("</user_query>", a)
                        chunk = text[a:b] if b > a else text[a:]
                        title = chunk.strip().splitlines()[0].strip() if chunk.strip() else ""
                        if title:
                            break
                        continue
                    if "<system-reminder>" in text or "<user_info>" in text:
                        continue
                    title = text.splitlines()[0].strip()
                    if title:
                        break
                f.close()
            except Exception:
                pass
    title = title.replace("\t", " ").replace("\n", " ").replace("\r", " ")[:160]
    print("%s\t%s\t%d\t%s" % (sid, enc, int(mt), title))
' && exit 0
fi
echo GROK_APP_SESS
if [ ! -d "$SESS" ]; then
  echo "TOTAL	0"
  exit 0
fi
tmp=$(mktemp 2>/dev/null || echo /tmp/grok-app-sess.$$)
find "$SESS" -mindepth 2 -maxdepth 2 -type d 2>/dev/null | while IFS= read -r d; do
  if [ -f "$d/summary.json" ] && grep -q '"session_kind"[[:space:]]*:[[:space:]]*"subagent' "$d/summary.json" 2>/dev/null; then
    continue
  fi
  if [ ! -s "$d/updates.jsonl" ]; then
    if [ ! -f "$d/summary.json" ] || ! grep -q '"generated_title"[[:space:]]*:[[:space:]]*"[^"]' "$d/summary.json" 2>/dev/null; then
      continue
    fi
  fi
  mt=$(stat -c %Y "$d" 2>/dev/null || date -r "$d" +%s 2>/dev/null || echo 0)
  printf "%s\t%s\n" "$mt" "$d"
done | sort -nr > "$tmp"
total=$(wc -l < "$tmp" | tr -d " ")
echo "TOTAL	$total"
i=0
while IFS= read -r line; do
  i=$((i + 1))
  if [ "$i" -le "$OFFSET" ]; then continue; fi
  if [ "$i" -gt $((OFFSET + LIMIT)) ]; then break; fi
  mt=${{line%%	*}}
  d=${{line#*	}}
  id=$(basename "$d")
  enc=$(basename "$(dirname "$d")")
  title=""
  if [ -f "$d/summary.json" ]; then
    title=$(sed -n "s/.*\"generated_title\"[[:space:]]*:[[:space:]]*\"\\([^\"]*\\)\".*/\\1/p" "$d/summary.json" | head -n 1)
  fi
  printf "%s\t%s\t%s\t%s\n" "$id" "$enc" "$mt" "$title"
done < "$tmp"
rm -f "$tmp"
exit 0
"#
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshDirEntry {
    pub name: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshListDirResult {
    pub ok: bool,
    pub alias: String,
    pub path: String,
    pub entries: Vec<SshDirEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshRemoteSession {
    pub id: String,
    pub cwd: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshListSessionsResult {
    pub ok: bool,
    pub alias: String,
    pub sessions: Vec<SshRemoteSession>,
    #[serde(default)]
    pub total: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshWatchResult {
    pub ok: bool,
    pub alias: String,
    pub watching: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

fn parse_ls_stdout(stdout: &str) -> Option<(String, Vec<SshDirEntry>)> {
    let mut lines = stdout.lines().map(|l| l.trim_end_matches('\r'));
    while let Some(line) = lines.next() {
        if line.trim() == "GROK_APP_LS_ERR" {
            return None;
        }
        if line.trim() == "GROK_APP_LS" {
            let path = lines.next().unwrap_or("").trim().to_string();
            let mut entries = Vec::new();
            for rest in lines {
                let name = rest.trim();
                if name.is_empty() || name == "." || name == ".." {
                    continue;
                }
                let is_dir = name.ends_with('/');
                let name = name.trim_end_matches('/').to_string();
                if name.is_empty() {
                    continue;
                }
                entries.push(SshDirEntry { name, is_dir });
            }
            return Some((path, entries));
        }
    }
    None
}

pub fn parse_sess_stdout(stdout: &str) -> Option<(u32, Vec<SshRemoteSession>)> {
    let mut lines = stdout.lines().map(|l| l.trim_end_matches('\r'));
    while let Some(line) = lines.next() {
        if line.trim() != "GROK_APP_SESS" {
            continue;
        }
        let mut sessions = Vec::new();
        let mut total: Option<u32> = None;
        for rest in lines {
            let rest = rest.trim();
            if rest.is_empty() {
                continue;
            }
            if rest.starts_with("TOTAL") {
                let n = rest
                    .split(|c: char| c == '\t' || c == ' ')
                    .nth(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                total = Some(n);
                continue;
            }
            let mut parts = rest.splitn(4, '\t');
            let id = parts.next().unwrap_or("").trim();
            let enc = parts.next().unwrap_or("").trim();
            let mtime = parts.next().unwrap_or("").trim();
            let title = parts.next().unwrap_or("").trim();
            if id.is_empty() {
                continue;
            }
            sessions.push(SshRemoteSession {
                id: id.to_string(),
                cwd: percent_decode_path(enc),
                title: title.to_string(),
                updated_at: unix_mtime_to_rfc3339(mtime),
            });
        }
        let total = total.unwrap_or(sessions.len() as u32);
        return Some((total, sessions));
    }
    None
}

fn persist_watch_alias(alias: &str, on: bool) -> Result<Vec<String>, String> {
    let mut s = crate::store::load_settings();
    let mut set: Vec<String> = s
        .ssh_watch_aliases
        .into_iter()
        .filter(|a| is_safe_ssh_alias(a))
        .collect();
    if on {
        if !set.iter().any(|a| a == alias) {
            set.push(alias.to_string());
        }
    } else {
        set.retain(|a| a != alias);
    }
    s.ssh_watch_aliases = set.clone();
    crate::store::save_settings(&s)?;
    Ok(set)
}

#[tauri::command]
pub async fn ssh_watch_start(alias: String) -> Result<SshWatchResult, String> {
    let alias = alias.trim().to_string();
    if !is_safe_ssh_alias(&alias) {
        return Ok(SshWatchResult {
            ok: false,
            alias,
            watching: false,
            error: Some("Host alias is not a concrete OpenSSH Host name".into()),
            error_code: Some("invalid_alias".into()),
        });
    }
    let Some(ssh) = find_ssh_binary() else {
        return Ok(SshWatchResult {
            ok: false,
            alias,
            watching: false,
            error: Some("OpenSSH client (ssh) was not found on this machine".into()),
            error_code: Some("ssh_missing".into()),
        });
    };
    let dir = control_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Ok(SshWatchResult {
            ok: false,
            alias,
            watching: false,
            error: Some(format!("Could not create SSH control dir: {e}")),
            error_code: Some("other".into()),
        });
    }
    let path = control_path(&alias);
    let mut cmd = Command::new(&ssh);
    process_util::apply_no_window_tokio(&mut cmd);
    apply_base_ssh_opts(&mut cmd);
    apply_control_opts(&mut cmd, &alias, "yes");
    cmd.arg("-fN")
        .arg(&alias)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = timeout(Duration::from_secs(SSH_OVERALL_TIMEOUT_SECS), cmd.output()).await;
    match out {
        Err(_) => {
            return Ok(SshWatchResult {
                ok: false,
                alias,
                watching: false,
                error: Some("Connection timed out".into()),
                error_code: Some("timeout".into()),
            });
        }
        Ok(Err(e)) => {
            return Ok(SshWatchResult {
                ok: false,
                alias,
                watching: false,
                error: Some(truncate_err(&e.to_string())),
                error_code: Some("other".into()),
            });
        }
        Ok(Ok(o)) if !o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            let mut check = Command::new(&ssh);
            process_util::apply_no_window_tokio(&mut check);
            check.arg("-O").arg("check");
            push_ssh_opt(&mut check, "ControlPath", path.to_string_lossy().as_ref());
            check
                .arg(&alias)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let already = timeout(Duration::from_secs(3), check.output())
                .await
                .ok()
                .and_then(|r| r.ok())
                .map(|c| c.status.success())
                .unwrap_or(false);
            if !already {
                let (code, msg) = classify_ssh_stderr(&stderr);
                return Ok(SshWatchResult {
                    ok: false,
                    alias,
                    watching: false,
                    error: Some(msg),
                    error_code: Some(code.into()),
                });
            }
        }
        Ok(Ok(_)) => {}
    }
    persist_watch_alias(&alias, true)?;
    Ok(SshWatchResult {
        ok: true,
        alias,
        watching: true,
        error: None,
        error_code: None,
    })
}

#[tauri::command]
pub async fn ssh_watch_stop(alias: String) -> Result<SshWatchResult, String> {
    let alias = alias.trim().to_string();
    if !is_safe_ssh_alias(&alias) {
        return Ok(SshWatchResult {
            ok: false,
            alias,
            watching: false,
            error: Some("Host alias is not a concrete OpenSSH Host name".into()),
            error_code: Some("invalid_alias".into()),
        });
    }
    if let Some(ssh) = find_ssh_binary() {
        let path = control_path(&alias);
        let mut cmd = Command::new(&ssh);
        process_util::apply_no_window_tokio(&mut cmd);
        cmd.arg("-O").arg("exit");
        push_ssh_opt(&mut cmd, "ControlPath", path.to_string_lossy().as_ref());
        cmd.arg(&alias)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let _ = timeout(Duration::from_secs(5), cmd.output()).await;
    }
    persist_watch_alias(&alias, false)?;
    Ok(SshWatchResult {
        ok: true,
        alias,
        watching: false,
        error: None,
        error_code: None,
    })
}

#[tauri::command]
pub async fn ssh_list_dir(alias: String, path: Option<String>) -> Result<SshListDirResult, String> {
    let alias = alias.trim().to_string();
    if !is_safe_ssh_alias(&alias) {
        return Ok(SshListDirResult {
            ok: false,
            alias,
            path: path.unwrap_or_default(),
            entries: Vec::new(),
            error: Some("invalid alias".into()),
        });
    }
    let dir = path.unwrap_or_default();
    if dir.contains('\0') {
        return Ok(SshListDirResult {
            ok: false,
            alias,
            path: dir,
            entries: Vec::new(),
            error: Some("invalid path".into()),
        });
    }
    match run_ssh(
        &alias,
        &remote_ls_script(&dir),
        true,
        SSH_OVERALL_TIMEOUT_SECS,
    )
    .await
    {
        Err(SshRunErr::Missing) => Ok(SshListDirResult {
            ok: false,
            alias,
            path: dir,
            entries: Vec::new(),
            error: Some("ssh missing".into()),
        }),
        Err(SshRunErr::Timeout { .. }) => Ok(SshListDirResult {
            ok: false,
            alias,
            path: dir,
            entries: Vec::new(),
            error: Some("timeout".into()),
        }),
        Err(SshRunErr::Spawn(e)) => Ok(SshListDirResult {
            ok: false,
            alias,
            path: dir,
            entries: Vec::new(),
            error: Some(truncate_err(&e)),
        }),
        Ok(run) if !run.success => {
            let (code, msg) = classify_ssh_stderr(&run.stderr);
            Ok(SshListDirResult {
                ok: false,
                alias,
                path: dir,
                entries: Vec::new(),
                error: Some(format!("{code}: {msg}")),
            })
        }
        Ok(run) => match parse_ls_stdout(&run.stdout) {
            Some((path, entries)) => Ok(SshListDirResult {
                ok: true,
                alias,
                path,
                entries,
                error: None,
            }),
            None => Ok(SshListDirResult {
                ok: false,
                alias,
                path: dir,
                entries: Vec::new(),
                error: Some("remote ls failed".into()),
            }),
        },
    }
}

const MAX_SSH_TEXT_BYTES: u64 = 2 * 1024 * 1024;

const REMOTE_READ_PY: &str = r#"python3 -c '
import json, os, sys, time
path = os.environ.get("GROK_APP_FILE", "")
rel = os.environ.get("GROK_APP_REL", "")
name = os.path.basename(path) or "file"
def emit(obj):
    sys.stdout.write("GROK_APP_READ\n")
    sys.stdout.write(json.dumps(obj, ensure_ascii=False))
    sys.stdout.write("\n")
    sys.stdout.flush()
if not path or not os.path.isfile(path):
    emit({"ok": False, "error": "not a file", "name": name, "relativePath": rel, "absolutePath": path, "size": 0, "kind": "text", "mime": "text/plain", "truncated": False, "mtimeMs": 0, "text": None})
    sys.exit(0)
st = os.stat(path)
size = st.st_size
mtime_ms = int(st.st_mtime * 1000)
limit = 2097152
raw = open(path, "rb").read(limit + 1)
truncated = len(raw) > limit
raw = raw[:limit]
text = None
err = None
try:
    text = raw.decode("utf-8")
except Exception:
    err = "not utf-8 text"
    text = None
emit({"ok": err is None, "error": err, "name": name, "relativePath": rel, "absolutePath": path, "size": size, "kind": "text", "mime": "text/plain", "truncated": truncated, "mtimeMs": mtime_ms, "text": text})
'
"#;

const REMOTE_WRITE_PY: &str = r#"python3 -c '
import json, os, sys, time
path = os.environ.get("GROK_APP_FILE", "")
rel = os.environ.get("GROK_APP_REL", "")
exp = os.environ.get("GROK_APP_EXPECT_MTIME", "").strip()
def emit(obj):
    sys.stdout.write("GROK_APP_WRITE\n")
    sys.stdout.write(json.dumps(obj, ensure_ascii=False))
    sys.stdout.write("\n")
    sys.stdout.flush()
if not path or not os.path.isfile(path):
    emit({"ok": False, "error": "not a file: " + path})
    sys.exit(0)
data = sys.stdin.buffer.read()
if len(data) > 2097152:
    emit({"ok": False, "error": "file too large to save in-app (max 2097152 bytes)"})
    sys.exit(0)
st = os.stat(path)
actual = int(st.st_mtime * 1000)
if exp:
    try:
        expected = int(exp)
    except Exception:
        expected = 0
    if expected > 0 and actual > 0 and actual != expected:
        emit({"ok": False, "error": "CONFLICT: file changed on disk (mtime %d, expected %d)" % (actual, expected)})
        sys.exit(0)
parent = os.path.dirname(path)
tmp = os.path.join(parent, ".%s.grok-save-%d" % (os.path.basename(path), os.getpid()))
open(tmp, "wb").write(data)
os.replace(tmp, path)
st = os.stat(path)
emit({"ok": True, "relativePath": rel, "absolutePath": path, "size": st.st_size, "mtimeMs": int(st.st_mtime * 1000)})
'
"#;

fn remote_read_script(abs: &str, rel: &str) -> String {
    format!(
        "export GROK_APP_FILE={}\nexport GROK_APP_REL={}\n{REMOTE_READ_PY}",
        posix_single_quote(abs),
        posix_single_quote(rel),
    )
}

fn remote_write_script(abs: &str, rel: &str, expected_mtime_ms: Option<u64>) -> String {
    let exp = expected_mtime_ms
        .filter(|n| *n > 0)
        .map(|n| n.to_string())
        .unwrap_or_default();
    format!(
        "export GROK_APP_FILE={}\nexport GROK_APP_REL={}\nexport GROK_APP_EXPECT_MTIME={}\n{REMOTE_WRITE_PY}",
        posix_single_quote(abs),
        posix_single_quote(rel),
        posix_single_quote(&exp),
    )
}

fn parse_marked_json(stdout: &str, marker: &str) -> Option<serde_json::Value> {
    let idx = stdout.find(marker)?;
    let rest = stdout[idx + marker.len()..].trim_start();
    let line = rest.lines().next()?.trim();
    serde_json::from_str(line).ok()
}

fn ssh_io_err(run: SshRunErr) -> String {
    match run {
        SshRunErr::Missing => "ssh missing".into(),
        SshRunErr::Timeout { .. } => "timeout".into(),
        SshRunErr::Spawn(e) => truncate_err(&e),
    }
}

#[tauri::command]
pub async fn ssh_read_file(
    alias: String,
    project_path: String,
    relative: String,
) -> Result<crate::fs_browser::FsReadResult, String> {
    let alias = alias.trim().to_string();
    if !is_safe_ssh_alias(&alias) {
        return Err("invalid alias".into());
    }
    let rel = relative.trim().to_string();
    let abs = join_remote_rel(&project_path, &rel)?;
    let name = abs.rsplit('/').next().unwrap_or("file").to_string();
    let kind = remote_file_kind(&name);
    let mime = remote_file_mime(kind).to_string();
    let script = remote_read_script(&abs, &rel);
    match run_ssh(&alias, &script, true, SSH_OVERALL_TIMEOUT_SECS).await {
        Err(e) => Err(ssh_io_err(e)),
        Ok(run) if !run.success => {
            let (_code, msg) = classify_ssh_stderr(&run.stderr);
            Err(msg)
        }
        Ok(run) => {
            let v = parse_marked_json(&run.stdout, "GROK_APP_READ")
                .ok_or_else(|| "remote read failed".to_string())?;
            let err = v
                .get("error")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            if v.get("ok").and_then(|x| x.as_bool()) == Some(false) {
                if let Some(e) = err.clone() {
                    if e.starts_with("not a file") {
                        return Err(e);
                    }
                }
            }
            Ok(crate::fs_browser::FsReadResult {
                relative_path: rel,
                name,
                absolute_path: abs,
                size: v.get("size").and_then(|x| x.as_u64()).unwrap_or(0),
                kind: kind.to_string(),
                mime,
                text: v
                    .get("text")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string()),
                base64: None,
                stream: false,
                truncated: v
                    .get("truncated")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false),
                error: err,
                mtime_ms: v.get("mtimeMs").and_then(|x| x.as_u64()).unwrap_or(0),
            })
        }
    }
}

#[tauri::command]
pub async fn ssh_write_file(
    alias: String,
    project_path: String,
    relative: String,
    content: String,
    expected_mtime_ms: Option<u64>,
) -> Result<crate::fs_browser::FsWriteResult, String> {
    let alias = alias.trim().to_string();
    if !is_safe_ssh_alias(&alias) {
        return Err("invalid alias".into());
    }
    let rel = relative.trim().to_string();
    let abs = join_remote_rel(&project_path, &rel)?;
    if content.len() as u64 > MAX_SSH_TEXT_BYTES {
        return Err(format!(
            "file too large to save in-app (max {MAX_SSH_TEXT_BYTES} bytes)"
        ));
    }
    let script = remote_write_script(&abs, &rel, expected_mtime_ms);
    match run_ssh_io(
        &alias,
        &script,
        true,
        SSH_OVERALL_TIMEOUT_SECS,
        Some(content.as_bytes()),
    )
    .await
    {
        Err(e) => Err(ssh_io_err(e)),
        Ok(run) if !run.success => {
            let (_code, msg) = classify_ssh_stderr(&run.stderr);
            Err(msg)
        }
        Ok(run) => {
            let v = parse_marked_json(&run.stdout, "GROK_APP_WRITE")
                .ok_or_else(|| "remote write failed".to_string())?;
            if v.get("ok").and_then(|x| x.as_bool()) == Some(false) {
                let err = v
                    .get("error")
                    .and_then(|x| x.as_str())
                    .unwrap_or("remote write failed");
                return Err(err.to_string());
            }
            Ok(crate::fs_browser::FsWriteResult {
                relative_path: rel,
                absolute_path: abs,
                size: v.get("size").and_then(|x| x.as_u64()).unwrap_or(0),
                mtime_ms: v.get("mtimeMs").and_then(|x| x.as_u64()).unwrap_or(0),
            })
        }
    }
}

fn sess_fail(alias: String, error: impl Into<String>) -> SshListSessionsResult {
    SshListSessionsResult {
        ok: false,
        alias,
        sessions: Vec::new(),
        total: 0,
        error: Some(error.into()),
    }
}

#[tauri::command]
pub async fn ssh_list_sessions(
    alias: String,
    offset: Option<u32>,
    limit: Option<u32>,
) -> Result<SshListSessionsResult, String> {
    let alias = alias.trim().to_string();
    if !is_safe_ssh_alias(&alias) {
        return Ok(sess_fail(alias, "invalid alias"));
    }
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(20);
    let remote = remote_sess_script(offset, limit);
    match run_ssh(&alias, &remote, true, SSH_OVERALL_TIMEOUT_SECS).await {
        Err(SshRunErr::Missing) => Ok(sess_fail(alias, "ssh missing")),
        Err(SshRunErr::Timeout { .. }) => Ok(sess_fail(alias, "timeout")),
        Err(SshRunErr::Spawn(e)) => Ok(sess_fail(alias, truncate_err(&e))),
        Ok(run) if !run.success => {
            let (_code, msg) = classify_ssh_stderr(&run.stderr);
            Ok(sess_fail(alias, msg))
        }
        Ok(run) => {
            let (total, sessions) = parse_sess_stdout(&run.stdout).unwrap_or((0, Vec::new()));
            Ok(SshListSessionsResult {
                ok: true,
                alias,
                sessions,
                total,
                error: None,
            })
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshOpenSessionResult {
    pub ok: bool,
    pub alias: String,
    pub remote_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default)]
    pub message_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn remote_hist_script(session_id: &str) -> String {
    format!(
        r#"SID={session_id}
SESS="$HOME/.grok/sessions"
if command -v python3 >/dev/null 2>&1; then
  SID="$SID" python3 -c '
import os, sys
def emit(s):
    sys.stdout.write(s)
    sys.stdout.write("\n")
    sys.stdout.flush()
sid = os.environ.get("SID", "")
root = os.path.expanduser("~/.grok/sessions")
found = None
if os.path.isdir(root) and sid:
    for enc in os.listdir(root):
        d = os.path.join(root, enc, sid)
        if os.path.isdir(d):
            found = d
            break
emit("GROK_APP_HIST")
if not found:
    emit("KIND\tmissing")
    emit("GROK_APP_HIST_END")
    sys.exit(0)
kind = "empty"
path = None
for name, label in (("chat_history.jsonl", "chat_history"), ("updates.jsonl", "updates")):
    p = os.path.join(found, name)
    if os.path.isfile(p) and os.path.getsize(p) > 0:
        kind = label
        path = p
        break
emit("KIND\t" + kind)
if path:
    f = open(path, "r", encoding="utf-8", errors="replace")
    data = f.read(2097152)
    f.close()
    sys.stdout.write(data)
    if data and not data.endswith("\n"):
        sys.stdout.write("\n")
    sys.stdout.flush()
emit("GROK_APP_HIST_END")
' && exit 0
fi
echo GROK_APP_HIST
d=$(find "$SESS" -mindepth 2 -maxdepth 2 -type d -name "$SID" 2>/dev/null | head -n 1)
if [ -z "$d" ]; then
  echo "KIND	missing"
  echo GROK_APP_HIST_END
  exit 0
fi
if [ -s "$d/chat_history.jsonl" ]; then
  echo "KIND	chat_history"
  head -c 2097152 "$d/chat_history.jsonl"
  echo
elif [ -s "$d/updates.jsonl" ]; then
  echo "KIND	updates"
  head -c 2097152 "$d/updates.jsonl"
  echo
else
  echo "KIND	empty"
fi
echo GROK_APP_HIST_END
exit 0
"#
    )
}

struct RemoteHist {
    kind: String,
    body: String,
}

fn parse_hist_stdout(stdout: &str) -> Option<RemoteHist> {
    let marker = "GROK_APP_HIST";
    let idx = stdout.find(marker)?;
    let prefix = &stdout[..idx];
    let rest = &stdout[idx + marker.len()..];
    let mut kind = "empty".to_string();
    let mut body = String::new();
    let mut after_kind = false;
    for line in rest.lines().map(|l| l.trim_end_matches('\r')) {
        let trimmed = line.trim();
        if !after_kind {
            if let Some(k) = trimmed.strip_prefix("KIND") {
                kind = k.trim().trim_start_matches('\t').trim().to_string();
                after_kind = true;
                continue;
            }
            if trimmed.is_empty() || trimmed == marker {
                continue;
            }
            after_kind = true;
        }
        if trimmed == "GROK_APP_HIST_END" {
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    // Python used to mix print() and buffer.write(), so the jsonl landed
    // *before* GROK_APP_HIST and the wrapped body was empty.
    if body.trim().is_empty() && !prefix.trim().is_empty() {
        body = prefix.to_string();
    }
    Some(RemoteHist { kind, body })
}

fn import_index_path() -> std::path::PathBuf {
    crate::paths::app_data_root().join("ssh-imported-sessions.json")
}

fn load_import_index() -> HashMap<String, String> {
    std::fs::read_to_string(import_index_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_import_index(map: &HashMap<String, String>) -> Result<(), String> {
    let dir = crate::paths::app_data_root();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let raw = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    std::fs::write(import_index_path(), raw).map_err(|e| e.to_string())
}

fn bind_imported_agent_session(
    mut meta: crate::store::SessionMeta,
    remote_id: &str,
) -> Result<crate::store::SessionMeta, String> {
    let rid = remote_id.trim();
    if crate::cli_sessions::validate_agent_session_id(rid).is_err() {
        return Ok(meta);
    }
    if meta.agent_session_id.as_deref() != Some(rid) {
        meta.agent_session_id = Some(rid.to_string());
        crate::store::update_session_meta(&meta)?;
    }
    Ok(meta)
}

fn persist_imported_journal(app_id: &str, pairs: Vec<(String, String)>) -> Result<(), String> {
    let now = chrono::Utc::now();
    let msgs: Vec<crate::store::ChatMessageStored> = pairs
        .into_iter()
        .enumerate()
        .map(|(i, (role, content))| crate::store::ChatMessageStored {
            id: uuid::Uuid::new_v4().to_string(),
            role,
            content,
            thought: None,
            created_at: now + chrono::Duration::milliseconds(i as i64),
            is_error: false,
            attachments: None,
            marker: None,
        })
        .collect();
    crate::store::save_messages(app_id, &msgs)
}

fn pairs_from_hist(hist: &RemoteHist) -> Vec<(String, String)> {
    if hist.kind == "missing" || hist.kind == "empty" || hist.body.trim().is_empty() {
        return Vec::new();
    }
    if hist.kind == "updates" {
        let pairs = crate::cli_sessions::parse_acp_updates_text(&hist.body);
        if !pairs.is_empty() {
            return pairs;
        }
        return crate::cli_sessions::parse_chat_history_text(&hist.body).unwrap_or_default();
    }
    crate::cli_sessions::parse_chat_history_text(&hist.body)
        .unwrap_or_else(|_| crate::cli_sessions::parse_acp_updates_text(&hist.body))
}

fn open_fail(
    alias: String,
    remote_session_id: String,
    error: impl Into<String>,
) -> SshOpenSessionResult {
    SshOpenSessionResult {
        ok: false,
        alias,
        remote_session_id,
        app_session_id: None,
        title: None,
        project_id: None,
        message_count: 0,
        error: Some(error.into()),
    }
}

#[tauri::command]
pub async fn ssh_open_session(
    alias: String,
    session_id: String,
    cwd: Option<String>,
    title_hint: Option<String>,
) -> Result<SshOpenSessionResult, String> {
    let alias = alias.trim().to_string();
    let session_id = session_id.trim().to_string();
    if !is_safe_ssh_alias(&alias) {
        return Ok(open_fail(alias, session_id, "invalid alias"));
    }
    if crate::cli_sessions::validate_agent_session_id(&session_id).is_err() {
        return Ok(open_fail(alias, session_id, "invalid session id"));
    }
    let script = remote_hist_script(&session_id);
    let hist = match run_ssh(&alias, &script, true, 30).await {
        Err(SshRunErr::Missing) => {
            return Ok(open_fail(alias, session_id, "ssh missing"));
        }
        Err(SshRunErr::Timeout { .. }) => {
            return Ok(open_fail(alias, session_id, "timeout"));
        }
        Err(SshRunErr::Spawn(e)) => {
            return Ok(open_fail(alias, session_id, truncate_err(&e)));
        }
        Ok(run) if !run.success => {
            let (_code, msg) = classify_ssh_stderr(&run.stderr);
            return Ok(open_fail(alias, session_id, msg));
        }
        Ok(run) => parse_hist_stdout(&run.stdout),
    };
    let Some(hist) = hist else {
        return Ok(open_fail(
            alias,
            session_id,
            "could not read remote session",
        ));
    };
    if hist.kind == "missing" {
        return Ok(open_fail(alias, session_id, "remote session not found"));
    }
    let pairs = pairs_from_hist(&hist);
    if pairs.is_empty() {
        return Ok(open_fail(
            alias,
            session_id,
            "this session has no chat content",
        ));
    }
    let title = title_hint
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !looks_like_agent_uuid(s))
        .or_else(|| {
            pairs
                .iter()
                .find(|(r, _)| r == "user")
                .map(|(_, c)| crate::session_title::heuristic_title(c))
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Remote chat".into());

    let project_id = cwd
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|path| crate::store::add_ssh_project(&alias, path.to_string(), true).ok())
        .map(|p| p.id);

    let key = format!("{alias}:{session_id}");
    let mut index = load_import_index();
    let existing = index.get(&key).cloned().filter(|id| {
        crate::store::load_sessions_index()
            .iter()
            .any(|s| s.id == *id)
    });

    let meta = if let Some(app_id) = existing {
        persist_imported_journal(&app_id, pairs.clone())?;
        let _ = crate::store::rename_session(&app_id, &title);
        let mut meta = crate::store::load_sessions_index()
            .into_iter()
            .find(|s| s.id == app_id)
            .ok_or_else(|| "imported session missing after write".to_string())?;
        if let Some(pid) = project_id.clone() {
            if meta.project_id.as_deref() != Some(pid.as_str()) {
                meta.project_id = Some(pid);
                crate::store::update_session_meta(&meta)?;
            }
        }
        meta
    } else {
        let meta = crate::store::create_session(project_id.clone(), Some(title.clone()), false)?;
        persist_imported_journal(&meta.id, pairs)?;
        index.insert(key, meta.id.clone());
        save_import_index(&index)?;
        meta
    };
    let meta = bind_imported_agent_session(meta, &session_id)?;

    let message_count = crate::store::load_messages(&meta.id).len() as u32;
    Ok(SshOpenSessionResult {
        ok: true,
        alias,
        remote_session_id: session_id,
        app_session_id: Some(meta.id),
        title: Some(meta.title),
        project_id: meta.project_id,
        message_count,
        error: None,
    })
}

// ── Remote skills (`grok inspect --json` + project `.grok/skills`) ─────────

#[derive(Debug, Clone)]
pub struct SshSkillRow {
    pub name: String,
    pub description: String,
    pub source: String,
    pub path: Option<String>,
    pub user_invocable: bool,
}

#[derive(Debug, Clone)]
pub struct SshSkillsFetch {
    pub inspect: Option<serde_json::Value>,
    pub error: Option<String>,
    pub project_skills: Vec<SshSkillRow>,
}

fn remote_inspect_script(project_path: Option<&str>) -> String {
    let dir = project_path
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.contains('\0'))
        .unwrap_or("");
    let q = posix_single_quote(dir);
    format!(
        r#"export PATH="$HOME/.grok/bin:$PATH"
DIR={q}
if [ -n "$DIR" ]; then
  case "$DIR" in
    ~*) DIR="$HOME${{DIR#~}}" ;;
  esac
  if [ -d "$DIR" ]; then
    cd "$DIR" || true
  fi
fi
BIN=""
if command -v grok >/dev/null 2>&1; then
  BIN=$(command -v grok)
elif [ -x "$HOME/.grok/bin/grok" ]; then
  BIN="$HOME/.grok/bin/grok"
fi
echo GROK_APP_INSPECT
if [ -z "$BIN" ]; then
  echo MISSING
  exit 0
fi
echo OK
"$BIN" inspect --json
exit 0
"#
    )
}

const REMOTE_SKILLS_PY: &str = r##"python3 -c '
import json, os, sys
root = os.environ.get("GROK_APP_SKILLS_ROOT", "")
out = []
def meta(text):
    name = None
    desc = ""
    inv = True
    t = text.lstrip("\ufeff")
    if not t.startswith("---"):
        return name, desc, inv
    rest = t[3:]
    if rest.startswith("\n"):
        rest = rest[1:]
    end = rest.find("\n---")
    block = rest if end < 0 else rest[:end]
    q = chr(39)
    for line in block.splitlines():
        s = line.strip()
        if not s or s.startswith("#") or ":" not in s:
            continue
        k, v = s.split(":", 1)
        k = k.strip().lower()
        v = v.strip()
        if len(v) >= 2 and ((v[0] == chr(34) and v[-1] == chr(34)) or (v[0] == q and v[-1] == q)):
            v = v[1:-1]
        if k == "name" and v.strip():
            name = v.strip()
        elif k == "description":
            desc = v
        elif k in ("user-invocable", "user_invocable", "userinvocable"):
            inv = v.strip().lower() not in ("false", "no", "0", "off")
    return name, desc, inv
if root and os.path.isdir(root):
    names = sorted(os.listdir(root))[:500]
    for name in names:
        if not name or name.startswith("."):
            continue
        d = os.path.join(root, name)
        if not os.path.isdir(d):
            continue
        md = os.path.join(d, "SKILL.md")
        if not os.path.isfile(md):
            md = os.path.join(d, "skill.md")
        if not os.path.isfile(md):
            continue
        try:
            raw = open(md, "rb").read(65536)
        except Exception:
            raw = b""
        text = raw.decode("utf-8", "replace")
        n, desc, inv = meta(text)
        if not n:
            n = name
        out.append({"name": n, "description": desc, "source": "project", "path": md, "userInvocable": inv})
sys.stdout.write("GROK_APP_SKILLS\n")
sys.stdout.write(json.dumps(out, ensure_ascii=False))
sys.stdout.write("\n")
sys.stdout.flush()
'
"##;

fn remote_project_skills_script(project_path: &str) -> String {
    let root = format!("{}/.grok/skills", project_path.trim().trim_end_matches('/'));
    format!(
        "export GROK_APP_SKILLS_ROOT={}\n{REMOTE_SKILLS_PY}",
        posix_single_quote(&root),
    )
}

fn parse_first_json_value(s: &str) -> Option<serde_json::Value> {
    let start = s.find('{')?;
    let slice = &s[start..];
    let mut de = serde_json::Deserializer::from_str(slice);
    serde::Deserialize::deserialize(&mut de).ok()
}

fn parse_inspect_stdout(stdout: &str) -> (Option<serde_json::Value>, Option<String>) {
    let Some(idx) = stdout.find("GROK_APP_INSPECT") else {
        return (None, Some("remote inspect failed".into()));
    };
    let rest = stdout[idx + "GROK_APP_INSPECT".len()..].trim_start();
    let status = rest.lines().next().unwrap_or("").trim();
    if status.eq_ignore_ascii_case("MISSING") {
        return (
            None,
            Some("Grok Build CLI not found on the remote host".into()),
        );
    }
    let after = rest.get(status.len()..).unwrap_or("").trim_start();
    match parse_first_json_value(after) {
        Some(v) => (Some(v), None),
        None => (None, Some("Failed to parse grok inspect JSON".into())),
    }
}

fn parse_remote_skills_stdout(stdout: &str) -> Vec<SshSkillRow> {
    let Some(v) = parse_marked_json(stdout, "GROK_APP_SKILLS") else {
        return Vec::new();
    };
    let Some(arr) = v.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in arr {
        let name = item
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if name.is_empty() {
            continue;
        }
        let description = item
            .get("description")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let source = item
            .get("source")
            .and_then(|x| x.as_str())
            .unwrap_or("project")
            .to_string();
        let path = item
            .get("path")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        let user_invocable = item
            .get("userInvocable")
            .or_else(|| item.get("user_invocable"))
            .and_then(|x| x.as_bool())
            .unwrap_or(true);
        out.push(SshSkillRow {
            name,
            description,
            source,
            path,
            user_invocable,
        });
    }
    out
}

/// Remote `grok inspect --json` plus `{project}/.grok/skills` scan.
pub async fn ssh_fetch_skills(alias: &str, project_path: Option<&str>) -> SshSkillsFetch {
    if !is_safe_ssh_alias(alias) {
        return SshSkillsFetch {
            inspect: None,
            error: Some("invalid alias".into()),
            project_skills: Vec::new(),
        };
    }
    let inspect_script = remote_inspect_script(project_path);
    let scan_script = project_path
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.starts_with('/') && !s.contains('\0'))
        .map(remote_project_skills_script);

    let inspect_fut = run_ssh(alias, &inspect_script, true, SSH_INSPECT_TIMEOUT_SECS);
    let scan_fut = async {
        match scan_script.as_deref() {
            Some(s) => run_ssh(alias, s, true, SSH_OVERALL_TIMEOUT_SECS).await,
            None => Ok(SshRun {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
                latency_ms: 0,
            }),
        }
    };
    let (inspect_run, scan_run) = tokio::join!(inspect_fut, scan_fut);

    let (inspect, error) = match inspect_run {
        Err(e) => (None, Some(ssh_io_err(e))),
        Ok(run) if !run.success => {
            let (_c, msg) = classify_ssh_stderr(&run.stderr);
            (None, Some(msg))
        }
        Ok(run) => parse_inspect_stdout(&run.stdout),
    };

    let project_skills = match scan_run {
        Ok(run) => parse_remote_skills_stdout(&run.stdout),
        Err(_) => Vec::new(),
    };

    SshSkillsFetch {
        inspect,
        error,
        project_skills,
    }
}

// ── Embedded browser: loopback URLs via SSH -L ─────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshLoopbackTarget {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub rest: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshBrowserPrepareResult {
    pub ok: bool,
    pub alias: String,
    pub url: String,
    pub display_url: String,
    pub tunneled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn is_loopback_http_host(host: &str) -> bool {
    let h = host
        .trim()
        .trim_matches(|c| c == '[' || c == ']')
        .to_ascii_lowercase();
    h == "localhost"
        || h == "127.0.0.1"
        || h == "::1"
        || h == "0.0.0.0"
        || h.ends_with(".localhost")
}

pub fn parse_loopback_http_url(raw: &str) -> Option<SshLoopbackTarget> {
    let u = url::Url::parse(raw.trim()).ok()?;
    if u.scheme() != "http" && u.scheme() != "https" {
        return None;
    }
    let host = u.host_str()?.to_ascii_lowercase();
    if !is_loopback_http_host(&host) {
        return None;
    }
    let port = u.port_or_known_default()?;
    let mut rest = String::new();
    rest.push_str(u.path());
    if let Some(q) = u.query() {
        rest.push('?');
        rest.push_str(q);
    }
    if let Some(f) = u.fragment() {
        rest.push('#');
        rest.push_str(f);
    }
    Some(SshLoopbackTarget {
        scheme: u.scheme().to_string(),
        host,
        port,
        rest,
    })
}

pub fn rewrite_loopback_url(target: &SshLoopbackTarget, local_port: u16) -> String {
    let rest = if target.rest.is_empty() {
        "/"
    } else {
        target.rest.as_str()
    };
    format!("{}://127.0.0.1:{local_port}{rest}", target.scheme)
}

fn forward_remote_host(host: &str) -> &'static str {
    let h = host
        .trim()
        .trim_matches(|c| c == '[' || c == ']')
        .to_ascii_lowercase();
    if h == "::1" {
        "::1"
    } else {
        "127.0.0.1"
    }
}

fn local_forward_spec(local_port: u16, remote_host: &str, remote_port: u16) -> String {
    if remote_host.contains(':') {
        format!("127.0.0.1:{local_port}:[{remote_host}]:{remote_port}")
    } else {
        format!("127.0.0.1:{local_port}:{remote_host}:{remote_port}")
    }
}

fn tunnel_ports() -> &'static Mutex<HashMap<(String, String, u16), u16>> {
    static M: OnceLock<Mutex<HashMap<(String, String, u16), u16>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pick_free_local_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("bind local port: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("local addr: {e}"))?
        .port();
    drop(listener);
    if port == 0 {
        return Err("could not allocate a local port".into());
    }
    Ok(port)
}

async fn mux_ctl(alias: &str, op: &str, extra: &[String]) -> Result<SshRun, SshRunErr> {
    let ssh = find_ssh_binary().ok_or(SshRunErr::Missing)?;
    let mut cmd = Command::new(&ssh);
    process_util::apply_no_window_tokio(&mut cmd);
    apply_base_ssh_opts(&mut cmd);
    push_ssh_opt(
        &mut cmd,
        "ControlPath",
        control_path(alias).to_string_lossy().as_ref(),
    );
    cmd.arg("-O").arg(op);
    for e in extra {
        cmd.arg(e);
    }
    cmd.arg(alias)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let started = Instant::now();
    match timeout(Duration::from_secs(SSH_CONNECT_TIMEOUT_SECS), cmd.output()).await {
        Err(_) => Err(SshRunErr::Timeout {
            latency_ms: started.elapsed().as_millis() as u64,
        }),
        Ok(Err(e)) => Err(SshRunErr::Spawn(e.to_string())),
        Ok(Ok(o)) => Ok(SshRun {
            success: o.status.success(),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
            latency_ms: started.elapsed().as_millis() as u64,
        }),
    }
}

async fn mux_running(alias: &str) -> bool {
    matches!(mux_ctl(alias, "check", &[]).await, Ok(run) if run.success)
}

async fn ensure_mux(alias: &str) -> Result<(), String> {
    let _ = ensure_control_dir();
    if mux_running(alias).await {
        return Ok(());
    }
    let ssh = find_ssh_binary()
        .ok_or_else(|| "OpenSSH client (ssh) was not found on this machine".to_string())?;
    let mut cmd = Command::new(&ssh);
    process_util::apply_no_window_tokio(&mut cmd);
    apply_base_ssh_opts(&mut cmd);
    apply_control_opts(&mut cmd, alias, "yes");
    cmd.arg("-fN")
        .arg(alias)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = timeout(Duration::from_secs(SSH_OVERALL_TIMEOUT_SECS), cmd.output()).await;
    match out {
        Err(_) => return Err("Connection timed out".into()),
        Ok(Err(e)) => return Err(e.to_string()),
        Ok(Ok(o)) => {
            if !o.status.success() && !mux_running(alias).await {
                let stderr = String::from_utf8_lossy(&o.stderr);
                let (_c, msg) = classify_ssh_stderr(&stderr);
                return Err(msg);
            }
        }
    }
    if mux_running(alias).await {
        Ok(())
    } else {
        Err("SSH multiplex master did not start".into())
    }
}

async fn spawn_dedicated_forward(alias: &str, spec: &str) -> Result<(), String> {
    let ssh = find_ssh_binary()
        .ok_or_else(|| "OpenSSH client (ssh) was not found on this machine".to_string())?;
    let mut cmd = Command::new(&ssh);
    process_util::apply_no_window_tokio(&mut cmd);
    apply_common_ssh_opts(&mut cmd, alias, true);
    push_ssh_opt(&mut cmd, "ExitOnForwardFailure", "yes");
    cmd.arg("-N")
        .arg("-L")
        .arg(spec)
        .arg(alias)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(false);
    let mut child = cmd.spawn().map_err(|e| format!("ssh -L spawn: {e}"))?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    match child.try_wait() {
        Ok(Some(st)) => {
            return Err(format!("SSH local forward exited ({st})"));
        }
        Ok(None) => {}
        Err(e) => return Err(format!("ssh -L wait: {e}")),
    }
    tauri::async_runtime::spawn(async move {
        let _ = child.wait().await;
    });
    Ok(())
}

async fn ensure_local_forward(
    alias: &str,
    remote_host: &str,
    remote_port: u16,
) -> Result<u16, String> {
    let key = (alias.to_string(), remote_host.to_string(), remote_port);
    if let Ok(g) = tunnel_ports().lock() {
        if let Some(port) = g.get(&key).copied() {
            return Ok(port);
        }
    }
    ensure_mux(alias).await?;
    let local_port = pick_free_local_port()?;
    let spec = local_forward_spec(local_port, remote_host, remote_port);
    let extra = vec!["-L".to_string(), spec.clone()];
    let forwarded = match mux_ctl(alias, "forward", &extra).await {
        Ok(run) if run.success => true,
        _ => false,
    };
    if !forwarded {
        spawn_dedicated_forward(alias, &spec).await?;
    }
    if let Ok(mut g) = tunnel_ports().lock() {
        g.insert(key, local_port);
    }
    Ok(local_port)
}

fn browser_prepare_fail(
    alias: String,
    display_url: String,
    error: String,
) -> SshBrowserPrepareResult {
    SshBrowserPrepareResult {
        ok: false,
        alias,
        url: display_url.clone(),
        display_url,
        tunneled: false,
        local_port: None,
        remote_host: None,
        remote_port: None,
        error: Some(error),
    }
}

/// Rewrite loopback URLs through SSH -L so the embedded webview hits the remote host.
#[tauri::command]
pub async fn ssh_browser_prepare(
    alias: String,
    url: String,
) -> Result<SshBrowserPrepareResult, String> {
    let alias = alias.trim().to_string();
    let display_url = url.trim().to_string();
    if !is_safe_ssh_alias(&alias) {
        return Ok(browser_prepare_fail(
            alias,
            display_url,
            "invalid alias".into(),
        ));
    }
    let Some(target) = parse_loopback_http_url(&display_url) else {
        return Ok(SshBrowserPrepareResult {
            ok: true,
            alias,
            url: display_url.clone(),
            display_url,
            tunneled: false,
            local_port: None,
            remote_host: None,
            remote_port: None,
            error: None,
        });
    };
    let remote_host = forward_remote_host(&target.host);
    match ensure_local_forward(&alias, remote_host, target.port).await {
        Ok(local_port) => Ok(SshBrowserPrepareResult {
            ok: true,
            alias,
            url: rewrite_loopback_url(&target, local_port),
            display_url,
            tunneled: true,
            local_port: Some(local_port),
            remote_host: Some(remote_host.to_string()),
            remote_port: Some(target.port),
            error: None,
        }),
        Err(e) => Ok(browser_prepare_fail(alias, display_url, e)),
    }
}

fn unix_mtime_to_rfc3339(mtime: &str) -> Option<String> {
    let n: i64 = mtime.parse().ok()?;
    if n <= 0 {
        return None;
    }
    let secs = if n > 1_000_000_000_000 { n / 1000 } else { n };
    chrono::DateTime::from_timestamp(secs, 0).map(|d| d.to_rfc3339())
}

fn looks_like_agent_uuid(s: &str) -> bool {
    let s = s.trim();
    let b = s.as_bytes();
    b.len() == 36
        && b[8] == b'-'
        && b[13] == b'-'
        && b[18] == b'-'
        && b[23] == b'-'
        && s.bytes().all(|c| c == b'-' || c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn parse(text: &str) -> Vec<SshHostDto> {
        parse_ssh_config(text, Path::new("/tmp"), &|_| None)
    }

    #[test]
    fn alias_rejects_patterns_and_flags() {
        assert!(is_safe_ssh_alias("devbox"));
        assert!(is_safe_ssh_alias("gw-01"));
        assert!(is_safe_ssh_alias("a.b_c-1"));
        assert!(!is_safe_ssh_alias(""));
        assert!(!is_safe_ssh_alias("*"));
        assert!(!is_safe_ssh_alias("*.example.com"));
        assert!(!is_safe_ssh_alias("-o"));
        assert!(!is_safe_ssh_alias("host;rm"));
        assert!(!is_safe_ssh_alias("host alias"));
        assert!(!is_safe_ssh_alias(".."));
    }

    #[test]
    fn skip_local_acp_only_for_real_ssh_aliases() {
        assert!(should_skip_local_acp_spawn(Some("uts")));
        assert!(should_skip_local_acp_spawn(Some("  gw-01  ")));
        assert!(!should_skip_local_acp_spawn(None));
        assert!(!should_skip_local_acp_spawn(Some("")));
        assert!(!should_skip_local_acp_spawn(Some("   ")));
        assert!(!should_skip_local_acp_spawn(Some("*")));
        assert!(!should_skip_local_acp_spawn(Some("host;rm")));
    }

    #[test]
    fn pick_ssh_alias_prefers_explicit_then_bound_then_path() {
        assert_eq!(
            pick_ssh_alias(Some("UTS"), Some("other"), Some("path")),
            Some("UTS".into())
        );
        assert_eq!(
            pick_ssh_alias(Some("  "), Some("gw-01"), Some("UTS")),
            Some("gw-01".into())
        );
        assert_eq!(pick_ssh_alias(None, None, Some("UTS")), Some("UTS".into()));
        assert_eq!(pick_ssh_alias(Some("*"), Some("host;rm"), None), None);
        assert_eq!(pick_ssh_alias(None, None, None), None);
    }

    #[test]
    fn local_acp_cwd_ok_rejects_ssh_and_missing_dirs() {
        assert!(!local_acp_cwd_ok(Some("UTS"), "/tmp"));
        assert!(!local_acp_cwd_ok(
            None,
            "/this/path/does/not/exist/grok-app-ssh"
        ));
        assert!(!local_acp_cwd_ok(None, ""));
        let here = std::env::temp_dir();
        assert!(local_acp_cwd_ok(None, here.to_string_lossy().as_ref()));
    }

    #[test]
    fn acp_session_cwd_ok_skips_local_isdir_for_ssh() {
        assert!(acp_session_cwd_ok(
            Some("UTS"),
            "/data/pengqlu/code/2026-07-25-ICLR",
        ));
        assert!(!acp_session_cwd_ok(
            None,
            "/data/pengqlu/code/2026-07-25-ICLR",
        ));
        assert!(!acp_session_cwd_ok(Some("UTS"), ""));
        assert!(!acp_session_cwd_ok(Some("UTS"), "/tmp\0x"));
        let here = std::env::temp_dir();
        assert!(acp_session_cwd_ok(None, here.to_string_lossy().as_ref()));
    }

    #[test]
    fn listable_matches_grok_resume_not_raw_dirs() {
        assert!(remote_session_is_listable(
            None,
            "R-Lens幻觉读出RRQ资格审查与初筛证伪",
            true
        ));
        assert!(remote_session_is_listable(Some(""), "数学题", true));
        assert!(remote_session_is_listable(None, "", true));
        assert!(!remote_session_is_listable(None, "", false));
        assert!(!remote_session_is_listable(
            Some("subagent"),
            "Freeze Qwen",
            true
        ));
        assert!(!remote_session_is_listable(
            Some("subagent_resume"),
            "overnight inspect",
            true
        ));
        assert!(!remote_session_is_listable(Some("SUBAGENT"), "", true));
    }

    #[test]
    fn remote_sess_script_skips_subagent_and_empty_shells() {
        let s = remote_sess_script(0, 20);
        assert!(s.contains("session_kind"));
        assert!(s.contains("startswith(\"subagent\")"));
        assert!(s.contains("updates.jsonl"));
        assert!(s.contains("not has_up and not title"));
    }

    #[test]
    fn skips_glob_only_hosts() {
        let hosts = parse(
            r#"
Host *
  ServerAliveInterval 60
Host *.internal
  User nobody
"#,
        );
        assert!(hosts.is_empty());
    }

    #[test]
    fn parses_concrete_host() {
        let hosts = parse(
            r#"
# comment
Host devbox
  HostName 10.0.0.8
  User deploy
  Port 2222
  IdentityFile ~/.ssh/id_ed25519
Host *
  User ignoreme
"#,
        );
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].alias, "devbox");
        assert_eq!(hosts[0].hostname.as_deref(), Some("10.0.0.8"));
        assert_eq!(hosts[0].user.as_deref(), Some("deploy"));
        assert_eq!(hosts[0].port, Some(2222));
        assert_eq!(hosts[0].identity_file.as_deref(), Some("~/.ssh/id_ed25519"));
    }

    #[test]
    fn host_line_with_equals_and_quotes() {
        let hosts = parse(
            r#"
Host "build-server"
  HostName="box.example.com"
  User = 'ci'
"#,
        );
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].alias, "build-server");
        assert_eq!(hosts[0].hostname.as_deref(), Some("box.example.com"));
        assert_eq!(hosts[0].user.as_deref(), Some("ci"));
    }

    #[test]
    fn multiple_aliases_on_one_host_line() {
        let hosts = parse("Host alpha bravo\n  User me\n");
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].alias, "alpha");
        assert_eq!(hosts[1].alias, "bravo");
        assert_eq!(hosts[0].user.as_deref(), Some("me"));
        assert_eq!(hosts[1].user.as_deref(), Some("me"));
    }

    #[test]
    fn first_hostname_wins_inside_block() {
        let hosts = parse("Host x\n  HostName one\n  HostName two\n");
        assert_eq!(hosts[0].hostname.as_deref(), Some("one"));
    }

    #[test]
    fn match_ends_host_block() {
        let hosts = parse("Host x\n  User a\nMatch host y\n  User b\nHost z\n  User c\n");
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].alias, "x");
        assert_eq!(hosts[0].user.as_deref(), Some("a"));
        assert_eq!(hosts[1].alias, "z");
        assert_eq!(hosts[1].user.as_deref(), Some("c"));
    }

    #[test]
    fn include_via_reader() {
        let mut files: HashMap<PathBuf, String> = HashMap::new();
        files.insert(
            PathBuf::from("/tmp/extra"),
            "Host extra\n  HostName extra.example\n".into(),
        );
        let hosts = parse_ssh_config(
            "Include extra\nHost main\n  User me\n",
            Path::new("/tmp"),
            &|p| files.get(p).cloned(),
        );
        let aliases: Vec<_> = hosts.iter().map(|h| h.alias.as_str()).collect();
        assert!(aliases.contains(&"extra"));
        assert!(aliases.contains(&"main"));
    }

    #[test]
    fn duplicate_alias_keeps_first() {
        let hosts = parse("Host x\n  User a\nHost x\n  User b\n");
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].user.as_deref(), Some("a"));
    }

    #[test]
    fn comment_not_inside_quotes() {
        let hosts = parse("Host x\n  User \"a#b\"\n");
        assert_eq!(hosts[0].user.as_deref(), Some("a#b"));
    }

    #[test]
    fn probe_stdout_ok() {
        let raw =
            "noise\nGROK_APP_PROBE\nCLI_OK\nAUTH_OK\n/home/u/.grok/bin/grok\ngrok 1.0.5 (abc)\n";
        let p = parse_probe_stdout(raw).unwrap();
        assert_eq!(p.cli, "ok");
        assert_eq!(p.auth, "ok");
        assert_eq!(p.path.as_deref(), Some("/home/u/.grok/bin/grok"));
        assert_eq!(p.version.as_deref(), Some("grok 1.0.5 (abc)"));
    }

    #[test]
    fn probe_stdout_missing_cli() {
        let raw = "GROK_APP_PROBE\nCLI_MISSING\nAUTH_MISSING\n\n\n";
        let p = parse_probe_stdout(raw).unwrap();
        assert_eq!(p.cli, "missing");
        assert_eq!(p.auth, "missing");
        assert!(p.path.is_none());
    }

    #[test]
    fn probe_stdout_rejects_garbage() {
        assert!(parse_probe_stdout("hello").is_none());
        assert!(parse_probe_stdout("GROK_APP_PROBE\nNOPE\nAUTH_OK\n\n\n").is_none());
    }

    #[test]
    fn commands_quote_alias_as_argv_word() {
        let (install, login, ir, lr) = commands_for_alias("devbox");
        assert_eq!(
            install,
            "ssh devbox 'curl -fsSL https://x.ai/cli/install.sh | bash'"
        );
        assert_eq!(login, "ssh -t devbox 'grok login --device-auth'");
        assert_eq!(ir, INSTALL_REMOTE);
        assert_eq!(lr, LOGIN_REMOTE);
    }

    #[test]
    fn classify_host_key() {
        let (code, _) = classify_ssh_stderr("Host key verification failed.\n");
        assert_eq!(code, "host_key");
    }

    #[test]
    fn posix_single_quote_escapes() {
        assert_eq!(posix_single_quote("abc"), "'abc'");
        assert_eq!(posix_single_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn percent_decode_cwd() {
        assert_eq!(percent_decode_path("%2Fhome%2Fme%2Fproj"), "/home/me/proj");
        assert_eq!(percent_decode_path("/plain"), "/plain");
    }

    #[test]
    fn join_remote_rel_rejects_escape() {
        assert_eq!(
            join_remote_rel("/data/pengqlu/code", "README.md").unwrap(),
            "/data/pengqlu/code/README.md"
        );
        assert_eq!(
            join_remote_rel("/data/pengqlu/code/", "docs/a.md").unwrap(),
            "/data/pengqlu/code/docs/a.md"
        );
        assert!(join_remote_rel("/data/pengqlu/code", "../etc/passwd").is_err());
        assert!(join_remote_rel("relative", "a.md").is_err());
    }

    #[test]
    fn parse_marked_json_reads_header_line() {
        let raw = "noise\nGROK_APP_READ\n{\"ok\":true,\"size\":4,\"text\":\"hi\"}\n";
        let v = parse_marked_json(raw, "GROK_APP_READ").unwrap();
        assert_eq!(v.get("ok").and_then(|x| x.as_bool()), Some(true));
        assert_eq!(v.get("text").and_then(|x| x.as_str()), Some("hi"));
    }

    #[test]
    fn remote_sess_script_embeds_page_bounds() {
        let s = remote_sess_script(20, 20);
        assert!(s.contains("OFFSET=20"));
        assert!(s.contains("LIMIT=20"));
        assert!(!s.contains("OFFSET={"));
    }

    #[test]
    fn parse_hist_stdout_splits_kind_and_body() {
        let raw = "GROK_APP_HIST\nKIND\tchat_history\n{\"type\":\"user\",\"content\":\"hi\"}\nGROK_APP_HIST_END\n";
        let h = parse_hist_stdout(raw).unwrap();
        assert_eq!(h.kind, "chat_history");
        assert!(h.body.contains("hi"));
        let pairs = pairs_from_hist(&h);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].1, "hi");
    }

    #[test]
    fn parse_hist_recovers_jsonl_emitted_before_markers() {
        // Repro: python print() + stdout.buffer.write() on a non-TTY SSH pipe.
        let raw = "{\"type\":\"user\",\"content\":\"<user_query>\\n标注任务\\n</user_query>\"}\n{\"type\":\"assistant\",\"content\":\"ok\"}\nGROK_APP_HIST\nKIND\tchat_history\nGROK_APP_HIST_END\n";
        let h = parse_hist_stdout(raw).unwrap();
        assert_eq!(h.kind, "chat_history");
        let pairs = pairs_from_hist(&h);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].1, "标注任务");
        assert_eq!(pairs[1].1, "ok");
    }

    #[test]
    fn looks_like_agent_uuid_matches_grok_ids() {
        assert!(looks_like_agent_uuid(
            "01a01907-adf3-7e00-a7a8-aee1082b0556"
        ));
        assert!(!looks_like_agent_uuid("帮我看一下 hallucination"));
    }

    #[test]
    fn parse_remote_sessions() {
        let raw =
            "noise\nGROK_APP_SESS\nTOTAL\t35\nid1\t%2Fwork\t171000\tFix bug\nid2\t%2Ftmp\t0\t\n";
        let (total, s) = parse_sess_stdout(raw).unwrap();
        assert_eq!(total, 35);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].cwd, "/work");
        assert_eq!(s[0].title, "Fix bug");
        assert!(s[0].updated_at.as_deref().unwrap_or("").contains("1970"));
        assert_eq!(s[1].title, "");
        assert_eq!(s[1].updated_at, None);
    }

    #[test]
    fn controlpath_with_spaces_is_quoted_for_ssh_o() {
        let path = "/Users/me/Library/Application Support/com.grokapp.grok-app/ssh-cm/UTS.sock";
        let a = ssh_config_assignment("ControlPath", path);
        assert_eq!(
            a,
            r#"ControlPath="/Users/me/Library/Application Support/com.grokapp.grok-app/ssh-cm/UTS.sock""#
        );
        assert!(!a.starts_with("ControlPath=/Users"));
    }

    #[test]
    fn simple_controlpath_stays_unquoted() {
        assert_eq!(
            ssh_config_assignment("ControlPath", "/tmp/grok-app-ssh-cm/UTS.sock"),
            "ControlPath=/tmp/grok-app-ssh-cm/UTS.sock"
        );
    }

    #[test]
    fn openssh_rejects_unquoted_controlpath_with_spaces() {
        let Some(ssh) = find_ssh_binary() else {
            return;
        };
        let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
        let out = std::process::Command::new(ssh)
            .args([
                "-G",
                "-F",
                null,
                "-o",
                "ControlPath=/tmp/Application Support/x.sock",
                "-o",
                "BatchMode=yes",
                "127.0.0.1",
            ])
            .output()
            .expect("ssh -G");
        let stderr = String::from_utf8_lossy(&out.stderr).to_ascii_lowercase();
        assert!(
            stderr.contains("extra arguments"),
            "expected OpenSSH extra-arguments error, got: {stderr}"
        );
    }

    #[test]
    fn openssh_accepts_quoted_controlpath_with_spaces() {
        let Some(ssh) = find_ssh_binary() else {
            return;
        };
        let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
        let path = "/tmp/Application Support/x.sock";
        let opt = ssh_config_assignment("ControlPath", path);
        let out = std::process::Command::new(ssh)
            .args([
                "-G",
                "-F",
                null,
                "-o",
                &opt,
                "-o",
                "BatchMode=yes",
                "127.0.0.1",
            ])
            .output()
            .expect("ssh -G");
        let stderr = String::from_utf8_lossy(&out.stderr).to_ascii_lowercase();
        assert!(
            !stderr.contains("extra arguments"),
            "quoted ControlPath still rejected: {stderr}"
        );
        assert!(out.status.success(), "ssh -G failed: {stderr}");
        let stdout = String::from_utf8_lossy(&out.stdout).to_ascii_lowercase();
        assert!(
            stdout.contains("controlpath"),
            "ssh -G did not echo controlpath"
        );
    }

    #[test]
    fn ssh_pty_remote_cmd_quotes_cwd() {
        let cmd = ssh_pty_remote_cmd(Some("/data/pengqlu/my proj"));
        assert!(cmd.contains(posix_single_quote("/data/pengqlu/my proj").as_str()));
        assert!(cmd.contains("exec ${SHELL:-bash} -l"));
        assert!(!cmd.contains("UTS"));
        assert_eq!(ssh_pty_remote_cmd(None), "exec ${SHELL:-bash} -l");
        assert_eq!(ssh_pty_remote_cmd(Some("  ")), "exec ${SHELL:-bash} -l");
    }

    #[test]
    fn ssh_pty_argv_keeps_alias_as_own_word() {
        let Some(_) = find_ssh_binary() else {
            return;
        };
        let argv = ssh_pty_argv("UTS", Some("/data/pengqlu/code")).expect("argv");
        assert!(argv[0].contains("ssh"));
        assert_eq!(argv[1], "-tt");
        assert!(argv.iter().any(|a| a == "UTS"));
        assert!(argv.iter().any(|a| a.contains("ControlMaster=auto")));
        let remote = argv.last().expect("remote cmd");
        assert!(remote.contains("/data/pengqlu/code"));
        assert!(!argv.iter().any(|a| a.contains("ssh UTS")));
        assert!(ssh_pty_argv("host;rm", None).is_err());
    }

    #[test]
    fn ssh_acp_remote_command_quotes_cwd_and_flags() {
        let script = ssh_acp_remote_command(
            "/data/pengqlu/my proj",
            &[
                "--no-auto-update".into(),
                "agent".into(),
                "--no-leader".into(),
                "stdio".into(),
            ],
        )
        .unwrap();
        assert!(script.contains(posix_single_quote("/data/pengqlu/my proj").as_str()));
        assert!(script.contains("'--no-auto-update'"));
        assert!(script.contains("'stdio'"));
        assert!(script.contains("GROK_APP_CLI_MISSING"));
        assert!(!script.contains("UTS"));
        let quoted =
            ssh_acp_remote_command("/tmp", &["--rules".into(), "it's fine".into()]).unwrap();
        assert!(quoted.contains(posix_single_quote("it's fine").as_str()));
        assert!(ssh_acp_remote_command("/tmp\0", &[]).is_err());
    }

    #[test]
    fn ssh_acp_argv_is_one_remote_script() {
        let Some(_) = find_ssh_binary() else {
            return;
        };
        let argv = ssh_acp_argv(
            "UTS",
            "/data/pengqlu/my proj",
            &["--no-auto-update".into(), "agent".into(), "stdio".into()],
        )
        .expect("argv");
        assert_eq!(argv[1], "-T");
        assert!(!argv.iter().any(|a| a == "-tt"));
        assert!(!argv.iter().any(|a| a == "bash"));
        assert!(!argv.iter().any(|a| a == "-lc"));
        assert!(argv.iter().any(|a| a == "UTS"));
        assert_eq!(argv.iter().filter(|a| *a == "UTS").count(), 1);
        let script = argv.last().expect("remote script");
        assert!(script.contains("GROK_APP_CLI_MISSING"));
        assert!(script.contains(posix_single_quote("/data/pengqlu/my proj").as_str()));
        assert!(script.contains("'stdio'"));
        assert!(!script.contains("UTS"));
        assert!(ssh_acp_argv("host;rm", "/tmp", &[]).is_err());
    }

    #[test]
    fn loopback_http_url_rewrites_to_local_bind() {
        let t = parse_loopback_http_url("http://localhost:5173/app?x=1#h").unwrap();
        assert_eq!(t.host, "localhost");
        assert_eq!(t.port, 5173);
        assert_eq!(t.scheme, "http");
        assert_eq!(
            rewrite_loopback_url(&t, 49152),
            "http://127.0.0.1:49152/app?x=1#h"
        );
        assert!(parse_loopback_http_url("https://www.google.com").is_none());
        assert!(parse_loopback_http_url("http://127.0.0.1:3000").is_some());
        assert!(parse_loopback_http_url("http://[::1]:8080/").is_some());
        assert!(is_loopback_http_host("0.0.0.0"));
        assert!(!is_loopback_http_host("example.com"));
        assert_eq!(
            local_forward_spec(9, "127.0.0.1", 3000),
            "127.0.0.1:9:127.0.0.1:3000"
        );
        assert_eq!(local_forward_spec(9, "::1", 3000), "127.0.0.1:9:[::1]:3000");
    }

    #[test]
    fn parse_inspect_stdout_reads_json_after_ok() {
        let raw = "noise\nGROK_APP_INSPECT\nOK\n{\"skills\":[{\"name\":\"foo\"}]}\ntrailing\n";
        let (v, err) = parse_inspect_stdout(raw);
        assert!(err.is_none());
        let parsed = v.unwrap();
        let names: Vec<_> = parsed
            .get("skills")
            .and_then(|x| x.as_array())
            .unwrap()
            .iter()
            .filter_map(|s| s.get("name").and_then(|n| n.as_str()))
            .collect();
        assert_eq!(names, vec!["foo"]);
        let missing = parse_inspect_stdout("GROK_APP_INSPECT\nMISSING\n");
        assert!(missing.0.is_none());
        assert!(missing.1.unwrap().contains("not found"));
    }
}
