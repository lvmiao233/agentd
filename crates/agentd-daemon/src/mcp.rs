use agentd_core::profile::TrustLevel;
use agentd_core::AgentError;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::{timeout, Duration};
use tracing::info;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum McpTransport {
    Stdio,
}

pub(crate) type McpTrustLevel = TrustLevel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum McpServerHealth {
    Healthy,
    Degraded,
    Unreachable,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct McpToolDescriptor {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) input_schema: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct McpAvailableTool {
    pub(crate) server_id: String,
    pub(crate) tool_name: String,
    pub(crate) description: Option<String>,
    pub(crate) input_schema: Option<Value>,
    pub(crate) trust_level: McpTrustLevel,
    pub(crate) health: McpServerHealth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct McpServerConfig {
    pub(crate) name: String,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) transport: McpTransport,
    pub(crate) trust_level: McpTrustLevel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct McpRegistryEntry {
    pub(crate) server_id: String,
    pub(crate) capabilities: Vec<String>,
    pub(crate) trust_level: McpTrustLevel,
    pub(crate) health: McpServerHealth,
}

#[derive(Debug, Default)]
pub(crate) struct McpRegistry {
    entries: BTreeMap<String, McpRegistryEntry>,
}

impl McpRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn upsert(&mut self, entry: McpRegistryEntry) -> Option<McpRegistryEntry> {
        self.entries.insert(entry.server_id.clone(), entry)
    }

    #[cfg(test)]
    pub(crate) fn get(&self, server_id: &str) -> Option<&McpRegistryEntry> {
        self.entries.get(server_id)
    }

    pub(crate) fn remove(&mut self, server_id: &str) -> Option<McpRegistryEntry> {
        self.entries.remove(server_id)
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn list(&self) -> Vec<&McpRegistryEntry> {
        self.entries.values().collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct McpHostAuditEvent {
    pub(crate) server_id: String,
    pub(crate) action: String,
    pub(crate) success: bool,
    pub(crate) message: String,
}

#[derive(Debug)]
pub(crate) struct McpServerHandle {
    pub(crate) process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) transport: McpTransport,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) trust_level: McpTrustLevel,
    pub(crate) health: McpServerHealth,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) tools: Vec<McpToolDescriptor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct McpHostHealthSnapshot {
    pub(crate) total: usize,
    pub(crate) healthy: usize,
    pub(crate) degraded: usize,
    pub(crate) unreachable: usize,
}

#[derive(Debug)]
pub(crate) struct McpHost {
    servers: BTreeMap<String, McpServerHandle>,
    registry: McpRegistry,
    audit_events: Vec<McpHostAuditEvent>,
    initialize_timeout: Duration,
    call_timeout: Duration,
    stop_timeout: Duration,
    next_request_id: AtomicU64,
}

impl Default for McpHost {
    fn default() -> Self {
        Self {
            servers: BTreeMap::new(),
            registry: McpRegistry::new(),
            audit_events: Vec::new(),
            initialize_timeout: Duration::from_secs(3),
            call_timeout: Duration::from_secs(30),
            stop_timeout: Duration::from_secs(2),
            next_request_id: AtomicU64::new(3),
        }
    }
}

impl McpHost {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) async fn start_declared_servers(
        &mut self,
        configs: &[McpServerConfig],
    ) -> Result<(), AgentError> {
        let mut started_server_ids = Vec::new();

        for config in configs {
            match self.start_single_server(config).await {
                Ok(()) => {
                    started_server_ids.push(config.name.clone());
                    self.record_audit(
                        &config.name,
                        "startup",
                        true,
                        "mcp server started and initialized",
                    );
                }
                Err(err) => {
                    self.record_audit(
                        &config.name,
                        "startup",
                        false,
                        &format!("mcp server start failed: {err}"),
                    );

                    let rollback_err = self.rollback_started_servers(&started_server_ids).await;
                    if let Err(rollback_err) = rollback_err {
                        self.record_audit(
                            &config.name,
                            "rollback",
                            false,
                            &format!("rollback failed: {rollback_err}"),
                        );
                    } else {
                        self.record_audit(
                            &config.name,
                            "rollback",
                            true,
                            "rollback completed after startup failure",
                        );
                    }
                    return Err(err);
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn stop_all(&mut self) -> Result<(), AgentError> {
        let server_ids = self.servers.keys().cloned().collect::<Vec<_>>();
        let mut first_error: Option<AgentError> = None;

        for server_id in server_ids {
            if let Some(mut handle) = self.servers.remove(&server_id) {
                if let Err(err) = self.terminate_server(&server_id, &mut handle).await {
                    if first_error.is_none() {
                        first_error = Some(err);
                    }
                    self.record_audit(&server_id, "shutdown", false, "mcp server stop failed");
                } else {
                    self.record_audit(&server_id, "shutdown", true, "mcp server stopped");
                }
                self.registry.remove(&server_id);
            }
        }

        if let Some(err) = first_error {
            return Err(err);
        }

        Ok(())
    }

    pub(crate) fn refresh_health(&mut self) -> Result<McpHostHealthSnapshot, AgentError> {
        let server_ids = self.servers.keys().cloned().collect::<Vec<_>>();
        let mut healthy = 0usize;
        let mut degraded = 0usize;
        let mut unreachable = 0usize;

        for server_id in server_ids {
            let mut computed_health: Option<McpServerHealth> = None;
            if let Some(handle) = self.servers.get_mut(&server_id) {
                let is_running = match handle.process.try_wait() {
                    Ok(None) => true,
                    Ok(Some(_)) => false,
                    Err(err) => {
                        return Err(AgentError::Runtime(format!(
                            "check mcp server health failed for {server_id}: {err}"
                        )));
                    }
                };

                handle.health = if is_running && handle.tools.is_empty() {
                    McpServerHealth::Degraded
                } else if is_running {
                    McpServerHealth::Healthy
                } else {
                    McpServerHealth::Unreachable
                };
                computed_health = Some(handle.health);
            }

            self.sync_registry_entry_from_handle(&server_id);

            if let Some(health) = computed_health {
                match health {
                    McpServerHealth::Healthy => healthy = healthy.saturating_add(1),
                    McpServerHealth::Degraded => degraded = degraded.saturating_add(1),
                    McpServerHealth::Unreachable => unreachable = unreachable.saturating_add(1),
                }
            }
        }

        Ok(McpHostHealthSnapshot {
            total: self.servers.len(),
            healthy,
            degraded,
            unreachable,
        })
    }

    pub(crate) fn list_available_tools(&self) -> Vec<McpAvailableTool> {
        self.servers
            .iter()
            .filter(|(_, handle)| handle.health == McpServerHealth::Healthy)
            .flat_map(|(server_id, handle)| {
                handle.tools.iter().map(move |tool| McpAvailableTool {
                    server_id: server_id.clone(),
                    tool_name: tool.name.clone(),
                    description: tool.description.clone(),
                    input_schema: tool.input_schema.clone(),
                    trust_level: handle.trust_level,
                    health: handle.health,
                })
            })
            .collect()
    }

    pub(crate) fn list_servers(&self) -> Vec<McpRegistryEntry> {
        self.registry.entries.values().cloned().collect()
    }

    pub(crate) async fn invoke_tool(
        &mut self,
        server_id: &str,
        tool_name: &str,
        args: Value,
    ) -> Result<Value, AgentError> {
        let handle = self
            .servers
            .get_mut(server_id)
            .ok_or_else(|| AgentError::NotFound(format!("mcp server not found: {server_id}")))?;

        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let request = serde_json::to_string(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": args,
            },
        }))
        .map_err(|err| {
            AgentError::Runtime(format!(
                "serialize tools/call request failed for {server_id}:{tool_name}: {err}"
            ))
        })?;

        handle
            .stdin
            .write_all(request.as_bytes())
            .await
            .map_err(|err| {
                AgentError::Runtime(format!(
                    "write tools/call request failed for {server_id}:{tool_name}: {err}"
                ))
            })?;
        handle.stdin.write_all(b"\n").await.map_err(|err| {
            AgentError::Runtime(format!(
                "write tools/call request delimiter failed for {server_id}:{tool_name}: {err}"
            ))
        })?;
        handle.stdin.flush().await.map_err(|err| {
            AgentError::Runtime(format!(
                "flush tools/call request failed for {server_id}:{tool_name}: {err}"
            ))
        })?;

        let mut response_line = String::new();
        let read = timeout(
            self.call_timeout,
            handle.stdout.read_line(&mut response_line),
        )
        .await
        .map_err(|_| {
            AgentError::Runtime(format!("tools/call timed out for {server_id}:{tool_name}"))
        })?
        .map_err(|err| {
            AgentError::Runtime(format!(
                "read tools/call response failed for {server_id}:{tool_name}: {err}"
            ))
        })?;

        if read == 0 {
            return Err(AgentError::Runtime(format!(
                "tools/call response was empty for {server_id}:{tool_name}"
            )));
        }

        let parsed: Value = serde_json::from_str(response_line.trim()).map_err(|err| {
            AgentError::Runtime(format!(
                "parse tools/call response failed for {server_id}:{tool_name}: {err}"
            ))
        })?;

        let response_id = parsed.get("id").and_then(Value::as_u64).unwrap_or_default();
        if response_id != request_id {
            return Err(AgentError::Runtime(format!(
                "tools/call response id mismatch for {server_id}:{tool_name} (expected={request_id}, got={response_id})"
            )));
        }

        if let Some(error) = parsed.get("error") {
            return Err(AgentError::Runtime(format!(
                "tools/call returned error for {server_id}:{tool_name}: {error}"
            )));
        }

        parsed.get("result").cloned().ok_or_else(|| {
            AgentError::Runtime(format!(
                "tools/call response missing result for {server_id}:{tool_name}"
            ))
        })
    }

    pub(crate) async fn onboard_server(
        &mut self,
        config: McpServerConfig,
    ) -> Result<McpRegistryEntry, AgentError> {
        if self.servers.contains_key(&config.name) {
            return Err(AgentError::InvalidInput(format!(
                "mcp server already exists: {}",
                config.name
            )));
        }

        let server_id = config.name.clone();
        match self.start_single_server(&config).await {
            Ok(()) => {
                self.record_audit(
                    &server_id,
                    "onboarding",
                    true,
                    "third-party mcp server onboarded",
                );
                self.registry
                    .entries
                    .get(&server_id)
                    .cloned()
                    .ok_or_else(|| {
                        AgentError::Runtime(format!(
                            "onboarded mcp server missing from registry: {server_id}"
                        ))
                    })
            }
            Err(err) => {
                self.record_audit(
                    &server_id,
                    "onboarding",
                    false,
                    &format!("third-party onboarding failed: {err}"),
                );
                Err(err)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn registry(&self) -> &McpRegistry {
        &self.registry
    }

    #[cfg(test)]
    pub(crate) fn server_count(&self) -> usize {
        self.servers.len()
    }

    #[cfg(test)]
    pub(crate) fn server_handle(&self, server_id: &str) -> Option<&McpServerHandle> {
        self.servers.get(server_id)
    }

    #[cfg(test)]
    pub(crate) fn audit_events(&self) -> &[McpHostAuditEvent] {
        &self.audit_events
    }

    async fn start_single_server(&mut self, config: &McpServerConfig) -> Result<(), AgentError> {
        let mut command = Command::new(&config.command);
        command
            .args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        let mut child = command.spawn().map_err(|err| {
            AgentError::Runtime(format!("spawn mcp server {} failed: {err}", config.name))
        })?;

        let mut stdin = child.stdin.take().ok_or_else(|| {
            AgentError::Runtime(format!("mcp server {} missing stdin pipe", config.name))
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AgentError::Runtime(format!("mcp server {} missing stdout pipe", config.name))
        })?;

        let mut stdout = BufReader::new(stdout);
        let tools = match self
            .perform_initialize(&mut stdin, &mut stdout, config)
            .await
        {
            Ok(tools) => tools,
            Err(err) => {
                let _ = terminate_child_process(&mut child, self.stop_timeout).await;
                return Err(err);
            }
        };

        if let Ok(Some(status)) = child.try_wait() {
            return Err(AgentError::Runtime(format!(
                "mcp server {} exited after initialize handshake: {status}",
                config.name
            )));
        }

        self.servers.insert(
            config.name.clone(),
            McpServerHandle {
                process: child,
                stdin,
                stdout,
                transport: config.transport,
                trust_level: config.trust_level,
                health: McpServerHealth::Healthy,
                tools,
            },
        );
        self.sync_registry_entry_from_handle(&config.name);

        Ok(())
    }

    fn sync_registry_entry_from_handle(&mut self, server_id: &str) {
        let Some((capabilities, trust_level, health)) = self.servers.get(server_id).map(|handle| {
            (
                handle.tools.iter().map(|tool| tool.name.clone()).collect(),
                handle.trust_level,
                handle.health,
            )
        }) else {
            return;
        };

        let entry = McpRegistryEntry {
            server_id: server_id.to_string(),
            capabilities,
            trust_level,
            health,
        };
        self.registry.upsert(entry);
    }

    async fn perform_initialize(
        &self,
        stdin: &mut ChildStdin,
        stdout: &mut BufReader<ChildStdout>,
        config: &McpServerConfig,
    ) -> Result<Vec<McpToolDescriptor>, AgentError> {
        let init_request = serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {
                    "name": "agentd",
                    "version": "0.1.0"
                }
            }
        }))
        .map_err(|err| {
            AgentError::Runtime(format!(
                "serialize initialize request for {} failed: {err}",
                config.name
            ))
        })?;

        write_json_line(stdin, &init_request, &config.name, "initialize request").await?;

        let mut response_line = String::new();
        let bytes = timeout(
            self.initialize_timeout,
            stdout.read_line(&mut response_line),
        )
        .await
        .map_err(|_| {
            AgentError::Runtime(format!(
                "initialize handshake timed out for {}",
                config.name
            ))
        })?
        .map_err(|err| {
            AgentError::Runtime(format!(
                "read initialize response from {} failed: {err}",
                config.name
            ))
        })?;
        if bytes == 0 {
            return Err(AgentError::Runtime(format!(
                "initialize handshake returned empty response for {}",
                config.name
            )));
        }

        let response_json: Value = serde_json::from_str(response_line.trim()).map_err(|err| {
            AgentError::Runtime(format!(
                "parse initialize response for {} failed: {err}",
                config.name
            ))
        })?;

        validate_initialize_response(&response_json, config)?;

        let initialized_request = serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }))
        .map_err(|err| {
            AgentError::Runtime(format!(
                "serialize initialized notification for {} failed: {err}",
                config.name
            ))
        })?;
        write_json_line(
            stdin,
            &initialized_request,
            &config.name,
            "initialized notification",
        )
        .await?;

        let tools_list_request = serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }))
        .map_err(|err| {
            AgentError::Runtime(format!(
                "serialize tools/list request for {} failed: {err}",
                config.name
            ))
        })?;
        write_json_line(
            stdin,
            &tools_list_request,
            &config.name,
            "tools/list request",
        )
        .await?;

        let mut tools_list_response_line = String::new();
        let bytes = timeout(
            self.initialize_timeout,
            stdout.read_line(&mut tools_list_response_line),
        )
        .await
        .map_err(|_| AgentError::Runtime(format!("tools/list timed out for {}", config.name)))?
        .map_err(|err| {
            AgentError::Runtime(format!(
                "read tools/list response from {} failed: {err}",
                config.name
            ))
        })?;
        if bytes == 0 {
            return Err(AgentError::Runtime(format!(
                "tools/list returned empty response for {}",
                config.name
            )));
        }

        let tools_list_response_json: Value = serde_json::from_str(tools_list_response_line.trim())
            .map_err(|err| {
                AgentError::Runtime(format!(
                    "parse tools/list response for {} failed: {err}",
                    config.name
                ))
            })?;

        parse_tools_list_response(&tools_list_response_json, config)
    }

    async fn rollback_started_servers(&mut self, server_ids: &[String]) -> Result<(), AgentError> {
        for server_id in server_ids.iter().rev() {
            if let Some(mut handle) = self.servers.remove(server_id) {
                self.terminate_server(server_id, &mut handle).await?;
                self.registry.remove(server_id);
            }
        }
        Ok(())
    }

    async fn terminate_server(
        &self,
        server_id: &str,
        handle: &mut McpServerHandle,
    ) -> Result<(), AgentError> {
        terminate_child_process(&mut handle.process, self.stop_timeout)
            .await
            .map_err(|err| {
                AgentError::Runtime(format!("stop mcp server {server_id} failed: {err}"))
            })
    }

    fn record_audit(&mut self, server_id: &str, action: &str, success: bool, message: &str) {
        self.audit_events.push(McpHostAuditEvent {
            server_id: server_id.to_string(),
            action: action.to_string(),
            success,
            message: message.to_string(),
        });
    }
}

