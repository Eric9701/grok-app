//! Structural lint: broken wikilinks and pages nothing points at.
//! `index.md` and `log.md` are aggregate files, not orphan candidates.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::Serialize;

use super::path::require_project;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiBroken {
    pub path: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiLint {
    pub broken: Vec<WikiBroken>,
    pub orphans: Vec<String>,
}

const RAW_FILE_CAP: usize = 200;

/// Files under `{project}/raw`, as `raw/...` paths. Hidden names and symlinks are skipped.
pub fn list_raw(project: &Path) -> Result<Vec<String>, String> {
    let root = require_project(project)?;
    let raw = root.join("raw");
    let mut files = Vec::new();
    if raw.is_dir() {
        collect_files(&raw, &raw, &mut files)?;
    }
    files.sort();
    files.truncate(RAW_FILE_CAP);
    Ok(files)
}

/// Directories under `raw/` and `wiki/`, including those two roots.
/// Hidden names and symlinks are skipped, so empty template folders still show.
pub fn list_dirs(project: &Path) -> Result<Vec<String>, String> {
    let root = require_project(project)?;
    let mut dirs = Vec::new();
    for top in ["raw", "wiki"] {
        let base = root.join(top);
        if base.is_dir() && !is_link(&base) {
            dirs.push(top.to_string());
            collect_dirs(&base, top, &mut dirs)?;
        }
    }
    dirs.sort();
    Ok(dirs)
}

fn is_link(path: &Path) -> bool {
    path.symlink_metadata()
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
}

fn collect_dirs(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("wiki_io:{e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("wiki_io:{e}"))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name.contains('/') || name.contains('\\') {
            continue;
        }
        let path = entry.path();
        if is_link(&path) || !path.is_dir() {
            continue;
        }
        let rel = format!("{prefix}/{name}");
        out.push(rel.clone());
        collect_dirs(&path, &rel, out)?;
    }
    Ok(())
}

pub fn list_pages(project: &Path) -> Result<Vec<String>, String> {
    let root = require_project(project)?;
    let wiki = root.join("wiki");
    let mut pages = Vec::new();
    if wiki.is_dir() {
        collect(&wiki, &wiki, &mut pages)?;
    }
    Ok(pages.into_iter().map(|rel| format!("wiki/{rel}")).collect())
}

pub fn lint(project: &Path) -> Result<WikiLint, String> {
    let root = require_project(project)?;
    let wiki = root.join("wiki");
    let mut pages = Vec::new();
    if wiki.is_dir() {
        collect(&wiki, &wiki, &mut pages)?;
    }
    let by_rel: BTreeMap<String, ()> = pages.iter().map(|p| (p.clone(), ())).collect();
    let mut inbound: BTreeSet<String> = BTreeSet::new();
    let mut broken = Vec::new();
    for rel in &pages {
        let text = fs::read_to_string(wiki.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR)))
            .unwrap_or_default();
        for target in wikilinks(&text) {
            match resolve(&target, &by_rel) {
                Some(hit) => {
                    inbound.insert(hit);
                }
                None => broken.push(WikiBroken {
                    path: format!("wiki/{rel}"),
                    target,
                }),
            }
        }
    }
    let orphans = pages
        .into_iter()
        .filter(|rel| !is_aggregate(rel) && !inbound.contains(rel))
        .map(|rel| format!("wiki/{rel}"))
        .collect();
    Ok(WikiLint { broken, orphans })
}

fn is_aggregate(rel: &str) -> bool {
    rel == "index.md" || rel == "log.md"
}

fn collect_files(dir: &Path, root: &Path, out: &mut Vec<String>) -> Result<(), String> {
    if out.len() >= RAW_FILE_CAP {
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|e| format!("wiki_io:{e}"))?;
    for entry in entries {
        if out.len() >= RAW_FILE_CAP {
            break;
        }
        let entry = entry.map_err(|e| format!("wiki_io:{e}"))?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if path
            .symlink_metadata()
            .map(|meta| meta.file_type().is_symlink())
            .unwrap_or(false)
        {
            continue;
        }
        if path.is_dir() {
            collect_files(&path, root, out)?;
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|_| "wiki_path".to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if rel.split('/').any(|segment| segment.is_empty() || segment.starts_with('.')) {
            continue;
        }
        out.push(format!("raw/{rel}"));
    }
    Ok(())
}

fn collect(dir: &Path, wiki: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("wiki_io:{e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("wiki_io:{e}"))?;
        let path = entry.path();
        if path
            .symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            continue;
        }
        if path.is_dir() {
            collect(&path, wiki, out)?;
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let rel = path
            .strip_prefix(wiki)
            .map_err(|_| "wiki_path".to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if rel.split('/').any(|s| s.starts_with('.')) {
            continue;
        }
        out.push(rel);
    }
    out.sort();
    Ok(())
}

fn wikilinks(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'[' {
            if let Some(end) = text[i + 2..].find("]]") {
                let raw = &text[i + 2..i + 2 + end];
                let target = raw.split(['|', '#']).next().unwrap_or("").trim();
                if !target.is_empty() {
                    out.push(target.replace('\\', "/"));
                }
                i += 2 + end + 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn resolve(target: &str, pages: &BTreeMap<String, ()>) -> Option<String> {
    let mut t = target.trim().trim_start_matches("./").to_string();
    if t.starts_with("wiki/") {
        t = t.trim_start_matches("wiki/").to_string();
    }
    if !t.ends_with(".md") {
        t.push_str(".md");
    }
    if pages.contains_key(&t) {
        return Some(t);
    }
    if !t.contains('/') {
        let stem = t.trim_end_matches(".md");
        return pages
            .keys()
            .find(|rel| rel.rsplit('/').next().unwrap_or("").trim_end_matches(".md") == stem)
            .cloned();
    }
    None
}
