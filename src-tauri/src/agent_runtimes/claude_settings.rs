//! Claude Code's selected model, read from `settings.json`.
//!
//! Matches Claude Code 2.1: after settings `env` is applied, the model is
//! `ANTHROPIC_MODEL`, otherwise the `model` field. Aliases `opus` / `sonnet` /
//! `haiku` / `best` / `opusplan` expand through `ANTHROPIC_DEFAULT_*_MODEL`
//! when that env value is present. A `[1m]` suffix is kept.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

pub fn default_config_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())?;
    let home = home.trim();
    if home.is_empty() {
        return None;
    }
    Some(PathBuf::from(home).join(".claude"))
}

pub fn effective_model(config_dir: &Path, cwd: Option<&Path>) -> Option<String> {
    let mut settings = json!({});
    if let Some(user) = read_settings(&config_dir.join("settings.json")) {
        overlay(&mut settings, user);
    }
    if let Some(dir) = cwd {
        let claude = dir.join(".claude");
        if let Some(project) = read_settings(&claude.join("settings.json")) {
            overlay(&mut settings, project);
        }
        if let Some(local) = read_settings(&claude.join("settings.local.json")) {
            overlay(&mut settings, local);
        }
    }
    let raw = text_at(&settings, &["env", "ANTHROPIC_MODEL"])
        .or_else(|| text_at(&settings, &["model"]))?;
    Some(expand_alias(&raw, &settings))
}

fn read_settings(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn overlay(base: &mut Value, over: Value) {
    if !base.is_object() {
        *base = json!({});
    }
    if let Some(model) = over
        .get("model")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        base["model"] = Value::String(model.to_string());
    }
    let Some(env) = over.get("env").and_then(|v| v.as_object()) else {
        return;
    };
    let dest = base
        .as_object_mut()
        .and_then(|obj| obj.entry("env").or_insert_with(|| json!({})).as_object_mut());
    if let Some(dest) = dest {
        for (key, value) in env {
            dest.insert(key.clone(), value.clone());
        }
    }
}

fn text_at(value: &Value, path: &[&str]) -> Option<String> {
    let mut cur = value;
    for key in path {
        cur = cur.get(*key)?;
    }
    cur.as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn expand_alias(raw: &str, settings: &Value) -> String {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    let one_m = lower.ends_with("[1m]");
    let base = if one_m {
        lower[..lower.len() - 4].trim()
    } else {
        lower.as_str()
    };
    let key = match base {
        "opus" | "best" => Some("ANTHROPIC_DEFAULT_OPUS_MODEL"),
        "sonnet" | "opusplan" => Some("ANTHROPIC_DEFAULT_SONNET_MODEL"),
        "haiku" => Some("ANTHROPIC_DEFAULT_HAIKU_MODEL"),
        _ => None,
    };
    if let Some(key) = key {
        if let Some(mapped) = text_at(settings, &["env", key]) {
            return if one_m {
                format!("{mapped}[1m]")
            } else {
                mapped
            };
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn env_model_wins_over_the_model_field() {
        let dir = std::env::temp_dir().join(format!("claude-settings-{}", uuid::Uuid::new_v4()));
        write(
            &dir,
            "settings.json",
            r#"{"model":"opus[1m]","env":{"ANTHROPIC_MODEL":"kimi-for-coding","ANTHROPIC_DEFAULT_OPUS_MODEL":"kimi-k2.5"}}"#,
        );
        assert_eq!(
            effective_model(&dir, None).as_deref(),
            Some("kimi-for-coding")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn model_alias_expands_when_env_model_is_absent() {
        let dir = std::env::temp_dir().join(format!("claude-settings-{}", uuid::Uuid::new_v4()));
        write(
            &dir,
            "settings.json",
            r#"{"model":"opus[1m]","env":{"ANTHROPIC_DEFAULT_OPUS_MODEL":"kimi-k2.5"}}"#,
        );
        assert_eq!(
            effective_model(&dir, None).as_deref(),
            Some("kimi-k2.5[1m]")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unknown_alias_is_left_for_claude() {
        let dir = std::env::temp_dir().join(format!("claude-settings-{}", uuid::Uuid::new_v4()));
        write(&dir, "settings.json", r#"{"model":"opus[1m]"}"#);
        assert_eq!(effective_model(&dir, None).as_deref(), Some("opus[1m]"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn project_local_settings_override_the_user_file() {
        let root = std::env::temp_dir().join(format!("claude-settings-{}", uuid::Uuid::new_v4()));
        let config = root.join("config");
        let project = root.join("project");
        write(
            &config,
            "settings.json",
            r#"{"env":{"ANTHROPIC_MODEL":"user-model"}}"#,
        );
        write(
            &project.join(".claude"),
            "settings.local.json",
            r#"{"env":{"ANTHROPIC_MODEL":"project-model"}}"#,
        );
        assert_eq!(
            effective_model(&config, Some(&project)).as_deref(),
            Some("project-model")
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
