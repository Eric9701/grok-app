//! ACP line ↔ Claude / Cursor stream-json. Pure so tests need no CLI.

use std::collections::HashMap;

use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// One line for the foreign CLI stdin.
    ToCli(String),
    /// One ACP JSON-RPC line for the host or the cloud relay.
    ToHost(String),
    /// CLI session id to store on the desktop binding.
    RememberCliSession(String),
}

#[derive(Debug)]
pub struct BridgeState {
    pub kind: String,
    pub permission: String,
    pub session_id: String,
    pub prompt_id: Option<Value>,
    pub emitted_text: bool,
    next_server_id: u64,
    /// Host permission RPC id → CLI control request id.
    pending_perm: HashMap<u64, String>,
}

impl BridgeState {
    pub fn new(kind: &str, permission: &str, resume_id: Option<&str>) -> Self {
        let session_id = resume_id
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        Self {
            kind: kind.to_string(),
            permission: if permission == "ask" {
                "ask".into()
            } else {
                "yolo".into()
            },
            session_id,
            prompt_id: None,
            emitted_text: false,
            next_server_id: 1_000_000,
            pending_perm: HashMap::new(),
        }
    }
}

pub fn push_acp_line(state: &mut BridgeState, line: &str) -> Vec<Effect> {
    let Ok(msg) = serde_json::from_str::<Value>(line.trim()) else {
        return Vec::new();
    };
    if msg.get("method").is_none() {
        return permission_result(state, &msg);
    }
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = msg.get("id").cloned();
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    match method {
        "initialize" => id
            .as_ref()
            .map(|id| {
                vec![Effect::ToHost(ok(
                    id,
                    json!({
                        "protocolVersion": 1,
                        "agentCapabilities": { "loadSession": true },
                    }),
                ))]
            })
            .unwrap_or_default(),
        "session/new" | "session/load" => {
            if let Some(sid) = params
                .pointer("/sessionId")
                .or_else(|| params.pointer("/_meta/sessionId"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                state.session_id = sid.to_string();
            }
            id.as_ref()
                .map(|id| {
                    vec![Effect::ToHost(ok(
                        id,
                        json!({ "sessionId": state.session_id }),
                    ))]
                })
                .unwrap_or_default()
        }
        "session/prompt" => {
            if let Some(sid) = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                state.session_id = sid.to_string();
            }
            state.prompt_id = id.clone();
            state.emitted_text = false;
            let text = prompt_text(&params);
            vec![Effect::ToCli(user_message(&text))]
        }
        "session/cancel" => {
            let mut out = vec![Effect::ToCli(interrupt_line())];
            if let Some(pid) = state.prompt_id.take() {
                out.push(Effect::ToHost(ok(&pid, json!({ "stopReason": "cancelled" }))));
            }
            if let Some(id) = id.as_ref() {
                out.push(Effect::ToHost(ok(id, json!({}))));
            }
            out
        }
        "session/set_model" | "session/set_mode" | "authenticate" => id
            .as_ref()
            .map(|id| vec![Effect::ToHost(ok(id, json!({})))])
            .unwrap_or_default(),
        _ => id
            .as_ref()
            .map(|id| vec![Effect::ToHost(err(id, -32601, &format!("Method not found: {method}")))])
            .unwrap_or_default(),
    }
}

