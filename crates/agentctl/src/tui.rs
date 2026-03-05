use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Read, Stdout, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use agentd_protocol::{JsonRpcRequest, JsonRpcResponse};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream as TokioUnixStream;

type DynError = Box<dyn std::error::Error>;
const STREAM_ERROR_EMITTED: &str = "__stream_error_emitted__";

#[derive(Debug, Clone)]
enum TuiCommand {
    Chat {
        input: String,
        model: String,
        agent_id: Option<String>,
    },
}

#[derive(Debug, Clone)]
enum StreamChunk {
    Token(String),
    ToolCall { title: String, detail: String },
    Done,
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamParseOutcome {
    Continue,
    Done,
    Errored,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Default)]
pub struct StatusBar {
    pub mode: String,
    pub hint: String,
}

#[derive(Debug, Clone, Default)]
pub struct EventPanel {
    pub entries: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PendingApproval {
    pub id: String,
    pub summary: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MultiAgentDelegationStatus {
    pub key: String,
    pub task_id: String,
    pub child_index: u64,
    pub agent_id: String,
    pub phase: String,
    pub attempt: u64,
    pub summary: String,
}

#[derive(Debug, Clone, Default)]
struct ToolCallFold {
    id: u64,
    title: String,
    detail: String,
    collapsed: bool,
}

#[derive(Debug, Clone)]
struct SessionSnapshot {
    messages: Vec<ChatMessage>,
    event_entries: Vec<String>,
    approval_queue: Vec<PendingApproval>,
    tool_calls: Vec<ToolCallFold>,
    active_model: String,
    active_agent_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentShellApp {
    socket_path: String,
    input_buffer: String,
    messages: Vec<ChatMessage>,
    status_bar: StatusBar,
    event_panel: EventPanel,
    approval_queue: Vec<PendingApproval>,
    multi_agent_status: Vec<MultiAgentDelegationStatus>,
    tool_calls: Vec<ToolCallFold>,
    saved_sessions: BTreeMap<String, SessionSnapshot>,
    active_model: String,
    active_agent_id: Option<String>,
    stream_seq: u64,
    stream_active: bool,
    stream_target_index: Option<usize>,
    pending_chat_inputs: VecDeque<String>,
    command_tx: Option<Sender<TuiCommand>>,
}

impl AgentShellApp {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::with_socket_path("/tmp/agentd.sock")
    }

    pub fn with_socket_path(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
            input_buffer: String::new(),
            messages: Vec::new(),
            status_bar: StatusBar {
                mode: "idle".to_string(),
                hint: "enter=submit | /usage /events /tools /compact /model /approve /deny /session /delegations | q/esc/ctrl-c=quit".to_string(),
            },
            event_panel: EventPanel::default(),
            approval_queue: Vec::new(),
            multi_agent_status: Vec::new(),
            tool_calls: Vec::new(),
            saved_sessions: BTreeMap::new(),
            active_model: "claude-4-sonnet".to_string(),
            active_agent_id: None,
            stream_seq: 0,
            stream_active: false,
            stream_target_index: None,
            pending_chat_inputs: VecDeque::new(),
            command_tx: None,
        }
    }

    #[cfg(test)]
    pub fn supported_slash_commands() -> &'static [&'static str] {
        &[
            "/usage",
            "/events",
            "/tools",
            "/compact",
            "/model",
            "/approve",
            "/deny",
            "/session",
            "/delegations",
        ]
    }

    fn push_system_message(&mut self, content: impl Into<String>) {
        self.messages.push(ChatMessage {
            role: "system".to_string(),
            content: content.into(),
        });
    }

    fn push_slash_error(&mut self, message: &str) {
        self.push_system_message(format!("slash error: {message}"));
        self.event_panel
            .entries
            .push(format!("slash error: {message}"));
    }

    fn set_command_sender(&mut self, command_tx: Sender<TuiCommand>) {
        self.command_tx = Some(command_tx);
    }

    fn append_stream_chunk(&mut self, chunk: &str) {
        if let Some(target_index) = self.stream_target_index {
            if let Some(target) = self.messages.get_mut(target_index) {
                if !target.content.is_empty() {
                    target.content.push(' ');
                }
                target.content.push_str(chunk);
            }
        }
    }

