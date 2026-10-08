//! Live **model** catalog for the composer.
//!
//! Official xAI cache (`models_cache.json`) is one source. Atlas enterprise
//! catalog lives in the live CLI home `config.toml` as `[model.*]` (often
//! `managed = true`) and is what `atlas /model` shows — merge those ids here.
//! App-written custom relays under Settings → Providers stay channels.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};

use crate::paths::resolve_agent_grok_home;
use crate::store;

/// Live per-model context windows reported by the agent during `initialize`
/// (ClaudeCode `_meta.modelState.availableModels[].totalContextTokens`).
/// Merged on top of cache-derived windows at read time. Grok CLI does not
/// populate this yet (soft-fail → empty).
static LIVE_CONTEXT_WINDOWS: LazyLock<Mutex<BTreeMap<String, u64>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

/// Merge live context windows discovered during `initialize`.
/// Called from `acp_client` after the handshake. Idempotent; latest wins.
pub fn merge_live_context_windows(windows: HashMap<String, u64>) {
    let mut guard = LIVE_CONTEXT_WINDOWS
        .lock()
        .expect("LIVE_CONTEXT_WINDOWS poisoned");
    for (id, tokens) in windows {
        guard.insert(id, tokens);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningEffort {
    pub id: String,
    pub value: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableModel {
    pub id: String,
    pub label: String,
    /// Always "official" for catalog entries (providers are not models).
    pub source: String,
    #[serde(default)]
    pub is_default: bool,
    /// Per-model reasoning efforts from CLI `info.reasoning_efforts` (may be empty).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasoning_efforts: Vec<ReasoningEffort>,
    /// Model context window in tokens (live-merged from `initialize` first,
    /// then cache `info.totalContextTokens` / `info.context_window`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableModelsResult {
    pub models: Vec<AvailableModel>,
    pub default_model_id: String,
    pub origin: Option<String>,
    pub fetched_at: Option<String>,
}

struct ParsedCacheModel {
    label: String,
    reasoning_efforts: Vec<ReasoningEffort>,
    context_window: Option<u64>,
}

fn user_grok_home() -> PathBuf {
    crate::process_util::user_home().join(".grok")
}

fn shared_cli_home() -> PathBuf {
    crate::paths::shared_cli_home()
}

/// True when `value` looks like At-Rest ENC (do not use as a display label).
fn looks_like_at_rest_enc(value: &str) -> bool {
    let t = value.trim();
    t.starts_with("ENC(") && t.ends_with(')')
}

/// Newest official catalog id used as the empty-cache / preferred default.
pub const OFFICIAL_FALLBACK_MODEL_ID: &str = "grok-4.7";
const OFFICIAL_FALLBACK_MODEL_LABEL: &str = "Grok 4.7";
const OFFICIAL_FAST_MODEL_ID: &str = "grok-4.7-build-fast";
const OFFICIAL_FAST_MODEL_LABEL: &str = "Grok 4.7 Fast";
/// Prefer standard 4.7 over Fast (2× price, not in the free tier), then older ids.
const OFFICIAL_PREFERRED_IDS: &[&str] =
    &["grok-4.7", "grok-4.7-build-fast", "grok-4.6", "grok-4.5"];

fn grok_45_fallback_efforts() -> Vec<ReasoningEffort> {
    vec![
        ReasoningEffort {
            id: "low".into(),
            value: "low".into(),
            label: "Low Effort".into(),
            description: "Quick, fast implementations".into(),
            is_default: false,
        },
        ReasoningEffort {
            id: "medium".into(),
            value: "medium".into(),
            label: "Medium Effort".into(),
            description: "Balanced effort with standard implementation and testing".into(),
            is_default: false,
        },
        ReasoningEffort {
            id: "high".into(),
            value: "high".into(),
            label: "High Effort".into(),
            description: "Higher implementation quality with extensive reasoning".into(),
            is_default: true,
        },
    ]
}

fn insert_fallback_model(
    by_id: &mut BTreeMap<String, AvailableModel>,
    id: &str,
    label: &str,
    efforts: Vec<ReasoningEffort>,
) {
    by_id.insert(
        id.into(),
        AvailableModel {
            id: id.into(),
            label: label.into(),
            source: "official".into(),
            is_default: false,
            reasoning_efforts: efforts,
            context_window: Some(500_000),
        },
    );
}

fn insert_static_fallback(by_id: &mut BTreeMap<String, AvailableModel>) {
    insert_fallback_model(
        by_id,
        OFFICIAL_FALLBACK_MODEL_ID,
        OFFICIAL_FALLBACK_MODEL_LABEL,
        official_fallback_efforts(),
    );
    insert_fallback_model(
        by_id,
        OFFICIAL_FAST_MODEL_ID,
        OFFICIAL_FAST_MODEL_LABEL,
        official_fallback_efforts(),
    );
    insert_fallback_model(by_id, "grok-4.6", "Grok 4.6", official_fallback_efforts());
    insert_fallback_model(by_id, "grok-4.5", "Grok 4.5", grok_45_fallback_efforts());
}

/// True when a models cache file lists `model_id` and it is not hidden.
pub fn cache_file_has_model(path: &Path, model_id: &str) -> bool {
    let id = model_id.trim();
    if id.is_empty() {
        return false;
    }
    let Ok(raw) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    let Some(body) = v.get("models").and_then(|m| m.get(id)) else {
        return false;
    };
    let hidden = body
        .pointer("/info/hidden")
        .and_then(|x| x.as_bool())
        .unwrap_or(false);
    !hidden
}

/// True when the agent home for this session mode, or `~/.grok`, lists `model_id`.
/// Does not call [`crate::store::load_settings`] (that migration calls this).
pub fn official_caches_have_model(session_data_mode: &str, model_id: &str) -> bool {
    let agent_cache =
        crate::paths::resolve_agent_grok_home(session_data_mode).join("models_cache.json");
    if cache_file_has_model(&agent_cache, model_id) {
        return true;
    }
    cache_file_has_model(&user_grok_home().join("models_cache.json"), model_id)
}

fn official_fallback_efforts() -> Vec<ReasoningEffort> {
    vec![
        ReasoningEffort {
            id: "low".into(),
            value: "low".into(),
            label: "Low Effort".into(),
            description: "Quick, fast implementations".into(),
            is_default: false,
        },
        ReasoningEffort {
            id: "medium".into(),
            value: "medium".into(),
            label: "Medium Effort".into(),
            description: "Balanced effort with standard implementation and testing".into(),
            is_default: false,
        },
        ReasoningEffort {
            id: "high".into(),
            value: "high".into(),
            label: "High Effort".into(),
            description: "Higher implementation quality with extensive reasoning".into(),
            is_default: false,
        },
        ReasoningEffort {
            id: "xhigh".into(),
            value: "xhigh".into(),
            label: "Extra High Effort".into(),
            description: "Highest effort and reasoning level".into(),
            is_default: true,
        },
    ]
}

/// CLI grok-4.6 cache marks both `xhigh` and `high` as default. Product
/// default on 4.6 is **xhigh**.
fn normalize_effort_defaults(efforts: &mut [ReasoningEffort]) {
    let default_count = efforts.iter().filter(|e| e.is_default).count();
    if default_count <= 1 {
        return;
    }
    if efforts.iter().any(|e| e.id.eq_ignore_ascii_case("xhigh")) {
        for e in efforts.iter_mut() {
            e.is_default = e.id.eq_ignore_ascii_case("xhigh");
        }
        return;
    }
    if !efforts.iter().any(|e| e.id.eq_ignore_ascii_case("high")) {
        return;
    }
    for e in efforts.iter_mut() {
        e.is_default = e.id.eq_ignore_ascii_case("high");
    }
}

fn preferred_official_model_id(
    by_id: &BTreeMap<String, AvailableModel>,
    settings_model: Option<&str>,
    config_default: Option<&str>,
) -> String {
    if let Some(id) = settings_model.map(str::trim).filter(|s| !s.is_empty()) {
        if by_id.contains_key(id) {
            return id.to_string();
        }
    }
    if let Some(id) = config_default.map(str::trim).filter(|s| !s.is_empty()) {
        if by_id.contains_key(id) {
            return id.to_string();
        }
    }
    for id in OFFICIAL_PREFERRED_IDS {
        if by_id.contains_key(*id) {
            return (*id).to_string();
        }
    }
    by_id
        .keys()
        .next()
        .cloned()
        .unwrap_or_else(|| OFFICIAL_FALLBACK_MODEL_ID.into())
}

/// Atlas / Grok Build catalog ids from `config.toml` `[model.*]`.
///
/// Catalog **id** is the table name. Display uses plaintext `name` (never ENC).
/// Does not copy `api_key` / routing `model` onto the UI struct.
fn parse_config_catalog_models(text: &str) -> BTreeMap<String, ParsedCacheModel> {
    let mut map = BTreeMap::new();
    for section in crate::providers::parse_model_sections_for_proxy(text) {
        let id = section.id.trim();
        if id.is_empty() {
            continue;
        }
        let hidden = section
            .fields
            .get("hidden")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false);
        if hidden {
            continue;
        }
        let label = section
            .fields
            .get("name")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && !looks_like_at_rest_enc(s))
            .unwrap_or(id)
            .to_string();
        let context_window = section
            .fields
            .get("context_window")
            .and_then(|s| s.trim().parse::<u64>().ok());
        map.insert(
            id.to_string(),
            ParsedCacheModel {
                label,
                reasoning_efforts: Vec::new(),
                context_window,
            },
        );
    }
    map
}

fn insert_parsed_models(
    by_id: &mut BTreeMap<String, AvailableModel>,
    map: BTreeMap<String, ParsedCacheModel>,
) {
    for (id, parsed) in map {
        by_id.entry(id.clone()).or_insert(AvailableModel {
            id,
            label: parsed.label,
            source: "official".into(),
            is_default: false,
            reasoning_efforts: parsed.reasoning_efforts,
            context_window: parsed.context_window,
        });
    }
}

/// Parse `/info/reasoning_efforts` from a models_cache entry body.
fn parse_reasoning_efforts(body: &serde_json::Value) -> Vec<ReasoningEffort> {
    let Some(arr) = body
        .pointer("/info/reasoning_efforts")
        .and_then(|x| x.as_array())
    else {
        return Vec::new();
    };
    let mut efforts: Vec<ReasoningEffort> = arr
        .iter()
        .filter_map(|item| {
            let id = item.get("id")?.as_str()?.trim();
            if id.is_empty() {
                return None;
            }
            let value = item
                .get("value")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(id)
                .to_string();
            let label = item
                .get("label")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(id)
                .to_string();
            let description = item
                .get("description")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            // CLI cache uses `"default": true`; host API exposes `isDefault`.
            let is_default = item
                .get("default")
                .and_then(|x| x.as_bool())
                .or_else(|| item.get("isDefault").and_then(|x| x.as_bool()))
                .or_else(|| item.get("is_default").and_then(|x| x.as_bool()))
                .unwrap_or(false);
            Some(ReasoningEffort {
                id: id.to_string(),
                value,
                label,
                description,
                is_default,
            })
        })
        .collect();
    normalize_effort_defaults(&mut efforts);
    efforts
}

#[allow(clippy::type_complexity)]
fn read_models_cache(
    path: &PathBuf,
) -> Option<(
    BTreeMap<String, ParsedCacheModel>,
    Option<String>,
    Option<String>,
)> {
    let raw = fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let models_obj = v.get("models")?.as_object()?;
    let mut map = BTreeMap::new();
    for (id, body) in models_obj {
        if id.trim().is_empty() {
            continue;
        }
        let hidden = body
            .pointer("/info/hidden")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        if hidden {
            continue;
        }
        // Skip entries that look like provider routes (have a custom base_url override
        // without being the official chat-proxy catalog shape). Official cache entries
        // expose info.model / info.name from cli-chat-proxy.
        let label = body
            .pointer("/info/name")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(id)
            .to_string();
        let reasoning_efforts = parse_reasoning_efforts(body);
        let context_window = body
            .pointer("/info/totalContextTokens")
            .and_then(|v| v.as_u64())
            .or_else(|| {
                body.pointer("/info/context_window")
                    .and_then(|v| v.as_u64())
            });
        map.insert(
            id.clone(),
            ParsedCacheModel {
                label,
                reasoning_efforts,
                context_window,
            },
        );
    }
    let origin = v
        .get("origin")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    let fetched_at = v
        .get("fetched_at")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    Some((map, origin, fetched_at))
}

/// Models the user can select in the composer.
///
/// Sources (first id wins, later files only fill gaps):
/// 1. `models_cache.json` under live GROK_HOME, then `~/.atlas`, then `~/.grok`
/// 2. Atlas / Grok Build `[model.*]` in the same homes' `config.toml`
///    (enterprise managed catalog — this is what `atlas /model` lists)
/// 3. Hard fallback `grok-4.6` when both are empty
///
/// App Settings → Providers relays are still channels; they are written only
/// to App `agent-home` and are not the Atlas enterprise list.
pub fn list_available_models() -> AvailableModelsResult {
    let settings = store::load_settings();
    let agent_home = resolve_agent_grok_home(&settings.session_data_mode);
    let atlas_home = shared_cli_home();
    let legacy_home = user_grok_home();

    let mut by_id: BTreeMap<String, AvailableModel> = BTreeMap::new();
    let mut origin = None;
    let mut fetched_at = None;
    let mut config_default = None;

    let mut cache_homes: Vec<PathBuf> = vec![agent_home.clone()];
    if atlas_home != agent_home {
        cache_homes.push(atlas_home.clone());
    }
    if legacy_home != agent_home && legacy_home != atlas_home {
        cache_homes.push(legacy_home.clone());
    }

    for home in &cache_homes {
        let cache = home.join("models_cache.json");
        if let Some((map, o, f)) = read_models_cache(&cache) {
            if origin.is_none() {
                origin = o;
            }
            if fetched_at.is_none() {
                fetched_at = f;
            }
            insert_parsed_models(&mut by_id, map);
        }
    }

    for home in &cache_homes {
        let cfg_path = home.join("config.toml");
        let Ok(text) = fs::read_to_string(&cfg_path) else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        if config_default.is_none() {
            config_default = crate::providers::get_models_default(&text);
        }
        insert_parsed_models(&mut by_id, parse_config_catalog_models(&text));
    }

    // Hard fallback — known official ids when cache + Atlas catalog empty.
    // Live CLI cache replaces this list, including Fast when the CLI ships it.
    if by_id.is_empty() {
        insert_static_fallback(&mut by_id);
    }

    // Overlay live windows discovered during `initialize` (ClaudeCode).
    // Live values win over cache-derived windows; absent entries stay as-is.
    {
        let live = LIVE_CONTEXT_WINDOWS
            .lock()
            .expect("LIVE_CONTEXT_WINDOWS poisoned");
        for (id, tokens) in live.iter() {
            if let Some(m) = by_id.get_mut(id) {
                m.context_window = Some(*tokens);
            }
        }
    }

    // Settings model id, then CLI `[models].default`, then newest official (4.6 > 4.5).
    let preferred = preferred_official_model_id(
        &by_id,
        settings.model_id.as_deref(),
        config_default.as_deref(),
    );

    let mut models: Vec<AvailableModel> = by_id.into_values().collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    for m in &mut models {
        m.is_default = m.id == preferred;
    }

    AvailableModelsResult {
        models,
        default_model_id: preferred,
        origin,
        fetched_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_cache_parses_official_entry() {
        let dir = std::env::temp_dir().join(format!("grok-app-models-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("models_cache.json");
        fs::write(
            &path,
            r#"{
              "fetched_at": "2026-07-23T00:00:00Z",
              "origin": "https://cli-chat-proxy.grok.com/v1/models",
              "models": {
                "grok-4.5": {
                  "info": { "id": "grok-4.5", "name": "Grok 4.5", "hidden": false }
                }
              }
            }"#,
        )
        .unwrap();
        let (map, origin, _) = read_models_cache(&path).expect("cache");
        assert_eq!(
            map.get("grok-4.5").map(|m| m.label.as_str()),
            Some("Grok 4.5")
        );
        assert!(map
            .get("grok-4.5")
            .map(|m| m.reasoning_efforts.is_empty())
            .unwrap_or(false));
        assert!(origin.unwrap().contains("cli-chat-proxy"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_reasoning_efforts_from_cache_pointer() {
        let body: serde_json::Value = serde_json::from_str(
            r#"{
              "info": {
                "reasoning_efforts": [
                  {
                    "id": "high",
                    "value": "high",
                    "label": "High Effort",
                    "description": "Highest quality",
                    "default": true
                  },
                  {
                    "id": "medium",
                    "value": "medium",
                    "label": "Medium Effort",
                    "description": "Balanced",
                    "default": false
                  },
                  {
                    "id": "low",
                    "value": "low",
                    "label": "Low Effort",
                    "description": "Quick",
                    "default": false
                  }
                ]
              }
            }"#,
        )
        .unwrap();
        let efforts = parse_reasoning_efforts(&body);
        assert_eq!(efforts.len(), 3);
        assert_eq!(efforts[0].id, "high");
        assert_eq!(efforts[0].value, "high");
        assert_eq!(efforts[0].label, "High Effort");
        assert_eq!(efforts[0].description, "Highest quality");
        assert!(efforts[0].is_default);
        assert!(!efforts[1].is_default);
        assert_eq!(efforts[2].id, "low");
    }

    #[test]
    fn parse_reasoning_efforts_collapses_dual_default_to_xhigh() {
        let body: serde_json::Value = serde_json::from_str(
            r#"{
              "info": {
                "reasoning_efforts": [
                  {
                    "id": "xhigh",
                    "value": "xhigh",
                    "label": "Extra High Effort",
                    "default": true
                  },
                  {
                    "id": "high",
                    "value": "high",
                    "label": "High Effort",
                    "default": true
                  },
                  {
                    "id": "medium",
                    "value": "medium",
                    "label": "Medium Effort",
                    "default": false
                  }
                ]
              }
            }"#,
        )
        .unwrap();
        let efforts = parse_reasoning_efforts(&body);
        assert_eq!(efforts.len(), 3);
        assert_eq!(efforts[0].id, "xhigh");
        assert!(efforts[0].is_default);
        assert_eq!(efforts[1].id, "high");
        assert!(!efforts[1].is_default);
        assert!(!efforts[2].is_default);
    }

    #[test]
    fn parse_reasoning_efforts_skips_empty_id() {
        let body: serde_json::Value = serde_json::from_str(
            r#"{
              "info": {
                "reasoning_efforts": [
                  { "id": "", "value": "x", "label": "X" },
                  { "id": "medium", "label": "Med" }
                ]
              }
            }"#,
        )
        .unwrap();
        let efforts = parse_reasoning_efforts(&body);
        assert_eq!(efforts.len(), 1);
        assert_eq!(efforts[0].id, "medium");
        assert_eq!(efforts[0].value, "medium");
        assert_eq!(efforts[0].label, "Med");
    }

    #[test]
    fn read_cache_includes_reasoning_efforts() {
        let dir =
            std::env::temp_dir().join(format!("grok-app-models-efforts-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("models_cache.json");
        fs::write(
            &path,
            r#"{
              "fetched_at": "2026-07-25T00:00:00Z",
              "origin": "https://cli-chat-proxy.grok.com/v1/models",
              "models": {
                "grok-4.5": {
                  "info": {
                    "id": "grok-4.5",
                    "name": "Grok 4.5",
                    "hidden": false,
                    "reasoning_efforts": [
                      {
                        "id": "high",
                        "value": "high",
                        "label": "High Effort",
                        "description": "Deep",
                        "default": true
                      }
                    ]
                  }
                }
              }
            }"#,
        )
        .unwrap();
        let (map, _, _) = read_models_cache(&path).expect("cache");
        let m = map.get("grok-4.5").expect("model");
        assert_eq!(m.reasoning_efforts.len(), 1);
        assert_eq!(m.reasoning_efforts[0].id, "high");
        assert!(m.reasoning_efforts[0].is_default);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_cache_parses_context_window_total_tokens() {
        let dir = std::env::temp_dir().join(format!(
            "grok-app-models-cw-total-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("models_cache.json");
        fs::write(
            &path,
            r#"{
              "models": {
                "grok-4.5": {
                  "info": { "name": "Grok 4.5", "totalContextTokens": 256000 }
                },
                "grok-4": {
                  "info": { "name": "Grok 4", "context_window": 128000 }
                },
                "grok-mini": {
                  "info": { "name": "Grok Mini" }
                }
              }
            }"#,
        )
        .unwrap();
        let (map, _, _) = read_models_cache(&path).expect("cache");
        assert_eq!(
            map.get("grok-4.5").and_then(|m| m.context_window),
            Some(256000)
        );
        assert_eq!(
            map.get("grok-4").and_then(|m| m.context_window),
            Some(128000)
        );
        assert_eq!(map.get("grok-mini").and_then(|m| m.context_window), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_config_catalog_uses_table_id_and_plaintext_name() {
        let text = r#"
[models]
default = "arch-kimi-for-coding"

[model.arch-kimi-for-coding]
name = "架构组kimi coding"
context_window = 200000
managed = true

[model."arch-glm-5.2"]
name = "arch-glm-5.2"
context_window = 200000
managed = true

[model.hidden-one]
name = "nope"
hidden = true

[model.enc-name]
name = "ENC(not-a-label)"
"#;
        let map = parse_config_catalog_models(text);
        assert_eq!(
            map.get("arch-kimi-for-coding").map(|m| m.label.as_str()),
            Some("架构组kimi coding")
        );
        assert_eq!(
            map.get("arch-kimi-for-coding").and_then(|m| m.context_window),
            Some(200000)
        );
        assert_eq!(
            map.get("arch-glm-5.2").map(|m| m.label.as_str()),
            Some("arch-glm-5.2")
        );
        assert!(!map.contains_key("hidden-one"));
        assert_eq!(
            map.get("enc-name").map(|m| m.label.as_str()),
            Some("enc-name")
        );
        assert_eq!(
            crate::providers::get_models_default(text).as_deref(),
            Some("arch-kimi-for-coding")
        );
    }

    #[test]
    fn preferred_id_uses_config_default_when_settings_unknown() {
        let mut by_id = BTreeMap::new();
        by_id.insert(
            "arch-kimi-for-coding".into(),
            AvailableModel {
                id: "arch-kimi-for-coding".into(),
                label: "kimi".into(),
                source: "official".into(),
                is_default: false,
                reasoning_efforts: vec![],
                context_window: None,
            },
        );
        let pick = preferred_official_model_id(
            &by_id,
            Some("grok-4.6"),
            Some("arch-kimi-for-coding"),
        );
        assert_eq!(pick, "arch-kimi-for-coding");
    }

    #[test]
    fn merge_live_context_windows_inserts_without_panic() {
        // Use a unique id to avoid interfering with other tests / list_available_models.
        let unique = format!(
            "test-merge-live-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let mut windows = HashMap::new();
        windows.insert(unique.clone(), 999999);
        merge_live_context_windows(windows);

        let guard = LIVE_CONTEXT_WINDOWS.lock().expect("not poisoned");
        assert_eq!(guard.get(&unique), Some(&999999));
    }

    fn stub_model(id: &str) -> AvailableModel {
        AvailableModel {
            id: id.into(),
            label: id.into(),
            source: "official".into(),
            is_default: false,
            reasoning_efforts: Vec::new(),
            context_window: Some(500_000),
        }
    }

    #[test]
    fn preferred_official_picks_47_before_fast() {
        let mut by_id = BTreeMap::new();
        for id in ["grok-4.5", "grok-4.6", "grok-4.7", "grok-4.7-build-fast"] {
            by_id.insert(id.to_string(), stub_model(id));
        }
        assert_eq!(preferred_official_model_id(&by_id, None, None), "grok-4.7");
        assert_eq!(
            preferred_official_model_id(&by_id, Some("grok-4.7-build-fast"), None),
            "grok-4.7-build-fast"
        );
        assert_eq!(
            preferred_official_model_id(&by_id, Some("grok-4.6"), None),
            "grok-4.6"
        );
        assert_eq!(
            preferred_official_model_id(&by_id, Some("not-in-cache"), None),
            "grok-4.7"
        );
    }

    #[test]
    fn cache_file_has_model_skips_hidden() {
        let dir = std::env::temp_dir().join(format!(
            "grok-app-models-has-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("models_cache.json");
        fs::write(
            &path,
            r#"{
              "models": {
                "grok-4.7": { "info": { "hidden": false } },
                "grok-4.7-build-fast": { "info": { "hidden": true } }
              }
            }"#,
        )
        .unwrap();
        assert!(cache_file_has_model(&path, "grok-4.7"));
        assert!(!cache_file_has_model(&path, "grok-4.7-build-fast"));
        assert!(!cache_file_has_model(&path, "grok-4.6"));
        let _ = fs::remove_dir_all(&dir);
    }
}