pub fn push_cli_line(state: &mut BridgeState, line: &str) -> Vec<Effect> {
    let Ok(msg) = serde_json::from_str::<Value>(line.trim()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(sid) = msg
        .get("session_id")
        .or_else(|| msg.get("sessionId"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        // Resume id for the next spawn. Keep the ACP id the host already
        // bound so cloud tasks and desktop updates stay on that id.
        if state.session_id.is_empty() {
            state.session_id = sid.to_string();
        }
        out.push(Effect::RememberCliSession(sid.to_string()));
    }
    match msg.get("type").and_then(|t| t.as_str()).unwrap_or("") {
        "assistant" | "message" => out.extend(assistant_blocks(state, &msg)),
        "stream_event" => {
            if let Some(text) = msg
                .pointer("/event/delta/text")
                .or_else(|| msg.pointer("/delta/text"))
                .and_then(|v| v.as_str())
            {
                push_chunk(&mut out, state, "agent_message_chunk", text);
            }
        }
        "tool_call" => {
            let name = msg
                .pointer("/tool_call/name")
                .or_else(|| msg.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("tool");
            let id = msg
                .pointer("/tool_call/id")
                .or_else(|| msg.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or(name);
            out.push(Effect::ToHost(update(
                &state.session_id,
                json!({
                    "sessionUpdate": "tool_call",
                    "toolCallId": id,
                    "title": name,
                    "kind": "execute",
                    "status": "pending",
                }),
            )));
        }
        "user" => out.extend(tool_results(state, &msg)),
        "result" => out.extend(finish_prompt(state, &msg)),
        "control_request" => out.extend(control_request(state, &msg)),
        _ => {}
    }
    let _ = &state.kind;
    out
}

fn permission_result(state: &mut BridgeState, msg: &Value) -> Vec<Effect> {
    let Some(id) = msg.get("id").and_then(|v| v.as_u64()) else {
        return Vec::new();
    };
    let Some(request_id) = state.pending_perm.remove(&id) else {
        return Vec::new();
    };
    let result = msg.get("result").cloned().unwrap_or(Value::Null);
    let allow = permission_allows(&result);
    vec![Effect::ToCli(control_response(&request_id, allow))]
}

fn assistant_blocks(state: &mut BridgeState, msg: &Value) -> Vec<Effect> {
    let mut out = Vec::new();
    let content = msg
        .pointer("/message/content")
        .or_else(|| msg.get("content"))
        .cloned()
        .unwrap_or(Value::Null);
    if let Some(text) = content.as_str() {
        push_chunk(&mut out, state, "agent_message_chunk", text);
        return out;
    }
    let Some(blocks) = content.as_array() else {
        if let Some(text) = msg
            .pointer("/message/text")
            .and_then(|v| v.as_str())
        {
            push_chunk(&mut out, state, "agent_message_chunk", text);
        }
        return out;
    };
    for block in blocks {
        let kind = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match kind {
            "thinking" | "reasoning" => {
                if let Some(text) = block.get("thinking").or_else(|| block.get("text")).and_then(|v| v.as_str()) {
                    push_chunk(&mut out, state, "agent_thought_chunk", text);
                }
            }
            "tool_use" => {
                let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("tool");
                let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("tool");
                out.push(Effect::ToHost(update(
                    &state.session_id,
                    json!({
                        "sessionUpdate": "tool_call",
                        "toolCallId": id,
                        "title": name,
                        "kind": "execute",
                        "status": "pending",
                    }),
                )));
            }
            _ => {
                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                    push_chunk(&mut out, state, "agent_message_chunk", text);
                }
            }
        }
    }
    out
}

fn tool_results(state: &BridgeState, msg: &Value) -> Vec<Effect> {
    let mut out = Vec::new();
    let Some(blocks) = msg
        .pointer("/message/content")
        .and_then(|v| v.as_array())
    else {
        return out;
    };
    for block in blocks {
        if block.get("type").and_then(|v| v.as_str()) != Some("tool_result") {
            continue;
        }
        let id = block
            .get("tool_use_id")
            .or_else(|| block.get("toolUseId"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if id.is_empty() {
            continue;
        }
        out.push(Effect::ToHost(update(
            &state.session_id,
            json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": id,
                "status": "completed",
            }),
        )));
    }
    out
}

fn finish_prompt(state: &mut BridgeState, msg: &Value) -> Vec<Effect> {
    let mut out = Vec::new();
    if !state.emitted_text {
        if let Some(text) = msg
            .get("result")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            push_chunk(&mut out, state, "agent_message_chunk", text);
        }
    }
    let stop = match msg.get("subtype").and_then(|v| v.as_str()) {
        Some("cancelled") | Some("canceled") => "cancelled",
        Some("error") => "error",
        _ => "end_turn",
    };
    if let Some(pid) = state.prompt_id.take() {
        out.push(Effect::ToHost(ok(&pid, json!({ "stopReason": stop }))));
    }
    out
}

fn control_request(state: &mut BridgeState, msg: &Value) -> Vec<Effect> {
    let request_id = msg
        .get("request_id")
        .or_else(|| msg.get("requestId"))
        .and_then(|v| v.as_str())
        .unwrap_or("perm")
        .to_string();
    let subtype = msg
        .pointer("/request/subtype")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if subtype != "can_use_tool" && subtype != "permission" {
        return vec![Effect::ToCli(control_response(&request_id, true))];
    }
    if state.permission != "ask" {
        return vec![Effect::ToCli(control_response(&request_id, true))];
    }
    let tool = msg
        .pointer("/request/tool_name")
        .or_else(|| msg.pointer("/request/toolName"))
        .and_then(|v| v.as_str())
        .unwrap_or("tool");
    let rpc = state.next_server_id;
    state.next_server_id = state.next_server_id.saturating_add(1);
    state.pending_perm.insert(rpc, request_id);
    vec![Effect::ToHost(
        serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "id": rpc,
            "method": "session/request_permission",
            "params": {
                "sessionId": state.session_id,
                "toolCall": {
                    "toolCallId": format!("perm-{rpc}"),
                    "title": tool,
                    "name": tool,
                    "kind": "execute",
                },
                "options": [
                    { "optionId": "allow_once", "name": "Allow", "kind": "allow" },
                    { "optionId": "reject_once", "name": "Reject", "kind": "reject" },
                ],
            },
        }))
        .unwrap_or_default(),
    )]
}

