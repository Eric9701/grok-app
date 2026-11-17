//! Read-only session collectors. Nothing here starts a CLI or writes `~/.atlas`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::cli_sessions::{percent_decode_component, validate_agent_session_id};
use crate::paths::resolve_agent_grok_home;
use crate::process_util::user_home;
use crate::store;

use super::session_config::{
    self, load_config, load_prompts, session_included, source_enabled, source_home, WikiPrompts,
};

const MAX_FILE_BYTES: u64 = 2_000_000;
const MAX_HITS: usize = 400;
const MESSAGE_CAP: usize = 1_200;
const TRANSCRIPT_CAP: usize = 12_000;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionHit {
    pub runtime: String,
    pub id: String,
    pub title: String,
    pub cwd: String,
    pub updated_at: String,
    pub included: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionSync {
    pub written: Vec<String>,
    pub rollups: Vec<String>,
}

#[derive(Clone)]
struct Roots {
    atlas: PathBuf,
    claude: PathBuf,
    cursor: PathBuf,
    codex: PathBuf,
}

pub fn scan(project: &Path) -> Result<Vec<SessionHit>, String> {
    let config = load_config(project)?;
    let mode = store::load_settings().session_data_mode;
    scan_with(project, &config, &roots_for(&config, &mode))
}

pub fn sync(project: &Path) -> Result<SessionSync, String> {
    let config = load_config(project)?;
    let prompts = load_prompts(project)?;
    let mode = store::load_settings().session_data_mode;
    let roots = roots_for(&config, &mode);
    let hits = scan_with(project, &config, &roots)?;
    materialize(project, &prompts, &hits, &roots)
}

fn roots_for(config: &session_config::SessionIngestConfig, session_data_mode: &str) -> Roots {
    Roots {
        atlas: chosen_home(
            source_home(config, "atlas"),
            resolve_agent_grok_home(session_data_mode),
        ),
        claude: chosen_home(source_home(config, "claude"), user_home().join(".claude")),
        cursor: chosen_home(source_home(config, "cursor"), user_home().join(".cursor")),
        codex: chosen_home(source_home(config, "codex"), user_home().join(".codex")),
    }
}

fn chosen_home(override_home: &str, fallback: PathBuf) -> PathBuf {
    let text = override_home.trim();
    if text.is_empty() {
        return fallback;
    }
    PathBuf::from(text)
}

fn scan_with(
    project: &Path,
    config: &session_config::SessionIngestConfig,
    roots: &Roots,
) -> Result<Vec<SessionHit>, String> {
    let _ = super::path::require_project(project)?;
    let mut hits = Vec::new();
    if source_enabled(config, "atlas") {
        collect_atlas(&roots.atlas, config, &mut hits);
    }
    if source_enabled(config, "claude") {
        collect_jsonl_tree(
            &roots.claude,
            "claude",
            &roots.claude.join("projects"),
            config,
            &mut hits,
        );
    }
    if source_enabled(config, "cursor") {
        collect_cursor(&roots.cursor, config, &mut hits);
    }
    if source_enabled(config, "codex") {
        collect_jsonl_tree(
            &roots.codex,
            "codex",
            &roots.codex.join("sessions"),
            config,
            &mut hits,
        );
    }
    hits.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    hits.truncate(MAX_HITS);
    Ok(hits)
}

fn materialize(
    project: &Path,
    prompts: &WikiPrompts,
    hits: &[SessionHit],
    roots: &Roots,
) -> Result<SessionSync, String> {
    let mut written = Vec::new();
    let mut groups: Vec<(String, String, Vec<String>)> = Vec::new();
    for hit in hits.iter().filter(|hit| hit.included) {
        let Some(root) = canonical_dir(home_for(hit, roots)) else {
            continue;
        };
        let transcript = transcript_for(hit, &root);
        let body = render_digest(prompts, hit, &transcript);
        let rel = format!("raw/sessions/{}/{}.md", hit.runtime, file_stem(&hit.id));
        write_rel(project, &rel, &body)?;
        written.push(rel);
        let slug = project_slug(&hit.cwd);
        if let Some(group) = groups.iter_mut().find(|item| item.0 == slug) {
            group.2.push(body);
        } else {
            groups.push((slug, hit.cwd.clone(), vec![body]));
        }
    }
    let mut rollups = Vec::new();
    for (slug, cwd, bodies) in groups {
        let rel = format!("raw/sessions/projects/{slug}.md");
        let mut page = format!("# {cwd}\n\n");
        page.push_str(&bodies.join("\n\n"));
        if !page.ends_with('\n') {
            page.push('\n');
        }
        write_rel(project, &rel, &page)?;
        rollups.push(rel);
    }
    Ok(SessionSync { written, rollups })
}

fn home_for(hit: &SessionHit, roots: &Roots) -> PathBuf {
    match hit.runtime.as_str() {
        "claude" => roots.claude.clone(),
        "cursor" => roots.cursor.clone(),
        "codex" => roots.codex.clone(),
        _ => roots.atlas.clone(),
    }
}

fn collect_atlas(
    home: &Path,
    config: &session_config::SessionIngestConfig,
    out: &mut Vec<SessionHit>,
) {
    let Some(root) = canonical_dir(home.to_path_buf()) else {
        return;
    };
    let sessions = root.join("sessions");
    if !sessions.is_dir() {
        return;
    }
    let Ok(cwd_dirs) = fs::read_dir(&sessions) else {
        return;
    };
    for cwd_ent in cwd_dirs.flatten() {
        if out.len() >= MAX_HITS {
            return;
        }
        let cwd_path = cwd_ent.path();
        if !cwd_path.is_dir() || is_link(&cwd_path) {
            continue;
        }
        let cwd_fallback = percent_decode_component(
            cwd_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(""),
        );
        let Ok(sid_dirs) = fs::read_dir(&cwd_path) else {
            continue;
        };
        for sid_ent in sid_dirs.flatten() {
            if out.len() >= MAX_HITS {
                return;
            }
            let dir = sid_ent.path();
            if !dir.is_dir() || is_link(&dir) {
                continue;
            }
            let id = dir.file_name().and_then(|name| name.to_str()).unwrap_or("");
            if validate_agent_session_id(id).is_err() || id.starts_with('.') {
                continue;
            }
            let summary_path = dir.join("summary.json");
            let summary = read_inside(&root, &summary_path).unwrap_or_default();
            let parsed: Value = serde_json::from_str(&summary).unwrap_or(Value::Null);
            let kind = parsed
                .get("session_kind")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if kind.starts_with("subagent") {
                continue;
            }
            let title = parsed
                .get("generated_title")
                .and_then(|value| value.as_str())
                .or_else(|| {
                    parsed
                        .get("session_summary")
                        .and_then(|value| value.as_str())
                })
                .unwrap_or("")
                .trim()
                .to_string();
            let has_updates = dir.join("updates.jsonl").is_file();
            if title.is_empty() && !has_updates {
                continue;
            }
            let cwd = parsed
                .pointer("/info/cwd")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
                .unwrap_or(&cwd_fallback)
                .to_string();
            let updated_at = parsed
                .get("last_active_at")
                .or_else(|| parsed.get("updated_at"))
                .or_else(|| parsed.get("created_at"))
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            push_hit(out, config, "atlas", id, &title, &cwd, &updated_at);
        }
    }
}

fn collect_cursor(
    home: &Path,
    config: &session_config::SessionIngestConfig,
    out: &mut Vec<SessionHit>,
) {
    let Some(root) = canonical_dir(home.to_path_buf()) else {
        return;
    };
    let projects = root.join("projects");
    if !projects.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(&projects) else {
        return;
    };
    for entry in entries.flatten() {
        let dir = entry.path().join("agent-transcripts");
        if dir.is_dir() {
            collect_jsonl_tree(&root, "cursor", &dir, config, out);
        }
    }
}

fn collect_jsonl_tree(
    root_home: &Path,
    runtime: &str,
    dir: &Path,
    config: &session_config::SessionIngestConfig,
    out: &mut Vec<SessionHit>,
) {
    let Some(root) = canonical_dir(root_home.to_path_buf()) else {
        return;
    };
    if !dir.exists() {
        return;
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        if out.len() >= MAX_HITS {
            return;
        }
        if is_link(&current) {
            continue;
        }
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            if out.len() >= MAX_HITS {
                return;
            }
            let path = entry.path();
            if is_link(&path) {
                continue;
            }
            if path.is_dir() {
                if !is_subagent_dir(&path) {
                    stack.push(path);
                }
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if validate_agent_session_id(id).is_err() || id.starts_with('.') {
                continue;
            }
            let Some(text) = read_inside(&root, &path).ok() else {
                continue;
            };
            let (title, json_cwd, _transcript) = messages_from_jsonl(&text);
            let cwd = if !json_cwd.is_empty() {
                json_cwd
            } else if runtime == "cursor" {
                cursor_cwd_from_path(&path)
            } else {
                String::new()
            };
            let updated = mtime_rfc3339(&path);
            push_hit(out, config, runtime, id, &title, &cwd, &updated);
        }
    }
}

/// Cursor stores the workspace in `projects/<slug>/agent-transcripts/`, not in the jsonl.
/// `e-GitHub-grok-app` is `E:\GitHub\grok-app`, including hyphens that belong to a folder name.
fn cursor_cwd_from_path(path: &Path) -> String {
    let Some(slug) = cursor_project_slug(path) else {
        return String::new();
    };
    resolve_cursor_slug(&slug)
        .map(|dir| dir.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn cursor_project_slug(path: &Path) -> Option<String> {
    let names: Vec<&str> = path
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    names.windows(3).find_map(|window| {
        if window[0] == "projects" && window[2] == "agent-transcripts" {
            Some(window[1].to_string())
        } else {
            None
        }
    })
}

fn resolve_cursor_slug(slug: &str) -> Option<PathBuf> {
    let (drive, rest) = slug.split_once('-')?;
    let mut letters = drive.chars();
    let letter = letters.next()?;
    if letters.next().is_some() || !letter.is_ascii_alphabetic() {
        return None;
    }
    let root = PathBuf::from(format!("{}:\\", letter.to_ascii_uppercase()));
    if !root.is_dir() {
        return None;
    }
    resolve_slug_rest(&root, rest)
}

fn resolve_slug_rest(dir: &Path, rest: &str) -> Option<PathBuf> {
    if rest.is_empty() {
        return Some(dir.to_path_buf());
    }
    let mut names = Vec::new();
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if is_link(&path) || !path.is_dir() {
            continue;
        }
        if let Some(name) = entry.file_name().to_str() {
            names.push(name.to_string());
        }
    }
    names.sort_by_key(|name| std::cmp::Reverse(name.len()));
    for name in names {
        if rest == name {
            return Some(dir.join(name));
        }
        let prefix = format!("{name}-");
        if let Some(next) = rest.strip_prefix(&prefix) {
            if let Some(found) = resolve_slug_rest(&dir.join(&name), next) {
                return Some(found);
            }
        }
    }
    None
}

fn push_hit(
    out: &mut Vec<SessionHit>,
    config: &session_config::SessionIngestConfig,
    runtime: &str,
    id: &str,
    title: &str,
    cwd: &str,
    updated_at: &str,
) {
    let title = if title.trim().is_empty() {
        id.chars().take(12).collect()
    } else {
        title.trim().chars().take(120).collect()
    };
    out.push(SessionHit {
        included: session_included(config, runtime, id, cwd),
        runtime: runtime.to_string(),
        id: id.to_string(),
        title,
        cwd: cwd.to_string(),
        updated_at: updated_at.to_string(),
    });
}

fn transcript_for(hit: &SessionHit, root: &Path) -> String {
    match hit.runtime.as_str() {
        "atlas" => atlas_transcript(root, &hit.id),
        _ => find_jsonl_transcript(root, hit),
    }
}

fn atlas_transcript(root: &Path, id: &str) -> String {
    let sessions = root.join("sessions");
    let Ok(cwd_dirs) = fs::read_dir(&sessions) else {
        return String::new();
    };
    for cwd_ent in cwd_dirs.flatten() {
        let history = cwd_ent.path().join(id).join("chat_history.jsonl");
        if let Some(text) = read_inside(root, &history).ok() {
            return match crate::cli_sessions::parse_chat_history_text(&text) {
                Ok(rows) => render_transcript(&rows),
                Err(_) => String::new(),
            };
        }
    }
    String::new()
}

fn find_jsonl_transcript(root: &Path, hit: &SessionHit) -> String {
    let start = match hit.runtime.as_str() {
        "claude" => root.join("projects"),
        "cursor" => root.join("projects"),
        "codex" => root.join("sessions"),
        _ => return String::new(),
    };
    let mut stack = vec![start];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if is_link(&path) {
                continue;
            }
            if path.is_dir() {
                if !is_subagent_dir(&path) {
                    stack.push(path);
                }
                continue;
            }
            if path.file_stem().and_then(|name| name.to_str()) == Some(hit.id.as_str()) {
                if let Some(text) = read_inside(root, &path).ok() {
                    let (_title, _cwd, transcript) = messages_from_jsonl(&text);
                    return transcript;
                }
            }
        }
    }
    String::new()
}