async fn write_json_line(
    stdin: &mut ChildStdin,
    payload: &str,
    server_name: &str,
    operation: &str,
) -> Result<(), AgentError> {
    stdin.write_all(payload.as_bytes()).await.map_err(|err| {
        AgentError::Runtime(format!("write {operation} to {server_name} failed: {err}"))
    })?;
    stdin.write_all(b"\n").await.map_err(|err| {
        AgentError::Runtime(format!(
            "write {operation} delimiter to {server_name} failed: {err}"
        ))
    })?;
    stdin.flush().await.map_err(|err| {
        AgentError::Runtime(format!("flush {operation} to {server_name} failed: {err}"))
    })?;
    Ok(())
}

async fn terminate_child_process(
    child: &mut Child,
    stop_timeout: Duration,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return Ok(());
    }

    child.start_kill()?;
    let wait_result = timeout(stop_timeout, child.wait()).await;
    match wait_result {
        Ok(status) => {
            let _ = status?;
            Ok(())
        }
        Err(_) => Err("timed out waiting mcp process stop".into()),
    }
}

fn validate_initialize_response(
    response_json: &Value,
    config: &McpServerConfig,
) -> Result<(), AgentError> {
    if let Some(error) = response_json.get("error") {
        return Err(AgentError::Runtime(format!(
            "initialize returned error for {}: {error}",
            config.name
        )));
    }

    let response_id = response_json
        .get("id")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if response_id != 1 {
        return Err(AgentError::Runtime(format!(
            "initialize response id mismatch for {} (expected=1, got={response_id})",
            config.name
        )));
    }

    response_json.get("result").ok_or_else(|| {
        AgentError::Runtime(format!(
            "initialize response missing result for {}",
            config.name
        ))
    })?;

    Ok(())
}