fn push_chunk(out: &mut Vec<Effect>, state: &mut BridgeState, kind: &str, text: &str) {
    if text.is_empty() {
        return;
    }
    if kind == "agent_message_chunk" {
        state.emitted_text = true;
    }
    out.push(Effect::ToHost(update(
        &state.session_id,
        json!({
            "sessionUpdate": kind,
            "content": { "type": "text", "text": text },
        }),
    )));
}

fn prompt_text(params: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(blocks) = params.get("prompt").and_then(|p| p.as_array()) {
        for block in blocks {
            if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                if !text.is_empty() {
                    parts.push(text.to_string());
                }
            }
        }
    }
    if parts.is_empty() {
        params
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        parts.join("\n")
    }
}

fn permission_allows(result: &Value) -> bool {
    let option = result
        .pointer("/outcome/optionId")
        .or_else(|| result.get("optionId"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if option.contains("reject") {
        return false;
    }
    let outcome = result
        .pointer("/outcome/outcome")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if outcome == "cancelled" {
        return false;
    }
    option.contains("allow") || outcome == "selected" || option.is_empty()
}

fn user_message(text: &str) -> String {
    serde_json::to_string(&json!({
        "type": "user",
        "message": {
            "role": "user",
            "content": [{ "type": "text", "text": text }],
        },
    }))
    .unwrap_or_else(|_| "{}".into())
}

fn interrupt_line() -> String {
    serde_json::to_string(&json!({
        "type": "control_request",
        "request_id": "cancel",
        "request": { "subtype": "interrupt" },
    }))
    .unwrap_or_else(|_| "{}".into())
}

fn control_response(request_id: &str, allow: bool) -> String {
    serde_json::to_string(&json!({
        "type": "control_response",
        "response": {
            "subtype": "success",
            "request_id": request_id,
            "response": { "behavior": if allow { "allow" } else { "deny" } },
        },
    }))
    .unwrap_or_else(|_| "{}".into())
}

fn ok(id: &Value, result: Value) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    }))
    .unwrap_or_else(|_| "{}".into())
}

fn err(id: &Value, code: i64, message: &str) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    }))
    .unwrap_or_else(|_| "{}".into())
}

