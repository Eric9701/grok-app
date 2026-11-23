//! Unified write layer for App agent-home `config.toml`.
//!
//! Independent session-data mode (`GROK_HOME` = App agent-home) may write
//! allowlisted keys. Shared mode refuses path resolve and strict writes
//! (`update_config_toml`) so ordinary saves never rewrite `~/.atlas/config.toml`.
//!
//! Diagnostics repair is the exception. An explicit user action may
//! backup-and-dedupe, or backup-and-replace with an empty valid file, the
//! `config.toml` for the active session mode — including shared `~/.atlas`.
//! Spawn-time [`ensure_agent_home_config_sane`] stays independent-only.
//!
//! Pure TOML helpers are line-oriented upserts (no full parse) so unrelated
//! sections and secrets stay intact. Diagnosis uses `toml` only to decide
//! whether the file already parses; it never logs file text.
//!
//! ## Safety
//! - Key match is **exact** (never `starts_with`) so `yolo` cannot clobber `yolo_mode`.
//! - Table headers accept trailing comments (`[ui] # note`).
//! - [`ensure_agent_home_config_sane`] dedupes broken files only; valid configs are
//!   never rewritten.
//! - [`repair_config_file`] writes a backup before any change. A file that
//!   already parses is left untouched, including its secrets.

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::paths::{agent_config_toml, ensure_app_dirs};

/// Serialize independent agent-home config writers (cascade sync + heal + prefs).
static CONFIG_WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Run `f` while holding the agent-home config write lock.
pub fn with_config_write_lock<T>(f: impl FnOnce() -> T) -> T {
    let _guard = CONFIG_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    f()
}

/// Normalize session_data_mode to `independent` | `shared`.
pub fn normalize_mode(session_data_mode: &str) -> &'static str {
    if session_data_mode.trim().eq_ignore_ascii_case("shared") {
        "shared"
    } else {
        "independent"
    }
}

/// Error message when shared mode refuses a write-path resolve.
pub const SHARED_MODE_REFUSED: &str =
    "shared session mode: agent-home config.toml writes refused (never rewrite ~/.grok)";

/// Resolve App agent-home `config.toml` for writes.
///
/// - independent → `…/agent-home/config.toml` (ensures app dirs)
/// - shared → `Err` (refuse; do not return `~/.grok/config.toml`)
pub fn resolve_writable_config_path(session_data_mode: &str) -> Result<PathBuf, String> {
    if normalize_mode(session_data_mode) == "shared" {
        return Err(SHARED_MODE_REFUSED.into());
    }
    let _ = ensure_app_dirs();
    Ok(agent_config_toml())
}

fn bool_lit(v: bool) -> &'static str {
    if v {
        "true"
    } else {
        "false"
    }
}