    fn push_tool_call(&mut self, title: String, detail: String) {
        self.stream_seq += 1;
        self.tool_calls.push(ToolCallFold {
            id: self.stream_seq,
            title: title.clone(),
            detail,
            collapsed: true,
        });
        self.event_panel.entries.push(format!(
            "tool_call#{} {} (collapsed)",
            self.stream_seq, title
        ));
    }

    fn begin_chat_stream(&mut self, input: String) {
        self.messages.push(ChatMessage {
            role: "user".to_string(),
            content: input.clone(),
        });

        self.status_bar.mode = "streaming".to_string();
        self.stream_active = true;
        self.messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: String::new(),
        });
        self.stream_target_index = Some(self.messages.len().saturating_sub(1));

        let command = TuiCommand::Chat {
            input,
            model: self.active_model.clone(),
            agent_id: self.active_agent_id.clone(),
        };
        if let Some(command_tx) = &self.command_tx {
            if let Err(err) = command_tx.send(command) {
                self.stream_active = false;
                self.status_bar.mode = "idle".to_string();
                self.stream_target_index = None;
                self.push_system_message(format!("stream dispatch failed: {err}"));
            }
        } else {
            self.stream_active = false;
            self.status_bar.mode = "idle".to_string();
            self.stream_target_index = None;
            self.push_system_message(
                "stream dispatch unavailable: command channel not initialized",
            );
        }
    }

    fn finish_streaming(&mut self) {
        if let Some(target_index) = self.stream_target_index {
            if let Some(target) = self.messages.get_mut(target_index) {
                if target.content.trim().is_empty() {
                    target.content = "(empty response)".to_string();
                }
            }
        }
        self.stream_active = false;
        self.stream_target_index = None;
        self.status_bar.mode = "idle".to_string();
        if let Some(next_input) = self.pending_chat_inputs.pop_front() {
            self.begin_chat_stream(next_input);
        }
    }

    fn handle_stream_chunk(&mut self, chunk: StreamChunk) {
        match chunk {
            StreamChunk::Token(text) => self.append_stream_chunk(&text),
            StreamChunk::ToolCall { title, detail } => self.push_tool_call(title, detail),
            StreamChunk::Done => self.finish_streaming(),
            StreamChunk::Error(message) => {
                self.push_system_message(format!("stream error: {message}"));
                self.event_panel
                    .entries
                    .push(format!("stream error: {message}"));
                self.finish_streaming();
            }
        }
    }

    fn current_or_default_agent_id(&mut self, explicit: Option<&str>) -> Result<String, String> {
        if let Some(agent_id) = explicit {
            let trimmed = agent_id.trim();
            if trimmed.is_empty() {
                return Err("agent_id must be non-empty".to_string());
            }
            let resolved = trimmed.to_string();
            self.active_agent_id = Some(resolved.clone());
            return Ok(resolved);
        }

        self.active_agent_id
            .clone()
            .ok_or_else(|| "missing agent_id (provide one as slash command arg)".to_string())
    }

    fn load_approval_queue<F>(&mut self, agent_id: &str, rpc: &mut F) -> Result<(), String>
    where
        F: FnMut(&str, Value) -> Result<Value, String>,
    {
        let result = rpc("ListApprovalQueue", json!({ "agent_id": agent_id }))?;
        let approvals = result
            .get("approvals")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        self.approval_queue = approvals
            .into_iter()
            .map(|item| {
                let id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("<unknown>")
                    .to_string();
                let tool = item
                    .get("tool")
                    .and_then(Value::as_str)
                    .unwrap_or("<unknown>");
                let reason = item
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("policy.ask");
                PendingApproval {
                    id,
                    summary: format!("{tool} | {reason}"),
                }
            })
            .collect();

        self.event_panel.entries.push(format!(
            "approval_queue refreshed: {} item(s)",
            self.approval_queue.len()
        ));
        Ok(())
    }

    fn resolve_approval<F>(
        &mut self,
        decision: &str,
        approval_id: &str,
        explicit_agent_id: Option<&str>,
        rpc: &mut F,
    ) -> Result<(), String>
    where
        F: FnMut(&str, Value) -> Result<Value, String>,
    {
        let agent_id = self.current_or_default_agent_id(explicit_agent_id)?;
        let result = rpc(
            "ResolveApproval",
            json!({
                "agent_id": agent_id,
                "approval_id": approval_id,
                "decision": decision,
            }),
        )?;

        self.approval_queue.retain(|item| item.id != approval_id);
        self.event_panel.entries.push(format!(
            "approval {} -> {}",
            approval_id,
            result
                .get("decision")
                .and_then(Value::as_str)
                .unwrap_or(decision)
        ));
        self.push_system_message(format!(
            "approval {} resolved as {}",
            approval_id,
            result
                .get("decision")
                .and_then(Value::as_str)
                .unwrap_or(decision)
        ));
        Ok(())
    }

    fn apply_multi_agent_event(&mut self, event: &Value) {
        let payload = event.get("payload").unwrap_or(event);
        if payload
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default()
            != "orchestrator"
        {
            return;
        }

        let task_id = payload
            .get("task_id")
            .and_then(Value::as_str)
            .unwrap_or("unknown-task")
            .to_string();
        let child_index = payload
            .get("child_index")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let agent_id = payload
            .get("agent_id")
            .and_then(Value::as_str)
            .unwrap_or("unknown-agent")
            .to_string();
        let phase = payload
            .get("phase")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let attempt = payload.get("attempt").and_then(Value::as_u64).unwrap_or(0);
        let error = payload.get("error").and_then(Value::as_str);
        let summary = if let Some(error) = error {
            format!("{phase} (attempt {attempt}) - {error}")
        } else {
            format!("{phase} (attempt {attempt})")
        };

        let key = format!("{task_id}#{child_index}");
        if let Some(existing) = self
            .multi_agent_status
            .iter_mut()
            .find(|status| status.key == key)
        {
            existing.agent_id = agent_id.clone();
            existing.phase = phase.clone();
            existing.attempt = attempt;
            existing.summary = summary.clone();
        } else {
            self.multi_agent_status.push(MultiAgentDelegationStatus {
                key,
                task_id: task_id.clone(),
                child_index,
                agent_id: agent_id.clone(),
                phase: phase.clone(),
                attempt,
                summary: summary.clone(),
            });
        }

        self.event_panel.entries.push(format!(
            "delegation task={} child={} agent={} {}",
            task_id, child_index, agent_id, summary
        ));
    }

    fn save_session(&mut self, name: &str) {
        let snapshot = SessionSnapshot {
            messages: self.messages.clone(),
            event_entries: self.event_panel.entries.clone(),
            approval_queue: self.approval_queue.clone(),
            tool_calls: self.tool_calls.clone(),
            active_model: self.active_model.clone(),
            active_agent_id: self.active_agent_id.clone(),
        };
        self.saved_sessions.insert(name.to_string(), snapshot);
        self.event_panel
            .entries
            .push(format!("session saved: {name}"));
        self.push_system_message(format!("session {name} saved"));
    }

    fn load_session(&mut self, name: &str) -> Result<(), String> {
        let Some(snapshot) = self.saved_sessions.get(name).cloned() else {
            return Err(format!("session not found: {name}"));
        };

        self.messages = snapshot.messages;
        self.event_panel.entries = snapshot.event_entries;
        self.approval_queue = snapshot.approval_queue;
        self.tool_calls = snapshot.tool_calls;
        self.active_model = snapshot.active_model;
        self.active_agent_id = snapshot.active_agent_id;
        self.event_panel
            .entries
            .push(format!("session loaded: {name}"));
        self.push_system_message(format!("session {name} loaded"));
        Ok(())
    }

    fn execute_slash_command_with_rpc<F>(&mut self, command: &str, rpc: &mut F)
    where
        F: FnMut(&str, Value) -> Result<Value, String>,
    {
        let mut parts = command.split_whitespace();
        let Some(head) = parts.next() else {
            return;
        };

        let outcome = match head {
            "/usage" => {
                let explicit_agent = parts.next();
                let agent_id = self.current_or_default_agent_id(explicit_agent);
                match agent_id {
                    Ok(agent_id) => {
                        rpc("GetUsage", json!({ "agent_id": agent_id, "window": null })).map(
                            |value| {
                                self.push_system_message(format!(
                                    "usage: {}",
                                    serde_json::to_string(&value)
                                        .unwrap_or_else(|_| "<invalid usage payload>".to_string())
                                ));
                            },
                        )
                    }
                    Err(err) => Err(err),
                }
            }
            "/events" => {
                let limit = parts
                    .next()
                    .and_then(|raw| raw.parse::<usize>().ok())
                    .unwrap_or(10);
                rpc("ListLifecycleEvents", json!({ "limit": limit })).map(|value| {
                    if let Some(events) = value.get("events").and_then(Value::as_array) {
                        for event in events {
                            self.apply_multi_agent_event(event);
                        }
                    }
                    self.push_system_message(format!(
                        "events: {}",
                        serde_json::to_string(&value)
                            .unwrap_or_else(|_| "<invalid events payload>".to_string())
                    ));
                })
            }
            "/tools" => {
                let explicit_agent = parts.next();
                let agent_id = self.current_or_default_agent_id(explicit_agent);
                match agent_id {
                    Ok(agent_id) => {
                        rpc("ListAvailableTools", json!({ "agent_id": agent_id })).map(|value| {
                            self.push_system_message(format!(
                                "tools: {}",
                                serde_json::to_string(&value)
                                    .unwrap_or_else(|_| "<invalid tools payload>".to_string())
                            ));
                        })
                    }
                    Err(err) => Err(err),
                }
            }
            "/compact" => {
                self.event_panel
                    .entries
                    .push("manual compact requested".to_string());
                self.push_system_message("compact requested");
                Ok(())
            }
            "/model" => {
                let model_name = match parts.next() {
                    Some(model_name) => model_name,
                    None => return self.push_slash_error("usage: /model <name>"),
                };
                if model_name.trim().is_empty() {
                    Err("model name must be non-empty".to_string())
                } else {
                    self.active_model = model_name.to_string();
                    self.event_panel
                        .entries
                        .push(format!("model switched to {}", self.active_model));
                    self.push_system_message(format!("model -> {}", self.active_model));
                    Ok(())
                }
            }
            "/approve" => {
                let approval_id = parts.next();
                if let Some(id) = approval_id {
                    self.resolve_approval("approve", id, parts.next(), rpc)
                } else {
                    let explicit_agent = parts.next();
                    let agent_id = self.current_or_default_agent_id(explicit_agent);
                    match agent_id {
                        Ok(agent_id) => self.load_approval_queue(&agent_id, rpc),
                        Err(err) => Err(err),
                    }
                }
            }
            "/deny" => {
                let id = match parts.next() {
                    Some(id) => id,
                    None => return self.push_slash_error("usage: /deny <id> [agent_id]"),
                };
                self.resolve_approval("deny", id, parts.next(), rpc)
            }
            "/session" => {
                let action = match parts.next() {
                    Some(action) => action,
                    None => {
                        return self
                            .push_slash_error("usage: /session save <name> | /session load <name>")
                    }
                };
                let name = match parts.next() {
                    Some(name) => name,
                    None => {
                        return self
                            .push_slash_error("usage: /session save <name> | /session load <name>")
                    }
                };
                match action {
                    "save" => {
                        self.save_session(name);
                        Ok(())
                    }
                    "load" => self.load_session(name),
                    _ => Err("usage: /session save <name> | /session load <name>".to_string()),
                }
            }
            "/delegations" => {
                self.push_system_message(format!(
                    "delegations: {}",
                    serde_json::to_string(&self.multi_agent_status)
                        .unwrap_or_else(|_| "<invalid delegations payload>".to_string())
                ));
                Ok(())
            }
            _ => Err(format!("unknown slash command: {head}")),
        };

        if let Err(err) = outcome {
            self.push_system_message(format!("slash error: {err}"));
            self.event_panel.entries.push(format!("slash error: {err}"));
        }
    }

    fn submit_input(&mut self) {
        let input = self.input_buffer.trim().to_string();
        if input.is_empty() {
            self.input_buffer.clear();
            return;
        }

        if input.starts_with('/') {
            let socket_path = self.socket_path.clone();
            let mut rpc =
                |method: &str, params: Value| call_rpc_over_uds(&socket_path, method, params);
            self.execute_slash_command_with_rpc(&input, &mut rpc);
            self.input_buffer.clear();
            return;
        }

        if self.stream_active {
            self.pending_chat_inputs.push_back(input.clone());
            self.event_panel
                .entries
                .push(format!("chat queued while streaming: {input}"));
            self.push_system_message("stream busy: queued input");
            self.input_buffer.clear();
            return;
        }

        self.begin_chat_stream(input);
        self.input_buffer.clear();
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> bool {
        if key_event.kind != KeyEventKind::Press {
            return true;
        }

        match key_event.code {
            KeyCode::Char('q') | KeyCode::Esc => false,
            KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => false,
            KeyCode::Enter => {
                self.submit_input();
                true
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
                true
            }
            KeyCode::Char(ch) => {
                self.input_buffer.push(ch);
                true
            }
            _ => true,
        }
    }

    pub fn render(&self, frame: &mut Frame<'_>) {
        let vertical_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(frame.size());

        let body_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(vertical_chunks[0]);

        let side_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(34),
                Constraint::Percentage(33),
                Constraint::Percentage(33),
            ])
            .split(body_chunks[1]);

        let mut message_lines: Vec<Line<'_>> = if self.messages.is_empty() {
            vec![Line::from("No messages yet. Type and press Enter.")]
        } else {
            self.messages
                .iter()
                .map(|message| Line::from(format!("{}: {}", message.role, message.content)))
                .collect()
        };
        if !self.tool_calls.is_empty() {
            message_lines.push(Line::from(""));
            for tool_call in self.tool_calls.iter().rev().take(5) {
                let line = if tool_call.collapsed {
                    format!("[tool #{}] {} (collapsed)", tool_call.id, tool_call.title)
                } else {
                    format!(
                        "[tool #{}] {} => {}",
                        tool_call.id, tool_call.title, tool_call.detail
                    )
                };
                message_lines.push(Line::from(line));
            }
        }

        let message_panel = Paragraph::new(Text::from(message_lines))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Messages + Stream"),
            )
            .wrap(Wrap { trim: false });

        let event_lines: Vec<Line<'_>> = if self.event_panel.entries.is_empty() {
            vec![Line::from("No events")]
        } else {
            self.event_panel
                .entries
                .iter()
                .rev()
                .take(8)
                .map(|entry| Line::from(entry.clone()))
                .collect()
        };
        let event_panel = Paragraph::new(Text::from(event_lines))
            .block(Block::default().borders(Borders::ALL).title("Events"))
            .wrap(Wrap { trim: false });

        let approval_lines: Vec<Line<'_>> = if self.approval_queue.is_empty() {
            vec![Line::from("No pending approvals")]
        } else {
            self.approval_queue
                .iter()
                .map(|approval| Line::from(format!("{} | {}", approval.id, approval.summary)))
                .collect()
        };
        let approvals_panel = Paragraph::new(Text::from(approval_lines))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Approval Queue"),
            )
            .wrap(Wrap { trim: false });

        let multi_agent_lines: Vec<Line<'_>> = if self.multi_agent_status.is_empty() {
            vec![Line::from("No delegations")]
        } else {
            self.multi_agent_status
                .iter()
                .rev()
                .take(8)
                .map(|status| {
                    Line::from(format!(
                        "task={} child={} agent={} {}",
                        status.task_id, status.child_index, status.agent_id, status.summary
                    ))
                })
                .collect()
        };
        let multi_agent_panel = Paragraph::new(Text::from(multi_agent_lines))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Multi-Agent Delegations"),
            )
            .wrap(Wrap { trim: false });

        let input_panel = Paragraph::new(format!("> {}", self.input_buffer))
            .block(Block::default().borders(Borders::ALL).title("Input"));

        let approval_preview = self
            .approval_queue
            .first()
            .map(|approval| format!("next_approval={} ({})", approval.id, approval.summary))
            .unwrap_or_else(|| "next_approval=none".to_string());
        let status_line = format!(
            "mode={} | model={} | agent_id={} | pending_events={} | pending_approvals={} | delegations={} | {} | {}",
            self.status_bar.mode,
            self.active_model,
            self.active_agent_id.as_deref().unwrap_or("<none>"),
            self.event_panel.entries.len(),
            self.approval_queue.len(),
            self.multi_agent_status.len(),
            approval_preview,
            self.status_bar.hint
        );
        let status_bar = Paragraph::new(status_line);

        frame.render_widget(message_panel, body_chunks[0]);
        frame.render_widget(event_panel, side_chunks[0]);
        frame.render_widget(approvals_panel, side_chunks[1]);
        frame.render_widget(multi_agent_panel, side_chunks[2]);
        frame.render_widget(input_panel, vertical_chunks[1]);
        frame.render_widget(status_bar, vertical_chunks[2]);
    }
}

