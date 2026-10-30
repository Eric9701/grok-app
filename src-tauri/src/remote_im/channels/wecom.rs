//! WeCom — WebSocket aibot mode + Webhook callback HTTP.

use super::super::outbound::{http_client, secret_or_opt};
use super::super::types::{ChannelInstance, IncomingMessage};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Official long-connection endpoint. Credentials go in `aibot_subscribe`, not the query string.
const WECOM_WS_URL: &str = "wss://openws.work.weixin.qq.com";
const WECOM_SUPERSEDED: &str = "wecom ws: superseded";
const WECOM_HEARTBEAT: Duration = Duration::from_secs(30);

struct WecomWsReply {
    req_id: String,
    text: String,
}

struct WecomSinkSlot {
    generation: u64,
    tx: mpsc::Sender<WecomWsReply>,
}

static WECOM_SINK_GEN: AtomicU64 = AtomicU64::new(1);
static WECOM_WS_SINKS: LazyLock<Mutex<HashMap<String, WecomSinkSlot>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

struct WecomSinkGuard {
    instance_id: String,
    generation: u64,
}

impl Drop for WecomSinkGuard {
    fn drop(&mut self) {
        let Ok(mut map) = WECOM_WS_SINKS.lock() else {
            return;
        };
        if map
            .get(&self.instance_id)
            .is_some_and(|slot| slot.generation == self.generation)
        {
            map.remove(&self.instance_id);
        }
    }
}

fn register_wecom_sink(instance_id: &str, tx: mpsc::Sender<WecomWsReply>) -> WecomSinkGuard {
    let generation = WECOM_SINK_GEN.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut map) = WECOM_WS_SINKS.lock() {
        map.insert(instance_id.to_string(), WecomSinkSlot { generation, tx });
    }
    WecomSinkGuard {
        instance_id: instance_id.to_string(),
        generation,
    }
}

fn enqueue_wecom_ws_reply(instance_id: &str, req_id: &str, text: &str) -> Result<(), String> {
    let tx = WECOM_WS_SINKS
        .lock()
        .map_err(|_| "wecom ws: reply queue locked".to_string())?
        .get(instance_id)
        .map(|slot| slot.tx.clone())
        .ok_or_else(|| "wecom ws: long connection is not authenticated".to_string())?;
    tx.try_send(WecomWsReply {
        req_id: req_id.to_string(),
        text: text.to_string(),
    })
    .map_err(|_| "wecom ws: reply queue unavailable".to_string())
}

fn wecom_subscribe_frame(bot_id: &str, secret: &str, req_id: &str) -> Value {
    json!({
        "cmd": "aibot_subscribe",
        "headers": { "req_id": req_id },
        "body": { "bot_id": bot_id, "secret": secret }
    })
}

fn wecom_ping_frame(req_id: &str) -> Value {
    json!({
        "cmd": "ping",
        "headers": { "req_id": req_id }
    })
}

fn wecom_reply_frame(req_id: &str, stream_id: &str, text: &str) -> Value {
    json!({
        "cmd": "aibot_respond_msg",
        "headers": { "req_id": req_id },
        "body": {
            "msgtype": "stream",
            "stream": { "id": stream_id, "content": text, "finish": true }
        }
    })
}