fn parse_tools_list_response(
    response_json: &Value,
    config: &McpServerConfig,
) -> Result<Vec<McpToolDescriptor>, AgentError> {
    if let Some(error) = response_json.get("error") {
        return Err(AgentError::Runtime(format!(
            "tools/list returned error for {}: {error}",
            config.name
        )));
    }

    let response_id = response_json
        .get("id")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if response_id != 2 {
        return Err(AgentError::Runtime(format!(
            "tools/list response id mismatch for {} (expected=2, got={response_id})",
            config.name
        )));
    }

    let tools = response_json
        .get("result")
        .and_then(|result| result.get("tools"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AgentError::Runtime(format!(
                "tools/list response missing result.tools for {}",
                config.name
            ))
        })?;

    let mut parsed = Vec::with_capacity(tools.len());
    for tool in tools {
        let Some(name) = tool.get("name").and_then(Value::as_str) else {
            continue;
        };
        if name.trim().is_empty() {
            continue;
        }

        parsed.push(McpToolDescriptor {
            name: name.to_string(),
            description: tool
                .get("description")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            input_schema: tool
                .get("inputSchema")
                .cloned()
                .or_else(|| tool.get("input_schema").cloned())
                .or_else(|| tool.get("parameters").cloned()),
        });
    }

    Ok(parsed)
}