fn update(session_id: &str, update: Value) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": { "sessionId": session_id, "update": update },
    }))
    .unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host_json(effects: &[Effect]) -> Vec<Value> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::ToHost(line) => serde_json::from_str(line).ok(),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn prompt_becomes_stream_json_user_line() {
        let mut state = BridgeState::new("claude", "yolo", None);
        let effects = push_acp_line(
            &mut state,
            r#"{"jsonrpc":"2.0","id":5,"method":"session/prompt","params":{"sessionId":"s1","prompt":[{"type":"text","text":"看一下进度"},{"type":"text","text":"第二段"}]}}"#,
        );
        assert_eq!(state.session_id, "s1");
        match &effects[0] {
            Effect::ToCli(line) => {
                let v: Value = serde_json::from_str(line).unwrap();
                assert_eq!(v["type"], "user");
                assert_eq!(v["message"]["content"][0]["text"], "看一下进度\n第二段");
            }
            other => panic!("expected cli line, got {other:?}"),
        }
    }

    #[test]
    fn assistant_and_result_become_acp_updates() {
        let mut state = BridgeState::new("cursor", "yolo", Some("cli-1"));
        state.prompt_id = Some(json!(5));
        state.session_id = "cli-1".into();
        let effects = push_cli_line(
            &mut state,
            r#"{"type":"assistant","session_id":"cli-1","message":{"content":[{"type":"text","text":"Hello"},{"type":"tool_use","id":"t1","name":"Bash"}]}}"#,
        );
        let hosts = host_json(&effects);
        assert_eq!(hosts[0]["method"], "session/update");
        assert_eq!(hosts[0]["params"]["update"]["sessionUpdate"], "agent_message_chunk");
        assert_eq!(hosts[0]["params"]["update"]["content"]["text"], "Hello");
        assert_eq!(hosts[1]["params"]["update"]["sessionUpdate"], "tool_call");
        assert!(effects.iter().any(|e| matches!(e, Effect::RememberCliSession(id) if id == "cli-1")));

        let done = push_cli_line(
            &mut state,
            r#"{"type":"result","subtype":"success","result":"ignored because text already streamed"}"#,
        );
        let done_hosts = host_json(&done);
        assert_eq!(done_hosts[0]["id"], 5);
        assert_eq!(done_hosts[0]["result"]["stopReason"], "end_turn");
    }

    #[test]
    fn result_text_is_a_message_when_nothing_streamed() {
        let mut state = BridgeState::new("claude", "yolo", None);
        state.prompt_id = Some(json!(9));
        let done = push_cli_line(&mut state, r#"{"type":"result","subtype":"success","result":"done"}"#);
        let hosts = host_json(&done);
        assert_eq!(hosts[0]["params"]["update"]["content"]["text"], "done");
        assert_eq!(hosts[1]["result"]["stopReason"], "end_turn");
    }

    #[test]
    fn ask_permission_round_trip() {
        let mut state = BridgeState::new("claude", "ask", None);
        let req = push_cli_line(
            &mut state,
            r#"{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"Bash"}}"#,
        );
        let hosts = host_json(&req);
        assert_eq!(hosts[0]["method"], "session/request_permission");
        let id = hosts[0]["id"].as_u64().unwrap();
        let reply = push_acp_line(
            &mut state,
            &format!(
                r#"{{"jsonrpc":"2.0","id":{id},"result":{{"outcome":{{"outcome":"selected","optionId":"reject_once"}}}}}}"#
            ),
        );
        match &reply[0] {
            Effect::ToCli(line) => assert!(line.contains("deny")),
            other => panic!("expected deny, got {other:?}"),
        }
    }

    #[test]
    fn yolo_auto_allows_permission() {
        let mut state = BridgeState::new("cursor", "yolo", None);
        let req = push_cli_line(
            &mut state,
            r#"{"type":"control_request","request_id":"r2","request":{"subtype":"can_use_tool","tool_name":"Bash"}}"#,
        );
        assert!(req.iter().all(|e| !matches!(e, Effect::ToHost(_))));
        match &req[0] {
            Effect::ToCli(line) => assert!(line.contains("allow")),
            other => panic!("expected allow, got {other:?}"),
        }
    }

    #[test]
    fn cancel_ends_the_open_prompt() {
        let mut state = BridgeState::new("claude", "yolo", None);
        state.prompt_id = Some(json!(3));
        let effects = push_acp_line(
            &mut state,
            r#"{"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":"s"}}"#,
        );
        assert!(effects.iter().any(|e| matches!(e, Effect::ToCli(_))));
        let hosts = host_json(&effects);
        assert_eq!(hosts[0]["result"]["stopReason"], "cancelled");
    }

    #[test]
    fn initialize_and_new_are_answered_without_a_cli() {
        let mut state = BridgeState::new("claude", "ask", Some("resume-9"));
        let init = host_json(&push_acp_line(
            &mut state,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        ));
        assert_eq!(init[0]["result"]["protocolVersion"], 1);
        let newer = host_json(&push_acp_line(
            &mut state,
            r#"{"jsonrpc":"2.0","id":2,"method":"session/new","params":{"cwd":"E:/work"}}"#,
        ));
        assert_eq!(newer[0]["result"]["sessionId"], "resume-9");
    }
}