fn ws_req_id(v: &Value) -> &str {
    v.pointer("/headers/req_id")
        .or_else(|| v.get("req_id"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
}

fn ws_errcode(v: &Value) -> i64 {
    v.get("errcode").and_then(|x| x.as_i64()).unwrap_or(0)
}

pub async fn run(
    inst: ChannelInstance,
    tx: mpsc::Sender<IncomingMessage>,
    cancel: watch::Receiver<bool>,
) -> Result<(), String> {
    let mode = secret_or_opt(&inst.secrets, &inst.options, "connect_mode")
        .or_else(|| {
            inst.options
                .get("connect_mode")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "websocket".into());

    if mode == "webhook" {
        return run_webhook(inst, tx, cancel).await;
    }
    run_websocket(inst, tx, cancel).await
}

async fn run_websocket(
    inst: ChannelInstance,
    tx: mpsc::Sender<IncomingMessage>,
    mut cancel: watch::Receiver<bool>,
) -> Result<(), String> {
    let bot_id = secret_or_opt(&inst.secrets, &inst.options, "bot_id")
        .ok_or_else(|| "wecom ws: missing bot_id".to_string())?;
    let bot_secret = secret_or_opt(&inst.secrets, &inst.options, "bot_secret")
        .ok_or_else(|| "wecom ws: missing bot_secret".to_string())?;

    tracing::info!(instance = %inst.id, "wecom aibot websocket starting");
    let mut backoff = 2u64;
    loop {
        if *cancel.borrow() {
            return Ok(());
        }
        match run_ws_once(&inst, &bot_id, &bot_secret, tx.clone(), &mut cancel).await {
            Ok(()) => {
                if *cancel.borrow() {
                    return Ok(());
                }
            }
            Err(e) if e == WECOM_SUPERSEDED => {
                tracing::warn!(
                    instance = %inst.id,
                    "wecom ws: replaced by a newer connection; not reconnecting"
                );
                loop {
                    if cancel.changed().await.is_err() {
                        return Ok(());
                    }
                    if *cancel.borrow() {
                        return Ok(());
                    }
                }
            }
            Err(e) => tracing::error!(instance = %inst.id, "wecom ws: {e}"),
        }
        tokio::select! {
            _ = cancel.changed() => { if *cancel.borrow() { return Ok(()); } }
            _ = tokio::time::sleep(Duration::from_secs(backoff)) => {}
        }
        backoff = (backoff * 2).min(60);
    }
}

async fn run_ws_once(
    inst: &ChannelInstance,
    bot_id: &str,
    bot_secret: &str,
    tx: mpsc::Sender<IncomingMessage>,
    cancel: &mut watch::Receiver<bool>,
) -> Result<(), String> {
    let (ws, _) = connect_async(WECOM_WS_URL)
        .await
        .map_err(|e| format!("wecom ws connect: {e}"))?;
    let (mut write, mut read) = ws.split();
    tracing::info!(instance = %inst.id, "wecom ws: connected");

    let (reply_tx, mut reply_rx) = mpsc::channel::<WecomWsReply>(32);
    let _sink = register_wecom_sink(&inst.id, reply_tx);

    let subscribe_req = format!("aibot_subscribe_{}", now_millis());
    let subscribe = wecom_subscribe_frame(bot_id, bot_secret, &subscribe_req);
    write
        .send(Message::Text(subscribe.to_string().into()))
        .await
        .map_err(|e| format!("wecom ws: subscribe send failed: {e}"))?;
    tracing::info!(instance = %inst.id, "wecom ws: subscribe frame sent");

    let mut authed = false;
    let mut missed_pongs = 0u32;
    let mut ping = tokio::time::interval(WECOM_HEARTBEAT);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    ping.tick().await;

    loop {
        tokio::select! {
            _ = cancel.changed() => {
                if *cancel.borrow() {
                    let _ = write.close().await;
                    return Ok(());
                }
            }
            reply = reply_rx.recv() => {
                let Some(reply) = reply else { continue };
                let stream_id = format!("stream_{}", uuid::Uuid::new_v4().simple());
                let frame = wecom_reply_frame(&reply.req_id, &stream_id, &reply.text);
                tracing::info!(
                    instance = %inst.id,
                    req_id = %reply.req_id,
                    text_len = reply.text.len(),
                    "wecom ws: reply frame"
                );
                write
                    .send(Message::Text(frame.to_string().into()))
                    .await
                    .map_err(|e| format!("wecom ws: reply send failed: {e}"))?;
            }
            _ = ping.tick(), if authed => {
                if missed_pongs >= 2 {
                    return Err("wecom ws: heartbeat timeout".into());
                }
                missed_pongs += 1;
                let frame = wecom_ping_frame(&format!("ping_{}", now_millis()));
                write
                    .send(Message::Text(frame.to_string().into()))
                    .await
                    .map_err(|e| format!("wecom ws: ping send failed: {e}"))?;
            }
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(t))) => {
                        let v: Value = match serde_json::from_str(&t) {
                            Ok(v) => v,
                            Err(_) => {
                                tracing::info!(
                                    instance = %inst.id,
                                    bytes = t.len(),
                                    "wecom ws: non-json text frame"
                                );
                                continue;
                            }
                        };
                        let req_id = ws_req_id(&v);
                        if req_id.starts_with("aibot_subscribe") {
                            let code = ws_errcode(&v);
                            if code == 0 {
                                authed = true;
                                missed_pongs = 0;
                                tracing::info!(instance = %inst.id, "wecom ws: subscribed");
                            } else {
                                let errmsg = v.get("errmsg").and_then(|x| x.as_str()).unwrap_or("");
                                return Err(format!("wecom ws: subscribe failed errcode={code} {errmsg}"));
                            }
                            continue;
                        }
                        if req_id.starts_with("ping") {
                            if ws_errcode(&v) == 0 {
                                missed_pongs = 0;
                            }
                            continue;
                        }
                        if is_disconnected_event(&v) {
                            return Err(WECOM_SUPERSEDED.into());
                        }
                        let summary = wecom_ws_frame_summary(&v);
                        let parsed = parse_ws_msg(inst, &v);
                        tracing::info!(
                            instance = %inst.id,
                            bytes = t.len(),
                            cmd = %summary.cmd,
                            errcode = summary.errcode,
                            keys = %summary.keys,
                            body_keys = %summary.body_keys,
                            msgtype = %summary.msgtype,
                            has_text = summary.has_text,
                            parsed = parsed.is_some(),
                            "wecom ws: frame"
                        );
                        if let Some(inc) = parsed {
                            tracing::info!(
                                instance = %inst.id,
                                chat = %inc.chat_id,
                                sender = %inc.sender_id,
                                content_len = inc.content.len(),
                                "wecom ws: inbound message"
                            );
                            let _ = tx.send(inc).await;
                        }
                    }
                    Some(Ok(Message::Binary(b))) => {
                        tracing::info!(
                            instance = %inst.id,
                            bytes = b.len(),
                            "wecom ws: binary frame"
                        );
                    }
                    Some(Ok(Message::Close(frame))) => {
                        let code = frame.as_ref().map(|f| u16::from(f.code)).unwrap_or(0);
                        tracing::info!(instance = %inst.id, code, "wecom ws: server closed");
                        return Ok(());
                    }
                    None => {
                        tracing::info!(instance = %inst.id, "wecom ws: stream ended");
                        return Ok(());
                    }
                    Some(Ok(Message::Ping(p))) => { let _ = write.send(Message::Pong(p)).await; }
                    Some(Err(e)) => return Err(e.to_string()),
                    _ => {
                        tracing::info!(instance = %inst.id, "wecom ws: ignored frame");
                    }
                }
            }
        }
    }
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn is_disconnected_event(v: &Value) -> bool {
    v.pointer("/body/msgtype").and_then(|x| x.as_str()) == Some("event")
        && v.pointer("/body/event/eventtype").and_then(|x| x.as_str()) == Some("disconnected_event")
}

