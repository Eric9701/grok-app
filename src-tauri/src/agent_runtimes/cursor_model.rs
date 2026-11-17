//! Cursor Agent ACP model ids are not the labels in Settings or `agent models`.
//!
//! `session/new` advertises ids such as
//! `grok-4.7[context=256k,reasoning_effort=high,fast=true]` with name `grok-4.7`.
//! `session/set_model` rejects `Grok4.7`, `grok-4.7`, and `grok-4.7-high`.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpAdvertisedModel {
    pub model_id: String,
    pub name: String,
}

pub fn advertised_models_from_result(result: &Value) -> Vec<AcpAdvertisedModel> {
    let Some(models) = result
        .pointer("/models/availableModels")
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };
    models
        .iter()
        .filter_map(|model| {
            let model_id = model
                .get("modelId")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|id| !id.is_empty())?;
            let name = model
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            Some(AcpAdvertisedModel {
                model_id: model_id.to_string(),
                name,
            })
        })
        .collect()
}

/// Map a settings-box model onto a Cursor ACP `modelId`.
///
/// An empty catalog means this session did not advertise models, so the raw
/// text is sent and the RPC can still accept an exact id. A non-empty catalog
/// with no match returns `None` — sending the raw text is what leaves Cursor
/// on Auto.
pub fn resolve_cursor_model(requested: &str, models: &[AcpAdvertisedModel]) -> Option<String> {
    let requested = requested.trim();
    if requested.is_empty() {
        return None;
    }
    if models.is_empty() {
        return Some(requested.to_string());
    }
    if let Some(model) = models.iter().find(|model| model.model_id == requested) {
        return Some(model.model_id.clone());
    }
    let want = model_key(requested);
    if want.is_empty() {
        return None;
    }
    let mut name_hits: Vec<&AcpAdvertisedModel> = models
        .iter()
        .filter(|model| !model.name.is_empty() && model_key(&model.name) == want)
        .collect();
    if name_hits.is_empty() {
        name_hits = models
            .iter()
            .filter(|model| model_key(&model.model_id) == want)
            .collect();
    }
    match name_hits.as_slice() {
        [only] => return Some(only.model_id.clone()),
        [_, ..] => return None,
        [] => {}
    }
    let mut best: Option<&AcpAdvertisedModel> = None;
    let mut best_len = 0usize;
    let mut tied = false;
    for model in models {
        let key = model_key(&model.name);
        if key.is_empty() || key.len() >= want.len() || !want.starts_with(&key) {
            continue;
        }
        if !is_effort_suffix(&want[key.len()..]) {
            continue;
        }
        if key.len() > best_len {
            best = Some(model);
            best_len = key.len();
            tied = false;
        } else if key.len() == best_len {
            tied = true;
        }
    }
    if tied {
        return None;
    }
    best.map(|model| model.model_id.clone())
}

fn model_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn is_effort_suffix(rest: &str) -> bool {
    const TOKENS: [&str; 10] = [
        "xhigh", "extra", "medium", "minimal", "thinking", "high", "fast", "none", "low", "max",
    ];
    let mut rest = rest;
    if rest.is_empty() {
        return false;
    }
    while !rest.is_empty() {
        let Some(token) = TOKENS.iter().find(|token| rest.starts_with(*token)) else {
            return false;
        };
        rest = &rest[token.len()..];
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Vec<AcpAdvertisedModel> {
        advertised_models_from_result(&json!({
            "models": {
                "currentModelId": "default[]",
                "availableModels": [
                    {"modelId": "default[]", "name": "Auto"},
                    {"modelId": "grok-4.7[context=256k,reasoning_effort=high,fast=true]", "name": "grok-4.7"},
                    {"modelId": "grok-4.6[effort=high,fast=true]", "name": "grok-4.6"},
                    {"modelId": "composer-2.5[fast=true]", "name": "composer-2.5"}
                ]
            }
        }))
    }

    #[test]
    fn grok_display_names_resolve_to_the_advertised_id() {
        let models = sample();
        let id = "grok-4.7[context=256k,reasoning_effort=high,fast=true]";
        assert_eq!(
            resolve_cursor_model("Grok4.7", &models).as_deref(),
            Some(id)
        );
        assert_eq!(
            resolve_cursor_model("Grok 4.7", &models).as_deref(),
            Some(id)
        );
        assert_eq!(
            resolve_cursor_model("grok-4.7", &models).as_deref(),
            Some(id)
        );
        assert_eq!(
            resolve_cursor_model("grok-4.7-high", &models).as_deref(),
            Some(id)
        );
        assert_eq!(
            resolve_cursor_model("grok-4.7-high-fast", &models).as_deref(),
            Some(id)
        );
    }

    #[test]
    fn exact_advertised_id_and_auto_still_match() {
        let models = sample();
        assert_eq!(
            resolve_cursor_model(
                "grok-4.7[context=256k,reasoning_effort=high,fast=true]",
                &models
            )
            .as_deref(),
            Some("grok-4.7[context=256k,reasoning_effort=high,fast=true]")
        );
        assert_eq!(
            resolve_cursor_model("Auto", &models).as_deref(),
            Some("default[]")
        );
        assert_eq!(
            resolve_cursor_model("default", &models).as_deref(),
            Some("default[]")
        );
    }

    #[test]
    fn unknown_text_is_not_sent_when_cursor_listed_models() {
        let models = sample();
        assert_eq!(resolve_cursor_model("claude-sonnet", &models), None);
        assert_eq!(resolve_cursor_model("grok", &models), None);
        assert_eq!(resolve_cursor_model("grok-4.7-banana", &models), None);
    }

    #[test]
    fn effort_suffix_prefers_the_longer_model_name() {
        let models = advertised_models_from_result(&json!({
            "models": {
                "availableModels": [
                    {"modelId": "gpt5-id", "name": "gpt-5"},
                    {"modelId": "gpt52-id", "name": "gpt-5.2"}
                ]
            }
        }));
        assert_eq!(
            resolve_cursor_model("gpt-5.2-high", &models).as_deref(),
            Some("gpt52-id")
        );
        assert_eq!(
            resolve_cursor_model("gpt-5-high", &models).as_deref(),
            Some("gpt5-id")
        );
    }

    #[test]
    fn empty_catalog_keeps_the_raw_id() {
        assert_eq!(
            resolve_cursor_model("grok-4.7-high", &[]).as_deref(),
            Some("grok-4.7-high")
        );
    }
}