fn messages_from_jsonl(raw: &str) -> (String, String, String) {
    let mut rows = Vec::new();
    let mut cwd = String::new();
    let mut title = String::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if cwd.is_empty() {
            if let Some(found) = value.get("cwd").and_then(|item| item.as_str()) {
                cwd = found.to_string();
            }
        }
        if let Some((role, texts)) = role_and_texts(&value) {
            for text in texts {
                if title.is_empty() && is_user_role(&role) {
                    title = describe_user_text(&text);
                }
                push_row(&mut rows, &role, &text);
            }
        }
    }
    (title, cwd, render_transcript(&rows))
}

/// First user turn, preferring the text inside `<user_query>`.
/// Cursor puts the question there, after a timestamp and other wrappers.
fn describe_user_text(text: &str) -> String {
    let source = user_query_body(text).unwrap_or(text);
    let source = redact(source);
    let mut out = String::new();
    let mut gap = false;
    for ch in source.trim().chars() {
        if ch.is_whitespace() {
            gap = !out.is_empty();
            continue;
        }
        if gap {
            out.push(' ');
            gap = false;
        }
        out.push(ch);
        if out.chars().count() >= 80 {
            break;
        }
    }
    out
}

fn user_query_body(text: &str) -> Option<&str> {
    let marker = "<user_query>";
    let start = text.find(marker)? + marker.len();
    let rest = text.get(start..)?;
    let end = rest.find("</user_query>").unwrap_or(rest.len());
    let body = rest[..end].trim();
    if body.is_empty() {
        None
    } else {
        Some(body)
    }
}