#[derive(Debug, Deserialize)]
struct RawMcpConfigFile {
    server: Option<RawMcpServerConfig>,
}

#[derive(Debug, Deserialize)]
struct RawMcpServerConfig {
    name: Option<String>,
    command: Option<String>,
    args: Option<Vec<String>>,
    transport: Option<String>,
    trust_level: Option<String>,
}

fn require_non_empty_string(
    value: Option<String>,
    field_name: &str,
    config_path: &Path,
) -> Result<String, AgentError> {
    let Some(value) = value else {
        return Err(AgentError::InvalidInput(format!(
            "mcp config {} missing required field {field_name}",
            config_path.display()
        )));
    };

    if value.trim().is_empty() {
        return Err(AgentError::InvalidInput(format!(
            "mcp config {} has empty field {field_name}",
            config_path.display()
        )));
    }

    Ok(value)
}

fn parse_transport(value: Option<String>, config_path: &Path) -> Result<McpTransport, AgentError> {
    let raw = require_non_empty_string(value, "server.transport", config_path)?;
    match raw.as_str() {
        "stdio" => Ok(McpTransport::Stdio),
        _ => Err(AgentError::InvalidInput(format!(
            "mcp config {} invalid transport `{raw}` (expected: stdio)",
            config_path.display()
        ))),
    }
}