fn finish_join(original: &str, lines: &[String]) -> String {
    let mut joined = lines.join("\n");
    if (original.ends_with('\n') || original.is_empty()) && !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

fn finish_join_always_nl(_original: &str, lines: &[String]) -> String {
    let mut joined = lines.join("\n");
    if !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

/// 定位结束表名的 `]`，跳过**引号内**的 `]`。
///
/// TOML 表名可以带引号包住任意字符：`[model."claude-glm-5.3[1M]"]` 的**第一个**
/// `]` 属于名字本身。早先用 `find(']')` 取到的就是它，于是名字被截断成
/// `model."claude-glm-5.3[1M]`、剩余部分是 `"]` —— 既不是空也不是注释，整个表头
/// 被判定为**不是表头**，这张表的字段就被算进了上一张表的 scope，去重时把上一张
/// 表的同名赋值当成重复项删掉（用户的 `[model.ada-anthropic]` 正是这样丢掉了
/// `model` / `base_url` / `api_key` / `api_backend`）。
fn closing_bracket_index(t: &str, is_array: bool) -> Option<usize> {
    let bytes = t.as_bytes();
    let mut in_quotes = false;
    let mut i = if is_array { 2 } else { 1 };
    while i < bytes.len() {
        match bytes[i] {
            b'"' => in_quotes = !in_quotes,
            b']' if !in_quotes => {
                if is_array {
                    if bytes.get(i + 1) == Some(&b']') {
                        return Some(i);
                    }
                } else {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Parse a TOML table / array-table header.
///
/// Accepts trailing comments: `[ui] # note`, `[[hooks]] # x`.
/// Returns `(is_array_table, table_name)`.
pub fn parse_table_header(trimmed: &str) -> Option<(bool, &str)> {
    let t = trimmed.trim();
    if !t.starts_with('[') {
        return None;
    }
    let is_array = t.starts_with("[[");
    let end = closing_bracket_index(t, is_array)?;
    let (name, after_close) = if is_array {
        (t[2..end].trim(), t[end + 2..].trim())
    } else {
        (t[1..end].trim(), t[end + 1..].trim())
    };
    if name.is_empty() {
        return None;
    }
    // Only whitespace / comments allowed after the closing bracket.
    if !after_close.is_empty() && !after_close.starts_with('#') {
        return None;
    }
    Some((is_array, name))
}

/// Key of a simple `key = value` assignment line (None for headers / comments / blank).
pub fn assignment_key(trimmed: &str) -> Option<&str> {
    let t = trimmed.trim();
    if t.is_empty() || t.starts_with('#') || t.starts_with('[') {
        return None;
    }
    let eq = t.find('=')?;
    let key = t[..eq].trim();
    if key.is_empty() {
        return None;
    }
    Some(key)
}

/// Upsert a bare top-level `key = value` assignment (not inside a `[table]`).
pub fn set_top_level_assignment(text: &str, key: &str, value: &str) -> String {
    let line_val = format!("{key} = {value}");
    let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    let mut in_table = false;
    let mut first_table_idx: Option<usize> = None;

    for i in 0..lines.len() {
        let trimmed = lines[i].trim();
        if parse_table_header(trimmed).is_some() {
            if first_table_idx.is_none() {
                first_table_idx = Some(i);
            }
            in_table = true;
            continue;
        }
        if in_table {
            continue;
        }
        if assignment_key(trimmed) == Some(key) {
            lines[i] = line_val;
            return finish_join(text, &lines);
        }
    }

    if let Some(idx) = first_table_idx {
        lines.insert(idx, line_val);
        return finish_join_always_nl(text, &lines);
    }

    let base = text.trim_end();
    if base.is_empty() {
        format!("{line_val}\n")
    } else {
        format!("{base}\n{line_val}\n")
    }
}

/// Upsert top-level `key = true|false`.
pub fn set_top_level_bool(text: &str, key: &str, value: bool) -> String {
    set_top_level_assignment(text, key, bool_lit(value))
}

/// Upsert top-level `key = "value"` (quoted string).
#[allow(dead_code)]
pub fn set_top_level_string(text: &str, key: &str, value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    set_top_level_assignment(text, key, &format!("\"{escaped}\""))
}

/// Upsert `key = value` under `[table]` without touching other sections/keys.
///
/// Header match is by table **name**, so `[ui] # comment` counts as `[ui]`.
/// Key match is **exact** (not prefix).
pub fn set_table_key(text: &str, table: &str, key: &str, value: &str, quoted: bool) -> String {
    let header = format!("[{table}]");
    let line_val = if quoted {
        format!(
            "{key} = \"{}\"",
            value.replace('\\', "\\\\").replace('"', "\\\"")
        )
    } else {
        format!("{key} = {value}")
    };
    let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    let mut in_table = false;
    let mut table_start: Option<usize> = None;
    for i in 0..lines.len() {
        let trimmed = lines[i].trim();
        if let Some((is_array, name)) = parse_table_header(trimmed) {
            if !is_array && name == table {
                in_table = true;
                table_start = Some(i);
            } else if in_table {
                lines.insert(i, line_val);
                return finish_join_always_nl(text, &lines);
            } else {
                in_table = false;
            }
            continue;
        }
        if in_table && assignment_key(trimmed) == Some(key) {
            lines[i] = line_val;
            return finish_join(text, &lines);
        }
    }
    if let Some(start) = table_start {
        lines.insert(start + 1, line_val);
        return finish_join_always_nl(text, &lines);
    }
    let block = format!("\n{header}\n{line_val}\n");
    let base = text.trim_end();
    if base.is_empty() {
        format!("{header}\n{line_val}\n")
    } else {
        format!("{base}{block}")
    }
}

/// Upsert `[table] key = true|false`.
pub fn set_table_bool(text: &str, table: &str, key: &str, value: bool) -> String {
    set_table_key(text, table, key, bool_lit(value), false)
}

/// Pin App agent-home so Grok does **not** merge Claude/Cursor MCP catalogs.
///
/// Without this, `~/.claude.json` mcpServers (Playwright, open-websearch, …)
/// are imported by default and stall first-tool discovery for ~30s.
pub fn ensure_compat_mcp_disabled(text: &str) -> String {
    let t = set_table_bool(text, "compat.claude", "mcps", false);
    set_table_bool(&t, "compat.cursor", "mcps", false)
}

/// Write `[compat.claude/cursor] mcps = false` into independent agent-home.
pub fn apply_compat_mcp_disabled(session_data_mode: &str) -> Result<(), String> {
    update_config_toml_if_independent(session_data_mode, ensure_compat_mcp_disabled).map(|_| ())
}

/// Upsert `[table] key = "value"`.
pub fn set_table_string(text: &str, table: &str, key: &str, value: &str) -> String {
    set_table_key(text, table, key, value, true)
}

/// Parse a TOML bool literal (`true` / `false`).
pub fn parse_toml_bool(raw: &str) -> Option<bool> {
    match raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase()
        .as_str()
    {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Parse a TOML string / bare value (strip surrounding quotes; drop inline `#`).
pub fn parse_toml_scalar(raw: &str) -> String {
    let s = raw.trim();
    if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
    {
        return s[1..s.len() - 1].to_string();
    }
    s.split('#').next().unwrap_or(s).trim().to_string()
}

fn value_for_key_in_scope<'a>(text: &'a str, table: Option<&str>, key: &str) -> Option<&'a str> {
    let mut current: Option<&str> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((is_array, name)) = parse_table_header(trimmed) {
            // Array tables are not used for simple get helpers.
            current = if is_array { None } else { Some(name) };
            continue;
        }
        let Some(k) = assignment_key(trimmed) else {
            continue;
        };
        if k != key {
            continue;
        }
        let in_scope = match table {
            None => current.is_none(),
            Some(t) => current == Some(t),
        };
        if in_scope {
            let eq = trimmed.find('=')?;
            return Some(trimmed[eq + 1..].trim());
        }
    }
    None
}

/// Read top-level bool when present.
#[allow(dead_code)]
pub fn get_top_level_bool(text: &str, key: &str) -> Option<bool> {
    value_for_key_in_scope(text, None, key).and_then(parse_toml_bool)
}

/// Read `[table].key` bool when present.
pub fn get_table_bool(text: &str, table: &str, key: &str) -> Option<bool> {
    value_for_key_in_scope(text, Some(table), key).and_then(parse_toml_bool)
}

/// Read top-level string / scalar when present.
#[allow(dead_code)]
pub fn get_top_level_string(text: &str, key: &str) -> Option<String> {
    value_for_key_in_scope(text, None, key)
        .map(parse_toml_scalar)
        .filter(|s| !s.is_empty())
}

/// Read `[table].key` string / scalar when present.
pub fn get_table_string(text: &str, table: &str, key: &str) -> Option<String> {
    value_for_key_in_scope(text, Some(table), key)
        .map(parse_toml_scalar)
        .filter(|s| !s.is_empty())
}

// ── Duplicate-key scan / heal ────────────────────────────────────────────────

/// Scope id for duplicate detection.
///
/// - Top-level: `""`
/// - Named table `[ui]`: `"ui"` (all `[ui]` sections share one scope — TOML
///   merges them and rejects duplicate keys across fragments)
/// - Array table instance `[[hooks]]`: `"[[hooks]]#N"` so each element is
///   independent (keys may repeat across elements)
fn scope_id_for_header(
    is_array: bool,
    name: &str,
    array_ordinals: &mut HashMap<String, usize>,
) -> String {
    if is_array {
        let n = array_ordinals.entry(name.to_string()).or_insert(0);
        let id = format!("[[{name}]]#{n}");
        *n += 1;
        id
    } else {
        name.to_string()
    }
}

/// Count duplicate assignments: same key twice in the same table scope.
///
/// Returns `(duplicate_assignment_lines, examples)` where examples are
/// `"scope.key"` strings (capped).
pub fn count_duplicate_assignments(text: &str) -> (usize, Vec<String>) {
    let mut scope = String::new();
    let mut array_ordinals: HashMap<String, usize> = HashMap::new();
    let mut last: HashMap<(String, String), usize> = HashMap::new();
    let mut dups = 0usize;
    let mut examples: Vec<String> = Vec::new();

    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if let Some((is_array, name)) = parse_table_header(trimmed) {
            scope = scope_id_for_header(is_array, name, &mut array_ordinals);
            continue;
        }
        let Some(key) = assignment_key(trimmed) else {
            continue;
        };
        let map_key = (scope.clone(), key.to_string());
        if last.insert(map_key, i).is_some() {
            dups += 1;
            if examples.len() < 8 {
                let label = if scope.is_empty() {
                    key.to_string()
                } else {
                    format!("{scope}.{key}")
                };
                if !examples.contains(&label) {
                    examples.push(label);
                }
            }
        }
    }
    (dups, examples)
}

/// Drop earlier duplicate assignments; keep the **last** occurrence per
/// `(table_scope, key)`. Comments, blanks, headers, and unique keys are
/// preserved in order. Valid configs with no dups return an identical string.
pub fn dedupe_assignment_keys(text: &str) -> (String, usize) {
    let (dup_count, _) = count_duplicate_assignments(text);
    if dup_count == 0 {
        return (text.to_string(), 0);
    }

    // Pass 1: last line index for each (scope, key).
    let mut scope = String::new();
    let mut array_ordinals: HashMap<String, usize> = HashMap::new();
    let mut last: HashMap<(String, String), usize> = HashMap::new();
    let lines: Vec<&str> = text.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if let Some((is_array, name)) = parse_table_header(trimmed) {
            scope = scope_id_for_header(is_array, name, &mut array_ordinals);
            continue;
        }
        if let Some(key) = assignment_key(trimmed) {
            last.insert((scope.clone(), key.to_string()), i);
        }
    }

    // Pass 2: emit lines, skip non-last duplicate assignments.
    scope.clear();
    array_ordinals.clear();
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let mut removed = 0usize;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if let Some((is_array, name)) = parse_table_header(trimmed) {
            scope = scope_id_for_header(is_array, name, &mut array_ordinals);
            out.push(line);
            continue;
        }
        if let Some(key) = assignment_key(trimmed) {
            let map_key = (scope.clone(), key.to_string());
            if last.get(&map_key).copied() != Some(i) {
                removed += 1;
                continue;
            }
        }
        out.push(line);
    }

    let mut joined = out.join("\n");
    if (text.ends_with('\n') || text.is_empty()) && !joined.ends_with('\n') {
        joined.push('\n');
    }
    (joined, removed)
}

/// Result of [`ensure_agent_home_config_sane`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigHealReport {
    /// True when the on-disk file was rewritten.
    pub changed: bool,
    /// Absolute path of agent-home config (if resolved).
    pub path: Option<String>,
    /// Number of duplicate assignment lines removed.
    pub removed_duplicates: usize,
    /// Backup path when a heal write occurred.
    pub backup_path: Option<String>,
    /// Short reason when skipped / no-op.
    pub note: &'static str,
}

impl ConfigHealReport {
    fn skipped(note: &'static str) -> Self {
        Self {
            changed: false,
            path: None,
            removed_duplicates: 0,
            backup_path: None,
            note,
        }
    }
}

/// If independent agent-home `config.toml` has duplicate keys, backup and
/// dedupe (keep last). **Valid files are never written.**
///
/// Shared mode: no-op. Missing / empty file: no-op.
pub fn ensure_agent_home_config_sane(session_data_mode: &str) -> Result<ConfigHealReport, String> {
    if normalize_mode(session_data_mode) == "shared" {
        return Ok(ConfigHealReport::skipped("shared_mode"));
    }

    with_config_write_lock(|| {
        let path = match resolve_writable_config_path(session_data_mode) {
            Ok(p) => p,
            Err(_) => return Ok(ConfigHealReport::skipped("path_refused")),
        };
        if !path.is_file() {
            return Ok(ConfigHealReport {
                changed: false,
                path: Some(path.display().to_string()),
                removed_duplicates: 0,
                backup_path: None,
                note: "missing",
            });
        }
        let existing = fs::read_to_string(&path).unwrap_or_default();
        if existing.trim().is_empty() {
            return Ok(ConfigHealReport {
                changed: false,
                path: Some(path.display().to_string()),
                removed_duplicates: 0,
                backup_path: None,
                note: "empty",
            });
        }

        let (dup_count, examples) = count_duplicate_assignments(&existing);
        if dup_count == 0 {
            return Ok(ConfigHealReport {
                changed: false,
                path: Some(path.display().to_string()),
                removed_duplicates: 0,
                backup_path: None,
                note: "ok",
            });
        }

        let (healed, removed) = dedupe_assignment_keys(&existing);
        if removed == 0 || healed == existing {
            return Ok(ConfigHealReport {
                changed: false,
                path: Some(path.display().to_string()),
                removed_duplicates: 0,
                backup_path: None,
                note: "dedupe_noop",
            });
        }
        let (still, _) = count_duplicate_assignments(&healed);
        if still > 0 {
            return Err(format!(
                "agent-home config.toml still has duplicate keys after heal (examples: {})",
                examples.join(", ")
            ));
        }

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let backup = path.with_file_name(format!("config.toml.bak-heal-{nanos}"));
        fs::copy(&path, &backup).map_err(|e| format!("backup config.toml: {e}"))?;
        fs::write(&path, &healed).map_err(|e| format!("write healed config.toml: {e}"))?;

        tracing::warn!(
            target: "agent_home_config",
            path = %path.display(),
            backup = %backup.display(),
            removed,
            examples = %examples.join(","),
            "healed agent-home config.toml duplicate keys (kept last assignment)"
        );

        Ok(ConfigHealReport {
            changed: true,
            path: Some(path.display().to_string()),
            removed_duplicates: removed,
            backup_path: Some(backup.display().to_string()),
            note: "healed",
        })
    })
}

/// Strict write: read → transform → write agent-home config.toml.
/// Shared mode → `Err` via [`resolve_writable_config_path`].
pub fn update_config_toml(
    session_data_mode: &str,
    transform: impl FnOnce(&str) -> String,
) -> Result<PathBuf, String> {
    with_config_write_lock(|| {
        let path = resolve_writable_config_path(session_data_mode)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("create agent-home: {e}"))?;
        }
        let existing = fs::read_to_string(&path).unwrap_or_default();
        let next = transform(&existing);
        fs::write(&path, next).map_err(|e| format!("write config: {e}"))?;
        Ok(path)
    })
}