fn call_rpc_over_uds(socket_path: &str, method: &str, params: Value) -> Result<Value, String> {
    let mut stream = UnixStream::connect(socket_path)
        .map_err(|err| format!("connect uds {} failed: {err}", socket_path))?;

    let request = JsonRpcRequest::new(json!(1), method, params);
    let payload =
        serde_json::to_vec(&request).map_err(|err| format!("encode rpc request failed: {err}"))?;

    stream
        .write_all(&payload)
        .map_err(|err| format!("write rpc request failed: {err}"))?;
    stream
        .shutdown(Shutdown::Write)
        .map_err(|err| format!("shutdown write side failed: {err}"))?;

    let mut response_payload = Vec::new();
    stream
        .read_to_end(&mut response_payload)
        .map_err(|err| format!("read rpc response failed: {err}"))?;
    let response: JsonRpcResponse = serde_json::from_slice(&response_payload)
        .map_err(|err| format!("decode rpc response failed: {err}"))?;

    if let Some(error) = response.error {
        return Err(format!("RPC error {}: {}", error.code, error.message));
    }

    Ok(response.result.unwrap_or(json!(null)))
}

fn emit_chunk_from_value(value: &Value, chunk_tx: &Sender<StreamChunk>) -> StreamParseOutcome {
    let mut done = false;
    let mut payload = value;

    if let Some(result) = payload.get("result") {
        payload = result;
    }

    if let Some(error) = payload.get("error") {
        let error_message = error
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| error.as_str().map(str::to_string))
            .unwrap_or_else(|| "unknown stream error".to_string());
        let _ = chunk_tx.send(StreamChunk::Error(error_message));
        return StreamParseOutcome::Errored;
    }

    if let Some(status) = payload.get("status").and_then(Value::as_str) {
        if matches!(status, "failed" | "blocked") {
            let msg = payload
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("stream failed");
            let _ = chunk_tx.send(StreamChunk::Error(msg.to_string()));
            return StreamParseOutcome::Errored;
        }
        if matches!(status, "completed" | "done") {
            done = true;
        }
    }

    if let Some(kind) = payload
        .get("type")
        .and_then(Value::as_str)
        .or_else(|| payload.get("event").and_then(Value::as_str))
        .or_else(|| payload.get("kind").and_then(Value::as_str))
    {
        if matches!(kind, "done" | "completed" | "finish" | "finished") {
            done = true;
        }
    }

    if let Some(llm_output) = payload
        .get("llm")
        .and_then(|llm| llm.get("output"))
        .and_then(Value::as_str)
    {
        let _ = chunk_tx.send(StreamChunk::Token(llm_output.to_string()));
    }

    for field in ["token", "delta", "text", "content", "output"] {
        if let Some(text) = payload.get(field).and_then(Value::as_str) {
            if !text.trim().is_empty() {
                let _ = chunk_tx.send(StreamChunk::Token(text.to_string()));
                break;
            }
        }
    }

    if let Some(tool_call) = payload.get("tool_call").and_then(Value::as_object) {
        let title = tool_call
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("tool_call")
            .to_string();
        let detail = serde_json::to_string(tool_call).unwrap_or_else(|_| "{}".to_string());
        let _ = chunk_tx.send(StreamChunk::ToolCall { title, detail });
    }

    if let Some(tool_calls) = payload
        .get("tool")
        .and_then(|tool| tool.get("calls"))
        .and_then(Value::as_array)
    {
        for tool_call in tool_calls {
            let title = tool_call
                .get("name")
                .or_else(|| tool_call.get("tool"))
                .and_then(Value::as_str)
                .unwrap_or("tool_call")
                .to_string();
            let detail = serde_json::to_string(tool_call).unwrap_or_else(|_| "{}".to_string());
            let _ = chunk_tx.send(StreamChunk::ToolCall { title, detail });
        }
    }

    if done {
        StreamParseOutcome::Done
    } else {
        StreamParseOutcome::Continue
    }
}