fn is_user_role(role: &str) -> bool {
    matches!(role, "user" | "human")
}

fn role_and_texts(value: &Value) -> Option<(String, Vec<String>)> {
    if let Some(message) = value.get("message").filter(|message| message.is_object()) {
        let role = message
            .get("role")
            .or_else(|| value.get("role"))
            .or_else(|| value.get("type"))
            .and_then(|role| role.as_str());
        if let Some(role) = role {
            let texts = texts_from_content(message.get("content").unwrap_or(&Value::Null));
            if !texts.is_empty() || message.get("role").is_some() {
                return Some((role.to_string(), texts));
            }
        }
    }
    if let Some(payload) = value.get("payload") {
        if payload.get("type").and_then(|kind| kind.as_str()) == Some("user_message") {
            if let Some(text) = payload.get("message").and_then(|item| item.as_str()) {
                return Some(("user".into(), vec![text.to_string()]));
            }
        }
        if let Some(role) = payload.get("role").and_then(|role| role.as_str()) {
            return Some((
                role.to_string(),
                texts_from_content(payload.get("content").unwrap_or(&Value::Null)),
            ));
        }
    }
    let role = value
        .get("role")
        .or_else(|| value.get("type"))
        .and_then(|role| role.as_str())?;
    Some((
        role.to_string(),
        texts_from_content(value.get("content").unwrap_or(&Value::Null)),
    ))
}