/// Soft-skip shared: `Ok(None)` without touching disk.
/// Independent: apply transform and return `Ok(Some(path))`.
///
/// Prefer this for AppSettings sync helpers that keep the toggle in App state
/// when session mode is shared.
pub fn update_config_toml_if_independent(
    session_data_mode: &str,
    transform: impl FnOnce(&str) -> String,
) -> Result<Option<PathBuf>, String> {
    if normalize_mode(session_data_mode) == "shared" {
        return Ok(None);
    }
    update_config_toml(session_data_mode, transform).map(Some)
}

/// Hard cap so a huge non-config file is reported, not replaced.
const MAX_CONFIG_REPAIR_BYTES: u64 = 8 * 1024 * 1024;

/// Empty valid document written only after a backup, when the file does not parse.
const REPAIR_RESET_BODY: &str = "\
# AtlasBuddy repaired this file because it did not parse as TOML.\n\
# The previous contents were copied to a .bak-heal file beside it.\n\
";

/// What Settings → diagnostics shows for the active `config.toml`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigTomlDiagnosis {
    pub path: String,
    pub mode: String,
    /// `ok` | `missing` | `empty` | `duplicate` | `syntax` | `unreadable`
    pub status: String,
    pub duplicate_count: u32,
    pub examples: Vec<String>,
    /// Stable code (`duplicate_keys`, `parse_error`, `not_utf8`, `too_large`) or a short I/O line.
    /// Never the file body.
    pub detail: String,
    pub repairable: bool,
}

