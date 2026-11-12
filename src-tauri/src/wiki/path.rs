//! Project-relative paths for the wiki engine.
//!
//! Pages may only be written under `wiki/*.md`. Reads may use any non-hidden
//! file inside the project. Nothing here touches `~/.atlas`.

use std::fs;
use std::path::{Path, PathBuf};

const MAX_BYTES: u64 = 2_000_000;

pub fn require_project(project: &Path) -> Result<PathBuf, String> {
    if !project.is_absolute() {
        return Err("wiki_project".into());
    }
    let canon = project
        .canonicalize()
        .map_err(|_| "wiki_project".to_string())?;
    if !canon.is_dir() {
        return Err("wiki_project".into());
    }
    Ok(canon)
}

/// Normalize a project-relative path. Rejects escapes and hidden segments.
pub fn normalize_rel(raw: &str) -> Result<String, String> {
    let text = raw.trim().replace('\\', "/");
    if text.is_empty() || text.starts_with('/') || is_windows_abs(&text) {
        return Err("wiki_path".into());
    }
    let mut parts = Vec::new();
    for segment in text.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." || segment.starts_with('.') {
            return Err("wiki_path".into());
        }
        parts.push(segment);
    }
    if parts.is_empty() {
        return Err("wiki_path".into());
    }
    Ok(parts.join("/"))
}

pub fn require_wiki_page(raw: &str) -> Result<String, String> {
    let rel = normalize_rel(raw)?;
    if !rel.starts_with("wiki/") || !rel.ends_with(".md") || rel == "wiki/.md" {
        return Err("wiki_path".into());
    }
    Ok(rel)
}

pub fn resolve_inside(project: &Path, rel: &str) -> Result<PathBuf, String> {
    let project = require_project(project)?;
    let rel = normalize_rel(rel)?;
    let mut cursor = project.clone();
    for segment in rel.split('/') {
        cursor.push(segment);
        if is_symlink(&cursor) {
            return Err("wiki_symlink".into());
        }
    }
    if cursor.exists() {
        let canon = cursor.canonicalize().map_err(|e| format!("wiki_io:{e}"))?;
        if !canon.starts_with(&project) {
            return Err("wiki_path".into());
        }
    }
    Ok(cursor)
}

pub fn read_text(project: &Path, rel: &str) -> Result<String, String> {
    let path = resolve_inside(project, rel)?;
    let meta = fs::metadata(&path).map_err(|_| "wiki_missing".to_string())?;
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return Err("wiki_missing".into());
    }
    fs::read_to_string(&path).map_err(|e| format!("wiki_io:{e}"))
}

/// `canonicalize` on Windows returns `\\?\E:\...`. Settings and the page
/// should show the path a person typed.
pub fn user_path(path: &Path) -> String {
    user_path_string(&path.display().to_string())
}

pub fn user_path_string(raw: &str) -> String {
    let text = raw.trim();
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        if let Some(unc) = rest.strip_prefix(r"UNC\") {
            return format!(r"\\{unc}");
        }
        return rest.to_string();
    }
    if let Some(rest) = text.strip_prefix("//?/") {
        if let Some(unc) = rest.strip_prefix("UNC/") {
            return format!("//{unc}");
        }
        return rest.to_string();
    }
    text.to_string()
}

fn is_windows_abs(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'/' || b[2] == b'\\')
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
}
