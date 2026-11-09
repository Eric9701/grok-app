//! Inbound WebSocket for remote `atlas agent --grok-ws-url` workers.
//!
//! One text frame is one ACP line. Identity is `agent_id` and `token` on `/ws`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};

use super::protocol::{inbound_frame_to_line, remote_agent_id, token_matches, LineEvent};
use super::{can_take_worker, hub_token, now_ms};

const PING_EVERY: Duration = Duration::from_secs(15);
const SILENCE_MS: u64 = 45_000;

pub async fn serve(listener: TcpListener, shutdown: oneshot::Receiver<()>) {
    let app = Router::new().route("/ws", get(ws_route));
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown.await;
        })
        .await;
    if let Err(err) = result {
        tracing::warn!(target: "fleet", "listener stopped: {err}");
    }
}

async fn ws_route(
    Query(q): Query<HashMap<String, String>>,
    upgrade: WebSocketUpgrade,
) -> impl IntoResponse {
    let agent_raw = q.get("agent_id").map(|s| s.as_str()).unwrap_or("");
    let token = q.get("token").cloned().unwrap_or_default();
    if !token_matches(&hub_token(), &token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(agent_id) = remote_agent_id(agent_raw) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if !can_take_worker(&agent_id) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    upgrade
        .on_upgrade(move |socket| super::accept_worker(agent_id, socket))
        .into_response()
}

pub async fn pump(
    socket: WebSocket,
    mut out_rx: mpsc::UnboundedReceiver<String>,
    line_tx: mpsc::UnboundedSender<LineEvent>,
    stop: Arc<AtomicBool>,
) {
    let (mut sink, mut stream) = socket.split();
    let last = AtomicU64::new(now_ms());
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        tokio::select! {
            _ = ping.tick() => {
                if now_ms().saturating_sub(last.load(Ordering::Relaxed)) > SILENCE_MS {
                    break;
                }
                if sink.send(Message::Ping(vec![1].into())).await.is_err() {
                    break;
                }
            }
            incoming = stream.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        last.store(now_ms(), Ordering::Relaxed);
                        if let Some(line) = inbound_frame_to_line(text.as_str()) {
                            if line_tx.send(LineEvent::Line(line)).is_err() {
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        last.store(now_ms(), Ordering::Relaxed);
                        if sink.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        last.store(now_ms(), Ordering::Relaxed);
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(_)) => {}
                }
            }
            outgoing = out_rx.recv() => {
                match outgoing {
                    Some(line) => {
                        if sink.send(Message::Text(line.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
    }
    let _ = line_tx.send(LineEvent::Closed);
    let _ = sink.close().await;
}
