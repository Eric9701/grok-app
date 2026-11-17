//! Per-library session ingest config and extraction prompts.
//!
//! Files live in `{library}/.atlas-wiki/`. `wiki_read` cannot see them:
//! project paths reject a segment that starts with `.`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cli_sessions::{normalize_cwd_path, validate_agent_session_id};

pub const RUNTIMES: &[&str] = &["atlas", "claude", "cursor", "codex"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionSource {
    pub runtime: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub home: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionRef {
    pub runtime: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionIngestConfig {
    pub sources: Vec<SessionSource>,
    #[serde(default)]
    pub cwd_prefixes: Vec<String>,
    #[serde(default)]
    pub included: Vec<SessionRef>,
    #[serde(default)]
    pub excluded: Vec<SessionRef>,
    #[serde(default)]
    pub auto_include_new: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct WikiPrompts {
    #[serde(default)]
    pub digest: String,
    #[serde(default)]
    pub analyze: String,
    #[serde(default)]
    pub generate: String,
}

fn default_true() -> bool {
    true
}

pub fn default_config() -> SessionIngestConfig {
    SessionIngestConfig {
        sources: RUNTIMES
            .iter()
            .map(|runtime| SessionSource {
                runtime: (*runtime).to_string(),
                enabled: true,
                home: String::new(),
            })
            .collect(),
        cwd_prefixes: Vec::new(),
        included: Vec::new(),
        excluded: Vec::new(),
        auto_include_new: false,
    }
}

fn config_path(project: &Path) -> Result<PathBuf, String> {
    Ok(super::path::require_project(project)?
        .join(".atlas-wiki")
        .join("session-ingest.json"))
}

fn prompts_path(project: &Path) -> Result<PathBuf, String> {
    Ok(super::path::require_project(project)?
        .join(".atlas-wiki")
        .join("prompts.json"))
}

pub fn load_config(project: &Path) -> Result<SessionIngestConfig, String> {
    let path = config_path(project)?;
    if !path.is_file() {
        return Ok(default_config());
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("wiki_io:{e}"))?;
    let parsed: SessionIngestConfig =
        serde_json::from_str(&text).unwrap_or_else(|_| default_config());
    Ok(normalize_config(parsed)?)
}

pub fn save_config(
    project: &Path,
    config: SessionIngestConfig,
) -> Result<SessionIngestConfig, String> {
    let config = normalize_config(config)?;
    let path = config_path(project)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("wiki_io:{e}"))?;
    }
    let text = serde_json::to_string_pretty(&config).map_err(|e| format!("wiki_io:{e}"))?;
    fs::write(&path, text).map_err(|e| format!("wiki_io:{e}"))?;
    Ok(config)
}

pub fn load_prompts(project: &Path) -> Result<WikiPrompts, String> {
    let path = prompts_path(project)?;
    if !path.is_file() {
        return Ok(WikiPrompts::default());
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("wiki_io:{e}"))?;
    Ok(serde_json::from_str(&text).unwrap_or_default())
}

pub fn save_prompts(project: &Path, prompts: WikiPrompts) -> Result<WikiPrompts, String> {
    let prompts = WikiPrompts {
        digest: prompts.digest,
        analyze: prompts.analyze,
        generate: prompts.generate,
    };
    let path = prompts_path(project)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("wiki_io:{e}"))?;
    }
    let text = serde_json::to_string_pretty(&prompts).map_err(|e| format!("wiki_io:{e}"))?;
    fs::write(&path, text).map_err(|e| format!("wiki_io:{e}"))?;
    Ok(prompts)
}

pub fn normalize_config(config: SessionIngestConfig) -> Result<SessionIngestConfig, String> {
    let mut sources = Vec::new();
    for runtime in RUNTIMES {
        let found = config
            .sources
            .iter()
            .find(|source| source.runtime == *runtime);
        let home = found
            .map(|source| source.home.trim().to_string())
            .unwrap_or_default();
        check_home_text(&home)?;
        sources.push(SessionSource {
            runtime: (*runtime).to_string(),
            enabled: found.map(|source| source.enabled).unwrap_or(true),
            home,
        });
    }
    let mut cwd_prefixes = Vec::new();
    for prefix in config.cwd_prefixes {
        let prefix = prefix.trim().to_string();
        if prefix.is_empty() {
            continue;
        }
        if prefix.contains('\0') || has_dot_dot(&prefix) {
            return Err("wiki_path".into());
        }
        cwd_prefixes.push(prefix);
    }
    Ok(SessionIngestConfig {
        sources,
        cwd_prefixes,
        included: normalize_refs(config.included)?,
        excluded: normalize_refs(config.excluded)?,
        auto_include_new: config.auto_include_new,
    })
}

