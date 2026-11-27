//! Pending SQL approvals pushed by the embedded Tabularis process.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter};

pub const APPROVAL_EVENT: &str = "tabularis://approval";
pub const SEND_TO_CHAT_EVENT: &str = "tabularis://send-to-chat";
pub const STATUS_EVENT: &str = "tabularis://status";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalNotice {
    pub id: String,
    pub connection_id: Option<String>,
    pub connection_name: Option<String>,
    pub database: Option<String>,
    pub query: String,
    pub query_kind: Option<String>,
    pub created_at: Option<String>,
}

static PENDING: std::sync::LazyLock<parking_lot::Mutex<Vec<ApprovalNotice>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(Vec::new()));

pub fn record(app: &AppHandle, mut body: Value) -> Result<ApprovalNotice, String> {
    let id = body
        .get("id")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if id.is_empty() {
        return Err("approval id is required".into());
    }
    let notice = ApprovalNotice {
        id: id.clone(),
        connection_id: body.get("connectionId").and_then(|v| v.as_str()).map(str::to_string),
        connection_name: body
            .get("connectionName")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        database: body
            .get("database")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        query: body.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        query_kind: body.get("queryKind").and_then(|v| v.as_str()).map(str::to_string),
        created_at: body.get("createdAt").and_then(|v| v.as_str()).map(str::to_string),
    };
    {
        let mut pending = PENDING.lock();
        pending.retain(|item| item.id != id);
        pending.push(notice.clone());
        if pending.len() > 20 {
            let overflow = pending.len() - 20;
            pending.drain(0..overflow);
        }
    }
    body["pendingCount"] = serde_json::json!(PENDING.lock().len());
    let _ = app.emit(APPROVAL_EVENT, &notice);
    Ok(notice)
}

pub fn list() -> Vec<ApprovalNotice> {
    PENDING.lock().clone()
}

pub fn dismiss(id: &str) {
    PENDING.lock().retain(|item| item.id != id);
}

pub fn count() -> usize {
    PENDING.lock().len()
}
