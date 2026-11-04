//! Read-only view of `[endpoints]` in the shared Atlas CLI home.
//!
//! Settings shows this block from `~/.atlas/config.toml`. The host never
//! rewrites that file. Only this table is returned — model keys and other
//! sections stay on disk.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Hard cap so a huge config cannot fill the settings card.
const MAX_BYTES: u64 = 512 * 1024;
const MAX_ENTRIES: usize = 32;
const MAX_VALUE_CHARS: usize = 2048;

/// One scalar from `[endpoints]`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AtlasEndpointEntry {
    pub key: String,
    pub value: String,
}

/// Snapshot for Settings → Runtime → CLI.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AtlasEndpointsSnapshot {
    pub path: String,
    pub exists: bool,
    pub entries: Vec<AtlasEndpointEntry>,
    /// True when the file was longer than [`MAX_BYTES`].
    pub truncated: bool,
    pub error: Option<String>,
}

/// Read `~/.atlas/config.toml` and return only `[endpoints]`.
pub fn read_atlas_endpoints() -> AtlasEndpointsSnapshot {
    read_atlas_endpoints_at(&shared_config_path())
}

fn shared_config_path() -> PathBuf {
    crate::paths::shared_cli_home().join("config.toml")
}

pub fn read_atlas_endpoints_at(path: &Path) -> AtlasEndpointsSnapshot {
    let path_str = path.display().to_string();
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return empty_snapshot(path_str, false, None);
        }
        Err(e) => return empty_snapshot(path_str, false, Some(e.to_string())),
    };
    if !meta.is_file() {
        return empty_snapshot(path_str, false, Some("not a file".into()));
    }
    let truncated = meta.len() > MAX_BYTES;
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return empty_snapshot(path_str, true, Some(e.to_string())),
    };
    let mut buf = Vec::new();
    if let Err(e) = file.take(MAX_BYTES).read_to_end(&mut buf) {
        return empty_snapshot(path_str, true, Some(e.to_string()));
    }
    let text = String::from_utf8_lossy(&buf);
    let text = text.trim_start_matches('\u{feff}');
    AtlasEndpointsSnapshot {
        path: path_str,
        exists: true,
        entries: parse_endpoints_table(text),
        truncated,
        error: None,
    }
}

fn empty_snapshot(path: String, exists: bool, error: Option<String>) -> AtlasEndpointsSnapshot {
    AtlasEndpointsSnapshot {
        path,
        exists,
        entries: Vec::new(),
        truncated: false,
        error,
    }
}

/// Pure extract of the flat `[endpoints]` table. Later tables are ignored.
/// Secret-looking keys and `ENC(...)` / `sk-` values are dropped.
pub fn parse_endpoints_table(text: &str) -> Vec<AtlasEndpointEntry> {
    let mut out = Vec::new();
    let mut in_endpoints = false;
    for line in text.lines() {
        let trimmed = line.trim().trim_start_matches('\u{feff}');
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(header) = table_name(trimmed) {
            in_endpoints = header == "endpoints";
            continue;
        }
        if !in_endpoints || out.len() >= MAX_ENTRIES {
            continue;
        }
        let Some((key, value)) = parse_assignment(trimmed) else {
            continue;
        };
        if is_secret_key(&key) || looks_secret_value(&value) {
            continue;
        }
        out.push(AtlasEndpointEntry {
            key,
            value: truncate_chars(&value, MAX_VALUE_CHARS),
        });
    }
    out
}

fn table_name(trimmed: &str) -> Option<String> {
    if !trimmed.starts_with('[') {
        return None;
    }
    let (inner, array) = if let Some(rest) = trimmed.strip_prefix("[[") {
        let end = rest.find("]]")?;
        (&rest[..end], true)
    } else {
        let end = trimmed.find(']')?;
        (&trimmed[1..end], false)
    };
    let name = inner.trim();
    if name.is_empty() || name.contains('[') || name.contains(']') {
        return None;
    }
    if array {
        return Some(format!("[[{name}]]"));
    }
    Some(name.to_string())
}

fn parse_assignment(trimmed: &str) -> Option<(String, String)> {
    let eq = trimmed.find('=')?;
    let key = trimmed[..eq].trim();
    if !is_plain_key(key) {
        return None;
    }
    let raw = trimmed[eq + 1..].trim();
    if raw.is_empty() {
        return Some((key.to_string(), String::new()));
    }
    let value = if raw.starts_with("\"\"\"") || raw.starts_with("'''") {
        return None;
    } else if raw.starts_with('"') {
        parse_basic_string(raw)?
    } else if raw.starts_with('\'') {
        parse_literal_string(raw)?
    } else {
        strip_bare_comment(raw).to_string()
    };
    Some((key.to_string(), value))
}

