//! SSH hosts from OpenSSH config: list, test, probe remote Grok CLI.
//!
//! Wave 1 of remote workspaces. The App does **not** spawn a remote agent here.
//! Transport is the system `ssh` binary so `~/.ssh/config` (ProxyJump, keys,
//! ssh-agent) keeps working. Aliases are argv, never interpolated into a shell.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::time::timeout;

use crate::process_util;

const SSH_CONNECT_TIMEOUT_SECS: u64 = 8;
const SSH_OVERALL_TIMEOUT_SECS: u64 = 15;
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
    if l.contains("host key")
        || l.contains("known_hosts")
        || l.contains("authenticity of host")
    {
        (
            "host_key",
            truncate_err(stderr).if_empty(
                "Host key not in known_hosts. Run ssh <alias> once in a terminal.",
            ),
        )
    } else if l.contains("permission denied") {
        ("auth", truncate_err(stderr).if_empty("Permission denied"))
    } else if l.contains("timed out")
        || l.contains("timeout")
        || l.contains("connection timed out")
    {
        ("timeout", truncate_err(stderr).if_empty("Connection timed out"))
    } else if l.contains("could not resolve")
        || l.contains("name or service not known")
        || l.contains("nodename nor servname")
    {
        ("connect", truncate_err(stderr).if_empty("Could not resolve host"))
    } else if l.contains("connection refused") {
        ("connect", truncate_err(stderr).if_empty("Connection refused"))
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
    let (install_cmd, login_cmd, install_remote_cmd, login_remote_cmd) =
        commands_for_alias(alias);
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

    let (install_cmd, login_cmd, install_remote_cmd, login_remote_cmd) =
        commands_for_alias(&alias);

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
    let joined = timeout(
        Duration::from_secs(SSH_OVERALL_TIMEOUT_SECS),
        cmd.output(),
    )
    .await;
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

pub fn percent_decode_path(enc: &str) -> String {
    let bytes = enc.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""),
                16,
            ) {
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
    let ssh = find_ssh_binary().ok_or(SshRunErr::Missing)?;
    let mut cmd = Command::new(&ssh);
    process_util::apply_no_window_tokio(&mut cmd);
    apply_common_ssh_opts(&mut cmd, alias, mux);
    cmd.arg(alias)
        .arg(remote)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let started = Instant::now();
    match timeout(Duration::from_secs(secs), cmd.output()).await {
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
            try:
                mt = os.path.getmtime(d)
            except OSError:
                continue
            rows.append((mt, sid, enc, d))
rows.sort(reverse=True)
print("GROK_APP_SESS")
print("TOTAL\t%d" % len(rows))
for mt, sid, enc, d in rows[off:off+lim]:
    title = ""
    sp = os.path.join(d, "summary.json")
    if os.path.isfile(sp):
        try:
            s = json.load(open(sp))
            title = (s.get("generated_title") or s.get("session_summary") or s.get("title") or "").strip()
        except Exception:
            title = ""
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
                    if text:
                        title = text.splitlines()[0].strip()
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
                updated_at: if mtime.is_empty() || mtime == "0" {
                    None
                } else {
                    Some(mtime.to_string())
                },
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
            push_ssh_opt(
                &mut check,
                "ControlPath",
                path.to_string_lossy().as_ref(),
            );
            check.arg(&alias)
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
    match run_ssh(&alias, &remote_ls_script(&dir), true, SSH_OVERALL_TIMEOUT_SECS).await {
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
sid = os.environ.get("SID", "")
root = os.path.expanduser("~/.grok/sessions")
found = None
if os.path.isdir(root) and sid:
    for enc in os.listdir(root):
        d = os.path.join(root, enc, sid)
        if os.path.isdir(d):
            found = d
            break
print("GROK_APP_HIST")
if not found:
    print("KIND\tmissing")
    print("GROK_APP_HIST_END")
    sys.exit(0)
kind = "empty"
path = None
for name, label in (("chat_history.jsonl", "chat_history"), ("updates.jsonl", "updates")):
    p = os.path.join(found, name)
    if os.path.isfile(p) and os.path.getsize(p) > 0:
        kind = label
        path = p
        break
print("KIND\t" + kind)
if path:
    f = open(path, "rb")
    data = f.read(2097152)
    f.close()
    sys.stdout.buffer.write(data)
    if data and not data.endswith(b"\n"):
        sys.stdout.buffer.write(b"\n")
print("GROK_APP_HIST_END")
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
    let mut lines = stdout.lines().map(|l| l.trim_end_matches('\r'));
    while let Some(line) = lines.next() {
        if line.trim() != "GROK_APP_HIST" {
            continue;
        }
        let mut kind = "empty".to_string();
        let mut body = String::new();
        if let Some(next) = lines.next() {
            let next = next.trim();
            if let Some(k) = next.strip_prefix("KIND") {
                kind = k.trim().trim_start_matches('\t').trim().to_string();
            } else {
                body.push_str(next);
                body.push('\n');
            }
        }
        for rest in lines {
            if rest.trim() == "GROK_APP_HIST_END" {
                break;
            }
            body.push_str(rest);
            body.push('\n');
        }
        return Some(RemoteHist { kind, body });
    }
    None
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

fn persist_imported_journal(
    app_id: &str,
    pairs: Vec<(String, String)>,
) -> Result<(), String> {
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
    crate::cli_sessions::parse_chat_history_text(&hist.body).unwrap_or_else(|_| {
        crate::cli_sessions::parse_acp_updates_text(&hist.body)
    })
}

fn open_fail(alias: String, remote_session_id: String, error: impl Into<String>) -> SshOpenSessionResult {
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
        return Ok(open_fail(alias, session_id, "could not read remote session"));
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
    let existing = index
        .get(&key)
        .cloned()
        .filter(|id| {
            crate::store::load_sessions_index()
                .iter()
                .any(|s| s.id == *id)
        });

    let meta = if let Some(app_id) = existing {
        persist_imported_journal(&app_id, pairs.clone())?;
        let _ = crate::store::rename_session(&app_id, &title);
        crate::store::load_sessions_index()
            .into_iter()
            .find(|s| s.id == app_id)
            .ok_or_else(|| "imported session missing after write".to_string())?
    } else {
        let meta = crate::store::create_session(project_id.clone(), Some(title.clone()), false)?;
        persist_imported_journal(&meta.id, pairs)?;
        index.insert(key, meta.id.clone());
        save_import_index(&index)?;
        meta
    };

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
        assert_eq!(
            hosts[0].identity_file.as_deref(),
            Some("~/.ssh/id_ed25519")
        );
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
        let raw = "noise\nGROK_APP_PROBE\nCLI_OK\nAUTH_OK\n/home/u/.grok/bin/grok\ngrok 1.0.5 (abc)\n";
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
        assert_eq!(install, "ssh devbox 'curl -fsSL https://x.ai/cli/install.sh | bash'");
        assert_eq!(login, "ssh -t devbox 'grok login --device-auth'");
        assert_eq!(ir, INSTALL_REMOTE);
        assert_eq!(lr, LOGIN_REMOTE);
    }

    #[test]
    fn classify_host_key() {
        let (code, _) = classify_ssh_stderr(
            "Host key verification failed.\n",
        );
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
    fn remote_sess_script_embeds_page_bounds() {
        let s = remote_sess_script(20, 20);
        assert!(s.contains("OFFSET=20"));
        assert!(s.contains("LIMIT=20"));
        assert!(!s.contains("OFFSET={"));
    }

    #[test]
    fn parse_hist_stdout_splits_kind_and_body() {
        let raw = "noise\nGROK_APP_HIST\nKIND\tchat_history\n{\"type\":\"user\",\"content\":\"hi\"}\nGROK_APP_HIST_END\n";
        let h = parse_hist_stdout(raw).unwrap();
        assert_eq!(h.kind, "chat_history");
        assert!(h.body.contains("hello") || h.body.contains("hi"));
    }

    #[test]
    fn looks_like_agent_uuid_matches_grok_ids() {
        assert!(looks_like_agent_uuid("01a01907-adf3-7e00-a7a8-aee1082b0556"));
        assert!(!looks_like_agent_uuid("帮我看一下 hallucination"));
    }

    #[test]
    fn parse_remote_sessions() {
        let raw = "noise\nGROK_APP_SESS\nTOTAL\t35\nid1\t%2Fwork\t171000\tFix bug\nid2\t%2Ftmp\t0\t\n";
        let (total, s) = parse_sess_stdout(raw).unwrap();
        assert_eq!(total, 35);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].cwd, "/work");
        assert_eq!(s[0].title, "Fix bug");
        assert_eq!(s[1].title, "");
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
}
