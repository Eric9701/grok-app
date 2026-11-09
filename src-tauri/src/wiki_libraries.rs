//! Personal knowledge-base directories stored on AppSettings.
//!
//! The list is app configuration. It does not write `~/.atlas` or `config.toml`.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

const MAX_LIBRARIES: usize = 24;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WikiLibraryRecord {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub agent_default: bool,
}

pub fn normalize_libraries(raw: Vec<WikiLibraryRecord>) -> Vec<WikiLibraryRecord> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut default_taken = false;
    for item in raw {
        if out.len() >= MAX_LIBRARIES {
            break;
        }
        let Some(path) = clean_path(&item.path) else {
            continue;
        };
        if !seen.insert(path_key(&path)) {
            continue;
        }
        let name = clean_name(&item.name, &path);
        let id = clean_id(&item.id);
        let agent_default = item.agent_default && !default_taken;
        if agent_default {
            default_taken = true;
        }
        out.push(WikiLibraryRecord {
            id,
            name,
            path,
            agent_default,
        });
    }
    out
}

/// Short spawn rule for the library marked as the agent default.
pub fn agent_rules(libs: &[WikiLibraryRecord]) -> Option<String> {
    let lib = libs.iter().find(|item| item.agent_default)?;
    let path = lib.path.replace('`', "").trim().to_string();
    if path.is_empty() {
        return None;
    }
    let name = lib.name.replace('`', "").trim().to_string();
    Some(format!(
        "Personal knowledge base: `{path}` ({name}).\nRead and update markdown under `wiki/` in that directory. This is the default knowledge base for this session."
    ))
}

fn clean_path(raw: &str) -> Option<String> {
    let text = raw.trim().replace('\0', "");
    if text.is_empty() || text.contains('\n') || text.contains('\r') {
        return None;
    }
    if text.chars().count() > 1024 {
        return None;
    }
    let path = PathBuf::from(&text);
    if !path.is_absolute() {
        return None;
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return None;
    }
    Some(text)
}

fn clean_name(raw: &str, path: &str) -> String {
    let mut name = raw.replace(['\n', '\r', '\0'], " ");
    name = name.trim().to_string();
    if name.is_empty() {
        name = Path::new(path)
            .file_name()
            .and_then(|part| part.to_str())
            .unwrap_or("wiki")
            .to_string();
    }
    name.chars().take(80).collect()
}

fn clean_id(raw: &str) -> String {
    let id: String = raw.chars().filter(|ch| !ch.is_control()).take(80).collect();
    let id = id.trim().to_string();
    if id.is_empty() {
        Uuid::new_v4().to_string()
    } else {
        id
    }
}

fn path_key(path: &str) -> String {
    let slash = path.trim().replace('\\', "/");
    let slash = slash.trim_end_matches('/');
    if slash.as_bytes().get(1) == Some(&b':') {
        slash.to_lowercase()
    } else {
        slash.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib(path: &str, default: bool) -> WikiLibraryRecord {
        WikiLibraryRecord {
            id: String::new(),
            name: String::new(),
            path: path.into(),
            agent_default: default,
        }
    }

    #[test]
    fn normalize_keeps_one_absolute_default_and_drops_escapes() {
        let rows = normalize_libraries(vec![
            lib("../secret", true),
            lib("notes", true),
            lib(r"E:\notes", true),
            lib("E:/notes", true),
            lib(r"E:\other", true),
        ]);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].agent_default);
        assert!(!rows[1].agent_default);
        assert_eq!(rows[0].path, r"E:\notes");
        assert_eq!(rows[0].name, "notes");
        assert!(!rows[0].id.is_empty());
    }

    #[test]
    fn agent_rules_follow_the_marked_library() {
        let rows = normalize_libraries(vec![lib(r"E:\notes", false), lib(r"E:\other", true)]);
        let text = agent_rules(&rows).expect("default");
        assert!(text.contains(r"E:\other"));
        assert!(!text.contains(r"E:\notes"));
        assert!(agent_rules(&normalize_libraries(vec![lib(r"E:\notes", false)])).is_none());
    }
}