fn is_plain_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

fn parse_basic_string(raw: &str) -> Option<String> {
    let chars: Vec<char> = raw.chars().collect();
    if chars.first().copied() != Some('"') {
        return None;
    }
    let mut out = String::new();
    let mut i = 1;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            let n = *chars.get(i + 1)?;
            match n {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                other => {
                    out.push('\\');
                    out.push(other);
                }
            }
            i += 2;
            continue;
        }
        if c == '"' {
            return Some(out);
        }
        out.push(c);
        i += 1;
    }
    None
}

fn parse_literal_string(raw: &str) -> Option<String> {
    let rest = raw.strip_prefix('\'')?;
    let end = rest.find('\'')?;
    Some(rest[..end].to_string())
}

fn strip_bare_comment(raw: &str) -> &str {
    let mut prev_space = true;
    for (i, c) in raw.char_indices() {
        if c == '#' && prev_space {
            return raw[..i].trim_end();
        }
        prev_space = c.is_whitespace();
    }
    raw.trim()
}

fn is_secret_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    [
        "api_key",
        "apikey",
        "secret",
        "password",
        "passwd",
        "token",
        "authorization",
        "private_key",
        "access_key",
    ]
    .iter()
    .any(|n| k.contains(n))
}

fn looks_secret_value(value: &str) -> bool {
    let t = value.trim();
    t.starts_with("ENC(") || t.starts_with("sk-") || t.starts_with("sk_")
}

fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut out: String = value.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[tauri::command]
pub async fn atlas_endpoints_read() -> Result<AtlasEndpointsSnapshot, String> {
    tokio::task::spawn_blocking(read_atlas_endpoints)
        .await
        .map_err(|e| format!("atlas endpoints read failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_endpoints_and_stops_before_secrets() {
        let text = r#"
[cli]
installer = "internal"

[endpoints]
cli_chat_proxy_base_url = "http://cli.example.test/atlas/v1"
cli_update_base_url = "http://cli.example.test/atlas/cli"

[model.example]
api_key = "sk-secretsecretsecret"
"#;
        let rows = parse_endpoints_table(text);
        assert_eq!(
            rows,
            vec![
                AtlasEndpointEntry {
                    key: "cli_chat_proxy_base_url".into(),
                    value: "http://cli.example.test/atlas/v1".into(),
                },
                AtlasEndpointEntry {
                    key: "cli_update_base_url".into(),
                    value: "http://cli.example.test/atlas/cli".into(),
                },
            ]
        );
    }

    #[test]
    fn keeps_fragment_and_strips_inline_comment() {
        let text = r#"
[ endpoints ]
cli_chat_proxy_base_url = "https://cli.example.test/v1#frag"
cli_update_base_url = "https://cli.example.test/cli" # note
"#;
        let rows = parse_endpoints_table(text);
        assert_eq!(rows[0].value, "https://cli.example.test/v1#frag");
        assert_eq!(rows[1].value, "https://cli.example.test/cli");
    }

    #[test]
    fn drops_secret_keys_and_enc_values() {
        let text = r#"
[endpoints]
cli_chat_proxy_base_url = "https://cli.example.test/v1"
api_key = "sk-shouldnotappear000"
cli_update_base_url = "ENC(abcdef)"
"#;
        let rows = parse_endpoints_table(text);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, "cli_chat_proxy_base_url");
    }

    #[test]
    fn ignores_array_and_nested_tables() {
        let text = r#"
[[endpoints]]
cli_chat_proxy_base_url = "https://no.example.test"

[endpoints.extra]
cli_update_base_url = "https://nested.example.test"
"#;
        assert!(parse_endpoints_table(text).is_empty());
    }

    #[test]
    fn read_at_missing_and_present() {
        let missing = std::env::temp_dir().join(format!(
            "atlas-endpoints-missing-{}-nope.toml",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&missing);
        let snap = read_atlas_endpoints_at(&missing);
        assert!(!snap.exists);
        assert!(snap.error.is_none());
        assert!(snap.entries.is_empty());

        let present = std::env::temp_dir().join(format!(
            "atlas-endpoints-present-{}-yes.toml",
            std::process::id()
        ));
        std::fs::write(
            &present,
            "[endpoints]\ncli_update_base_url = \"https://cli.example.test/cli\"\n",
        )
        .unwrap();
        let snap = read_atlas_endpoints_at(&present);
        assert!(snap.exists);
        assert_eq!(snap.entries.len(), 1);
        assert!(!snap.truncated);
        let _ = std::fs::remove_file(&present);
    }
}