/// Result of an explicit diagnostics repair. A no-op leaves the file bytes unchanged.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigTomlRepairResult {
    pub changed: bool,
    pub path: String,
    pub mode: String,
    /// `deduped` | `reset` | `ok` | `missing` | `empty` | `unreadable` | `refused`
    pub action: String,
    pub removed_duplicates: u32,
    pub backup_path: Option<String>,
    pub status: String,
    pub detail: String,
}

/// Diagnose the `config.toml` the active session mode actually reads.
pub fn diagnose_active_config(session_data_mode: &str) -> ConfigTomlDiagnosis {
    let path = crate::agent_config_view::config_toml_path(session_data_mode);
    diagnose_config_file(session_data_mode, &path)
}

/// User-initiated repair of that same file, including shared `~/.atlas/config.toml`.
pub fn repair_active_config(
    session_data_mode: &str,
) -> Result<ConfigTomlRepairResult, String> {
    let path = crate::agent_config_view::config_toml_path(session_data_mode);
    repair_config_file(session_data_mode, &path)
}

/// Diagnose `path` without writing. `session_data_mode` is recorded, not used to pick the path.
pub fn diagnose_config_file(session_data_mode: &str, path: &Path) -> ConfigTomlDiagnosis {
    with_config_write_lock(|| diagnose_config_file_unlocked(session_data_mode, path))
}

/// Repair `path`. Files that already parse are not written. Shared mode is allowed.
pub fn repair_config_file(
    session_data_mode: &str,
    path: &Path,
) -> Result<ConfigTomlRepairResult, String> {
    with_config_write_lock(|| repair_config_file_unlocked(session_data_mode, path))
}