async fn stream_chat_rpc_over_uds(
    socket_path: &str,
    input: String,
    model: String,
    agent_id: Option<String>,
    chunk_tx: &Sender<StreamChunk>,
) -> Result<(), String> {
    let mut stream = TokioUnixStream::connect(socket_path)
        .await
        .map_err(|err| format!("connect uds {} failed: {err}", socket_path))?;

    let request = JsonRpcRequest::new(
        json!(1),
        "RunAgent",
        json!({
            "input": input,
            "model": model,
            "agent_id": agent_id,
            "stream": true,
        }),
    );
    let payload =
        serde_json::to_vec(&request).map_err(|err| format!("encode rpc request failed: {err}"))?;

    stream
        .write_all(&payload)
        .await
        .map_err(|err| format!("write rpc request failed: {err}"))?;
    stream
        .shutdown()
        .await
        .map_err(|err| format!("shutdown write side failed: {err}"))?;

    let mut read_buf = [0_u8; 4096];
    let mut pending = Vec::<u8>::new();
    while let Ok(read) = stream.read(&mut read_buf).await {
        if read == 0 {
            break;
        }
        pending.extend_from_slice(&read_buf[..read]);

        while let Some(line_end) = pending.iter().position(|byte| *byte == b'\n') {
            let line = pending.drain(..=line_end).collect::<Vec<_>>();
            let mut line = String::from_utf8_lossy(&line).to_string();
            if let Some(stripped) = line.strip_prefix("data:") {
                line = stripped.to_string();
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
                match emit_chunk_from_value(&value, chunk_tx) {
                    StreamParseOutcome::Continue => {}
                    StreamParseOutcome::Done => return Ok(()),
                    StreamParseOutcome::Errored => return Err(STREAM_ERROR_EMITTED.to_string()),
                }
            } else {
                let _ = chunk_tx.send(StreamChunk::Token(trimmed.to_string()));
            }
        }
    }

    let trailing = String::from_utf8_lossy(&pending).trim().to_string();
    if !trailing.is_empty() {
        if let Ok(value) = serde_json::from_str::<Value>(&trailing) {
            if emit_chunk_from_value(&value, chunk_tx) == StreamParseOutcome::Errored {
                return Err(STREAM_ERROR_EMITTED.to_string());
            }
        } else {
            let _ = chunk_tx.send(StreamChunk::Token(trailing));
        }
    }

    Ok(())
}

