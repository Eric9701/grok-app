//! Compile token log for one knowledge-base directory.
//!
//! Stored at `{project}/.atlas-wiki/usage.json`. It does not write `~/.atlas`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::path;

const MAX_ENTRIES: usize = 40;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WikiUsageEntry {
    pub at: String,
    pub source: String,
    pub model_id: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub reported: bool,
}

pub fn list(project: &Path) -> Result<Vec<WikiUsageEntry>, String> {
    let _ = path::require_project(project)?;
    Ok(load(project)?)
}

pub fn append(
    project: &Path,
    source: &str,
    model_id: &str,
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    reported: bool,
) -> Result<Vec<WikiUsageEntry>, String> {
    let source = path::normalize_rel(source)?;
    let model_id: String = model_id
        .chars()
        .filter(|ch| !ch.is_control())
        .take(80)
        .collect();
    let (input_tokens, output_tokens, total_tokens) = if reported {
        (input_tokens, output_tokens, total_tokens)
    } else {
        (0, 0, 0)
    };
    let mut rows = load(project)?;
    rows.insert(
        0,
        WikiUsageEntry {
            at: chrono::Utc::now().to_rfc3339(),
            source,
            model_id,
            input_tokens,
            output_tokens,
            total_tokens,
            reported,
        },
    );
    rows.truncate(MAX_ENTRIES);
    save(project, &rows)?;
    Ok(rows)
}

fn usage_file(project: &Path) -> Result<PathBuf, String> {
    let root = path::require_project(project)?;
    Ok(root.join(".atlas-wiki").join("usage.json"))
}

fn load(project: &Path) -> Result<Vec<WikiUsageEntry>, String> {
    let path = usage_file(project)?;
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("wiki_io:{e}"))?;
    Ok(serde_json::from_str(&text).unwrap_or_default())
}

fn save(project: &Path, rows: &[WikiUsageEntry]) -> Result<(), String> {
    let path = usage_file(project)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("wiki_io:{e}"))?;
    }
    let body = serde_json::to_string_pretty(rows).map_err(|e| format!("wiki_io:{e}"))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, body).map_err(|e| format!("wiki_io:{e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("wiki_io:{e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_keeps_the_newest_row_and_drops_unreported_counts() {
        let dir = std::env::temp_dir().join(format!("atlas-wiki-usage-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let hidden = append(&dir, "raw/sources/note.md", "grok", 9, 9, 9, false).unwrap();
        assert!(!hidden[0].reported);
        assert_eq!(hidden[0].total_tokens, 0);
        let rows = append(&dir, "raw/sources/note.md", "grok", 12, 8, 20, true).unwrap();
        assert_eq!(rows[0].total_tokens, 20);
        assert_eq!(rows[1].reported, false);
        assert!(list(&dir).unwrap()[0].source.ends_with("note.md"));
        let _ = fs::remove_dir_all(&dir);
    }
}
