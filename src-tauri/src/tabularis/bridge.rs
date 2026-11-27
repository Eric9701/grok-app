//! Native file dialogs and chat handoff for the embedded Tabularis webview.

use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};

use super::approvals::SEND_TO_CHAT_EVENT;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickRequest {
    pub title: Option<String>,
    pub suggested_name: Option<String>,
}

pub fn pick_file(app: &AppHandle, request: PickRequest) -> Result<Value, String> {
    let path = dialog(app, request.title, |dlg| dlg.pick_file())?;
    Ok(json!({ "path": path }))
}

pub fn save_file(app: &AppHandle, request: PickRequest) -> Result<Value, String> {
    let suggested = request.suggested_name.clone();
    let path = dialog(app, request.title, move |mut dlg| {
        if let Some(name) = suggested.as_deref() {
            dlg = dlg.set_file_name(name);
        }
        dlg.save_file()
    })?;
    Ok(json!({ "path": path }))
}

pub fn pick_dir(app: &AppHandle, request: PickRequest) -> Result<Value, String> {
    let path = dialog(app, request.title, |dlg| dlg.pick_folder())?;
    Ok(json!({ "path": path }))
}

pub fn reveal_path(path: &str) -> Result<Value, String> {
    let target = path.trim();
    if target.is_empty() {
        return Ok(json!({ "ok": true }));
    }
    let path = PathBuf::from(target);
    crate::path_scope::grant_path(&path);
    crate::process_util::reveal_in_file_manager(&path)?;
    Ok(json!({ "ok": true }))
}

pub fn send_to_chat(app: &AppHandle, body: Value) -> Result<Value, String> {
    let _ = app.emit(SEND_TO_CHAT_EVENT, body);
    Ok(json!({ "ok": true }))
}

fn dialog<T>(
    app: &AppHandle,
    title: Option<String>,
    pick: impl FnOnce(rfd::FileDialog) -> Option<T> + Send + 'static,
) -> Result<Option<String>, String>
where
    T: Into<PathBuf> + Send + 'static,
{
    let mut dlg = rfd::FileDialog::new();
    if let Some(title) = title.filter(|value| !value.trim().is_empty()) {
        dlg = dlg.set_title(&title);
    }
    if let Some(window) = app.get_webview_window("main") {
        dlg = dlg.set_parent(&window);
    }
    Ok(pick(dlg).map(|path| {
        let path = path.into();
        crate::path_scope::grant_path(&path);
        path.display().to_string()
    }))
}

/// JavaScript installed in the Tabularis child webview before the page boots.
pub fn host_script(api_url: &str, token: &str) -> String {
    let url = serde_json::to_string(api_url).unwrap_or_else(|_| "\"\"".into());
    let token = serde_json::to_string(token).unwrap_or_else(|_| "\"\"".into());
    format!(
        r#"
(() => {{
  const base = {url};
  const token = {token};
  async function post(path, body) {{
    const response = await fetch(base + path, {{
      method: "POST",
      headers: {{
        "content-type": "application/json",
        "x-tabularis-host-token": token
      }},
      body: JSON.stringify(body || {{}})
    }});
    if (!response.ok) throw new Error("tabularis host " + response.status);
    if (response.status === 204) return null;
    return response.json();
  }}
  window.__TABULARIS_HOST__ = {{
    pickFile: (options) => post("/v1/tabularis/bridge/pick-file", options).then((row) => row && row.path || null),
    saveFile: (options) => post("/v1/tabularis/bridge/save-file", options).then((row) => row && row.path || null),
    pickDir: (options) => post("/v1/tabularis/bridge/pick-dir", options).then((row) => row && row.path || null),
    revealPath: (path) => post("/v1/tabularis/bridge/reveal-path", {{ path }}).then(() => undefined),
    sendToChat: (payload) => post("/v1/tabularis/bridge/send-to-chat", payload).then(() => undefined)
  }};
}})();
"#
    )
}