fn diagnose_config_file_unlocked(session_data_mode: &str, path: &Path) -> ConfigTomlDiagnosis {
    let mode = normalize_mode(session_data_mode).to_string();
    let path_s = path.display().to_string();
    let make = |status: &str, dup: u32, examples: Vec<String>, detail: &str, repairable: bool| {
        ConfigTomlDiagnosis {
            path: path_s.clone(),
            mode: mode.clone(),
            status: status.to_string(),
            duplicate_count: dup,
            examples,
            detail: detail.to_string(),
            repairable,
        }
    };

    if !path.is_file() {
        return make("missing", 0, Vec::new(), "", false);
    }
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) => return make("unreadable", 0, Vec::new(), &io_detail(&e), false),
    };
    if meta.len() > MAX_CONFIG_REPAIR_BYTES {
        return make("unreadable", 0, Vec::new(), "too_large", false);
    }
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => return make("unreadable", 0, Vec::new(), &io_detail(&e), false),
    };
    let text = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return make("syntax", 0, Vec::new(), "not_utf8", true),
    };
    if text.trim().is_empty() {
        return make("empty", 0, Vec::new(), "", false);
    }
    if toml_parses(&text) {
        return make("ok", 0, Vec::new(), "", false);
    }
    let (dup, raw_examples) = count_duplicate_assignments(&text);
    let examples = safe_example_keys(&raw_examples);
    let dup_u = u32::try_from(dup).unwrap_or(u32::MAX);
    if dup > 0 {
        let (healed, removed) = dedupe_assignment_keys(&text);
        if removed > 0 && toml_parses(&healed) {
            return make("duplicate", dup_u, examples, "duplicate_keys", true);
        }
    }
    make("syntax", dup_u, examples, "parse_error", true)
}

fn repair_config_file_unlocked(
    session_data_mode: &str,
    path: &Path,
) -> Result<ConfigTomlRepairResult, String> {
    let before = diagnose_config_file_unlocked(session_data_mode, path);
    if !before.repairable {
        let action = if before.detail == "too_large" {
            "refused"
        } else {
            before.status.as_str()
        };
        return Ok(repair_noop(&before, action));
    }

    let bytes = fs::read(path).map_err(|e| format!("read config.toml: {}", io_detail(&e)))?;
    let text = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => {
            let backup = backup_config_file(path)?;
            fs::write(path, REPAIR_RESET_BODY)
                .map_err(|e| format!("write repaired config.toml: {}", io_detail(&e)))?;
            return Ok(finish_repair(
                session_data_mode,
                path,
                "reset",
                0,
                Some(backup),
            ));
        }
    };

    let (dup, _) = count_duplicate_assignments(&text);
    if dup > 0 {
        let (healed, removed) = dedupe_assignment_keys(&text);
        if removed > 0 && healed != text && toml_parses(&healed) {
            let backup = backup_config_file(path)?;
            fs::write(path, &healed)
                .map_err(|e| format!("write healed config.toml: {}", io_detail(&e)))?;
            tracing::warn!(
                target: "agent_home_config",
                path = %path.display(),
                backup = %backup.display(),
                removed,
                "diagnostics repair deduped config.toml (kept last assignment)"
            );
            let removed_u = u32::try_from(removed).unwrap_or(u32::MAX);
            return Ok(finish_repair(
                session_data_mode,
                path,
                "deduped",
                removed_u,
                Some(backup),
            ));
        }
    }

    let backup = backup_config_file(path)?;
    fs::write(path, REPAIR_RESET_BODY)
        .map_err(|e| format!("write repaired config.toml: {}", io_detail(&e)))?;
    tracing::warn!(
        target: "agent_home_config",
        path = %path.display(),
        backup = %backup.display(),
        "diagnostics repair replaced unparseable config.toml with an empty valid file"
    );
    Ok(finish_repair(
        session_data_mode,
        path,
        "reset",
        0,
        Some(backup),
    ))
}

fn repair_noop(before: &ConfigTomlDiagnosis, action: &str) -> ConfigTomlRepairResult {
    ConfigTomlRepairResult {
        changed: false,
        path: before.path.clone(),
        mode: before.mode.clone(),
        action: action.to_string(),
        removed_duplicates: 0,
        backup_path: None,
        status: before.status.clone(),
        detail: before.detail.clone(),
    }
}

fn finish_repair(
    session_data_mode: &str,
    path: &Path,
    action: &str,
    removed: u32,
    backup: Option<PathBuf>,
) -> ConfigTomlRepairResult {
    let after = diagnose_config_file_unlocked(session_data_mode, path);
    ConfigTomlRepairResult {
        changed: true,
        path: path.display().to_string(),
        mode: normalize_mode(session_data_mode).to_string(),
        action: action.to_string(),
        removed_duplicates: removed,
        backup_path: backup.map(|p| p.display().to_string()),
        status: after.status,
        detail: after.detail,
    }
}

fn backup_config_file(path: &Path) -> Result<PathBuf, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let backup = path.with_file_name(format!("config.toml.bak-heal-{stamp}"));
    fs::copy(path, &backup).map_err(|e| format!("backup config.toml: {}", io_detail(&e)))?;
    Ok(backup)
}

fn toml_parses(text: &str) -> bool {
    toml::from_str::<toml::Value>(text).is_ok()
}

fn safe_example_keys(keys: &[String]) -> Vec<String> {
    keys.iter().filter_map(|k| safe_example_key(k)).take(5).collect()
}

fn safe_example_key(key: &str) -> Option<String> {
    let t = key.trim().trim_matches('"').trim_matches('\'');
    if t.is_empty() || t.len() > 48 {
        return None;
    }
    if t.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        Some(t.to_string())
    } else {
        None
    }
}

