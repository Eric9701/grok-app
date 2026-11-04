//! Persist the installer block from `install.ps1` after an App-managed CLI install.
//!
//! Two files get the same keys:
//! - shared `~/.atlas/config.toml` (terminal `atlas`, shared mode)
//! - App `agent-home/config.toml` (independent mode and custom relays)
//!
//! Only `[cli] installer` and the three `[endpoints]` URLs are written. Other
//! tables, comments, and secrets stay on the surrounding lines. A stable install
//! drops an existing `[cli] channel` line and does not write a new one.

use std::fs;
use std::path::Path;

use crate::agent_home_config::{assignment_key, parse_table_header, with_config_write_lock};

/// Endpoint URLs written into both config files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallerEndpoints {
    pub login_base_url: String,
    pub cli_chat_proxy_base_url: String,
    pub cli_update_base_url: String,
}

/// Inverse of the install script's default layout.
///
/// `…/atlas/cli` → server root, then `{server}/atlas` and `{server}/atlas/v1`.
/// A trailing slash is stripped. A base that does not end in `/atlas/cli` keeps
/// itself as the update URL; login is that base with a trailing `/cli` removed,
/// and the proxy is `{login}/v1`.
pub fn endpoints_from_mirror(mirror: &str) -> InstallerEndpoints {
    let update = mirror.trim().trim_end_matches('/').to_string();
    if let Some(server) = update.strip_suffix("/atlas/cli") {
        let server = server.trim_end_matches('/');
        return InstallerEndpoints {
            login_base_url: format!("{server}/atlas"),
            cli_chat_proxy_base_url: format!("{server}/atlas/v1"),
            cli_update_base_url: update,
        };
    }
    let login = update
        .strip_suffix("/cli")
        .unwrap_or(update.as_str())
        .trim_end_matches('/')
        .to_string();
    let proxy = format!("{login}/v1");
    InstallerEndpoints {
        login_base_url: login,
        cli_chat_proxy_base_url: proxy,
        cli_update_base_url: update,
    }
}

fn toml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Apply the stable-channel installer block to TOML text.
pub fn apply_installer_config(text: &str, endpoints: &InstallerEndpoints) -> String {
    let cli = upsert_section(
        text,
        "cli",
        &["installer", "channel"],
        &["installer = \"internal\"".to_string()],
    );
    let endpoint_lines = vec![
        format!(
            "login_base_url = {}",
            toml_string(&endpoints.login_base_url)
        ),
        format!(
            "cli_chat_proxy_base_url = {}",
            toml_string(&endpoints.cli_chat_proxy_base_url)
        ),
        format!(
            "cli_update_base_url = {}",
            toml_string(&endpoints.cli_update_base_url)
        ),
    ];
    upsert_section(
        &cli,
        "endpoints",
        &[
            "login_base_url",
            "cli_chat_proxy_base_url",
            "cli_update_base_url",
        ],
        &endpoint_lines,
    )
}

fn newline_of(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn split_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split('\n')
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect()
}

fn join_lines(lines: &[String], nl: &str, trailing: bool) -> String {
    let mut out = lines.join(nl);
    if trailing && !out.ends_with(nl) {
        out.push_str(nl);
    }
    out
}

/// Insert `new_lines` under `[section]`, dropping existing assignments for `keys`.
///
/// Missing file text gains the section. A missing section is appended. Other
/// sections and unrelated keys in the same section stay in place.
fn upsert_section(text: &str, section: &str, keys: &[&str], new_lines: &[String]) -> String {
    let nl = newline_of(text);
    let had_trailing_nl = text.is_empty() || text.ends_with('\n');
    let lines = split_lines(text);
    let has_section = lines.iter().any(|line| {
        matches!(
            parse_table_header(line.trim()),
            Some((false, name)) if name == section
        )
    });
    if !has_section {
        let mut block = Vec::new();
        if !lines.is_empty() && lines.iter().any(|l| !l.trim().is_empty()) {
            block.push(String::new());
        }
        block.push(format!("[{section}]"));
        block.extend(new_lines.iter().cloned());
        let mut all = lines;
        all.extend(block);
        return join_lines(&all, nl, true);
    }

    let mut out: Vec<String> = Vec::with_capacity(lines.len() + new_lines.len());
    let mut in_section = false;
    for line in lines {
        if let Some((is_array, name)) = parse_table_header(line.trim()) {
            if !is_array && name == section {
                out.push(line);
                out.extend(new_lines.iter().cloned());
                in_section = true;
                continue;
            }
            in_section = false;
            out.push(line);
            continue;
        }
        if in_section {
            if let Some(key) = assignment_key(line.trim()) {
                if keys.iter().any(|k| *k == key) {
                    continue;
                }
            }
        }
        out.push(line);
    }
    join_lines(&out, nl, had_trailing_nl)
}