fn normalize_refs(refs: Vec<SessionRef>) -> Result<Vec<SessionRef>, String> {
    let mut out = Vec::new();
    for reference in refs {
        if !RUNTIMES.contains(&reference.runtime.as_str()) {
            continue;
        }
        let id = validate_agent_session_id(&reference.id).map_err(|_| "wiki_path".to_string())?;
        if id.starts_with('.') {
            return Err("wiki_path".into());
        }
        let runtime = reference.runtime;
        if out
            .iter()
            .any(|item: &SessionRef| item.runtime == runtime && item.id == id)
        {
            continue;
        }
        out.push(SessionRef {
            runtime,
            id: id.to_string(),
        });
    }
    Ok(out)
}

pub fn check_home_text(raw: &str) -> Result<(), String> {
    let text = raw.trim();
    if text.is_empty() {
        return Ok(());
    }
    if text.contains('\0') || has_dot_dot(text) || !Path::new(text).is_absolute() {
        return Err("wiki_path".into());
    }
    Ok(())
}

fn has_dot_dot(text: &str) -> bool {
    text.split(['/', '\\']).any(|segment| segment == "..")
}

pub fn cwd_allowed(config: &SessionIngestConfig, cwd: &str) -> bool {
    let prefixes: Vec<&str> = config
        .cwd_prefixes
        .iter()
        .map(|prefix| prefix.trim())
        .filter(|prefix| !prefix.is_empty())
        .collect();
    if prefixes.is_empty() {
        return true;
    }
    let cwd_key = normalize_cwd_path(cwd);
    if cwd_key.is_empty() {
        return false;
    }
    prefixes.iter().any(|prefix| {
        let key = normalize_cwd_path(prefix);
        !key.is_empty() && (cwd_key == key || cwd_key.starts_with(&(key + "/")))
    })
}

/// Exclusions win. A checked id is included. New sessions join only when that switch is on.
pub fn session_included(config: &SessionIngestConfig, runtime: &str, id: &str, cwd: &str) -> bool {
    if config
        .excluded
        .iter()
        .any(|item| item.runtime == runtime && item.id == id)
    {
        return false;
    }
    if !cwd_allowed(config, cwd) {
        return false;
    }
    if config
        .included
        .iter()
        .any(|item| item.runtime == runtime && item.id == id)
    {
        return true;
    }
    config.auto_include_new
}

pub fn source_enabled(config: &SessionIngestConfig, runtime: &str) -> bool {
    config
        .sources
        .iter()
        .find(|source| source.runtime == runtime)
        .map(|source| source.enabled)
        .unwrap_or(false)
}

pub fn source_home<'a>(config: &'a SessionIngestConfig, runtime: &str) -> &'a str {
    config
        .sources
        .iter()
        .find(|source| source.runtime == runtime)
        .map(|source| source.home.trim())
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("atlas-wiki-cfg-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_files_use_defaults_and_round_trip() {
        let dir = scratch();
        let loaded = load_config(&dir).unwrap();
        assert!(loaded.sources.iter().all(|source| source.enabled));
        assert!(!loaded.auto_include_new);
        assert!(load_prompts(&dir).unwrap().analyze.is_empty());
        let mut next = loaded;
        next.auto_include_new = true;
        next.included.push(SessionRef {
            runtime: "atlas".into(),
            id: "abc".into(),
        });
        save_config(&dir, next).unwrap();
        let again = load_config(&dir).unwrap();
        assert!(again.auto_include_new);
        assert_eq!(again.included.len(), 1);
        save_prompts(
            &dir,
            WikiPrompts {
                digest: "keep {{title}}".into(),
                analyze: String::new(),
                generate: "pages".into(),
            },
        )
        .unwrap();
        assert_eq!(load_prompts(&dir).unwrap().digest, "keep {{title}}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_parent_segments_in_home_and_ids() {
        let mut config = default_config();
        config.sources[0].home = "C:/ok/../secret".into();
        assert_eq!(normalize_config(config).unwrap_err(), "wiki_path");
        let mut config = default_config();
        config.included.push(SessionRef {
            runtime: "claude".into(),
            id: "../x".into(),
        });
        assert_eq!(normalize_config(config).unwrap_err(), "wiki_path");
    }

    #[test]
    fn exclusion_beats_auto_include_and_prefixes() {
        let mut config = default_config();
        config.auto_include_new = true;
        config.cwd_prefixes.push("E:/work".into());
        config.excluded.push(SessionRef {
            runtime: "atlas".into(),
            id: "skip".into(),
        });
        assert!(!session_included(&config, "atlas", "skip", "E:/work/app"));
        assert!(session_included(&config, "atlas", "keep", "E:/work/app"));
        assert!(!session_included(&config, "atlas", "keep", "E:/other"));
        config.auto_include_new = false;
        config.included.push(SessionRef {
            runtime: "atlas".into(),
            id: "keep".into(),
        });
        assert!(session_included(&config, "atlas", "keep", "E:/work/app"));
        assert!(!session_included(&config, "atlas", "other", "E:/work/app"));
    }
}