fn spawn_chat_worker(
    socket_path: String,
    command_rx: Receiver<TuiCommand>,
    chunk_tx: Sender<StreamChunk>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(err) => {
                let _ = chunk_tx.send(StreamChunk::Error(format!(
                    "initialize background runtime failed: {err}"
                )));
                return;
            }
        };

        while let Ok(command) = command_rx.recv() {
            match command {
                TuiCommand::Chat {
                    input,
                    model,
                    agent_id,
                } => {
                    let result = runtime.block_on(stream_chat_rpc_over_uds(
                        &socket_path,
                        input,
                        model,
                        agent_id,
                        &chunk_tx,
                    ));
                    match result {
                        Ok(()) => {
                            let _ = chunk_tx.send(StreamChunk::Done);
                        }
                        Err(err) => {
                            if err != STREAM_ERROR_EMITTED {
                                let _ = chunk_tx.send(StreamChunk::Error(err));
                            }
                        }
                    }
                }
            }
        }
    })
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut AgentShellApp,
    chunk_rx: &Receiver<StreamChunk>,
) -> Result<(), DynError> {
    loop {
        while let Ok(chunk) = chunk_rx.try_recv() {
            app.handle_stream_chunk(chunk);
        }

        terminal.draw(|frame| app.render(frame))?;

        if event::poll(Duration::from_millis(50))? {
            let event = event::read()?;
            if let Event::Key(key_event) = event {
                if !app.handle_key_event(key_event) {
                    return Ok(());
                }
            }
        }
    }
}