pub(crate) fn parse_trust_level(raw: &str) -> Result<McpTrustLevel, AgentError> {
    McpTrustLevel::parse(raw)
}

fn parse_trust_level_from_config(
    value: Option<String>,
    config_path: &Path,
) -> Result<McpTrustLevel, AgentError> {
    let raw = require_non_empty_string(value, "server.trust_level", config_path)?;
    parse_trust_level(&raw)
}

fn parse_mcp_server_config_file(
    config_path: &Path,
    content: &str,
) -> Result<McpServerConfig, AgentError> {
    let parsed: RawMcpConfigFile = toml::from_str(content).map_err(|err| {
        AgentError::InvalidInput(format!(
            "mcp config {} parse failed: {err}",
            config_path.display()
        ))
    })?;

    let Some(server) = parsed.server else {
        return Err(AgentError::InvalidInput(format!(
            "mcp config {} missing [server] section",
            config_path.display()
        )));
    };

    let Some(args) = server.args else {
        return Err(AgentError::InvalidInput(format!(
            "mcp config {} missing required field server.args",
            config_path.display()
        )));
    };

    Ok(McpServerConfig {
        name: require_non_empty_string(server.name, "server.name", config_path)?,
        command: require_non_empty_string(server.command, "server.command", config_path)?,
        args,
        transport: parse_transport(server.transport, config_path)?,
        trust_level: parse_trust_level_from_config(server.trust_level, config_path)?,
    })
}

