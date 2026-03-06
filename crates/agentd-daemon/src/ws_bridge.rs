use super::{
    build_audit_context, handle_rpc_request, stream_run_agent_over_uds, OneApiConfig,
    RunAgentParams, RuntimeState,
};
use agentd_protocol::{
    A2AStreamParams, A2ATaskEvent, A2ATaskState, JsonRpcRequest, JsonRpcResponse,
};
use agentd_store::SqliteStore;
use chrono::Utc;
use futures_util::{ready, Sink, SinkExt, StreamExt};
use serde_json::{json, Value};
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncWrite, AsyncWriteExt, Result as TokioIoResult};
use tokio::net::TcpStream;
use tokio::time::Duration;
use tokio_tungstenite::tungstenite::{handshake::derive_accept_key, protocol::Role, Message};
use tokio_tungstenite::WebSocketStream;
use tracing::warn;

type DynError = Box<dyn std::error::Error + Send + Sync>;

pub(super) fn is_ws_upgrade_request(method: &str, path: &str, request: &str) -> bool {
    if method != "GET" || path != "/ws" {
        return false;
    }
    let has_upgrade = header_value(request, "upgrade")
        .map(|value| value.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);
    let has_connection_upgrade = header_value(request, "connection")
        .map(|value| value.to_ascii_lowercase().contains("upgrade"))
        .unwrap_or(false);
    let has_key = header_value(request, "sec-websocket-key").is_some();
    has_upgrade && has_connection_upgrade && has_key
}

pub(super) async fn serve_ws_bridge(
    mut stream: TcpStream,
    request: &str,
    store: Arc<SqliteStore>,
    state: RuntimeState,
    one_api_config: OneApiConfig,
) -> Result<(), DynError> {
    let Some(ws_key) = header_value(request, "sec-websocket-key") else {
        write_bad_ws_request(&mut stream, "missing Sec-WebSocket-Key").await?;
        return Ok(());
    };

    let accept_key = derive_accept_key(ws_key.as_bytes());
    let upgrade_response = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept_key}\r\n\r\n"
    );
    stream.write_all(upgrade_response.as_bytes()).await?;

    let mut ws = WebSocketStream::from_raw_socket(stream, Role::Server, None).await;
    while let Some(incoming) = ws.next().await {
        match incoming {
            Ok(Message::Text(payload)) => {
                if let Err(err) = handle_text_message(
                    &mut ws,
                    payload.as_ref(),
                    store.clone(),
                    state.clone(),
                    one_api_config.clone(),
                )
                .await
                {
                    warn!(%err, "websocket bridge message handling failed");
                    break;
                }
            }
            Ok(Message::Ping(payload)) => {
                ws.send(Message::Pong(payload)).await?;
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(err) => {
                warn!(%err, "websocket bridge receive failed");
                break;
            }
        }
    }

    Ok(())
}

async fn handle_text_message(
    ws: &mut WebSocketStream<TcpStream>,
    payload: &str,
    store: Arc<SqliteStore>,
    state: RuntimeState,
    one_api_config: OneApiConfig,
) -> Result<(), DynError> {
    let request = match serde_json::from_str::<JsonRpcRequest>(payload) {
        Ok(request) => request,
        Err(_) => {
            send_rpc_response(
                ws,
                &JsonRpcResponse::error(json!(null), -32700, "parse error"),
            )
            .await?;
            return Ok(());
        }
    };

    if request.jsonrpc != "2.0" {
        send_rpc_response(
            ws,
            &JsonRpcResponse::error(request.id, -32600, "invalid jsonrpc version"),
        )
        .await?;
        return Ok(());
    }

    if request.method == "A2A.SubscribeStream" {
        handle_a2a_stream_subscription(ws, request, state).await?;
        return Ok(());
    }

    if request.method == "RunAgent"
        && request
            .params
            .get("stream")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        handle_run_agent_stream_over_ws(ws, request, store, one_api_config).await?;
        return Ok(());
    }

    let response = handle_rpc_request(request, store, state, one_api_config).await;
    send_rpc_response(ws, &response).await?;
    Ok(())
}

async fn handle_run_agent_stream_over_ws(
    ws: &mut WebSocketStream<TcpStream>,
    request: JsonRpcRequest,
    store: Arc<SqliteStore>,
    one_api_config: OneApiConfig,
) -> Result<(), DynError> {
    let params = match serde_json::from_value::<RunAgentParams>(request.params.clone()) {
        Ok(params) => params,
        Err(err) => {
            send_rpc_response(
                ws,
                &JsonRpcResponse::error(request.id, -32602, format!("invalid params: {err}")),
            )
            .await?;
            return Ok(());
        }
    };

    let request_id = request.id.clone();
    send_rpc_response(
        ws,
        &JsonRpcResponse::success(request_id.clone(), json!({"stream": true})),
    )
    .await?;

    let mut writer = WsTextBridgeWriter::new(ws);
    let audit_context = build_audit_context(&request_id);
    stream_run_agent_over_uds(&mut writer, store, one_api_config, params, audit_context).await;
    writer.flush_pending().await?;
    Ok(())
}

