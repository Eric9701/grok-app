//! ACP entry for `tabularis --mcp`.

use serde_json::{json, Value};

use super::binary;
use super::lifecycle::normalize_mode;

pub fn mcp_server_acp_entry() -> Option<Value> {
    let settings = crate::store::load_settings();
    if !settings.tabularis_mcp_enabled {
        return None;
    }
    let binary = binary::resolve(settings.tabularis_binary_path.as_deref(), None)?;
    let mut env = Vec::new();
    if normalize_mode(settings.tabularis_data_dir_mode.as_deref()) == "isolated" {
        let dir = crate::paths::app_data_root().join("tabularis");
        env.push(json!({
            "name": "TABULARIS_DATA_DIR",
            "value": dir.display().to_string(),
        }));
    }
    Some(json!({
        "name": "tabularis",
        "command": binary.display().to_string(),
        "args": ["--mcp"],
        "env": env,
    }))
}
