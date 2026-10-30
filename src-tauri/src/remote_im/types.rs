//! Shared types for the in-process Remote IM runtime.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ChannelInstance {
    pub id: String,
    pub channel: String,
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub enabled: bool,
    pub secrets: HashMap<String, String>,
    pub options: Value,
    pub acl: Value,
    pub project_scope: Value,
}

#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub channel: String,
    pub instance_id: String,
    pub message_id: String,
    pub chat_id: String,
    pub chat_type: String, // p2p | group
    pub sender_id: String,
    pub content: String,
    pub mentioned_bot: bool,
    /// Telegram forum / private-topic thread (`message_thread_id`).
    pub thread_id: Option<String>,
    /// Encrypted media to download after the sender passes ACL.
    pub media_refs: Vec<InboundMediaRef>,
}

/// AES key whose `Debug` output never includes the secret.
#[derive(Clone)]
pub struct RedactedSecret(String);

impl RedactedSecret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for RedactedSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RedactedSecret([redacted])")
    }
}

/// Inbound file that still has to be downloaded. `Debug` keeps the host only.
#[derive(Clone)]
pub struct InboundMediaRef {
    pub kind: String,
    pub url: String,
    pub aes_key: RedactedSecret,
    pub filename: Option<String>,
}

impl std::fmt::Debug for InboundMediaRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InboundMediaRef")
            .field("kind", &self.kind)
            .field("host", &media_url_host(&self.url))
            .field("aes_key", &self.aes_key)
            .field("filename", &self.filename)
            .finish()
    }
}

fn media_url_host(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_default()
}

impl IncomingMessage {
    pub fn thread_id(&self) -> Option<&str> {
        self.thread_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedChannel {
    pub channel: String,
    pub instance_id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SessionRecord {
    pub session_id: String,
    pub work_dir: String,
    pub agent_session_id: Option<String>,
    pub turn_count: u32,
}

impl SessionRecord {
    #[allow(dead_code)]
    pub fn new(work_dir: &str) -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            work_dir: work_dir.to_string(),
            agent_session_id: None,
            turn_count: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TrustedProject {
    pub id: String,
    pub name: String,
    pub path: String,
}