async fn handle_a2a_stream_subscription(
    ws: &mut WebSocketStream<TcpStream>,
    request: JsonRpcRequest,
    state: RuntimeState,
) -> Result<(), DynError> {
    let params = match serde_json::from_value::<A2AStreamParams>(request.params.clone()) {
        Ok(params) => params,
        Err(err) => {
            send_rpc_response(
                ws,
                &JsonRpcResponse::error(request.id, -32602, format!("invalid params: {err}")),
            )
            .await?;
            return Ok(());
        }
    };

    let task_id = match uuid::Uuid::parse_str(&params.task_id) {
        Ok(task_id) => task_id,
        Err(err) => {
            send_rpc_response(
                ws,
                &JsonRpcResponse::error(request.id, -32602, format!("invalid task_id: {err}")),
            )
            .await?;
            return Ok(());
        }
    };

    let Some(task) = state.get_a2a_task(task_id).await else {
        send_rpc_response(
            ws,
            &JsonRpcResponse::error(request.id, -32044, "a2a task not found"),
        )
        .await?;
        return Ok(());
    };

    send_rpc_response(
        ws,
        &JsonRpcResponse::success(request.id, json!({"subscribed": task_id})),
    )
    .await?;

    let mut subscription = state.subscribe_a2a_stream();
    let history = state.a2a_event_history(task.id).await;
    let mut replay_cursor = history.last().map(|event| event.timestamp);
    if history.is_empty() {
        let initial_event = A2ATaskEvent {
            task_id: task.id,
            state: task.state,
            lifecycle_state: task.state.to_agent_lifecycle_state(),
            timestamp: Utc::now(),
            payload: json!({"task": task}),
        };
        send_stream_event(ws, &initial_event).await?;
    } else {
        for event in &history {
            send_stream_event(ws, event).await?;
        }
    }

    if history
        .last()
        .is_some_and(|event| is_terminal_state(event.state))
        || is_terminal_state(task.state)
    {
        return Ok(());
    }

    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(800), subscription.recv()).await {
            Ok(Ok(event)) if event.task_id == task_id => {
                if replay_cursor.is_some_and(|cursor| event.timestamp <= cursor) {
                    continue;
                }
                send_stream_event(ws, &event).await?;
                replay_cursor = Some(event.timestamp);
                if is_terminal_state(event.state) {
                    break;
                }
            }
            Ok(Ok(_)) => {}
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) | Err(_) => break,
        }
    }

    Ok(())
}

fn is_terminal_state(state: A2ATaskState) -> bool {
    matches!(
        state,
        A2ATaskState::Completed | A2ATaskState::Failed | A2ATaskState::Canceled
    )
}

async fn send_stream_event(
    ws: &mut WebSocketStream<TcpStream>,
    event: &A2ATaskEvent,
) -> Result<(), DynError> {
    let payload = json!({
        "jsonrpc": "2.0",
        "method": "A2A.StreamEvent",
        "params": event,
    });
    send_json(ws, &payload).await
}

async fn send_rpc_response(
    ws: &mut WebSocketStream<TcpStream>,
    response: &JsonRpcResponse,
) -> Result<(), DynError> {
    let payload = serde_json::to_value(response)?;
    send_json(ws, &payload).await
}

async fn send_json(ws: &mut WebSocketStream<TcpStream>, payload: &Value) -> Result<(), DynError> {
    let encoded = serde_json::to_string(payload)?;
    ws.send(Message::Text(encoded)).await?;
    Ok(())
}

struct WsTextBridgeWriter<'a> {
    ws: &'a mut WebSocketStream<TcpStream>,
    buffer: Vec<u8>,
}

impl<'a> WsTextBridgeWriter<'a> {
    fn new(ws: &'a mut WebSocketStream<TcpStream>) -> Self {
        Self {
            ws,
            buffer: Vec::new(),
        }
    }

    async fn flush_pending(&mut self) -> Result<(), DynError> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let text = String::from_utf8(std::mem::take(&mut self.buffer))?;
        self.ws.send(Message::Text(text)).await?;
        Ok(())
    }
}

impl AsyncWrite for WsTextBridgeWriter<'_> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        self.buffer.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        if self.buffer.is_empty() {
            return Poll::Ready(Ok(()));
        }

        ready!(Pin::new(&mut *self.ws).poll_ready(cx)).map_err(io::Error::other)?;
        let text = String::from_utf8(std::mem::take(&mut self.buffer)).map_err(io::Error::other)?;
        Pin::new(&mut *self.ws)
            .start_send(Message::Text(text))
            .map_err(io::Error::other)?;
        ready!(Pin::new(&mut *self.ws).poll_flush(cx)).map_err(io::Error::other)?;
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.poll_flush(cx)
    }
}

async fn write_bad_ws_request(stream: &mut TcpStream, message: &str) -> TokioIoResult<()> {
    let body = format!("{{\"error\":\"{message}\"}}");
    let response = format!(
        "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;
    let _ = stream.shutdown().await;
    Ok(())
}

fn header_value(request: &str, key: &str) -> Option<String> {
    request
        .lines()
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter_map(|line| line.split_once(':'))
        .find_map(|(header_name, header_value)| {
            if header_name.trim().eq_ignore_ascii_case(key) {
                Some(header_value.trim().to_string())
            } else {
                None
            }
        })
}