pub fn run(socket_path: &str) -> Result<(), DynError> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let (command_tx, command_rx) = mpsc::channel::<TuiCommand>();
    let (chunk_tx, chunk_rx) = mpsc::channel::<StreamChunk>();
    let _worker_handle = spawn_chat_worker(socket_path.to_string(), command_rx, chunk_tx);

    let mut app = AgentShellApp::with_socket_path(socket_path);
    app.set_command_sender(command_tx);
    let event_loop_result = run_event_loop(&mut terminal, &mut app, &chunk_rx);

    app.command_tx = None;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    event_loop_result
}

#[cfg(test)]
#[test]
fn tui_app_handles_quit_key() {
    let mut app = AgentShellApp::new();
    let should_continue =
        app.handle_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(!should_continue);
}

#[cfg(test)]
#[test]
fn slash_commands_core_set_available() {
    let commands = AgentShellApp::supported_slash_commands();
    for required in [
        "/usage", "/events", "/tools", "/compact", "/model", "/approve", "/deny", "/session",
    ] {
        assert!(
            commands.contains(&required),
            "missing slash command: {required}"
        );
    }
}

#[cfg(test)]
#[test]
fn approval_queue_roundtrip() {
    assert!(approval_queue_roundtrip_probe());
}

#[cfg(test)]
#[test]
fn tui_multi_agent_panel_updates_on_events() {
    assert!(multi_agent_panel_updates_on_events_probe());
}

