//! Read, batch write, and the ingest cache. Writes land only after every
//! page in the batch passes the path gate.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::path::{self, require_project, require_wiki_page};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiOpen {
    pub project: String,
    pub has_schema: bool,
    pub has_purpose: bool,
    pub has_index: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiText {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiPageInput {
    pub path: String,
    pub content: String,
}

pub fn open(project: &Path) -> Result<WikiOpen, String> {
    let root = require_project(project)?;
    Ok(WikiOpen {
        project: root.display().to_string(),
        has_schema: root.join("wiki/schema.md").is_file(),
        has_purpose: root.join("wiki/purpose.md").is_file(),
        has_index: root.join("wiki/index.md").is_file(),
    })
}

pub fn read(project: &Path, rel: &str) -> Result<WikiText, String> {
    let rel = path::normalize_rel(rel)?;
    let content = path::read_text(project, &rel)?;
    Ok(WikiText { path: rel, content })
}

pub fn write_pages(
    project: &Path,
    pages: &[WikiPageInput],
    allow_overwrite: bool,
) -> Result<Vec<String>, String> {
    let root = require_project(project)?;
    if pages.is_empty() {
        return Err("wiki_no_pages".into());
    }
    let mut planned: Vec<(String, PathBuf, String)> = Vec::with_capacity(pages.len());
    let mut seen = BTreeMap::<String, ()>::new();
    for page in pages {
        let rel = require_wiki_page(&page.path)?;
        if seen.insert(rel.clone(), ()).is_some() {
            return Err("wiki_duplicate".into());
        }
        let dest = path::resolve_inside(&root, &rel)?;
        if dest.is_file() && !allow_overwrite {
            return Err("wiki_overwrite".into());
        }
        planned.push((rel, dest, page.content.clone()));
    }
    let mut written = Vec::new();
    for (rel, dest, content) in planned {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("wiki_io:{e}"))?;
        }
        let tmp = dest.with_extension("md.wiki-tmp");
        fs::write(&tmp, content.as_bytes()).map_err(|e| format!("wiki_io:{e}"))?;
        fs::rename(&tmp, &dest).map_err(|e| format!("wiki_io:{e}"))?;
        written.push(rel);
    }
    Ok(written)
}

pub fn remove_page(project: &Path, rel: &str) -> Result<(), String> {
    let root = require_project(project)?;
    let rel = require_wiki_page(rel)?;
    let dest = path::resolve_inside(&root, &rel)?;
    if dest.is_file() {
        fs::remove_file(&dest).map_err(|e| format!("wiki_io:{e}"))?;
    }
    Ok(())
}

pub fn fingerprint(project: &Path, rel: &str) -> Result<String, String> {
    let text = path::read_text(project, &path::normalize_rel(rel)?)?;
    let dig = Sha256::digest(text.as_bytes());
    Ok(hex::encode(dig))
}

fn cache_file(project: &Path) -> Result<PathBuf, String> {
    let root = require_project(project)?;
    Ok(root.join(".atlas-wiki").join("ingest-cache.json"))
}

fn load_cache(project: &Path) -> Result<BTreeMap<String, String>, String> {
    let path = cache_file(project)?;
    if !path.is_file() {
        return Ok(BTreeMap::new());
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("wiki_io:{e}"))?;
    Ok(serde_json::from_str(&text).unwrap_or_default())
}

pub fn cache_get(project: &Path, key: &str) -> Result<Option<String>, String> {
    let key = path::normalize_rel(key)?;
    Ok(load_cache(project)?.get(&key).cloned())
}

pub fn cache_put(project: &Path, key: &str, value: &str) -> Result<(), String> {
    let key = path::normalize_rel(key)?;
    let path = cache_file(project)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("wiki_io:{e}"))?;
    }
    let mut map = load_cache(project)?;
    map.insert(key, value.to_string());
    let body = serde_json::to_string_pretty(&map).map_err(|e| format!("wiki_io:{e}"))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, body).map_err(|e| format!("wiki_io:{e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("wiki_io:{e}"))?;
    Ok(())
}