fn io_detail(err: &std::io::Error) -> String {
    let kind = match err.kind() {
        ErrorKind::NotFound => "not_found",
        ErrorKind::PermissionDenied => "permission_denied",
        ErrorKind::AlreadyExists => "already_exists",
        _ => "",
    };
    if !kind.is_empty() {
        return kind.to_string();
    }
    let line = err.to_string();
    let line = line.lines().next().unwrap_or("io");
    line.chars().take(180).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_app_home(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!(
            "grok-agent-home-cfg-{}-{}-{}",
            label,
            std::process::id(),
            nanos
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn refuse_shared_path_resolve() {
        let err = resolve_writable_config_path("shared").unwrap_err();
        assert!(err.contains("shared"), "{err}");
        assert!(err.contains("refused") || err.contains("~/.grok"), "{err}");
        // Case-insensitive.
        assert!(resolve_writable_config_path("SHARED").is_err());
    }

    /// 表名里带 `]` 的引号名（`[model."claude-glm-5.3[1M]"]`）必须被认成表头，
    /// 否则它的字段会被算进上一张表的 scope，去重时把上一张表的同名赋值删掉。
    #[test]
    fn parse_table_header_quoted_name_with_bracket() {
        assert_eq!(
            parse_table_header(r#"[model."claude-glm-5.3[1M]"]"#),
            Some((false, r#"model."claude-glm-5.3[1M]""#))
        );
        assert_eq!(
            parse_table_header(r#"[model."a[b].c"] # note"#),
            Some((false, r#"model."a[b].c""#))
        );
        // 数组表名同样适用
        assert_eq!(
            parse_table_header(r#"[[model."x[1]"]]"#),
            Some((true, r#"model."x[1]""#))
        );
    }

    /// 别名表排在 provider 表之后时，provider 的传输字段不能被当成重复项删掉。
    #[test]
    fn dedupe_keeps_previous_table_when_next_name_has_bracket() {
        let text = "\
[model.ada-anthropic]
model = \"claude-glm-5.3-flash[1M]\"
base_url = \"http://relay/anthropic/v1\"
api_key = \"sk-test\"
api_backend = \"messages\"

[model.\"claude-glm-5.3[1M]\"]
model = \"claude-glm-5.3[1M]\"
base_url = \"http://relay/anthropic/v1\"
api_key = \"sk-test\"
app_model_for = \"ada-anthropic\"
";
        let (dup, examples) = count_duplicate_assignments(text);
        assert_eq!(dup, 0, "不同表之间不应被判为重复键: {examples:?}");
        let (healed, removed) = dedupe_assignment_keys(text);
        assert_eq!(removed, 0);
        assert_eq!(healed, text);
        assert!(healed.contains("api_backend"));
    }

    #[test]
    fn parse_table_header_trailing_comment() {
        assert_eq!(parse_table_header("[ui]"), Some((false, "ui")));
        assert_eq!(parse_table_header("[ui] # note"), Some((false, "ui")));
        assert_eq!(
            parse_table_header("[compat.claude]"),
            Some((false, "compat.claude"))
        );
        assert_eq!(parse_table_header("[[hooks]]"), Some((true, "hooks")));
        assert_eq!(parse_table_header("[[hooks]] # x"), Some((true, "hooks")));
        assert!(parse_table_header("[ui] junk").is_none());
        assert!(parse_table_header("not a header").is_none());
    }

    #[test]
    fn assignment_key_exact() {
        assert_eq!(assignment_key("yolo = true"), Some("yolo"));
        assert_eq!(assignment_key("yolo_mode = false"), Some("yolo_mode"));
        assert_eq!(assignment_key("# yolo = true"), None);
        assert_eq!(assignment_key("[ui]"), None);
    }

    #[test]
    fn set_and_get_top_level_bool() {
        let t = set_top_level_bool("", "auto_wake_enabled", true);
        assert!(t.contains("auto_wake_enabled = true"));
        assert_eq!(get_top_level_bool(&t, "auto_wake_enabled"), Some(true));

        let existing = "[ui]\nyolo = false\n\n[subagents]\nenabled = true\n";
        let next = set_top_level_bool(existing, "workflows_enabled", false);
        assert_eq!(get_top_level_bool(&next, "workflows_enabled"), Some(false));
        let ui_pos = next.find("[ui]").unwrap();
        let key_pos = next.find("workflows_enabled").unwrap();
        assert!(key_pos < ui_pos);
        assert!(next.contains("yolo = false"));

        let again = set_top_level_bool(&next, "workflows_enabled", true);
        assert_eq!(get_top_level_bool(&again, "workflows_enabled"), Some(true));
        assert_eq!(again.matches("workflows_enabled").count(), 1);
    }

    #[test]
    fn set_table_key_exact_not_prefix() {
        // Historical bug: starts_with("yolo") rewrote yolo_mode and left a
        // second yolo= line → CLI "duplicate key" at spawn.
        let base = "[ui]\npermission_mode = \"always-approve\"\nyolo_mode = false\nyolo = true\n";
        let next = set_table_bool(base, "ui", "yolo", true);
        assert!(next.contains("yolo_mode = false"), "{next}");
        assert_eq!(next.matches("yolo =").count(), 1, "{next}");
        assert!(next.contains("yolo = true"), "{next}");
        assert_eq!(count_duplicate_assignments(&next).0, 0);
    }

    #[test]
    fn set_table_key_matches_header_with_comment() {
        let base = "[ui] # perms\nyolo = false\n";
        let next = set_table_string(base, "ui", "permission_mode", "always-approve");
        // Must update existing [ui] fragment — not append a second [ui].
        assert_eq!(
            next.lines()
                .filter(|l| parse_table_header(l.trim()).is_some_and(|(_, n)| n == "ui"))
                .count(),
            1,
            "{next}"
        );
        assert!(
            next.contains("permission_mode = \"always-approve\""),
            "{next}"
        );
        assert!(next.contains("yolo = false"), "{next}");
        assert_eq!(count_duplicate_assignments(&next).0, 0, "{next}");
    }

    #[test]
    fn ensure_compat_mcp_disabled_sets_both() {
        let t = ensure_compat_mcp_disabled("");
        assert_eq!(get_table_bool(&t, "compat.claude", "mcps"), Some(false));
        assert_eq!(get_table_bool(&t, "compat.cursor", "mcps"), Some(false));
        let again = ensure_compat_mcp_disabled(&t);
        assert_eq!(get_table_bool(&again, "compat.claude", "mcps"), Some(false));
    }

    #[test]
    fn set_and_get_table_bool_and_string() {
        let t = set_table_bool("", "memory", "enabled", true);
        assert!(t.contains("[memory]"));
        assert!(t.contains("enabled = true"));
        assert_eq!(get_table_bool(&t, "memory", "enabled"), Some(true));

        let t2 = set_table_string(&t, "ui", "permission_mode", "acceptEdits");
        assert_eq!(
            get_table_string(&t2, "ui", "permission_mode").as_deref(),
            Some("acceptEdits")
        );
        assert_eq!(get_table_bool(&t2, "memory", "enabled"), Some(true));

        let t3 = set_table_bool(&t2, "memory", "enabled", false);
        assert_eq!(get_table_bool(&t3, "memory", "enabled"), Some(false));
        assert_eq!(t3.matches("enabled =").count(), 1);
    }

    #[test]
    fn top_level_string_roundtrip() {
        let t = set_top_level_string("", "note", "plain");
        assert!(t.contains("note = \"plain\""));
        assert_eq!(get_top_level_string(&t, "note").as_deref(), Some("plain"));
    }

    #[test]
    fn dedupe_keeps_last_and_preserves_unique() {
        let bad = "\
todo_gate_enabled = false
[ui]
permission_mode = \"always-approve\"
yolo = true
yolo = false
[subagents]
enabled = true
";
        assert!(count_duplicate_assignments(bad).0 >= 1);
        let (healed, removed) = dedupe_assignment_keys(bad);
        assert!(removed >= 1);
        assert_eq!(count_duplicate_assignments(&healed).0, 0, "{healed}");
        assert!(healed.contains("yolo = false"), "{healed}");
        assert!(!healed.contains("yolo = true") || healed.matches("yolo =").count() == 1);
        assert!(healed.contains("permission_mode"), "{healed}");
        assert!(healed.contains("[subagents]"), "{healed}");
        // Idempotent on clean text.
        let (again, r2) = dedupe_assignment_keys(&healed);
        assert_eq!(r2, 0);
        assert_eq!(again, healed);
    }

    #[test]
    fn dedupe_does_not_merge_array_table_elements() {
        let text = "\
[[hooks]]
event = \"a\"
command = \"x\"
[[hooks]]
event = \"b\"
command = \"y\"
";
        assert_eq!(count_duplicate_assignments(text).0, 0);
        let (out, removed) = dedupe_assignment_keys(text);
        assert_eq!(removed, 0);
        assert_eq!(out, text);
    }

    #[test]
    fn dedupe_merges_split_ui_tables() {
        // Two [ui] fragments with same key — TOML treats as one table.
        let bad = "[ui]\nyolo = false\n\n[ui]\nyolo = true\npermission_mode = \"x\"\n";
        assert!(count_duplicate_assignments(bad).0 >= 1);
        let (healed, removed) = dedupe_assignment_keys(bad);
        assert!(removed >= 1);
        assert_eq!(count_duplicate_assignments(&healed).0, 0, "{healed}");
        assert!(healed.contains("yolo = true"), "{healed}");
        assert!(healed.contains("permission_mode"), "{healed}");
    }

    #[test]
    fn ensure_sane_noops_on_valid_and_heals_dups() {
        let _guard = crate::paths::APP_HOME_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let home = temp_app_home("heal");
        std::env::set_var("GROK_APP_HOME", &home);
        let agent_home = home.join("agent-home");
        fs::create_dir_all(&agent_home).unwrap();
        let cfg = agent_home.join("config.toml");

        // Valid → no write.
        fs::write(&cfg, "[ui]\nyolo = true\n").unwrap();
        let before = fs::read_to_string(&cfg).unwrap();
        let r = ensure_agent_home_config_sane("independent").unwrap();
        assert!(!r.changed);
        assert_eq!(r.note, "ok");
        assert_eq!(fs::read_to_string(&cfg).unwrap(), before);

        // Dup → heal + backup.
        fs::write(
            &cfg,
            "[ui]\nyolo = true\nyolo = false\npermission_mode = \"always-approve\"\n",
        )
        .unwrap();
        let r2 = ensure_agent_home_config_sane("independent").unwrap();
        assert!(r2.changed);
        assert!(r2.removed_duplicates >= 1);
        assert!(r2.backup_path.is_some());
        let after = fs::read_to_string(&cfg).unwrap();
        assert_eq!(count_duplicate_assignments(&after).0, 0, "{after}");
        assert!(after.contains("yolo = false"), "{after}");

        // Shared → skip.
        let r3 = ensure_agent_home_config_sane("shared").unwrap();
        assert!(!r3.changed);
        assert_eq!(r3.note, "shared_mode");

        std::env::remove_var("GROK_APP_HOME");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn shared_soft_skip_and_independent_write() {
        let _guard = crate::paths::APP_HOME_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let home = temp_app_home("write");
        std::env::set_var("GROK_APP_HOME", &home);

        assert_eq!(
            update_config_toml_if_independent("shared", |t| set_top_level_bool(
                t,
                "auto_wake_enabled",
                true
            ))
            .unwrap(),
            None
        );
        // Strict refuse.
        assert!(update_config_toml("shared", |t| t.to_string()).is_err());

        let path = update_config_toml_if_independent("independent", |t| {
            set_top_level_bool(t, "auto_wake_enabled", true)
        })
        .unwrap()
        .expect("path");
        assert!(path.ends_with("config.toml"));
        let disk = fs::read_to_string(&path).unwrap();
        assert_eq!(get_top_level_bool(&disk, "auto_wake_enabled"), Some(true));

        std::env::remove_var("GROK_APP_HOME");
        let _ = fs::remove_dir_all(&home);
    }

    fn temp_config_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!(
            "grok-config-repair-{}-{}-{}",
            label,
            std::process::id(),
            nanos
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn bak_count(dir: &Path) -> usize {
        fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains("config.toml.bak-heal-")
            })
            .count()
    }

    #[test]
    fn diagnose_repair_dedupes_then_noops() {
        let dir = temp_config_dir("dup");
        let cfg = dir.join("config.toml");
        let original = "\
[ui]
yolo = true
yolo = false
api_key = \"sk-keep-me\"
";
        fs::write(&cfg, original).unwrap();

        let d = diagnose_config_file("shared", &cfg);
        assert_eq!(d.mode, "shared");
        assert_eq!(d.status, "duplicate");
        assert!(d.repairable);
        assert!(d.examples.iter().any(|k| k == "yolo"), "{:?}", d.examples);
        assert!(!d.detail.contains("sk-keep"));

        let r = repair_config_file("shared", &cfg).unwrap();
        assert!(r.changed);
        assert_eq!(r.action, "deduped");
        assert!(r.removed_duplicates >= 1);
        let backup = r.backup_path.expect("backup");
        assert_eq!(fs::read_to_string(&backup).unwrap(), original);
        let after = fs::read_to_string(&cfg).unwrap();
        assert!(after.contains("yolo = false"), "{after}");
        assert!(after.contains("sk-keep-me"), "{after}");
        assert!(toml::from_str::<toml::Value>(&after).is_ok(), "{after}");
        assert_eq!(diagnose_config_file("shared", &cfg).status, "ok");

        let before_bytes = fs::read(&cfg).unwrap();
        let again = repair_config_file("shared", &cfg).unwrap();
        assert!(!again.changed);
        assert!(again.backup_path.is_none());
        assert_eq!(fs::read(&cfg).unwrap(), before_bytes);
        assert_eq!(bak_count(&dir), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn diagnose_repair_resets_unparseable_and_keeps_backup() {
        let dir = temp_config_dir("syntax");
        let cfg = dir.join("config.toml");
        let original = "this is not toml {\napi_key = \"sk-only-in-backup\"\n";
        fs::write(&cfg, original).unwrap();

        let d = diagnose_config_file("independent", &cfg);
        assert_eq!(d.status, "syntax");
        assert!(d.repairable);
        assert!(!d.detail.contains("sk-only"));

        let r = repair_config_file("independent", &cfg).unwrap();
        assert!(r.changed);
        assert_eq!(r.action, "reset");
        let backup = r.backup_path.expect("backup");
        assert_eq!(fs::read_to_string(&backup).unwrap(), original);
        let after = fs::read_to_string(&cfg).unwrap();
        assert!(!after.contains("sk-only-in-backup"), "{after}");
        assert!(toml::from_str::<toml::Value>(&after).is_ok(), "{after}");
        assert_eq!(diagnose_config_file("independent", &cfg).status, "ok");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn diagnose_repair_leaves_valid_and_missing_alone() {
        let dir = temp_config_dir("valid");
        let cfg = dir.join("config.toml");
        let original = "\
[model.\"claude-glm-5.3[1M]\"]
model = \"claude-glm-5.3[1M]\"
api_key = \"sk-test\"
";
        fs::write(&cfg, original).unwrap();
        let before = fs::read(&cfg).unwrap();
        let d = diagnose_config_file("shared", &cfg);
        assert_eq!(d.status, "ok");
        assert!(!d.repairable);
        let r = repair_config_file("shared", &cfg).unwrap();
        assert!(!r.changed);
        assert_eq!(fs::read(&cfg).unwrap(), before);
        assert_eq!(bak_count(&dir), 0);

        let missing = dir.join("nope.toml");
        let m = diagnose_config_file("shared", &missing);
        assert_eq!(m.status, "missing");
        assert!(!m.repairable);
        let mr = repair_config_file("shared", &missing).unwrap();
        assert!(!mr.changed);
        assert!(!missing.exists());

        fs::write(&cfg, "   \n").unwrap();
        assert_eq!(diagnose_config_file("shared", &cfg).status, "empty");
        let er = repair_config_file("shared", &cfg).unwrap();
        assert!(!er.changed);
        assert_eq!(fs::read_to_string(&cfg).unwrap(), "   \n");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn diagnose_repair_resets_invalid_utf8() {
        let dir = temp_config_dir("utf8");
        let cfg = dir.join("config.toml");
        let original = vec![0x61, 0x70, 0x69, 0xFF, 0x00];
        fs::write(&cfg, &original).unwrap();
        let d = diagnose_config_file("shared", &cfg);
        assert_eq!(d.status, "syntax");
        assert_eq!(d.detail, "not_utf8");
        assert!(d.repairable);
        let r = repair_config_file("shared", &cfg).unwrap();
        assert_eq!(r.action, "reset");
        let backup = fs::read(r.backup_path.expect("backup")).unwrap();
        assert_eq!(backup, original);
        assert!(toml::from_str::<toml::Value>(&fs::read_to_string(&cfg).unwrap()).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }
}