struct WecomWsFrameSummary {
    cmd: String,
    errcode: i64,
    keys: String,
    body_keys: String,
    msgtype: String,
    has_text: bool,
}

/// Shape only. Values such as Secret and message text stay out of the log.
fn wecom_ws_frame_summary(v: &Value) -> WecomWsFrameSummary {
    let body = v.get("body");
    let text_at = |pointer: &str| {
        v.pointer(pointer)
            .and_then(|x| x.as_str())
            .is_some_and(|s| !s.trim().is_empty())
    };
    let has_text = text_at("/text/content")
        || v.get("content")
            .and_then(|x| x.as_str())
            .is_some_and(|s| !s.trim().is_empty())
        || text_at("/body/text/content");
    WecomWsFrameSummary {
        cmd: v
            .get("cmd")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        errcode: v.get("errcode").and_then(|x| x.as_i64()).unwrap_or(-1),
        keys: json_key_list(v),
        body_keys: body.map(json_key_list).unwrap_or_default(),
        msgtype: v
            .get("msgtype")
            .or_else(|| body.and_then(|b| b.get("msgtype")))
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        has_text,
    }
}

fn json_key_list(v: &Value) -> String {
    let Some(map) = v.as_object() else {
        return String::new();
    };
    let mut keys: Vec<&str> = map.keys().map(|s| s.as_str()).collect();
    keys.sort_unstable();
    keys.join(",")
}

fn parse_ws_msg(inst: &ChannelInstance, v: &Value) -> Option<IncomingMessage> {
    let cmd = v.get("cmd").and_then(|x| x.as_str()).unwrap_or("");
    if !cmd.is_empty() && cmd != "aibot_msg_callback" {
        return None;
    }
    let body = v.get("body").filter(|b| b.is_object());
    if body.and_then(|b| b.get("msgtype")).and_then(|x| x.as_str()) == Some("event") {
        return None;
    }
    let payload = body.unwrap_or(v);
    let text = wecom_text(payload, v);
    let media_refs = wecom_media_refs(payload);
    if text.is_empty() && media_refs.is_empty() {
        return None;
    }
    let sender = body
        .and_then(|b| b.get("from"))
        .or_else(|| v.get("from"))
        .and_then(|f| f.get("userid").or_else(|| f.get("user_id")))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let chat = body
        .and_then(|b| b.get("chatid").or_else(|| b.get("chat_id")))
        .or_else(|| v.get("chatid").or_else(|| v.get("chat_id")))
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(sender.as_str())
        .to_string();
    let chat_type = match body
        .and_then(|b| b.get("chattype"))
        .and_then(|x| x.as_str())
    {
        Some("group") => "group",
        Some(_) => "p2p",
        None if body
            .and_then(|b| b.get("chatid"))
            .and_then(|x| x.as_str())
            .is_some_and(|s| !s.is_empty()) =>
        {
            "group"
        }
        None => "p2p",
    };
    let req_id = ws_req_id(v);
    let message_id = if req_id.is_empty() {
        body.and_then(|b| b.get("msgid"))
            .or_else(|| v.get("msgid"))
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        req_id.to_string()
    };
    Some(IncomingMessage {
        channel: inst.channel.clone(),
        instance_id: inst.id.clone(),
        message_id,
        chat_id: chat,
        chat_type: chat_type.into(),
        sender_id: sender,
        content: text,
        mentioned_bot: true,
        thread_id: None,
        media_refs,
    })
}