#[cfg(test)]
pub(crate) fn multi_agent_panel_updates_on_events_probe() -> bool {
    let mut app = AgentShellApp::new();

    let mut rpc = |method: &str, _params: Value| -> Result<Value, String> {
        match method {
            "ListLifecycleEvents" => Ok(json!({
                "events": [
                    {
                        "event_type": "orchestrator",
                        "payload": {
                            "kind": "orchestrator",
                            "phase": "delegated",
                            "task_id": "task-24",
                            "child_index": 1,
                            "agent_id": "agent-a",
                            "attempt": 1
                        }
                    },
                    {
                        "event_type": "orchestrator",
                        "payload": {
                            "kind": "orchestrator",
                            "phase": "retrying",
                            "task_id": "task-24",
                            "child_index": 1,
                            "agent_id": "agent-a",
                            "attempt": 1,
                            "error": "temporary failure"
                        }
                    },
                    {
                        "event_type": "orchestrator",
                        "payload": {
                            "kind": "orchestrator",
                            "phase": "completed",
                            "task_id": "task-24",
                            "child_index": 1,
                            "agent_id": "agent-a",
                            "attempt": 2
                        }
                    }
                ]
            })),
            _ => Err(format!("unexpected method: {method}")),
        }
    };

    app.execute_slash_command_with_rpc("/events 10", &mut rpc);

    if app.multi_agent_status.len() != 1 {
        return false;
    }
    let status = &app.multi_agent_status[0];
    if status.task_id != "task-24"
        || status.child_index != 1
        || status.agent_id != "agent-a"
        || status.phase != "completed"
        || status.attempt != 2
    {
        return false;
    }

    app.event_panel
        .entries
        .iter()
        .any(|entry| entry.contains("delegation task=task-24 child=1"))
}

#[cfg(test)]
pub(crate) fn approval_queue_roundtrip_probe() -> bool {
    let mut app = AgentShellApp::new();
    app.active_agent_id = Some("agent-1".to_string());

    let mut rpc = |method: &str, params: Value| -> Result<Value, String> {
        match method {
            "ListApprovalQueue" => Ok(json!({
                "approvals": [
                    {
                        "id": "req-1",
                        "tool": "mcp.fs.read_file",
                        "reason": "policy.ask"
                    }
                ]
            })),
            "ResolveApproval" => {
                assert_eq!(params["approval_id"], json!("req-1"));
                assert_eq!(params["decision"], json!("deny"));
                Ok(json!({
                    "approval_id": "req-1",
                    "decision": "deny",
                    "resolved": true
                }))
            }
            _ => Err(format!("unexpected method: {method}")),
        }
    };

    app.execute_slash_command_with_rpc("/approve", &mut rpc);
    if app.approval_queue.len() != 1 || app.approval_queue[0].id != "req-1" {
        return false;
    }

    app.execute_slash_command_with_rpc("/deny req-1", &mut rpc);
    app.approval_queue.is_empty()
        && app
            .messages
            .iter()
            .any(|message| message.content.contains("approval req-1 resolved as deny"))
}