fn read_config(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("read {}: {e}", path.display())),
    }
}

/// Create or update one config file. Parent directories are created.
pub fn write_installer_config(path: &Path, endpoints: &InstallerEndpoints) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
    }
    let existing = read_config(path)?;
    let next = apply_installer_config(&existing, endpoints);
    fs::write(path, next).map_err(|e| format!("write {}: {e}", path.display()))
}

/// Write the installer block into the shared CLI home and App agent-home.
pub fn persist_installer_configs(mirror: &str) -> Result<(), String> {
    let endpoints = endpoints_from_mirror(mirror);
    let shared = crate::paths::shared_cli_home().join("config.toml");
    write_installer_config(&shared, &endpoints).map_err(|e| format!("shared config.toml: {e}"))?;
    with_config_write_lock(|| {
        crate::paths::ensure_app_dirs().map_err(|e| format!("agent-home dir: {e}"))?;
        let agent = crate::paths::agent_config_toml();
        write_installer_config(&agent, &endpoints)
            .map_err(|e| format!("agent-home config.toml: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_derive_login_proxy_and_update() {
        let intra = endpoints_from_mirror("http://10.218.220.237:22255/atlas/cli");
        assert_eq!(
            intra.cli_update_base_url,
            "http://10.218.220.237:22255/atlas/cli"
        );
        assert_eq!(intra.login_base_url, "http://10.218.220.237:22255/atlas");
        assert_eq!(
            intra.cli_chat_proxy_base_url,
            "http://10.218.220.237:22255/atlas/v1"
        );

        let public = endpoints_from_mirror("https://atlas.myai.tech/atlas/cli/");
        assert_eq!(public.login_base_url, "https://atlas.myai.tech/atlas");
        assert_eq!(
            public.cli_chat_proxy_base_url,
            "https://atlas.myai.tech/atlas/v1"
        );
        assert_eq!(
            public.cli_update_base_url,
            "https://atlas.myai.tech/atlas/cli"
        );
    }

    #[test]
    fn empty_file_gains_cli_and_endpoints() {
        let ep = endpoints_from_mirror("https://atlas.myai.tech/atlas/cli");
        let out = apply_installer_config("", &ep);
        assert!(out.starts_with("[cli]\ninstaller = \"internal\"\n"));
        assert!(out.contains("[endpoints]\n"));
        assert!(out.contains("login_base_url = \"https://atlas.myai.tech/atlas\"\n"));
        assert!(out.contains("cli_chat_proxy_base_url = \"https://atlas.myai.tech/atlas/v1\"\n"));
        assert!(out.contains("cli_update_base_url = \"https://atlas.myai.tech/atlas/cli\"\n"));
        assert!(!out.contains("channel"));
    }

    #[test]
    fn existing_model_and_other_endpoint_keys_stay() {
        let original = "\
[model.arch-kimi]
name = \"Kimi\"
api_key = \"ENC(secret)\"

[endpoints]
login_base_url = \"https://old.example/atlas\"
extra = \"keep-me\"
cli_update_base_url = \"ENC(old-update)\"

[cli] # note
channel = \"alpha\"
installer = \"npm\"
verbose = true
";
        let ep = endpoints_from_mirror("http://10.218.220.237:22255/atlas/cli/");
        let out = apply_installer_config(original, &ep);
        assert!(out.contains("name = \"Kimi\""));
        assert!(out.contains("api_key = \"ENC(secret)\""));
        assert!(out.contains("extra = \"keep-me\""));
        assert!(out.contains("verbose = true"));
        assert!(out.contains("[cli] # note"));
        assert!(!out.contains("channel"));
        assert!(!out.contains("ENC(old-update)"));
        assert!(!out.contains("https://old.example/atlas"));
        assert!(out.contains("installer = \"internal\""));
        assert!(out.contains("cli_update_base_url = \"http://10.218.220.237:22255/atlas/cli\""));
        assert!(out.contains("login_base_url = \"http://10.218.220.237:22255/atlas\""));
        assert!(out.contains("cli_chat_proxy_base_url = \"http://10.218.220.237:22255/atlas/v1\""));
    }

    #[test]
    fn write_roundtrip_keeps_unrelated_table() {
        let dir = std::env::temp_dir().join(format!("cli-install-config-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(&path, "[model.keep]\napi_key = \"ENC(x)\"\n").unwrap();
        let ep = endpoints_from_mirror("https://atlas.myai.tech/atlas/cli");
        write_installer_config(&path, &ep).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("api_key = \"ENC(x)\""));
        assert!(text.contains("installer = \"internal\""));
        assert!(text.contains("https://atlas.myai.tech/atlas/v1"));
        let _ = fs::remove_dir_all(&dir);
    }
}