pub(crate) fn load_mcp_server_configs(
    configs_dir: &Path,
) -> Result<Vec<McpServerConfig>, Box<dyn std::error::Error>> {
    if !configs_dir.exists() {
        info!(configs_dir = %configs_dir.display(), "MCP config directory not found, skipping MCP config load");
        return Ok(Vec::new());
    }

    if !configs_dir.is_dir() {
        return Err(AgentError::InvalidInput(format!(
            "mcp config path is not a directory: {}",
            configs_dir.display()
        ))
        .into());
    }

    let mut config_paths = std::fs::read_dir(configs_dir)?
        .filter_map(|entry_result| entry_result.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("toml"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    config_paths.sort();

    let mut configs = Vec::with_capacity(config_paths.len());
    for config_path in config_paths {
        let content = std::fs::read_to_string(&config_path).map_err(|err| {
            AgentError::InvalidInput(format!(
                "mcp config {} read failed: {err}",
                config_path.display()
            ))
        })?;
        let config = parse_mcp_server_config_file(&config_path, &content)?;
        configs.push(config);
    }

    Ok(configs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_stdio_server(name: &str, capability: &str) -> McpServerConfig {
        McpServerConfig {
            name: name.to_string(),
            command: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                format!(
                    "while IFS= read -r _line; do if printf '%s' \"$_line\" | grep -q '\"method\":\"initialize\"'; then printf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"protocolVersion\":\"2025-03-26\",\"capabilities\":{{\"tools\":{{\"listChanged\":false}}}},\"serverInfo\":{{\"name\":\"{name}\",\"version\":\"0.1.0\"}}}}}}'; elif printf '%s' \"$_line\" | grep -q '\"method\":\"tools/list\"'; then printf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{{\"tools\":[{{\"name\":\"{capability}\",\"description\":\"test tool\",\"inputSchema\":{{\"type\":\"object\",\"properties\":{{}}}}}}]}}}}'; elif printf '%s' \"$_line\" | grep -q '\"method\":\"tools/call\"'; then printf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{{\"content\":[{{\"type\":\"text\",\"text\":\"tool-ok\"}}],\"isError\":false}}}}'; fi; done"
                ),
            ],
            transport: McpTransport::Stdio,
            trust_level: McpTrustLevel::Builtin,
        }
    }

    fn invalid_initialize_stdio_server(name: &str) -> McpServerConfig {
        McpServerConfig {
            name: name.to_string(),
            command: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                "read _line; printf '%s\\n' 'not-json'; sleep 30".to_string(),
            ],
            transport: McpTransport::Stdio,
            trust_level: McpTrustLevel::Builtin,
        }
    }

    #[test]
    fn mcp_registry_roundtrip_entry() {
        let mut registry = McpRegistry::new();
        let previous = registry.upsert(McpRegistryEntry {
            server_id: "mcp-search".to_string(),
            capabilities: vec!["search.query".to_string(), "search.fetch".to_string()],
            trust_level: McpTrustLevel::Verified,
            health: McpServerHealth::Healthy,
        });
        assert!(previous.is_none());

        let entry = registry.get("mcp-search").expect("entry should exist");
        assert_eq!(entry.server_id, "mcp-search");
        assert_eq!(entry.trust_level, McpTrustLevel::Verified);
        assert_eq!(registry.len(), 1);

        let digest = entry.capabilities.join(",");
        assert_eq!(digest, "search.query,search.fetch");

        let replaced = registry.upsert(McpRegistryEntry {
            server_id: "mcp-search".to_string(),
            capabilities: vec!["search.query".to_string()],
            trust_level: McpTrustLevel::Community,
            health: McpServerHealth::Degraded,
        });
        assert!(replaced.is_some());
        assert_eq!(registry.len(), 1);
        let updated = registry
            .get("mcp-search")
            .expect("updated entry should exist");
        assert_eq!(updated.capabilities.join(","), "search.query");
        assert_eq!(updated.trust_level, McpTrustLevel::Community);
        assert_eq!(updated.health, McpServerHealth::Degraded);

        let removed = registry.remove("mcp-search");
        assert!(removed.is_some());
        assert!(registry.get("mcp-search").is_none());
        assert!(registry.is_empty());

        let previous = registry.upsert(McpRegistryEntry {
            server_id: "mcp-git".to_string(),
            capabilities: vec!["git.status".to_string()],
            trust_level: McpTrustLevel::Community,
            health: McpServerHealth::Unreachable,
        });
        assert!(previous.is_none());
        assert_eq!(registry.list().len(), 1);
    }

    fn short_lived_stdio_server(name: &str, capability: &str) -> McpServerConfig {
        McpServerConfig {
            name: name.to_string(),
            command: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                format!(
                    "while IFS= read -r _line; do if printf '%s' \"$_line\" | grep -q '\"method\":\"initialize\"'; then printf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"protocolVersion\":\"2025-03-26\",\"capabilities\":{{\"tools\":{{\"listChanged\":false}}}},\"serverInfo\":{{\"name\":\"{name}\",\"version\":\"0.1.0\"}}}}}}'; elif printf '%s' \"$_line\" | grep -q '\"method\":\"tools/list\"'; then printf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{{\"tools\":[{{\"name\":\"{capability}\",\"description\":\"test tool\",\"inputSchema\":{{\"type\":\"object\",\"properties\":{{}}}}}}]}}}}'; break; fi; done; sleep 0.1"
                ),
            ],
            transport: McpTransport::Stdio,
            trust_level: McpTrustLevel::Builtin,
        }
    }

    #[test]
    fn mcp_registry_rejects_unknown_trust() {
        let err = parse_trust_level("unknown").expect_err("unknown trust level should fail");
        let message = err.to_string();
        assert!(message.contains("invalid trust_level"));
    }

    #[tokio::test]
    async fn mcp_host_starts_declared_servers() {
        let mut host = McpHost::new();
        let configs = vec![
            valid_stdio_server("mcp-fs", "fs.read_file"),
            valid_stdio_server("mcp-shell", "shell.execute"),
        ];

        host.start_declared_servers(&configs)
            .await
            .expect("mcp host should start configured servers");

        let health = host
            .refresh_health()
            .expect("mcp health refresh should succeed");
        assert_eq!(health.total, configs.len());
        assert_eq!(health.healthy, configs.len());
        assert_eq!(host.registry().len(), configs.len());

        let fs_handle = host
            .server_handle("mcp-fs")
            .expect("mcp-fs handle should be cached");
        assert_eq!(
            fs_handle
                .tools
                .iter()
                .map(|tool| tool.name.clone())
                .collect::<Vec<_>>(),
            vec!["fs.read_file"]
        );
        assert_eq!(fs_handle.health, McpServerHealth::Healthy);
        assert_eq!(fs_handle.transport, McpTransport::Stdio);
        assert_eq!(fs_handle.trust_level, McpTrustLevel::Builtin);

        host.stop_all().await.expect("mcp host stop should succeed");
        assert_eq!(host.server_count(), 0);
        assert!(host.registry().is_empty());
    }

    #[tokio::test]
    async fn mcp_host_rolls_back_on_init_failure() {
        let mut host = McpHost::new();
        let configs = vec![
            valid_stdio_server("mcp-fs", "fs.read_file"),
            invalid_initialize_stdio_server("mcp-bad"),
        ];

        let err = host
            .start_declared_servers(&configs)
            .await
            .expect_err("initialize failure should rollback host startup");
        let error_text = err.to_string();
        assert!(
            error_text.contains("initialize") || error_text.contains("parse"),
            "unexpected error text: {error_text}"
        );

        assert_eq!(
            host.server_count(),
            0,
            "started servers must be rolled back"
        );
        assert!(
            host.registry().is_empty(),
            "registry entries must be rolled back"
        );

        let startup_failure = host.audit_events().iter().any(|event| {
            event.action == "startup" && !event.success && event.server_id == "mcp-bad"
        });
        assert!(startup_failure, "startup failure must be audited");

        let rollback_event = host.audit_events().iter().any(|event| {
            event.action == "rollback" && event.success && event.server_id == "mcp-bad"
        });
        assert!(rollback_event, "rollback completion must be audited");
    }

    #[tokio::test]
    async fn mcp_registry_syncs_capabilities_from_initialize() {
        let mut host = McpHost::new();
        let configs = vec![valid_stdio_server("mcp-search", "search.query")];

        host.start_declared_servers(&configs)
            .await
            .expect("mcp host should start configured server");

        let entry = host
            .registry()
            .get("mcp-search")
            .expect("registry entry should be present");
        assert_eq!(entry.capabilities, vec!["search.query".to_string()]);
        assert_eq!(entry.health, McpServerHealth::Healthy);

        let available_tools = host.list_available_tools();
        assert!(available_tools
            .iter()
            .any(|tool| tool.server_id == "mcp-search" && tool.tool_name == "search.query"));

        host.stop_all().await.expect("mcp host stop should succeed");
    }

    #[tokio::test]
    async fn unhealthy_server_removed_from_available_tools() {
        let mut host = McpHost::new();
        let configs = vec![
            valid_stdio_server("mcp-fs", "fs.read_file"),
            short_lived_stdio_server("mcp-transient", "transient.echo"),
        ];

        host.start_declared_servers(&configs)
            .await
            .expect("mcp host should start configured servers");

        tokio::time::sleep(Duration::from_millis(250)).await;
        let health = host
            .refresh_health()
            .expect("mcp health refresh should succeed");
        assert_eq!(health.total, 2);
        assert_eq!(health.healthy, 1);
        assert_eq!(health.unreachable, 1);

        let available_tools = host.list_available_tools();
        assert!(available_tools
            .iter()
            .any(|tool| tool.server_id == "mcp-fs" && tool.tool_name == "fs.read_file"));
        assert!(!available_tools
            .iter()
            .any(|tool| tool.server_id == "mcp-transient" && tool.tool_name == "transient.echo"));

        host.stop_all().await.expect("mcp host stop should succeed");
    }

    #[tokio::test]
    async fn mcp_onboard_server_registers_third_party_capability() {
        let mut host = McpHost::new();
        host.start_declared_servers(&[valid_stdio_server("mcp-fs", "fs.read_file")])
            .await
            .expect("builtin server should start");

        let onboarded = host
            .onboard_server(McpServerConfig {
                name: "mcp-figma".to_string(),
                command: "/bin/sh".to_string(),
                args: vec![
                    "-c".to_string(),
                    "while IFS= read -r _line; do if printf '%s' \"$_line\" | grep -q '\"method\":\"initialize\"'; then printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":\"2025-03-26\",\"capabilities\":{\"tools\":{\"listChanged\":false}},\"serverInfo\":{\"name\":\"mcp-figma\",\"version\":\"0.1.0\"}}}'; elif printf '%s' \"$_line\" | grep -q '\"method\":\"tools/list\"'; then printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"tools\":[{\"name\":\"figma.export_frame\",\"description\":\"export frame\",\"inputSchema\":{\"type\":\"object\",\"properties\":{}}}]}}'; fi; done; sleep 30"
                        .to_string(),
                ],
                transport: McpTransport::Stdio,
                trust_level: McpTrustLevel::Community,
            })
            .await
            .expect("third-party server should onboard successfully");

        assert_eq!(onboarded.server_id, "mcp-figma");
        assert_eq!(
            onboarded.capabilities,
            vec!["figma.export_frame".to_string()]
        );
        assert_eq!(onboarded.trust_level, McpTrustLevel::Community);

        let servers = host.list_servers();
        assert!(servers.iter().any(|entry| entry.server_id == "mcp-fs"));
        assert!(servers.iter().any(|entry| entry.server_id == "mcp-figma"));

        host.stop_all().await.expect("mcp host stop should succeed");
    }

    #[tokio::test]
    async fn third_party_onboarding_failure_isolated_from_builtin_tools() {
        let mut host = McpHost::new();
        host.start_declared_servers(&[valid_stdio_server("mcp-fs", "fs.read_file")])
            .await
            .expect("builtin server should start");

        let err = host
            .onboard_server(invalid_initialize_stdio_server("mcp-figma"))
            .await
            .expect_err("invalid third-party handshake should fail");
        let err_text = err.to_string();
        assert!(
            err_text.contains("initialize") || err_text.contains("parse"),
            "unexpected onboarding error: {err_text}"
        );

        let health = host
            .refresh_health()
            .expect("refresh mcp health should succeed");
        assert_eq!(
            health.total, 1,
            "failed onboarding should not purge builtin"
        );
        assert_eq!(health.healthy, 1);

        let available_tools = host.list_available_tools();
        assert!(available_tools
            .iter()
            .any(|tool| tool.server_id == "mcp-fs" && tool.tool_name == "fs.read_file"));
        assert!(!available_tools
            .iter()
            .any(|tool| tool.server_id == "mcp-figma"));

        let failure_audited = host.audit_events().iter().any(|event| {
            event.action == "onboarding" && !event.success && event.server_id == "mcp-figma"
        });
        assert!(
            failure_audited,
            "onboarding handshake failure should be captured in audit"
        );

        host.stop_all().await.expect("mcp host stop should succeed");
    }
}