fn texts_from_content(content: &Value) -> Vec<String> {
    match content {
        Value::String(text) => vec![text.clone()],
        Value::Array(items) => {
            let mut out = Vec::new();
            for item in items {
                if item.is_string() {
                    if let Some(text) = item.as_str() {
                        out.push(text.to_string());
                    }
                    continue;
                }
                let kind = item
                    .get("type")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                if matches!(kind, "tool_use" | "tool_result" | "tool" | "function_call") {
                    continue;
                }
                if let Some(text) = item.get("text").and_then(|value| value.as_str()) {
                    out.push(text.to_string());
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

fn is_subagent_dir(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some("subagents")
}

fn push_row(rows: &mut Vec<(String, String)>, role: &str, text: &str) {
    let role = match role {
        "user" | "human" => "user",
        "assistant" | "agent" => "assistant",
        _ => return,
    };
    let text = if role == "user" {
        user_query_body(text).unwrap_or(text)
    } else {
        text
    };
    let text = redact(&truncate(text.trim(), MESSAGE_CAP));
    if text.is_empty() {
        return;
    }
    rows.push((role.to_string(), text));
}

fn render_transcript(rows: &[(String, String)]) -> String {
    let mut out = String::new();
    for (role, text) in rows {
        let heading = if role == "user" { "User" } else { "Assistant" };
        let block = format!("## {heading}\n{text}\n\n");
        if out.len() + block.len() > TRANSCRIPT_CAP {
            break;
        }
        out.push_str(&block);
    }
    out
}

pub fn render_digest(prompts: &WikiPrompts, hit: &SessionHit, transcript: &str) -> String {
    let template = if prompts.digest.trim().is_empty() {
        "# {{title}}\n\n- runtime: {{runtime}}\n- id: {{id}}\n- cwd: {{cwd}}\n- updated: {{updated}}\n\n{{transcript}}\n"
    } else {
        prompts.digest.as_str()
    };
    let mut text = template.to_string();
    for (key, value) in [
        ("{{title}}", hit.title.as_str()),
        ("{{runtime}}", hit.runtime.as_str()),
        ("{{id}}", hit.id.as_str()),
        ("{{cwd}}", hit.cwd.as_str()),
        ("{{updated}}", hit.updated_at.as_str()),
        ("{{transcript}}", transcript),
    ] {
        text = text.replace(key, value);
    }
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

pub fn redact(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < bytes.len() {
        if let Some(end) = secret_end(bytes, index) {
            out.push_str("[redacted]");
            index = end;
            continue;
        }
        let ch = text[index..].chars().next().unwrap_or('\u{FFFD}');
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

fn secret_end(bytes: &[u8], index: usize) -> Option<usize> {
    let rest = &bytes[index..];
    if rest.starts_with(b"sk-") {
        let end = index + 3 + take_token(&bytes[index + 3..], 16);
        if end - index >= 19 {
            return Some(end);
        }
    }
    if rest.len() >= 7 && rest[..7].eq_ignore_ascii_case(b"bearer ") {
        let end = index + 7 + take_token(&bytes[index + 7..], 8);
        if end - (index + 7) >= 8 {
            return Some(end);
        }
    }
    if rest.starts_with(b"AKIA") {
        let end = index + 4 + take_token(&bytes[index + 4..], 16);
        if end - index >= 20 {
            return Some(end);
        }
    }
    for marker in [b"api_key=" as &[u8], b"api-key=", b"api_key:", b"api-key:"] {
        if rest.len() >= marker.len() && rest[..marker.len()].eq_ignore_ascii_case(marker) {
            let start = index + marker.len();
            let mut end = start;
            while end < bytes.len() && !bytes[end].is_ascii_whitespace() {
                end += 1;
            }
            if end - start >= 6 {
                return Some(end);
            }
        }
    }
    None
}

fn take_token(bytes: &[u8], min: usize) -> usize {
    let mut count = 0;
    for byte in bytes {
        if byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'-' {
            count += 1;
        } else {
            break;
        }
    }
    if count >= min {
        count
    } else {
        0
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

fn file_stem(id: &str) -> String {
    let mut out = String::new();
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            if !out.ends_with('-') {
                out.push('-');
            }
        }
    }
    let out = out.trim_matches('-').chars().take(80).collect::<String>();
    if out.is_empty() {
        "session".into()
    } else {
        out
    }
}

fn project_slug(cwd: &str) -> String {
    let norm = crate::cli_sessions::normalize_cwd_path(cwd);
    let mut raw = String::new();
    for ch in norm.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            raw.push(ch);
        } else if !raw.ends_with('-') {
            raw.push('-');
        }
    }
    let raw = raw.trim_matches('-').chars().take(48).collect::<String>();
    let base = if raw.is_empty() {
        "project".to_string()
    } else {
        raw
    };
    let mut hash = 2_166_136_261u32;
    for byte in norm.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    format!("{base}-{hash:x}")
}

fn write_rel(project: &Path, rel: &str, body: &str) -> Result<(), String> {
    let dest = super::path::resolve_inside(project, rel)?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("wiki_io:{e}"))?;
    }
    fs::write(&dest, body).map_err(|e| format!("wiki_io:{e}"))
}

pub fn read_inside(root: &Path, file: &Path) -> Result<String, String> {
    if !file.exists() {
        return Err("wiki_missing".into());
    }
    if is_link(file) {
        return Err("wiki_symlink".into());
    }
    let canon_root = root.canonicalize().map_err(|_| "wiki_path".to_string())?;
    let canon = file.canonicalize().map_err(|_| "wiki_path".to_string())?;
    if !canon.starts_with(&canon_root) {
        return Err("wiki_path".into());
    }
    let meta = fs::metadata(&canon).map_err(|_| "wiki_missing".to_string())?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return Err("wiki_missing".into());
    }
    fs::read_to_string(&canon).map_err(|e| format!("wiki_io:{e}"))
}

fn canonical_dir(path: PathBuf) -> Option<PathBuf> {
    if is_link(&path) {
        return None;
    }
    let canon = path.canonicalize().ok()?;
    if canon.is_dir() {
        Some(canon)
    } else {
        None
    }
}

fn is_link(path: &Path) -> bool {
    path.symlink_metadata()
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
}

fn mtime_rfc3339(path: &Path) -> String {
    let Ok(meta) = path.metadata() else {
        return String::new();
    };
    let Ok(modified) = meta.modified() else {
        return String::new();
    };
    let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) else {
        return String::new();
    };
    chrono::DateTime::<chrono::Utc>::from_timestamp(duration.as_secs() as i64, 0)
        .map(|stamp| stamp.to_rfc3339())
        .unwrap_or_default()
}

/// True when a raw-relative path is one session file, not a project rollup.
pub fn is_hidden_session_file(rel_under_raw: &str) -> bool {
    let mut parts = rel_under_raw.split('/');
    if parts.next() != Some("sessions") {
        return false;
    }
    !matches!(parts.next(), Some("projects") | None)
}

/// True when a project-relative directory is a per-runtime session folder.
pub fn is_hidden_session_dir(rel: &str) -> bool {
    let mut parts = rel.split('/');
    if parts.next() != Some("raw") || parts.next() != Some("sessions") {
        return false;
    }
    !matches!(parts.next(), Some("projects") | None)
}

#[cfg(test)]
mod tests {
    use super::super::session_config::{default_config, save_config, SessionRef};
    use super::*;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("atlas-wiki-sess-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn redacts_keys_and_drops_tool_blocks() {
        let raw = concat!(
            "{\"type\":\"user\",\"cwd\":\"E:/work\",\"message\":{\"role\":\"user\",\"content\":\"hello sk-abcdefghijklmnopqrstuvwxyz\"}}\n",
            "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"ok\"},{\"type\":\"tool_use\",\"name\":\"bash\",\"input\":{\"command\":\"rm -rf\"}}]}}\n",
        );
        let (title, cwd, transcript) = messages_from_jsonl(raw);
        assert_eq!(cwd, "E:/work");
        assert!(title.starts_with("hello"));
        assert!(transcript.contains("[redacted]"));
        assert!(!transcript.contains("sk-abc"));
        assert!(!transcript.contains("rm -rf"));
        assert!(transcript.contains("ok"));
    }

    #[test]
    fn cursor_title_uses_the_nested_user_query() {
        let prefix = "x".repeat(2000);
        let raw = format!(
            "{{\"role\":\"user\",\"message\":{{\"content\":[{{\"type\":\"text\",\"text\":\"{prefix}<user_query>\\n企业微信附件能够传多大？\\n</user_query>\"}}]}}}}\n"
        );
        let (title, _, _) = messages_from_jsonl(&raw);
        assert_eq!(title, "企业微信附件能够传多大？");

        let short = "{\"role\":\"user\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"<user_query>from cursor</user_query>\"}]}}\n";
        let (title, _, transcript) = messages_from_jsonl(short);
        assert_eq!(title, "from cursor");
        assert!(transcript.contains("from cursor"));
    }

    #[test]
    fn read_inside_rejects_paths_outside_the_home() {
        let home = scratch();
        let outside = scratch();
        fs::write(outside.join("note.txt"), "secret").unwrap();
        let err = read_inside(&home, &outside.join("note.txt")).unwrap_err();
        assert_eq!(err, "wiki_path");
        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn atlas_skips_subagents_and_unchecked_sessions_are_not_written() {
        let project = scratch();
        let home = scratch();
        let parent = home.join("sessions").join("E%3A%5Cwork").join("parent1");
        let child = home.join("sessions").join("E%3A%5Cwork").join("child1");
        fs::create_dir_all(&parent).unwrap();
        fs::create_dir_all(&child).unwrap();
        fs::write(
            parent.join("summary.json"),
            r#"{"generated_title":"Parent","session_kind":"chat","info":{"cwd":"E:/work"},"updated_at":"2026-11-01T00:00:00Z"}"#,
        )
        .unwrap();
        fs::write(parent.join("updates.jsonl"), "{}\n").unwrap();
        fs::write(
            parent.join("chat_history.jsonl"),
            "{\"type\":\"user\",\"content\":\"fix the pump\"}\n{\"type\":\"assistant\",\"content\":\"done\"}\n",
        )
        .unwrap();
        fs::write(
            child.join("summary.json"),
            r#"{"generated_title":"Child","session_kind":"subagent","info":{"cwd":"E:/work"}}"#,
        )
        .unwrap();
        fs::write(child.join("updates.jsonl"), "{}\n").unwrap();
        let mut config = default_config();
        for source in &mut config.sources {
            source.enabled = source.runtime == "atlas";
        }
        save_config(&project, config.clone()).unwrap();
        let roots = Roots {
            atlas: home.clone(),
            claude: scratch(),
            cursor: scratch(),
            codex: scratch(),
        };
        let hits = scan_with(&project, &load_config(&project).unwrap(), &roots).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "parent1");
        assert!(!hits[0].included);
        let synced = materialize(&project, &WikiPrompts::default(), &hits, &roots).unwrap();
        assert!(synced.written.is_empty());
        assert!(synced.rollups.is_empty());
        assert!(!project.join("raw/sessions/atlas/parent1.md").exists());

        let mut selected = load_config(&project).unwrap();
        selected.included.push(SessionRef {
            runtime: "atlas".into(),
            id: "parent1".into(),
        });
        save_config(&project, selected).unwrap();
        let hits = scan_with(&project, &load_config(&project).unwrap(), &roots).unwrap();
        let synced = materialize(&project, &WikiPrompts::default(), &hits, &roots).unwrap();
        assert_eq!(
            synced.written,
            vec!["raw/sessions/atlas/parent1.md".to_string()]
        );
        assert_eq!(synced.rollups.len(), 1);
        let page = fs::read_to_string(project.join(&synced.written[0])).unwrap();
        assert!(page.contains("fix the pump"));
        assert!(page.contains("Parent"));
        assert!(is_hidden_session_file("sessions/atlas/parent1.md"));
        assert!(!is_hidden_session_file(
            synced.rollups[0].strip_prefix("raw/").unwrap_or("")
        ));

        let _ = fs::remove_dir_all(&project);
        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&roots.claude);
        let _ = fs::remove_dir_all(&roots.cursor);
        let _ = fs::remove_dir_all(&roots.codex);
    }

    #[test]
    fn claude_cursor_and_codex_jsonl_are_listed() {
        let project = scratch();
        let claude = scratch();
        let cursor = scratch();
        let codex = scratch();
        let atlas = scratch();
        fs::create_dir_all(claude.join("projects").join("work")).unwrap();
        fs::write(
            claude.join("projects").join("work").join("claude1.jsonl"),
            "{\"type\":\"user\",\"cwd\":\"D:/repo\",\"message\":{\"role\":\"user\",\"content\":\"from claude\"}}\n",
        )
        .unwrap();
        fs::create_dir_all(
            cursor
                .join("projects")
                .join("app")
                .join("agent-transcripts"),
        )
        .unwrap();
        fs::write(
            cursor
                .join("projects")
                .join("app")
                .join("agent-transcripts")
                .join("cursor1.jsonl"),
            "{\"role\":\"user\",\"content\":\"from cursor\"}\n",
        )
        .unwrap();
        fs::create_dir_all(codex.join("sessions").join("2026").join("11")).unwrap();
        fs::write(
            codex
                .join("sessions")
                .join("2026")
                .join("11")
                .join("codex1.jsonl"),
            "{\"payload\":{\"type\":\"user_message\",\"message\":\"from codex\"}}\n",
        )
        .unwrap();
        let config = default_config();
        let roots = Roots {
            atlas,
            claude: claude.clone(),
            cursor: cursor.clone(),
            codex: codex.clone(),
        };
        let hits = scan_with(&project, &config, &roots).unwrap();
        let ids: Vec<_> = hits.iter().map(|hit| hit.id.as_str()).collect();
        assert!(ids.contains(&"claude1"));
        assert!(ids.contains(&"cursor1"));
        assert!(ids.contains(&"codex1"));
        let session_dir = cursor
            .join("projects")
            .join("app")
            .join("agent-transcripts")
            .join("8e0dd081-15d0-4234-a092-b0ab23990bbf");
        fs::create_dir_all(&session_dir).unwrap();
        fs::write(
            session_dir.join("8e0dd081-15d0-4234-a092-b0ab23990bbf.jsonl"),
            concat!(
                "{\"role\":\"user\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"<timestamp>t</timestamp>\\n<user_query>\\n阶段2 通用映射\\n</user_query>\"}]}}\n",
                "{\"role\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"先看现有实现\"},{\"type\":\"tool_use\",\"name\":\"Glob\",\"input\":{\"path\":\"skip-me\"}}]}}\n",
            ),
        )
        .unwrap();
        let sub = session_dir.join("subagents");
        fs::create_dir_all(&sub).unwrap();
        fs::write(
            sub.join("8e0dd081-15d0-4234-a092-b0ab23990bbf.jsonl"),
            "{\"role\":\"user\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"<user_query>subagent-only</user_query>\"}]}}\n",
        )
        .unwrap();
        let hits = scan_with(&project, &config, &roots).unwrap();
        let cursor_ids: Vec<_> = hits
            .iter()
            .filter(|hit| hit.runtime == "cursor")
            .map(|hit| hit.id.as_str())
            .collect();
        assert!(cursor_ids.contains(&"cursor1"));
        assert!(cursor_ids.contains(&"8e0dd081-15d0-4234-a092-b0ab23990bbf"));
        assert_eq!(
            cursor_ids
                .iter()
                .filter(|id| **id == "8e0dd081-15d0-4234-a092-b0ab23990bbf")
                .count(),
            1
        );
        assert!(hits.iter().any(|hit| hit.cwd == "D:/repo"));
        let mut selected = hits;
        for hit in &mut selected {
            hit.included = hit.id == "8e0dd081-15d0-4234-a092-b0ab23990bbf";
        }
        let synced = materialize(&project, &WikiPrompts::default(), &selected, &roots).unwrap();
        assert_eq!(synced.written.len(), 1);
        let page = fs::read_to_string(project.join(&synced.written[0])).unwrap();
        assert!(page.contains("阶段2 通用映射"));
        assert!(page.contains("先看现有实现"));
        assert!(!page.contains("subagent-only"));
        assert!(!page.contains("skip-me"));
        let nested = Path::new(
            r"C:\Users\me\.cursor\projects\e-GitHub-grok-app\agent-transcripts\abc\subagents\child.jsonl",
        );
        assert_eq!(
            cursor_project_slug(nested).as_deref(),
            Some("e-GitHub-grok-app")
        );
        let tree = scratch();
        fs::create_dir_all(tree.join("GitHub").join("grok-app")).unwrap();
        let resolved = resolve_slug_rest(&tree, "GitHub-grok-app").unwrap();
        assert_eq!(resolved, tree.join("GitHub").join("grok-app"));
        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&project);
        let _ = fs::remove_dir_all(&claude);
        let _ = fs::remove_dir_all(&cursor);
        let _ = fs::remove_dir_all(&codex);
        let _ = fs::remove_dir_all(&roots.atlas);
    }
}