fn wecom_text(payload: &Value, root: &Value) -> String {
    let msgtype = payload
        .get("msgtype")
        .and_then(|x| x.as_str())
        .unwrap_or("");
    if msgtype == "voice" {
        return payload
            .pointer("/voice/content")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
    }
    if msgtype == "mixed" {
        let mut parts = Vec::new();
        if let Some(items) = payload
            .pointer("/mixed/msg_item")
            .and_then(|x| x.as_array())
        {
            for item in items {
                if item.get("msgtype").and_then(|x| x.as_str()) != Some("text") {
                    continue;
                }
                if let Some(text) = item.pointer("/text/content").and_then(|x| x.as_str()) {
                    let text = text.trim();
                    if !text.is_empty() {
                        parts.push(text.to_string());
                    }
                }
            }
        }
        return parts.join("\n");
    }
    payload
        .pointer("/text/content")
        .or_else(|| root.pointer("/text/content"))
        .or_else(|| root.get("content"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

fn wecom_media_refs(payload: &Value) -> Vec<super::super::types::InboundMediaRef> {
    let mut out = Vec::new();
    let msgtype = payload
        .get("msgtype")
        .and_then(|x| x.as_str())
        .unwrap_or("");
    match msgtype {
        "image" | "file" | "video" => push_media(&mut out, msgtype, payload.get(msgtype)),
        "mixed" => {
            if let Some(items) = payload
                .pointer("/mixed/msg_item")
                .and_then(|x| x.as_array())
            {
                for item in items {
                    let kind = item.get("msgtype").and_then(|x| x.as_str()).unwrap_or("");
                    if matches!(kind, "image" | "file" | "video") {
                        push_media(&mut out, kind, item.get(kind));
                    }
                }
            }
        }
        _ => {}
    }
    out
}

fn push_media(
    out: &mut Vec<super::super::types::InboundMediaRef>,
    kind: &str,
    node: Option<&Value>,
) {
    use super::super::types::{InboundMediaRef, RedactedSecret};
    let Some(node) = node else {
        return;
    };
    let url = node
        .get("url")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim();
    let key = node
        .get("aeskey")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim();
    if url.is_empty() || key.is_empty() {
        return;
    }
    let filename = node
        .get("filename")
        .or_else(|| node.get("name"))
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    out.push(InboundMediaRef {
        kind: kind.to_string(),
        url: url.to_string(),
        aes_key: RedactedSecret::new(key),
        filename,
    });
}

/// Default local webhook port for WeCom callback mode.
pub const DEFAULT_WEBHOOK_PORT: u16 = 8081;

/// Shared-token fallback header when `msg_signature` is unavailable
/// (plain-JSON deployments that cannot compute the WeCom signature).
pub const SHARED_TOKEN_HEADER: &str = "x-grok-wecom-token";

fn const_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

/// ±5 minutes (Telegram / Slack webhook convention). Outside this window → 401.
pub const WECOM_TIMESTAMP_SKEW_SECS: i64 = 300;

/// Runtime `last_error` code when webhook mode is bound to loopback only.
/// UI maps this to an i18n hint; it is advisory, not a connector crash.
pub const WECOM_WEBHOOK_LOOPBACK_ADVISORY: &str = "wecom_webhook_loopback_needs_allow_external";

/// Official WeCom callback signature:
/// `SHA1(lexicographically sorted [token, timestamp, nonce, payload])`,
/// delivered as the `msg_signature` query parameter. Compared constant-time.
fn wecom_signature_ok(
    token: &str,
    timestamp: &str,
    nonce: &str,
    payload: &str,
    signature: Option<&str>,
) -> bool {
    use sha1::{Digest, Sha1};

    let Some(sig) = signature.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    let mut parts = [token, timestamp, nonce, payload];
    parts.sort_unstable();
    let mut hasher = Sha1::new();
    for part in parts {
        hasher.update(part.as_bytes());
    }
    let expected = hex::encode(hasher.finalize());
    const_time_eq(expected.as_bytes(), sig.as_bytes())
}

/// True when `timestamp` (unix seconds) is within ±`WECOM_TIMESTAMP_SKEW_SECS`.
fn wecom_timestamp_fresh(timestamp: &str, now_unix: i64) -> bool {
    let Ok(ts) = timestamp.trim().parse::<i64>() else {
        return false;
    };
    now_unix.abs_diff(ts) <= WECOM_TIMESTAMP_SKEW_SECS as u64
}

/// WeCom signature must match **and** the timestamp must be fresh.
/// The shared-token header is a non-WeCom fallback and does not carry a
/// Tencent timestamp — it still authenticates without the replay window.
fn wecom_callback_authorized(
    token: &str,
    timestamp: &str,
    nonce: &str,
    payload: &str,
    signature: Option<&str>,
    header_token: Option<&str>,
    now_unix: i64,
) -> bool {
    if header_token.is_some_and(|t| const_time_eq(t.as_bytes(), token.as_bytes())) {
        return true;
    }
    wecom_signature_ok(token, timestamp, nonce, payload, signature)
        && wecom_timestamp_fresh(timestamp, now_unix)
}

/// Advisory code when webhook listens on loopback (`allow_external` unset).
fn wecom_webhook_bind_advisory(allow_external: bool) -> Option<&'static str> {
    if allow_external {
        None
    } else {
        Some(WECOM_WEBHOOK_LOOPBACK_ADVISORY)
    }
}

/// Extract `k=v` from an HTTP query string (first occurrence, no decode).
fn query_param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then_some(v)
    })
}

/// Minimal percent-decoding (%XX only) so signatures are computed over the
/// decoded values the sender signed.
fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let Some(hex) = bytes
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok())
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(hex);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Split a raw HTTP request into (request-line, headers, body).
fn split_request(req: &str) -> (&str, &str, &str) {
    match req.split_once("\r\n\r\n") {
        Some((head, body)) => match head.split_once("\r\n") {
            Some((request_line, headers)) => (request_line, headers, body),
            None => (head, "", body),
        },
        None => (req.trim_end_matches(['\r', '\n']), "", ""),
    }
}

async fn run_webhook(
    inst: ChannelInstance,
    tx: mpsc::Sender<IncomingMessage>,
    mut cancel: watch::Receiver<bool>,
) -> Result<(), String> {
    let port: u16 = secret_or_opt(&inst.secrets, &inst.options, "port")
        .and_then(|s| s.parse().ok())
        .or_else(|| {
            inst.options
                .get("port")
                .and_then(|x| x.as_u64())
                .map(|n| n as u16)
        })
        .unwrap_or(DEFAULT_WEBHOOK_PORT);
    let path = secret_or_opt(&inst.secrets, &inst.options, "callback_path")
        .unwrap_or_else(|| "/wecom/callback".into());
    let callback_token = secret_or_opt(&inst.secrets, &inst.options, "callback_token")
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    if callback_token.is_none() {
        return Err(
            "wecom webhook: missing callback_token (Settings → WeCom requires it; \
             it is the Callback Token shown in the WeCom admin console)"
                .to_string(),
        );
    }

    // Default loopback; opt-in LAN bind only with allow_external=true (same
    // contract as the LINE channel).
    let allow_external = inst
        .options
        .get("allow_external")
        .or_else(|| inst.options.get("allowExternal"))
        .and_then(|x| x.as_bool())
        .unwrap_or(false);
    let bind_ip = if allow_external {
        [0, 0, 0, 0]
    } else {
        [127, 0, 0, 1]
    };

    tracing::info!(
        instance = %inst.id,
        port,
        %path,
        allow_external,
        "wecom webhook server starting"
    );

    // Bind first so the connector is reachable even if remote gettoken is slow.
    let addr = SocketAddr::from((bind_ip, port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("wecom bind {port}: {e}"))?;
    if let Some(note) = wecom_webhook_bind_advisory(allow_external) {
        let _ = super::super::config::set_instance_advisory(&inst.id, Some(note.to_string()));
    } else {
        let _ = super::super::config::set_instance_last_error(&inst.id, None);
    }

    // Best-effort corp credential check in background (must not block listen).
    let corp_id = secret_or_opt(&inst.secrets, &inst.options, "corp_id");
    let corp_secret = secret_or_opt(&inst.secrets, &inst.options, "corp_secret");
    if let (Some(cid), Some(sec)) = (corp_id, corp_secret) {
        tokio::spawn(async move {
            if let Ok(client) = http_client() {
                let _ = client
                    .get(format!(
                        "https://qyapi.weixin.qq.com/cgi-bin/gettoken?corpid={cid}&corpsecret={sec}"
                    ))
                    .send()
                    .await;
            }
        });
    }

    let inst = Arc::new(inst);
    let path = Arc::new(path);
    // `callback_token` presence was validated before binding; this fallback is
    // unreachable in practice and only satisfies the type checker without a panic.
    let callback_token = Arc::new(callback_token.unwrap_or_else(|| String::from("unreachable")));

    loop {
        tokio::select! {
            _ = cancel.changed() => {
                if *cancel.borrow() { return Ok(()); }
            }
            acc = listener.accept() => {
                let Ok((mut socket, _)) = acc else { continue };
                let tx = tx.clone();
                let inst = inst.clone();
                let path = path.clone();
                let callback_token = callback_token.clone();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = vec![0u8; 65536];
                    let n = match socket.read(&mut buf).await {
                        Ok(n) if n > 0 => n,
                        _ => return,
                    };
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let (request_line, _, body) = split_request(&req);

                    let uri = request_line.split_whitespace().nth(1).unwrap_or("");
                    let (uri_path, uri_query) = match uri.split_once('?') {
                        Some((p, q)) => (p, q),
                        None => (uri, ""),
                    };
                    // Path match on the decoded request target; tolerate a
                    // trailing-slash difference only.
                    let norm =
                        |p: &str| p.trim_end_matches('/').to_string();
                    if norm(&percent_decode(uri_path)) != norm(path.as_str()) {
                        let _ = socket.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n").await;
                        return;
                    }

                    // Official WeCom signature over [token, timestamp, nonce,
                    // payload]; GET verify signs the echostr, POSTs sign the body.
                    let sig = query_param(uri_query, "msg_signature").map(percent_decode);
                    let timestamp = query_param(uri_query, "timestamp")
                        .map(percent_decode)
                        .unwrap_or_default();
                    let nonce = query_param(uri_query, "nonce")
                        .map(percent_decode)
                        .unwrap_or_default();
                    let echostr = query_param(uri_query, "echostr").map(percent_decode);
                    let header_token = req.lines().skip(1).find_map(|l| {
                        l.split_once(':').filter(|(name, _)| {
                            name.eq_ignore_ascii_case(SHARED_TOKEN_HEADER)
                        }).map(|(_, v)| v.trim().to_string())
                    });

                    let is_get = request_line.starts_with("GET");
                    let payload_for_sig: &str = if is_get {
                        echostr.as_deref().unwrap_or("")
                    } else {
                        body
                    };
                    let now = super::super::resilience::now_unix_secs() as i64;
                    let verified = wecom_callback_authorized(
                        callback_token.as_str(),
                        &timestamp,
                        &nonce,
                        payload_for_sig,
                        sig.as_deref(),
                        header_token.as_deref(),
                        now,
                    );

                    // URL verify echo: only echo after verification (WeCom signs
                    // the encrypted echostr during endpoint setup).
                    if is_get {
                        if !verified {
                            tracing::warn!(instance = %inst.id, "wecom: bad or missing callback signature on verify");
                            let _ = socket.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n").await;
                            return;
                        }
                        if let Some(echo) = echostr.as_deref() {
                            let resp = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                                echo.len(),
                                echo
                            );
                            let _ = socket.write_all(resp.as_bytes()).await;
                            return;
                        }
                        let _ = socket.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n").await;
                        return;
                    }

                    if !verified {
                        tracing::warn!(instance = %inst.id, "wecom: bad or missing callback signature");
                        let _ = socket.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n").await;
                        return;
                    }

                    // JSON or XML simplified — try JSON
                    if let Ok(v) = serde_json::from_str::<Value>(body) {
                        let text = v
                            .get("Content")
                            .or_else(|| v.get("text"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("");
                        let user = v
                            .get("FromUserName")
                            .or_else(|| v.get("from"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("");
                        if !text.is_empty() {
                            let _ = tx.send(IncomingMessage {
                                channel: inst.channel.clone(),
                                instance_id: inst.id.clone(),
                                message_id: v.get("MsgId").and_then(|x| x.as_str()).unwrap_or("").into(),
                                chat_id: user.into(),
                                chat_type: "p2p".into(),
                                sender_id: user.into(),
                                content: text.into(),
                                mentioned_bot: true,
                                thread_id: None,
                                media_refs: Vec::new(),
                            }).await;
                        }
                    }
                    let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\n\r\nsuccess").await;
                });
            }
        }
    }
}

pub async fn send_text(
    secrets: &HashMap<String, String>,
    chat_id: &str,
    reply_to: Option<&str>,
    text: &str,
) -> Result<(), String> {
    if let Some(hook) = secrets.get("webhook") {
        let client = http_client()?;
        let res = client
            .post(hook)
            .json(&json!({
                "msgtype": "text",
                "text": { "content": text }
            }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !res.status().is_success() {
            return Err(format!("wecom webhook: {}", res.status()));
        }
        return Ok(());
    }
    // App chat send via access token
    if let (Some(corp_id), Some(secret)) = (secrets.get("corp_id"), secrets.get("corp_secret")) {
        let client = http_client()?;
        let tok: Value = client
            .get(format!(
                "https://qyapi.weixin.qq.com/cgi-bin/gettoken?corpid={corp_id}&corpsecret={secret}"
            ))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        let access = tok
            .get("access_token")
            .and_then(|x| x.as_str())
            .ok_or("no access_token")?;
        let agent = secrets.get("agent_id").map(|s| s.as_str()).unwrap_or("0");
        let res = client
            .post(format!(
                "https://qyapi.weixin.qq.com/cgi-bin/message/send?access_token={access}"
            ))
            .json(&json!({
                "touser": chat_id,
                "msgtype": "text",
                "agentid": agent.parse::<i64>().unwrap_or(0),
                "text": { "content": text }
            }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !res.status().is_success() {
            return Err(format!("wecom send: {}", res.status()));
        }
        return Ok(());
    }
    let req_id = reply_to.map(str::trim).filter(|s| !s.is_empty());
    let Some(req_id) = req_id else {
        return Err("wecom: no outbound route".into());
    };
    let instance_id = secrets
        .get("_instance_id")
        .map(String::as_str)
        .unwrap_or("");
    enqueue_wecom_ws_reply(instance_id, req_id, text)
}

pub fn protocol_name() -> &'static str {
    "wecom-ws-or-webhook"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Official WeCom signature: SHA1 over lexicographically sorted
    /// [token, timestamp, nonce, payload], hex-encoded.
    fn sign(token: &str, ts: &str, nonce: &str, payload: &str) -> String {
        use sha1::{Digest, Sha1};
        let mut parts = [token, ts, nonce, payload];
        parts.sort_unstable();
        let mut h = Sha1::new();
        for p in parts {
            h.update(p.as_bytes());
        }
        hex::encode(h.finalize())
    }

    #[test]
    fn ws_frame_summary_records_shape_without_secret_or_text() {
        let v = json!({
            "cmd": "aibot_msg_callback",
            "headers": { "req_id": "r1" },
            "body": {
                "msgid": "m1",
                "msgtype": "text",
                "secret": "do-not-log",
                "text": { "content": "hello from wecom" }
            }
        });
        let summary = wecom_ws_frame_summary(&v);
        let line = format!(
            "cmd={} errcode={} keys={} body_keys={} msgtype={} has_text={}",
            summary.cmd,
            summary.errcode,
            summary.keys,
            summary.body_keys,
            summary.msgtype,
            summary.has_text
        );
        assert_eq!(summary.cmd, "aibot_msg_callback");
        assert_eq!(summary.msgtype, "text");
        assert!(summary.has_text);
        assert!(summary.body_keys.contains("text"));
        assert!(!line.contains("do-not-log"));
        assert!(!line.contains("hello from wecom"));
    }

    fn sample_inst() -> ChannelInstance {
        ChannelInstance {
            id: "wecom-1".into(),
            channel: "wecom".into(),
            name: "t".into(),
            enabled: true,
            secrets: HashMap::new(),
            options: json!({}),
            acl: json!({}),
            project_scope: json!({}),
        }
    }

    #[test]
    fn subscribe_frame_matches_aibot_protocol() {
        let frame = wecom_subscribe_frame("bot", "sec", "aibot_subscribe_1");
        assert_eq!(frame["cmd"], "aibot_subscribe");
        assert_eq!(frame["headers"]["req_id"], "aibot_subscribe_1");
        assert_eq!(frame["body"]["bot_id"], "bot");
        assert_eq!(frame["body"]["secret"], "sec");
        assert!(frame.get("bot_secret").is_none());
        assert_eq!(WECOM_WS_URL, "wss://openws.work.weixin.qq.com");
    }

    #[test]
    fn parse_ws_msg_reads_body_and_keeps_req_id() {
        let v = json!({
            "cmd": "aibot_msg_callback",
            "headers": { "req_id": "req-9" },
            "body": {
                "msgid": "m1",
                "aibotid": "bot",
                "chattype": "single",
                "msgtype": "text",
                "from": { "userid": "user-1" },
                "text": { "content": " hello " }
            }
        });
        let msg = parse_ws_msg(&sample_inst(), &v).expect("text callback");
        assert_eq!(msg.message_id, "req-9");
        assert_eq!(msg.sender_id, "user-1");
        assert_eq!(msg.chat_id, "user-1");
        assert_eq!(msg.chat_type, "p2p");
        assert_eq!(msg.content, "hello");
        assert!(msg.media_refs.is_empty());
    }

    #[test]
    fn parse_ws_msg_keeps_file_without_text() {
        let v = json!({
            "cmd": "aibot_msg_callback",
            "headers": { "req_id": "req-file" },
            "body": {
                "msgid": "m-file",
                "chattype": "single",
                "msgtype": "file",
                "from": { "userid": "user-1" },
                "file": {
                    "url": "https://wwcdn.weixin.qq.com/file?token=secret",
                    "aeskey": "0123456789abcdef0123456789abcdef",
                    "filename": "aa.doc"
                }
            }
        });
        let msg = parse_ws_msg(&sample_inst(), &v).expect("file");
        assert!(msg.content.is_empty());
        assert_eq!(msg.media_refs.len(), 1);
        assert_eq!(msg.media_refs[0].kind, "file");
        assert_eq!(msg.media_refs[0].filename.as_deref(), Some("aa.doc"));
        let shown = format!("{msg:?}");
        assert!(!shown.contains("0123456789abcdef"));
        assert!(!shown.contains("token=secret"));
        assert!(shown.contains("[redacted]"));
    }

    #[test]
    fn parse_ws_msg_mixed_and_voice() {
        let mixed = json!({
            "cmd": "aibot_msg_callback",
            "headers": { "req_id": "req-mix" },
            "body": {
                "chattype": "single",
                "msgtype": "mixed",
                "from": { "userid": "user-1" },
                "mixed": {
                    "msg_item": [
                        { "msgtype": "text", "text": { "content": "看图" } },
                        { "msgtype": "image", "image": {
                            "url": "https://wwcdn.weixin.qq.com/i",
                            "aeskey": "0123456789abcdef0123456789abcdef"
                        } }
                    ]
                }
            }
        });
        let msg = parse_ws_msg(&sample_inst(), &mixed).expect("mixed");
        assert_eq!(msg.content, "看图");
        assert_eq!(msg.media_refs.len(), 1);
        assert_eq!(msg.media_refs[0].kind, "image");

        let voice = json!({
            "cmd": "aibot_msg_callback",
            "headers": { "req_id": "req-voice" },
            "body": {
                "chattype": "single",
                "msgtype": "voice",
                "from": { "userid": "user-1" },
                "voice": { "content": " 你好 " }
            }
        });
        let msg = parse_ws_msg(&sample_inst(), &voice).expect("voice");
        assert_eq!(msg.content, "你好");
        assert!(msg.media_refs.is_empty());
    }

    #[test]
    fn parse_ws_msg_skips_events_and_reads_group_chat() {
        let event = json!({
            "cmd": "aibot_event_callback",
            "headers": { "req_id": "ev-1" },
            "body": { "msgtype": "event", "event": { "eventtype": "enter_chat" } }
        });
        assert!(parse_ws_msg(&sample_inst(), &event).is_none());
        let group = json!({
            "cmd": "aibot_msg_callback",
            "headers": { "req_id": "req-g" },
            "body": {
                "chatid": "wr-group",
                "chattype": "group",
                "msgtype": "text",
                "from": { "userid": "user-2" },
                "text": { "content": "@bot hi" }
            }
        });
        let msg = parse_ws_msg(&sample_inst(), &group).expect("group text");
        assert_eq!(msg.chat_id, "wr-group");
        assert_eq!(msg.chat_type, "group");
        assert_eq!(msg.message_id, "req-g");
    }

    #[test]
    fn reply_frame_echoes_req_id_as_finished_stream() {
        let frame = wecom_reply_frame("req-9", "stream-1", "done");
        assert_eq!(frame["cmd"], "aibot_respond_msg");
        assert_eq!(frame["headers"]["req_id"], "req-9");
        assert_eq!(frame["body"]["msgtype"], "stream");
        assert_eq!(frame["body"]["stream"]["id"], "stream-1");
        assert_eq!(frame["body"]["stream"]["content"], "done");
        assert_eq!(frame["body"]["stream"]["finish"], true);
    }

    #[test]
    fn disconnected_event_is_recognized() {
        let v = json!({
            "body": {
                "msgtype": "event",
                "event": { "eventtype": "disconnected_event" }
            }
        });
        assert!(is_disconnected_event(&v));
    }

    #[test]
    fn signature_accepts_valid_and_rejects_missing_or_bad() {
        let sig = sign("tok", "1717171717", "nonce1", "{\"x\":1}");
        assert!(wecom_signature_ok(
            "tok",
            "1717171717",
            "nonce1",
            "{\"x\":1}",
            Some(&sig)
        ));
        // Wrong payload / token / timestamp must not verify.
        assert!(!wecom_signature_ok(
            "tok",
            "1717171717",
            "nonce1",
            "{\"y\":2}",
            Some(&sig)
        ));
        assert!(!wecom_signature_ok(
            "bad",
            "1717171717",
            "nonce1",
            "{\"x\":1}",
            Some(&sig)
        ));
        assert!(!wecom_signature_ok(
            "tok",
            "0000",
            "nonce1",
            "{\"x\":1}",
            Some(&sig)
        ));
        // Missing or empty signature fails closed.
        assert!(!wecom_signature_ok(
            "tok",
            "1717171717",
            "nonce1",
            "{\"x\":1}",
            None
        ));
        assert!(!wecom_signature_ok(
            "tok",
            "1717171717",
            "nonce1",
            "{\"x\":1}",
            Some("  ")
        ));
    }

    #[test]
    fn signature_is_order_independent_over_parts() {
        // The four parts sort before hashing — same inputs, any order.
        let a = wecom_signature_ok("t", "123", "n", "p", Some(&sign("t", "123", "n", "p")));
        let b = wecom_signature_ok("123", "t", "p", "n", Some(&sign("t", "123", "n", "p")));
        assert!(a && b);
    }

    #[test]
    fn timestamp_freshness_rejects_expired_future_and_invalid() {
        let now = 1_700_000_000i64;
        assert!(wecom_timestamp_fresh(&now.to_string(), now));
        // Inclusive ±300s boundary.
        assert!(wecom_timestamp_fresh(
            &(now - WECOM_TIMESTAMP_SKEW_SECS).to_string(),
            now
        ));
        assert!(wecom_timestamp_fresh(
            &(now + WECOM_TIMESTAMP_SKEW_SECS).to_string(),
            now
        ));
        assert!(!wecom_timestamp_fresh(
            &(now - WECOM_TIMESTAMP_SKEW_SECS - 1).to_string(),
            now
        ));
        assert!(!wecom_timestamp_fresh(
            &(now + WECOM_TIMESTAMP_SKEW_SECS + 1).to_string(),
            now
        ));
        assert!(!wecom_timestamp_fresh("", now));
        assert!(!wecom_timestamp_fresh("not-a-number", now));
        assert!(!wecom_timestamp_fresh("  ", now));
    }

    #[test]
    fn callback_auth_requires_fresh_timestamp_on_signature_path() {
        let now = 1_700_000_000i64;
        let token = "tok";
        let nonce = "n1";
        let payload = "{\"x\":1}";
        let fresh = now.to_string();
        let expired = (now - WECOM_TIMESTAMP_SKEW_SECS - 1).to_string();
        let future = (now + WECOM_TIMESTAMP_SKEW_SECS + 1).to_string();
        let sig_fresh = sign(token, &fresh, nonce, payload);
        let sig_expired = sign(token, &expired, nonce, payload);
        let sig_future = sign(token, &future, nonce, payload);

        assert!(wecom_callback_authorized(
            token,
            &fresh,
            nonce,
            payload,
            Some(&sig_fresh),
            None,
            now
        ));
        assert!(!wecom_callback_authorized(
            token,
            &expired,
            nonce,
            payload,
            Some(&sig_expired),
            None,
            now
        ));
        assert!(!wecom_callback_authorized(
            token,
            &future,
            nonce,
            payload,
            Some(&sig_future),
            None,
            now
        ));
        // Shared-token header still authenticates without a WeCom timestamp.
        assert!(wecom_callback_authorized(
            token,
            &expired,
            nonce,
            payload,
            None,
            Some(token),
            now
        ));
    }

    #[test]
    fn webhook_loopback_advisory_when_not_allow_external() {
        assert_eq!(
            wecom_webhook_bind_advisory(false),
            Some(WECOM_WEBHOOK_LOOPBACK_ADVISORY)
        );
        assert_eq!(wecom_webhook_bind_advisory(true), None);
    }

    #[test]
    fn const_time_eq_basic() {
        assert!(const_time_eq(b"abc", b"abc"));
        assert!(!const_time_eq(b"abc", b"abd"));
        assert!(!const_time_eq(b"abc", b"ab"));
        assert!(const_time_eq(b"", b""));
    }

    #[test]
    fn percent_decode_handles_escapes_and_passes_through() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("%2Fx"), "/x");
        assert_eq!(percent_decode("plain"), "plain");
        // Truncated escape stays literal.
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%G1"), "%G1");
    }

    #[test]
    fn query_param_finds_first_exact_key() {
        let q = "msg_signature=abc&timestamp=5&nonce=xyz&echostr=EN%2FCODE";
        assert_eq!(query_param(q, "timestamp"), Some("5"));
        assert_eq!(query_param(q, "nonce"), Some("xyz"));
        assert_eq!(query_param(q, "echostr"), Some("EN%2FCODE"));
        assert_eq!(query_param(q, "missing"), None);
        // Prefix keys do not match.
        assert_eq!(query_param("nonceX=1", "nonce"), None);
    }

    #[test]
    fn split_request_parses_head_and_body() {
        let raw = "POST /cb HTTP/1.1\r\nHost: x\r\n\r\n{\"a\":1}";
        let (line, headers, body) = split_request(raw);
        assert_eq!(line, "POST /cb HTTP/1.1");
        assert_eq!(headers, "Host: x");
        assert_eq!(body, "{\"a\":1}");
        // Bodyless request still yields an empty body slice.
        let (line2, _, body2) = split_request("GET /cb HTTP/1.1\r\n");
        assert_eq!(line2, "GET /cb HTTP/1.1");
        assert_eq!(body2, "");
    }
}
