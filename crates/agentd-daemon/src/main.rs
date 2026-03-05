mod cgroup;
mod firecracker;
mod lifecycle;
mod mcp;
mod ws_bridge;

use agentd_core::audit::{
    AuditContext, EventPayload, EventResult, EventSeverity, EventType, PolicyReplayReference,
};
use agentd_core::policy::{
    PolicyAgentContext, PolicyResourceContext, PolicyTimeContext, PolicyToolContext,
};
use agentd_core::profile::{ModelConfig, PermissionPolicy, TrustLevel};
use agentd_core::{
    AgentError, AgentLifecycleState, AgentProfile, AuditEvent, PolicyDecision, PolicyEngine,
    PolicyEngineLayers, PolicyEvaluation, PolicyGatewayDecision, PolicyInputContext, PolicyLayer,
    PolicyRule, RegorusPolicyEngine, SessionPolicyOverrides,
};
use agentd_protocol::{
    A2ATask, A2ATaskEvent, A2ATaskState, CreateA2ATaskRequest, CreateA2ATaskResponse,
    GetA2ATaskResponse, JsonRpcRequest, JsonRpcResponse,
};
use agentd_store::agent::{delegation_candidates_from_profiles, ContextSessionSnapshot};
use agentd_store::{AgentStore, OneApiMapping, SqliteStore, UsageWindow};
use cgroup::{CgroupManager, CgroupResourceLimits};
use chrono::Utc;
use clap::Parser;
use lifecycle::{FirecrackerRuntimeSpec, LifecycleManager, ManagedAgentSpec, ManagedRuntimeSpec};
use mcp::{load_mcp_server_configs, McpHost};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::SocketAddr;
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UnixListener};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, watch, Mutex, RwLock};
use tokio::time::{timeout, Duration};
use tracing::{error, info, warn};

const AGENTD_MDNS_SERVICE_TYPE: &str = "_agentd._tcp.local.";
const AGENTD_MDNS_DISCOVERY_TIMEOUT_MS: u64 = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiscoveryAgentRecord {
    agent_id: String,
    name: String,
    model: String,
    provider: String,
    endpoint: String,
    source: String,
    health: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RegistryRegisterRequest {
    agent_id: String,
    name: String,
    model: String,
    provider: String,
    endpoint: String,
    #[serde(default)]
    health: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegistryAgentEntry {
    agent_id: String,
    name: String,
    model: String,
    provider: String,
    endpoint: String,
    health: String,
    updated_at: String,
}

trait LanDiscovery: Send + Sync {
    fn discover(&self) -> Result<Vec<DiscoveryAgentRecord>, String>;
}

#[derive(Debug, Default)]
struct MdnsLanDiscovery;

impl LanDiscovery for MdnsLanDiscovery {
    fn discover(&self) -> Result<Vec<DiscoveryAgentRecord>, String> {
        let daemon =
            ServiceDaemon::new().map_err(|err| format!("create mdns daemon failed: {err}"))?;
        let receiver = daemon
            .browse(AGENTD_MDNS_SERVICE_TYPE)
            .map_err(|err| format!("browse mdns service failed: {err}"))?;
        let deadline =
            std::time::Instant::now() + Duration::from_millis(AGENTD_MDNS_DISCOVERY_TIMEOUT_MS);
        let mut discovered = HashMap::new();

        while std::time::Instant::now() < deadline {
            if let Ok(ServiceEvent::ServiceResolved(info)) =
                receiver.recv_timeout(Duration::from_millis(100))
            {
                let endpoint = info
                    .get_property("endpoint")
                    .map(|value| value.val_str().to_string())
                    .unwrap_or_else(|| {
                        let host = info
                            .get_addresses()
                            .iter()
                            .next()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| "127.0.0.1".to_string());
                        format!("http://{}:{}", host, info.get_port())
                    });
                let agent_id = info
                    .get_property("agent_id")
                    .map(|value| value.val_str().to_string())
                    .unwrap_or_else(|| info.get_fullname().to_string());
                let name = info
                    .get_property("name")
                    .map(|value| value.val_str().to_string())
                    .unwrap_or_else(|| info.get_fullname().to_string());
                let model = info
                    .get_property("model")
                    .map(|value| value.val_str().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                let provider = info
                    .get_property("provider")
                    .map(|value| value.val_str().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                let health = info
                    .get_property("health")
                    .map(|value| value.val_str().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                discovered.insert(
                    agent_id.clone(),
                    DiscoveryAgentRecord {
                        agent_id,
                        name,
                        model,
                        provider,
                        endpoint,
                        source: "lan".to_string(),
                        health,
                    },
                );
            }
        }

        let _ = daemon.stop_browse(AGENTD_MDNS_SERVICE_TYPE);
        let _ = daemon.shutdown();

        Ok(discovered.into_values().collect())
    }
}

#[cfg(test)]
#[derive(Debug)]
struct StaticLanDiscovery {
    records: Vec<DiscoveryAgentRecord>,
}

#[cfg(test)]
impl LanDiscovery for StaticLanDiscovery {
    fn discover(&self) -> Result<Vec<DiscoveryAgentRecord>, String> {
        Ok(self.records.clone())
    }
}

#[derive(Parser, Debug)]
#[command(name = "agentd")]
#[command(about = "Agentd daemon - System-level AI Agent runtime")]
struct Args {
    #[arg(long, default_value = "configs/agentd.toml")]
    config: String,

    #[arg(long)]
    health_host: Option<String>,

    #[arg(long)]
    health_port: Option<u16>,

    #[arg(long)]
    db_path: Option<String>,

    #[arg(long)]
    one_api_enabled: Option<bool>,

    #[arg(long, short)]
    verbose: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct AppConfig {
    #[serde(default)]
    daemon: DaemonConfig,
    #[serde(default)]
    one_api: OneApiConfig,
}

#[derive(Debug, Clone, Deserialize)]
struct DaemonConfig {
    #[serde(default = "default_health_host")]
    health_host: String,
    #[serde(default = "default_health_port")]
    health_port: u16,
    #[serde(default = "default_shutdown_timeout_secs")]
    shutdown_timeout_secs: u64,
    #[serde(default = "default_socket_path")]
    socket_path: String,
    #[serde(default = "default_db_path")]
    db_path: String,
    #[serde(default = "default_cgroup_root")]
    cgroup_root: String,
    #[serde(default = "default_cgroup_parent")]
    cgroup_parent: String,
    #[serde(default = "default_agent_card_root")]
    agent_card_root: String,
    #[serde(default = "default_agent_profiles_dir")]
    agent_profiles_dir: String,
    #[serde(default = "default_mcp_servers_dir")]
    mcp_servers_dir: String,
}

#[derive(Debug, Clone, Deserialize)]
struct OneApiConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_one_api_command")]
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default = "default_one_api_health_url")]
    health_url: String,
    #[serde(default = "default_one_api_startup_timeout_secs")]
    startup_timeout_secs: u64,
    #[serde(default = "default_one_api_restart_max_attempts")]
    restart_max_attempts: u32,
    #[serde(default = "default_one_api_restart_backoff_secs")]
    restart_backoff_secs: u64,
    #[serde(default)]
    management_enabled: bool,
    #[serde(default = "default_one_api_management_base_url")]
    management_base_url: String,
    #[serde(default)]
    management_api_key: Option<String>,
    #[serde(default = "default_one_api_management_timeout_secs")]
    management_timeout_secs: u64,
    #[serde(default = "default_one_api_management_retries")]
    management_retries: u32,
    #[serde(default = "default_one_api_management_retry_backoff_secs")]
    management_retry_backoff_secs: u64,
    #[serde(default = "default_one_api_create_token_path")]
    create_token_path: String,
    #[serde(default = "default_one_api_create_channel_path")]
    create_channel_path: String,
    #[serde(default)]
    provision_channel: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            health_host: default_health_host(),
            health_port: default_health_port(),
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            socket_path: default_socket_path(),
            db_path: default_db_path(),
            cgroup_root: default_cgroup_root(),
            cgroup_parent: default_cgroup_parent(),
            agent_card_root: default_agent_card_root(),
            agent_profiles_dir: default_agent_profiles_dir(),
            mcp_servers_dir: default_mcp_servers_dir(),
        }
    }
}

impl Default for OneApiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            command: default_one_api_command(),
            args: Vec::new(),
            health_url: default_one_api_health_url(),
            startup_timeout_secs: default_one_api_startup_timeout_secs(),
            restart_max_attempts: default_one_api_restart_max_attempts(),
            restart_backoff_secs: default_one_api_restart_backoff_secs(),
            management_enabled: false,
            management_base_url: default_one_api_management_base_url(),
            management_api_key: None,
            management_timeout_secs: default_one_api_management_timeout_secs(),
            management_retries: default_one_api_management_retries(),
            management_retry_backoff_secs: default_one_api_management_retry_backoff_secs(),
            create_token_path: default_one_api_create_token_path(),
            create_channel_path: default_one_api_create_channel_path(),
            provision_channel: false,
        }
    }
}

fn default_health_host() -> String {
    "127.0.0.1".to_string()
}

fn default_health_port() -> u16 {
    7000
}

fn default_shutdown_timeout_secs() -> u64 {
    5
}

fn default_socket_path() -> String {
    "/tmp/agentd.sock".to_string()
}

fn default_db_path() -> String {
    "data/agentd.db".to_string()
}

fn default_cgroup_root() -> String {
    "/sys/fs/cgroup".to_string()
}

fn default_cgroup_parent() -> String {
    "agentd".to_string()
}

fn default_agent_card_root() -> String {
    "data/agents".to_string()
}

fn default_agent_profiles_dir() -> String {
    "configs/agents".to_string()
}

fn default_mcp_servers_dir() -> String {
    "configs/mcp-servers".to_string()
}

fn default_one_api_command() -> String {
    "one-api".to_string()
}

fn default_one_api_health_url() -> String {
    "http://127.0.0.1:3000/health".to_string()
}

fn default_one_api_startup_timeout_secs() -> u64 {
    30
}

fn default_one_api_restart_max_attempts() -> u32 {
    3
}

fn default_one_api_restart_backoff_secs() -> u64 {
    2
}

fn default_one_api_management_base_url() -> String {
    "http://127.0.0.1:3000".to_string()
}

fn default_one_api_management_timeout_secs() -> u64 {
    5
}

fn default_one_api_management_retries() -> u32 {
    3
}

fn default_one_api_management_retry_backoff_secs() -> u64 {
    1
}

fn default_one_api_create_token_path() -> String {
    "/api/token/".to_string()
}

fn default_one_api_create_channel_path() -> String {
    "/api/channel/".to_string()
}

#[derive(Clone)]
struct RuntimeState {
    one_api_status: Arc<RwLock<String>>,
    create_agent_lock: Arc<Mutex<()>>,
    lifecycle_manager: LifecycleManager,
    agent_card_root: Arc<PathBuf>,
    mcp_host: Arc<Mutex<McpHost>>,
    firecracker_executor: Option<Arc<firecracker::FirecrackerExecutor>>,
    a2a_tasks: Arc<RwLock<HashMap<uuid::Uuid, A2ATask>>>,
    a2a_stream_tx: broadcast::Sender<A2ATaskEvent>,
    registry_agents: Arc<RwLock<HashMap<String, RegistryAgentEntry>>>,
    lan_discovery: Arc<RwLock<Arc<dyn LanDiscovery>>>,
}

impl RuntimeState {
    #[cfg(test)]
    fn new(initial_status: &str) -> Self {
        Self::with_lifecycle(
            initial_status,
            LifecycleManager::new(CgroupManager::new("/tmp/agentd-cgroup", "agentd")),
        )
    }

    #[cfg(test)]
    fn with_lifecycle(initial_status: &str, lifecycle_manager: LifecycleManager) -> Self {
        Self::with_lifecycle_and_agent_card_root(
            initial_status,
            lifecycle_manager,
            PathBuf::from(default_agent_card_root()),
        )
    }

    #[cfg(test)]
    fn with_lifecycle_and_agent_card_root(
        initial_status: &str,
        lifecycle_manager: LifecycleManager,
        agent_card_root: PathBuf,
    ) -> Self {
        Self::with_lifecycle_and_agent_card_root_and_mcp(
            initial_status,
            lifecycle_manager,
            agent_card_root,
            Arc::new(Mutex::new(McpHost::new())),
        )
    }

    fn with_lifecycle_and_agent_card_root_and_mcp(
        initial_status: &str,
        lifecycle_manager: LifecycleManager,
        agent_card_root: PathBuf,
        mcp_host: Arc<Mutex<McpHost>>,
    ) -> Self {
        Self::with_lifecycle_and_agent_card_root_and_mcp_and_firecracker(
            initial_status,
            lifecycle_manager,
            agent_card_root,
            mcp_host,
            None,
        )
    }

    fn with_lifecycle_and_agent_card_root_and_mcp_and_firecracker(
        initial_status: &str,
        lifecycle_manager: LifecycleManager,
        agent_card_root: PathBuf,
        mcp_host: Arc<Mutex<McpHost>>,
        firecracker_executor: Option<Arc<firecracker::FirecrackerExecutor>>,
    ) -> Self {
        let (a2a_stream_tx, _) = broadcast::channel(1024);
        Self {
            one_api_status: Arc::new(RwLock::new(initial_status.to_string())),
            create_agent_lock: Arc::new(Mutex::new(())),
            lifecycle_manager,
            agent_card_root: Arc::new(agent_card_root),
            mcp_host,
            firecracker_executor,
            a2a_tasks: Arc::new(RwLock::new(HashMap::new())),
            a2a_stream_tx,
            registry_agents: Arc::new(RwLock::new(HashMap::new())),
            lan_discovery: Arc::new(RwLock::new(Arc::new(MdnsLanDiscovery))),
        }
    }

    async fn set_one_api_status(&self, status: &str) {
        let mut guard = self.one_api_status.write().await;
        *guard = status.to_string();
    }

    async fn one_api_status(&self) -> String {
        self.one_api_status.read().await.clone()
    }

    fn lifecycle(&self) -> LifecycleManager {
        self.lifecycle_manager.clone()
    }

    fn mcp_host(&self) -> Arc<Mutex<McpHost>> {
        self.mcp_host.clone()
    }

    fn firecracker_executor(&self) -> Option<Arc<firecracker::FirecrackerExecutor>> {
        self.firecracker_executor.clone()
    }

    fn subscribe_a2a_stream(&self) -> broadcast::Receiver<A2ATaskEvent> {
        self.a2a_stream_tx.subscribe()
    }

    async fn create_a2a_task(&self, request: CreateA2ATaskRequest) -> A2ATask {
        let now = Utc::now();
        let task = A2ATask {
            id: uuid::Uuid::new_v4(),
            agent_id: request.agent_id,
            state: A2ATaskState::Submitted,
            input: request.input,
            output: None,
            error: None,
            created_at: now,
            updated_at: now,
        };

        {
            let mut tasks = self.a2a_tasks.write().await;
            tasks.insert(task.id, task.clone());
        }
        self.publish_a2a_event(task.id, A2ATaskState::Submitted, json!({}))
            .await;

        task
    }

    async fn get_a2a_task(&self, task_id: uuid::Uuid) -> Option<A2ATask> {
        self.a2a_tasks.read().await.get(&task_id).cloned()
    }

    async fn transition_a2a_task(
        &self,
        task_id: uuid::Uuid,
        next_state: A2ATaskState,
        output: Option<Value>,
        error: Option<String>,
        payload: Value,
    ) -> Result<A2ATask, AgentError> {
        let mut tasks = self.a2a_tasks.write().await;
        let task = tasks
            .get_mut(&task_id)
            .ok_or_else(|| AgentError::NotFound(format!("a2a task not found: {task_id}")))?;

        if !task.state.can_transition_to(next_state) {
            return Err(AgentError::InvalidInput(format!(
                "invalid a2a state transition: {} -> {}",
                serde_json::to_string(&task.state).unwrap_or_else(|_| "\"unknown\"".to_string()),
                serde_json::to_string(&next_state).unwrap_or_else(|_| "\"unknown\"".to_string())
            )));
        }

        task.state = next_state;
        task.updated_at = Utc::now();
        if let Some(output) = output {
            task.output = Some(output);
        }
        if let Some(error) = error {
            task.error = Some(error);
        }
        let updated = task.clone();
        drop(tasks);

        self.publish_a2a_event(task_id, next_state, payload).await;
        Ok(updated)
    }

    async fn publish_a2a_event(&self, task_id: uuid::Uuid, state: A2ATaskState, payload: Value) {
        let event = A2ATaskEvent {
            task_id,
            state,
            lifecycle_state: state.to_agent_lifecycle_state(),
            timestamp: Utc::now(),
            payload,
        };
        let _ = self.a2a_stream_tx.send(event);
    }

    async fn upsert_registry_agent(&self, entry: RegistryAgentEntry) {
        let mut registry = self.registry_agents.write().await;
        registry.insert(entry.agent_id.clone(), entry);
    }

    async fn list_registry_agents(&self) -> Vec<RegistryAgentEntry> {
        self.registry_agents
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    async fn discover_lan_agents(&self) -> Result<Vec<DiscoveryAgentRecord>, AgentError> {
        let discovery = self.lan_discovery.read().await.clone();
        tokio::task::spawn_blocking(move || discovery.discover())
            .await
            .map_err(|err| AgentError::Runtime(format!("join lan discovery task failed: {err}")))?
            .map_err(|err| AgentError::Runtime(format!("lan discovery failed: {err}")))
    }

    #[cfg(test)]
    async fn set_lan_discovery_for_test(&self, discovery: Arc<dyn LanDiscovery>) {
        let mut guard = self.lan_discovery.write().await;
        *guard = discovery;
    }
}

#[derive(Debug, Clone)]
struct A2AHttpClient {
    http: reqwest::Client,
}

#[cfg(test)]
#[derive(Debug, Clone, serde::Deserialize)]
struct A2AAgentCard {
    agent_id: String,
    name: String,
    version: String,
    model: String,
    provider: String,
    #[serde(default)]
    capabilities: Value,
}

impl A2AHttpClient {
    fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .no_proxy()
                .build()
                .expect("build reqwest client without proxy"),
        }
    }

    #[cfg(test)]
    async fn discover_agent(&self, base_url: &str) -> Result<A2AAgentCard, AgentError> {
        let url = format!(
            "{}/.well-known/agent.json",
            normalize_http_base_url(base_url)?
        );
        let response =
            self.http.get(url).send().await.map_err(|err| {
                AgentError::Runtime(format!("discover agent request failed: {err}"))
            })?;
        let response = response
            .error_for_status()
            .map_err(|err| AgentError::Runtime(format!("discover agent failed: {err}")))?;
        response
            .json::<A2AAgentCard>()
            .await
            .map_err(|err| AgentError::Runtime(format!("decode agent card failed: {err}")))
    }

    async fn create_task(
        &self,
        base_url: &str,
        payload: &CreateA2ATaskRequest,
    ) -> Result<A2ATask, AgentError> {
        let url = format!("{}/a2a/tasks", normalize_http_base_url(base_url)?);
        let response = self
            .http
            .post(url)
            .json(payload)
            .send()
            .await
            .map_err(|err| AgentError::Runtime(format!("create task request failed: {err}")))?;
        let response = response
            .error_for_status()
            .map_err(|err| AgentError::Runtime(format!("create task failed: {err}")))?;
        let created = response
            .json::<CreateA2ATaskResponse>()
            .await
            .map_err(|err| {
                AgentError::Runtime(format!("decode create task response failed: {err}"))
            })?;
        Ok(created.task)
    }

    async fn get_task(&self, base_url: &str, task_id: uuid::Uuid) -> Result<A2ATask, AgentError> {
        let url = format!(
            "{}/a2a/tasks/{}",
            normalize_http_base_url(base_url)?,
            task_id
        );
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|err| AgentError::Runtime(format!("get task request failed: {err}")))?;
        let response = response
            .error_for_status()
            .map_err(|err| AgentError::Runtime(format!("get task failed: {err}")))?;
        let payload = response.json::<GetA2ATaskResponse>().await.map_err(|err| {
            AgentError::Runtime(format!("decode get task response failed: {err}"))
        })?;
        Ok(payload.task)
    }

    async fn stream_task(
        &self,
        base_url: &str,
        task_id: uuid::Uuid,
    ) -> Result<Vec<A2ATaskEvent>, AgentError> {
        let url = format!(
            "{}/a2a/stream?task_id={}",
            normalize_http_base_url(base_url)?,
            task_id
        );
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|err| AgentError::Runtime(format!("stream task request failed: {err}")))?;
        let response = response
            .error_for_status()
            .map_err(|err| AgentError::Runtime(format!("stream task failed: {err}")))?;
        let body = response
            .text()
            .await
            .map_err(|err| AgentError::Runtime(format!("read stream body failed: {err}")))?;

        let mut events = Vec::new();
        for line in body.lines() {
            if let Some(payload) = line.strip_prefix("data: ") {
                if payload.trim().is_empty() {
                    continue;
                }
                let event = serde_json::from_str::<A2ATaskEvent>(payload).map_err(|err| {
                    AgentError::Runtime(format!("decode stream payload failed: {err}"))
                })?;
                events.push(event);
            }
        }

        Ok(events)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct OrchestrateTaskParams {
    #[serde(default)]
    parent_agent_id: Option<String>,
    #[serde(default)]
    input: Value,
    #[serde(default)]
    subtasks: Option<Vec<Value>>,
    #[serde(default)]
    delegate_agent_ids: Vec<String>,
    #[serde(default)]
    retry_limit: Option<u8>,
}

#[derive(Debug, Clone, Serialize)]
struct OrchestratorChildResult {
    index: usize,
    agent_id: String,
    input: Value,
    attempts: u8,
    state: A2ATaskState,
    output: Option<Value>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct OrchestratorRunResult {
    task_id: uuid::Uuid,
    state: A2ATaskState,
    retry_limit: u8,
    merge_strategy: String,
    children: Vec<OrchestratorChildResult>,
    aggregated_output: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationSummary {
    text: String,
    #[serde(default)]
    key_files: Vec<String>,
    message_count: usize,
    #[serde(default)]
    source_head_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationSnapshotL2 {
    #[serde(default)]
    head_id: Option<String>,
    #[serde(default)]
    messages: Vec<Value>,
    #[serde(default)]
    tool_results_cache: Value,
    #[serde(default)]
    working_directory: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationContextPayload {
    level: String,
    source_agent_id: String,
    session_id: String,
    summary: MigrationSummary,
    #[serde(default)]
    snapshot: Option<MigrationSnapshotL2>,
}

#[derive(Debug, Clone, Deserialize)]
struct MigrateContextParams {
    source_agent_id: String,
    target_base_url: String,
    #[serde(default)]
    target_agent_id: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    key_files: Vec<String>,
    #[serde(default)]
    messages: Vec<Value>,
    #[serde(default)]
    head_id: Option<String>,
    #[serde(default)]
    tool_results_cache: Value,
    #[serde(default)]
    working_directory: BTreeMap<String, String>,
    #[serde(default)]
    include_snapshot: bool,
}

fn compact_summary_line(value: &str) -> Option<String> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }
    if normalized.len() > 180 {
        return Some(format!("{}...", &normalized[..177]));
    }
    Some(normalized)
}

fn build_migration_summary(
    messages: &[Value],
    key_files: &[String],
    source_head_id: Option<String>,
) -> MigrationSummary {
    let mut facts = Vec::new();
    let mut seen = HashSet::new();

    for message in messages {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let Some(content) = message.get("content").and_then(Value::as_str) else {
            continue;
        };
        let Some(line) = compact_summary_line(content) else {
            continue;
        };
        let key = format!("{role}:{line}");
        if !seen.insert(key) {
            continue;
        }
        facts.push(format!("- {role}: {line}"));
        if facts.len() >= 8 {
            break;
        }
    }

    if facts.is_empty() {
        facts.push("- no key facts extracted".to_string());
    }

    MigrationSummary {
        text: format!("context summary (migration l1):\n{}", facts.join("\n")),
        key_files: key_files.to_vec(),
        message_count: messages.len(),
        source_head_id,
    }
}

fn build_migration_snapshot(
    params: &MigrateContextParams,
) -> Result<MigrationSnapshotL2, AgentError> {
    let tool_results_cache = if params.tool_results_cache.is_null() {
        json!({})
    } else if params.tool_results_cache.is_object() {
        params.tool_results_cache.clone()
    } else {
        return Err(AgentError::InvalidInput(
            "tool_results_cache must be a JSON object".to_string(),
        ));
    };

    Ok(MigrationSnapshotL2 {
        head_id: params.head_id.clone(),
        messages: params.messages.clone(),
        tool_results_cache,
        working_directory: params.working_directory.clone(),
    })
}

fn build_migration_context_payload(
    params: &MigrateContextParams,
    session_id: String,
    summary: MigrationSummary,
    snapshot: MigrationSnapshotL2,
) -> MigrationContextPayload {
    let level = if params.include_snapshot { "l2" } else { "l1" };
    MigrationContextPayload {
        level: level.to_string(),
        source_agent_id: params.source_agent_id.clone(),
        session_id,
        summary,
        snapshot: params.include_snapshot.then_some(snapshot),
    }
}

fn restore_task_migration_context(task_input: &Value) -> Result<Option<Value>, AgentError> {
    let Some(raw_context) = task_input.get("migration_context") else {
        return Ok(None);
    };
    let migration_context = serde_json::from_value::<MigrationContextPayload>(raw_context.clone())
        .map_err(|err| AgentError::InvalidInput(format!("invalid migration_context: {err}")))?;

    let summary = migration_context.summary.clone();
    let resume_prompt = summary.text.clone();

    let mut restored = json!({
        "resumed": true,
        "level": migration_context.level,
        "session_id": migration_context.session_id,
        "source_agent_id": migration_context.source_agent_id,
        "summary": summary,
        "resume_prompt": resume_prompt,
    });

    if let Some(snapshot) = migration_context.snapshot {
        restored["snapshot"] = json!({
            "head_id": snapshot.head_id,
            "message_count": snapshot.messages.len(),
            "tool_cache_entries": snapshot
                .tool_results_cache
                .as_object()
                .map(|obj| obj.len())
                .unwrap_or(0),
            "working_directory_files": snapshot.working_directory.keys().cloned().collect::<Vec<_>>(),
        });
    }

    Ok(Some(restored))
}

async fn rollback_source_context_session(
    store: &SqliteStore,
    session_id: &str,
) -> Result<(), AgentError> {
    let _ = store
        .update_context_session_migration_state(session_id, "active")
        .await?;
    Ok(())
}

async fn migrate_context_to_target(
    store: &SqliteStore,
    params: MigrateContextParams,
) -> Result<Value, AgentError> {
    let source_agent_id = uuid::Uuid::parse_str(&params.source_agent_id)
        .map_err(|err| AgentError::InvalidInput(format!("invalid source_agent_id: {err}")))?;
    let _ = store.get_agent(source_agent_id).await?;

    if params.target_base_url.trim().is_empty() {
        return Err(AgentError::InvalidInput(
            "target_base_url must be non-empty".to_string(),
        ));
    }

    let session_id = params
        .session_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("session-migrate-{}", uuid::Uuid::new_v4()));
    let snapshot = build_migration_snapshot(&params)?;
    let summary = build_migration_summary(
        &snapshot.messages,
        &params.key_files,
        snapshot.head_id.clone(),
    );
    let migration_context = build_migration_context_payload(
        &params,
        session_id.clone(),
        summary.clone(),
        snapshot.clone(),
    );

    let now = Utc::now().to_rfc3339();
    let persisted_snapshot = ContextSessionSnapshot {
        session_id: session_id.clone(),
        agent_id: params.source_agent_id.clone(),
        head_id: snapshot.head_id.clone(),
        messages: snapshot.messages.clone(),
        tool_results_cache: snapshot.tool_results_cache.clone(),
        working_directory: snapshot.working_directory.clone(),
        summary: summary.text.clone(),
        key_files: summary.key_files.clone(),
        migration_state: "active".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    let _ = store
        .upsert_context_session_snapshot(persisted_snapshot)
        .await?;

    let target_agent_id = params
        .target_agent_id
        .as_deref()
        .map(uuid::Uuid::parse_str)
        .transpose()
        .map_err(|err| AgentError::InvalidInput(format!("invalid target_agent_id: {err}")))?;

    let client = A2AHttpClient::new();
    let created_task = match client
        .create_task(
            &params.target_base_url,
            &CreateA2ATaskRequest {
                agent_id: target_agent_id,
                input: json!({
                    "kind": "context_migration",
                    "migration_context": migration_context,
                }),
            },
        )
        .await
    {
        Ok(task) => task,
        Err(err) => {
            rollback_source_context_session(store, &session_id).await?;
            return Err(AgentError::Runtime(format!(
                "context migration create task failed: {err}"
            )));
        }
    };

    if let Err(err) = client
        .stream_task(&params.target_base_url, created_task.id)
        .await
    {
        rollback_source_context_session(store, &session_id).await?;
        return Err(AgentError::Runtime(format!(
            "context migration stream failed: {err}"
        )));
    }

    let final_task = match client
        .get_task(&params.target_base_url, created_task.id)
        .await
    {
        Ok(task) => task,
        Err(err) => {
            rollback_source_context_session(store, &session_id).await?;
            return Err(AgentError::Runtime(format!(
                "context migration final task fetch failed: {err}"
            )));
        }
    };

    if final_task.state != A2ATaskState::Completed {
        rollback_source_context_session(store, &session_id).await?;
        return Err(AgentError::Runtime(format!(
            "context migration target task ended in state {}",
            serde_json::to_string(&final_task.state).unwrap_or_else(|_| "\"unknown\"".to_string())
        )));
    }

    let migrated_snapshot = store
        .update_context_session_migration_state(&session_id, "migrated")
        .await?;

    Ok(json!({
        "session_id": session_id,
        "migration_level": if params.include_snapshot { "l2" } else { "l1" },
        "target_task_id": created_task.id,
        "target_state": final_task.state,
        "target_output": final_task.output,
        "summary": summary,
        "source_session_state": migrated_snapshot.migration_state,
    }))
}

fn split_orchestrator_subtasks(input: &Value, explicit_subtasks: Option<Vec<Value>>) -> Vec<Value> {
    if let Some(subtasks) = explicit_subtasks {
        return subtasks;
    }

    if let Some(items) = input
        .get("subtasks")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
    {
        return items.clone();
    }

    if let Some(items) = input.as_array().filter(|items| !items.is_empty()) {
        return items.clone();
    }

    vec![input.clone()]
}

fn resolve_orchestrator_agents(
    explicit_agents: &[String],
    local_candidates: &[agentd_store::agent::DelegationAgentSummary],
    parent_agent_id: Option<uuid::Uuid>,
) -> Vec<String> {
    let mut resolved = explicit_agents
        .iter()
        .map(|agent_id| agent_id.trim())
        .filter(|agent_id| !agent_id.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    if resolved.is_empty() {
        resolved = local_candidates
            .iter()
            .map(|candidate| candidate.agent_id.clone())
            .collect();
    }

    if resolved.is_empty() {
        if let Some(parent_agent_id) = parent_agent_id {
            resolved.push(parent_agent_id.to_string());
        } else {
            resolved.push("local-orchestrator".to_string());
        }
    }

    resolved
}

fn aggregate_orchestrator_results(children: &[OrchestratorChildResult]) -> Value {
    let mut ordered = children.to_vec();
    ordered.sort_by_key(|child| child.index);

    let retried_children = ordered.iter().filter(|child| child.attempts > 1).count();
    let failed_children = ordered
        .iter()
        .filter(|child| child.state == A2ATaskState::Failed)
        .count();
    let succeeded_children = ordered
        .iter()
        .filter(|child| child.state == A2ATaskState::Completed)
        .count();

    json!({
        "results": ordered
            .iter()
            .map(|child| {
                json!({
                    "index": child.index,
                    "agent_id": child.agent_id,
                    "attempts": child.attempts,
                    "state": child.state,
                    "output": child.output,
                    "error": child.error,
                })
            })
            .collect::<Vec<_>>(),
        "summary": {
            "total_children": ordered.len(),
            "succeeded_children": succeeded_children,
            "failed_children": failed_children,
            "retried_children": retried_children,
        }
    })
}

async fn orchestrate_task_with_delegate<F>(
    state: &RuntimeState,
    store: &SqliteStore,
    params: OrchestrateTaskParams,
    mut delegate: F,
) -> Result<OrchestratorRunResult, AgentError>
where
    F: FnMut(&str, &Value, usize, u8) -> Result<Value, AgentError>,
{
    let parent_agent_id = params
        .parent_agent_id
        .as_deref()
        .map(uuid::Uuid::parse_str)
        .transpose()
        .map_err(|err| AgentError::InvalidInput(format!("invalid parent_agent_id: {err}")))?;

    let parent_task = state
        .create_a2a_task(CreateA2ATaskRequest {
            agent_id: parent_agent_id,
            input: params.input.clone(),
        })
        .await;

    let _ = state
        .transition_a2a_task(
            parent_task.id,
            A2ATaskState::Working,
            None,
            None,
            json!({
                "kind": "orchestrator",
                "phase": "started",
                "task_id": parent_task.id,
            }),
        )
        .await;

    let local_profiles = store.list_agents().await?;
    let local_candidates = delegation_candidates_from_profiles(&local_profiles);
    let delegation_agents = resolve_orchestrator_agents(
        &params.delegate_agent_ids,
        &local_candidates,
        parent_agent_id,
    );

    let subtasks = split_orchestrator_subtasks(&params.input, params.subtasks);
    let retry_limit = params.retry_limit.unwrap_or(1);
    let max_attempts = retry_limit.saturating_add(1);

    let mut children = Vec::with_capacity(subtasks.len());
    for (index, subtask) in subtasks.into_iter().enumerate() {
        let agent_id = delegation_agents[index % delegation_agents.len()].clone();
        let mut attempts = 0_u8;

        let child = loop {
            attempts = attempts.saturating_add(1);
            state
                .publish_a2a_event(
                    parent_task.id,
                    A2ATaskState::Working,
                    json!({
                        "kind": "orchestrator",
                        "phase": "delegated",
                        "task_id": parent_task.id,
                        "child_index": index,
                        "agent_id": agent_id,
                        "attempt": attempts,
                        "input": subtask,
                    }),
                )
                .await;

            match delegate(&agent_id, &subtask, index, attempts) {
                Ok(output) => {
                    state
                        .publish_a2a_event(
                            parent_task.id,
                            A2ATaskState::Working,
                            json!({
                                "kind": "orchestrator",
                                "phase": "completed",
                                "task_id": parent_task.id,
                                "child_index": index,
                                "agent_id": agent_id,
                                "attempt": attempts,
                                "output": output,
                            }),
                        )
                        .await;
                    break OrchestratorChildResult {
                        index,
                        agent_id: agent_id.clone(),
                        input: subtask.clone(),
                        attempts,
                        state: A2ATaskState::Completed,
                        output: Some(output),
                        error: None,
                    };
                }
                Err(err) if attempts < max_attempts => {
                    state
                        .publish_a2a_event(
                            parent_task.id,
                            A2ATaskState::Working,
                            json!({
                                "kind": "orchestrator",
                                "phase": "retrying",
                                "task_id": parent_task.id,
                                "child_index": index,
                                "agent_id": agent_id,
                                "attempt": attempts,
                                "error": err.to_string(),
                            }),
                        )
                        .await;
                }
                Err(err) => {
                    let error_message = err.to_string();
                    state
                        .publish_a2a_event(
                            parent_task.id,
                            A2ATaskState::Working,
                            json!({
                                "kind": "orchestrator",
                                "phase": "failed",
                                "task_id": parent_task.id,
                                "child_index": index,
                                "agent_id": agent_id,
                                "attempt": attempts,
                                "error": error_message,
                            }),
                        )
                        .await;
                    break OrchestratorChildResult {
                        index,
                        agent_id: agent_id.clone(),
                        input: subtask.clone(),
                        attempts,
                        state: A2ATaskState::Failed,
                        output: None,
                        error: Some(error_message),
                    };
                }
            }
        };

        children.push(child);
    }

    let aggregated_output = aggregate_orchestrator_results(&children);
    let failed_children = children
        .iter()
        .filter(|child| child.state == A2ATaskState::Failed)
        .count();

    let final_state = if failed_children == 0 {
        A2ATaskState::Completed
    } else {
        A2ATaskState::Failed
    };
    let final_error = if failed_children == 0 {
        None
    } else {
        Some(format!(
            "orchestrator failed with {failed_children} child task(s)"
        ))
    };

    let _ = state
        .transition_a2a_task(
            parent_task.id,
            final_state,
            Some(aggregated_output.clone()),
            final_error,
            json!({
                "kind": "orchestrator",
                "phase": "aggregated",
                "task_id": parent_task.id,
                "state": final_state,
                "aggregated_output": aggregated_output,
            }),
        )
        .await;

    Ok(OrchestratorRunResult {
        task_id: parent_task.id,
        state: final_state,
        retry_limit,
        merge_strategy: "deterministic_list".to_string(),
        children,
        aggregated_output,
    })
}

fn normalize_http_base_url(base_url: &str) -> Result<String, AgentError> {
    let normalized = base_url.trim().trim_end_matches('/');
    if normalized.is_empty() {
        return Err(AgentError::InvalidInput(
            "base url must be non-empty".to_string(),
        ));
    }
    if !(normalized.starts_with("http://") || normalized.starts_with("https://")) {
        return Err(AgentError::InvalidInput(format!(
            "base url must start with http:// or https://, got: {normalized}"
        )));
    }
    Ok(normalized.to_string())
}

fn registry_entry_to_discovery(entry: &RegistryAgentEntry) -> DiscoveryAgentRecord {
    DiscoveryAgentRecord {
        agent_id: entry.agent_id.clone(),
        name: entry.name.clone(),
        model: entry.model.clone(),
        provider: entry.provider.clone(),
        endpoint: entry.endpoint.clone(),
        source: "registry".to_string(),
        health: entry.health.clone(),
    }
}

async fn fetch_remote_registry_agents(
    base_url: &str,
) -> Result<Vec<RegistryAgentEntry>, AgentError> {
    let url = format!("{}/registry/agents", normalize_http_base_url(base_url)?);
    let http = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|err| AgentError::Runtime(format!("build reqwest client failed: {err}")))?;
    let response = http
        .get(url)
        .send()
        .await
        .map_err(|err| AgentError::Runtime(format!("fetch registry agents failed: {err}")))?;
    let response = response
        .error_for_status()
        .map_err(|err| AgentError::Runtime(format!("registry endpoint returned error: {err}")))?;
    let payload = response
        .json::<Value>()
        .await
        .map_err(|err| AgentError::Runtime(format!("decode registry payload failed: {err}")))?;
    serde_json::from_value::<Vec<RegistryAgentEntry>>(payload["agents"].clone())
        .map_err(|err| AgentError::Runtime(format!("decode registry entries failed: {err}")))
}

fn start_mdns_advertisement(bind_addr: SocketAddr) -> Result<ServiceDaemon, AgentError> {
    let daemon = ServiceDaemon::new()
        .map_err(|err| AgentError::Runtime(format!("create mdns daemon failed: {err}")))?;
    let instance_name = format!("agentd-{}", bind_addr.port());
    let host_name = format!("{}.local.", bind_addr.ip());
    let properties = [
        ("endpoint", format!("http://{bind_addr}")),
        ("agent_id", instance_name.clone()),
        ("name", "agentd-daemon".to_string()),
        ("model", "daemon".to_string()),
        ("provider", "agentd".to_string()),
        ("health", "ready".to_string()),
    ];
    let service = ServiceInfo::new(
        AGENTD_MDNS_SERVICE_TYPE,
        &instance_name,
        &host_name,
        bind_addr.ip().to_string(),
        bind_addr.port(),
        &properties[..],
    )
    .map_err(|err| AgentError::Runtime(format!("build mdns service info failed: {err}")))?;
    daemon
        .register(service)
        .map_err(|err| AgentError::Runtime(format!("register mdns service failed: {err}")))?;
    Ok(daemon)
}

fn load_config(path: &str) -> Result<AppConfig, Box<dyn std::error::Error>> {
    if !Path::new(path).exists() {
        info!(config_path = path, "Config file not found, using defaults");
        return Ok(AppConfig::default());
    }

    let content = std::fs::read_to_string(path)?;
    let config = toml::from_str::<AppConfig>(&content)?;
    Ok(config)
}

fn notify_systemd(state: &str) {
    let Some(socket_path) = std::env::var_os("NOTIFY_SOCKET") else {
        return;
    };

    let socket_path = socket_path.to_string_lossy();
    let target = if let Some(stripped) = socket_path.strip_prefix('@') {
        format!("\0{stripped}")
    } else {
        socket_path.to_string()
    };

    let send_result = (|| -> std::io::Result<()> {
        let sock = UnixDatagram::unbound()?;
        sock.connect(target)?;
        let _ = sock.send(state.as_bytes())?;
        Ok(())
    })();

    if let Err(err) = send_result {
        warn!(%err, state, "Failed to send systemd notification");
    }
}

#[cfg(test)]
fn cleanup_sqlite_files(db_path: &Path) {
    let db_path_str = db_path.to_string_lossy();
    for suffix in ["", "-wal", "-shm"] {
        let path = format!("{db_path_str}{suffix}");
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
fn temp_mcp_configs_dir() -> PathBuf {
    std::env::temp_dir().join(format!("agentd-mcp-config-test-{}", uuid::Uuid::new_v4()))
}

#[cfg(test)]
#[test]
fn mcp_config_loads_all_servers() {
    let dir = temp_mcp_configs_dir();
    std::fs::create_dir_all(&dir).expect("create temp mcp dir");

    std::fs::write(
        dir.join("a-fs.toml"),
        r#"
[server]
name = "mcp-fs"
command = "python"
args = ["-m", "agentd_mcp_fs"]
transport = "stdio"
trust_level = "builtin"
"#,
    )
    .expect("write fs config");

    std::fs::write(
        dir.join("b-shell.toml"),
        r#"
[server]
name = "mcp-shell"
command = "python"
args = ["-m", "agentd_mcp_shell"]
transport = "stdio"
trust_level = "verified"
"#,
    )
    .expect("write shell config");

    let configs = load_mcp_server_configs(&dir).expect("mcp configs should load");
    assert_eq!(configs.len(), 2);
    assert_eq!(configs[0].name, "mcp-fs");
    assert_eq!(configs[0].transport, mcp::McpTransport::Stdio);
    assert_eq!(configs[0].trust_level, mcp::McpTrustLevel::Builtin);
    assert_eq!(configs[1].name, "mcp-shell");
    assert_eq!(configs[1].transport, mcp::McpTransport::Stdio);
    assert_eq!(configs[1].trust_level, mcp::McpTrustLevel::Verified);

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(test)]
#[test]
fn mcp_config_rejects_missing_required_field() {
    let dir = temp_mcp_configs_dir();
    std::fs::create_dir_all(&dir).expect("create temp mcp dir");

    std::fs::write(
        dir.join("invalid-missing-command.toml"),
        r#"
[server]
name = "mcp-fs"
args = ["-m", "agentd_mcp_fs"]
transport = "stdio"
trust_level = "builtin"
"#,
    )
    .expect("write invalid config");

    let err = load_mcp_server_configs(&dir).expect_err("missing command should fail");
    let err_msg = err.to_string();
    assert!(err_msg.contains("missing required field server.command"));

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(test)]
#[test]
fn mcp_config_rejects_invalid_transport() {
    let dir = temp_mcp_configs_dir();
    std::fs::create_dir_all(&dir).expect("create temp mcp dir");

    std::fs::write(
        dir.join("invalid-transport.toml"),
        r#"
[server]
name = "mcp-fs"
command = "python"
args = ["-m", "agentd_mcp_fs"]
transport = "http"
trust_level = "builtin"
"#,
    )
    .expect("write invalid transport config");

    let err = load_mcp_server_configs(&dir).expect_err("invalid transport should fail");
    let err_msg = err.to_string();
    assert!(err_msg.contains("invalid transport"));

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(test)]
fn mcp_valid_stdio_server(name: &str, capability: &str) -> mcp::McpServerConfig {
    mcp::McpServerConfig {
        name: name.to_string(),
        command: "/bin/sh".to_string(),
        args: vec![
            "-c".to_string(),
            format!(
                "read _line; printf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"capabilities\":{{\"tools\":[\"{capability}\"]}}}}}}'; sleep 30"
            ),
        ],
        transport: mcp::McpTransport::Stdio,
        trust_level: mcp::McpTrustLevel::Builtin,
    }
}

#[cfg(test)]
fn mcp_invalid_initialize_stdio_server(name: &str) -> mcp::McpServerConfig {
    mcp::McpServerConfig {
        name: name.to_string(),
        command: "/bin/sh".to_string(),
        args: vec![
            "-c".to_string(),
            "read _line; printf '%s\\n' 'not-json'; sleep 30".to_string(),
        ],
        transport: mcp::McpTransport::Stdio,
        trust_level: mcp::McpTrustLevel::Builtin,
    }
}

#[cfg(test)]
fn mcp_short_lived_stdio_server(name: &str, capability: &str) -> mcp::McpServerConfig {
    mcp::McpServerConfig {
        name: name.to_string(),
        command: "/bin/sh".to_string(),
        args: vec![
            "-c".to_string(),
            format!(
                "read _line; printf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"capabilities\":{{\"tools\":[\"{capability}\"]}}}}}}'; sleep 0.1"
            ),
        ],
        transport: mcp::McpTransport::Stdio,
        trust_level: mcp::McpTrustLevel::Builtin,
    }
}

#[cfg(test)]
#[tokio::test]
async fn mcp_host_starts_declared_servers() {
    let mut host = mcp::McpHost::new();
    let configs = vec![
        mcp_valid_stdio_server("mcp-fs", "fs.read_file"),
        mcp_valid_stdio_server("mcp-shell", "shell.execute"),
    ];

    host.start_declared_servers(&configs)
        .await
        .expect("mcp host should start configured servers");
    let health = host
        .refresh_health()
        .expect("mcp host health check should succeed");

    assert_eq!(health.total, configs.len());
    assert_eq!(health.healthy, configs.len());
    assert_eq!(host.registry().len(), configs.len());

    let fs_handle = host
        .server_handle("mcp-fs")
        .expect("mcp-fs handle should exist");
    assert_eq!(fs_handle.initialize_capabilities, vec!["fs.read_file"]);

    host.stop_all().await.expect("host stop should succeed");
    assert_eq!(host.server_count(), 0);
    assert!(host.registry().is_empty());
}

#[cfg(test)]
#[tokio::test]
async fn mcp_host_rolls_back_on_init_failure() {
    let mut host = mcp::McpHost::new();
    let configs = vec![
        mcp_valid_stdio_server("mcp-fs", "fs.read_file"),
        mcp_invalid_initialize_stdio_server("mcp-bad"),
    ];

    let err = host
        .start_declared_servers(&configs)
        .await
        .expect_err("initialize failure should trigger rollback");
    let err_text = err.to_string();
    assert!(
        err_text.contains("initialize") || err_text.contains("parse"),
        "unexpected startup error: {err_text}"
    );

    assert_eq!(host.server_count(), 0);
    assert!(host.registry().is_empty());

    let startup_failure = host
        .audit_events()
        .iter()
        .any(|event| event.action == "startup" && !event.success && event.server_id == "mcp-bad");
    assert!(startup_failure, "startup failure should be audited");

    let rollback_event = host
        .audit_events()
        .iter()
        .any(|event| event.action == "rollback" && event.success && event.server_id == "mcp-bad");
    assert!(rollback_event, "rollback completion should be audited");
}

#[cfg(test)]
#[tokio::test]
async fn mcp_registry_syncs_capabilities_from_initialize() {
    let mut host = mcp::McpHost::new();
    let configs = vec![mcp_valid_stdio_server("mcp-search", "search.query")];

    host.start_declared_servers(&configs)
        .await
        .expect("mcp host should start configured server");

    let registry_entry = host
        .registry()
        .get("mcp-search")
        .expect("registry entry should exist");
    assert_eq!(
        registry_entry.capabilities,
        vec!["search.query".to_string()]
    );
    assert_eq!(registry_entry.health, mcp::McpServerHealth::Healthy);

    let available_tools = host.list_available_tools();
    assert!(available_tools
        .iter()
        .any(|tool| tool.server_id == "mcp-search" && tool.tool_name == "search.query"));

    host.stop_all().await.expect("mcp host stop should succeed");
}

#[cfg(test)]
fn temp_firecracker_runtime_dir() -> PathBuf {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    std::env::temp_dir().join(format!("adfc-{}", &suffix[..10]))
}

#[cfg(test)]
fn write_test_placeholder(path: &Path) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create test parent directory");
    }
    std::fs::write(path, b"placeholder").expect("write test placeholder file");
}

#[cfg(test)]
fn firecracker_echo_script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/firecracker/vsock-agent-echo.py")
}

#[cfg(test)]
fn build_firecracker_executor_for_test(root: &Path) -> firecracker::FirecrackerExecutor {
    let kernel = root.join("vmlinux.bin");
    let rootfs = root.join("rootfs.ext4");
    write_test_placeholder(&kernel);
    write_test_placeholder(&rootfs);

    firecracker::FirecrackerExecutor::builder()
        .kernel_path(kernel)
        .rootfs_path(rootfs)
        .default_vcpu_count(1)
        .default_mem_size_mib(512)
        .default_network_policy(firecracker::NetworkIsolationPolicy::AllowAll)
        .default_jailer(Some(firecracker::JailerConfig::default()))
        .vsock_root_dir(root.join("vsock"))
        .build()
        .expect("build firecracker executor")
}

#[cfg(test)]
async fn firecracker_wait_process_exit(pid: u32) {
    for _ in 0..40 {
        if !PathBuf::from(format!("/proc/{pid}")).exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(test)]
#[tokio::test]
async fn unhealthy_server_removed_from_available_tools() {
    let mut host = mcp::McpHost::new();
    let configs = vec![
        mcp_valid_stdio_server("mcp-fs", "fs.read_file"),
        mcp_short_lived_stdio_server("mcp-transient", "transient.echo"),
    ];

    host.start_declared_servers(&configs)
        .await
        .expect("mcp host should start configured servers");

    tokio::time::sleep(Duration::from_millis(250)).await;
    let health = host
        .refresh_health()
        .expect("mcp host health refresh should succeed");
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

#[cfg(test)]
#[tokio::test]
async fn firecracker_executor_launches_vm() {
    let root = temp_firecracker_runtime_dir();
    let script = firecracker_echo_script_path();
    assert!(script.exists(), "vsock echo script should exist");

    let executor = build_firecracker_executor_for_test(&root);
    let agent_id = uuid::Uuid::new_v4();
    let network = firecracker::FirecrackerNetworkConfig {
        tap_device: "fc-test0".to_string(),
        host_ipv4: "10.10.0.1/30".to_string(),
        guest_ipv4: "10.10.0.2/30".to_string(),
    };

    let mut vm = executor
        .launch_agent(firecracker::FirecrackerAgentLaunchSpec {
            agent_id,
            command: "/usr/bin/env".to_string(),
            args: vec!["python3".to_string(), script.display().to_string()],
            env: HashMap::new(),
            vcpu_count: Some(2),
            mem_size_mib: Some(1024),
            network: Some(network.clone()),
            network_policy: None,
            jailer: None,
            launch_timeout: Duration::from_secs(2),
        })
        .await
        .expect("firecracker vm launch should succeed");

    assert_eq!(vm.agent_id(), agent_id);
    assert_eq!(vm.config().kernel_path, root.join("vmlinux.bin"));
    assert_eq!(vm.config().rootfs_path, root.join("rootfs.ext4"));
    assert_eq!(vm.config().vcpu_count, 2);
    assert_eq!(vm.config().mem_size_mib, 1024);
    assert_eq!(vm.config().network, Some(network));

    let ready = vm
        .roundtrip_json(&json!({"rpc": "daemon.ready"}))
        .await
        .expect("firecracker vsock should be ready after launch");
    assert_eq!(ready["status"], json!("ok"));

    let socket_path = vm.config().vsock_path.clone();
    vm.shutdown()
        .await
        .expect("firecracker vm shutdown should succeed");
    assert!(!socket_path.exists(), "vsock socket should be cleaned up");

    let _ = std::fs::remove_dir_all(root);
}

#[cfg(test)]
#[tokio::test]
async fn list_available_tools_filters_by_policy() {
    let db_path = std::env::temp_dir().join(format!(
        "agentd-daemon-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));

    let mut host = mcp::McpHost::new();
    host.start_declared_servers(&[
        mcp_valid_stdio_server("mcp-fs", "fs.read_file"),
        mcp_valid_stdio_server("mcp-shell", "shell.execute"),
    ])
    .await
    .expect("mcp host should start configured servers");
    let state = RuntimeState::with_lifecycle_and_agent_card_root_and_mcp(
        "disabled",
        LifecycleManager::new(CgroupManager::new("/tmp/agentd-cgroup", "agentd")),
        PathBuf::from(default_agent_card_root()),
        Arc::new(Mutex::new(host)),
    );

    let create_response = handle_rpc_request(
        JsonRpcRequest::new(
            json!(8801),
            "CreateAgent",
            json!({
                "name": "list-tools-policy-agent",
                "model": "claude-4-sonnet",
                "permission_policy": "ask",
                "denied_tools": ["mcp.fs.read_file"],
            }),
        ),
        store.clone(),
        state.clone(),
        OneApiConfig::default(),
    )
    .await;
    assert!(
        create_response.error.is_none(),
        "create should succeed: {create_response:?}"
    );
    let agent_id = create_response
        .result
        .expect("create result should exist")
        .get("agent")
        .expect("agent field should exist")
        .get("id")
        .expect("id field should exist")
        .as_str()
        .expect("agent id should be string")
        .to_string();

    let list_response = handle_rpc_request(
        JsonRpcRequest::new(
            json!(8802),
            "ListAvailableTools",
            json!({
                "agent_id": agent_id,
            }),
        ),
        store,
        state.clone(),
        OneApiConfig::default(),
    )
    .await;
    assert!(
        list_response.error.is_none(),
        "list available tools should succeed: {list_response:?}"
    );

    let tools = list_response
        .result
        .expect("list result should exist")
        .get("tools")
        .expect("tools field should exist")
        .as_array()
        .expect("tools should be array")
        .clone();

    assert!(
        tools.iter().any(|tool| {
            tool["server"] == json!("mcp-shell")
                && tool["tool"] == json!("shell.execute")
                && tool["policy_tool"] == json!("mcp.shell.execute")
        }),
        "shell tool should be visible"
    );
    assert!(
        !tools
            .iter()
            .any(|tool| tool["policy_tool"] == json!("mcp.fs.read_file")),
        "denied fs tool should be filtered out"
    );

    let mcp_host = state.mcp_host();
    mcp_host
        .lock()
        .await
        .stop_all()
        .await
        .expect("mcp host stop should succeed");
    cleanup_sqlite_files(&db_path);
}

#[cfg(test)]
#[tokio::test]
async fn onboard_mcp_server_registers_server_for_settings_management() {
    let db_path = std::env::temp_dir().join(format!(
        "agentd-daemon-task28-onboard-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));

    let mut host = mcp::McpHost::new();
    host.start_declared_servers(&[mcp_valid_stdio_server("mcp-fs", "fs.read_file")])
        .await
        .expect("mcp host should start builtin server");
    let state = RuntimeState::with_lifecycle_and_agent_card_root_and_mcp(
        "disabled",
        LifecycleManager::new(CgroupManager::new("/tmp/agentd-cgroup", "agentd")),
        PathBuf::from(default_agent_card_root()),
        Arc::new(Mutex::new(host)),
    );

    let onboard_response = handle_rpc_request(
        JsonRpcRequest::new(
            json!(28801),
            "OnboardMcpServer",
            json!({
                "name": "mcp-figma",
                "command": "/bin/sh",
                "args": [
                    "-c",
                    "read _line; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"capabilities\":{\"tools\":[\"figma.export_frame\"]}}}'; sleep 30"
                ],
                "transport": "stdio",
                "trust_level": "community"
            }),
        ),
        store.clone(),
        state.clone(),
        OneApiConfig::default(),
    )
    .await;
    assert!(
        onboard_response.error.is_none(),
        "onboard should succeed: {onboard_response:?}"
    );
    let onboarded = onboard_response
        .result
        .expect("onboard result should exist")
        .get("server")
        .expect("server field should exist")
        .clone();
    assert_eq!(onboarded["server"], json!("mcp-figma"));
    assert_eq!(onboarded["trust_level"], json!("community"));

    let list_response = handle_rpc_request(
        JsonRpcRequest::new(json!(28802), "ListMcpServers", json!({})),
        store,
        state.clone(),
        OneApiConfig::default(),
    )
    .await;
    assert!(
        list_response.error.is_none(),
        "list mcp servers should succeed: {list_response:?}"
    );
    let servers = list_response
        .result
        .expect("list result should exist")
        .get("servers")
        .expect("servers field should exist")
        .as_array()
        .expect("servers should be array")
        .clone();
    assert!(
        servers
            .iter()
            .any(|server| server["server"] == json!("mcp-fs")),
        "builtin server should remain listed"
    );
    assert!(
        servers
            .iter()
            .any(|server| server["server"] == json!("mcp-figma")),
        "onboarded third-party server should be listed"
    );

    let mcp_host = state.mcp_host();
    mcp_host
        .lock()
        .await
        .stop_all()
        .await
        .expect("mcp host stop should succeed");
    cleanup_sqlite_files(&db_path);
}

#[cfg(test)]
#[tokio::test]
async fn onboard_third_party_mcp_handshake_failure_isolated() {
    let db_path = std::env::temp_dir().join(format!(
        "agentd-daemon-task28-isolation-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));

    let mut host = mcp::McpHost::new();
    host.start_declared_servers(&[mcp_valid_stdio_server("mcp-fs", "fs.read_file")])
        .await
        .expect("mcp host should start configured server");
    let state = RuntimeState::with_lifecycle_and_agent_card_root_and_mcp(
        "disabled",
        LifecycleManager::new(CgroupManager::new("/tmp/agentd-cgroup", "agentd")),
        PathBuf::from(default_agent_card_root()),
        Arc::new(Mutex::new(host)),
    );

    let create_response = handle_rpc_request(
        JsonRpcRequest::new(
            json!(28803),
            "CreateAgent",
            json!({
                "name": "task28-third-party-isolation",
                "model": "claude-4-sonnet",
                "permission_policy": "ask"
            }),
        ),
        store.clone(),
        state.clone(),
        OneApiConfig::default(),
    )
    .await;
    assert!(
        create_response.error.is_none(),
        "create should succeed: {create_response:?}"
    );
    let agent_id = create_response
        .result
        .expect("create result should exist")
        .get("agent")
        .expect("agent field should exist")
        .get("id")
        .expect("id field should exist")
        .as_str()
        .expect("agent id should be string")
        .to_string();

    let onboard_response = handle_rpc_request(
        JsonRpcRequest::new(
            json!(28804),
            "OnboardMcpServer",
            json!({
                "name": "mcp-bad-third-party",
                "command": "/bin/sh",
                "args": ["-c", "read _line; printf '%s\\n' 'not-json'; sleep 30"],
                "transport": "stdio",
                "trust_level": "community"
            }),
        ),
        store.clone(),
        state.clone(),
        OneApiConfig::default(),
    )
    .await;
    let onboarding_error = onboard_response
        .error
        .expect("invalid third-party handshake should fail");
    assert_eq!(onboarding_error.code, -32027);

    let list_response = handle_rpc_request(
        JsonRpcRequest::new(
            json!(28805),
            "ListAvailableTools",
            json!({
                "agent_id": agent_id,
            }),
        ),
        store,
        state.clone(),
        OneApiConfig::default(),
    )
    .await;
    assert!(
        list_response.error.is_none(),
        "list available tools should still succeed: {list_response:?}"
    );
    let tools = list_response
        .result
        .expect("list result should exist")
        .get("tools")
        .expect("tools field should exist")
        .as_array()
        .expect("tools should be array")
        .clone();
    assert!(
        tools
            .iter()
            .any(|tool| tool["policy_tool"] == json!("mcp.fs.read_file")),
        "builtin tool listing should remain available after third-party failure"
    );
    assert!(
        !tools
            .iter()
            .any(|tool| tool["server"] == json!("mcp-bad-third-party")),
        "failed third-party server should not leak into available tools"
    );

    let mcp_host = state.mcp_host();
    mcp_host
        .lock()
        .await
        .stop_all()
        .await
        .expect("mcp host stop should succeed");
    cleanup_sqlite_files(&db_path);
}

#[cfg(test)]
#[tokio::test]
async fn firecracker_vsock_roundtrip() {
    let root = temp_firecracker_runtime_dir();
    let script = firecracker_echo_script_path();
    assert!(script.exists(), "vsock echo script should exist");

    let executor = build_firecracker_executor_for_test(&root);
    let agent_id = uuid::Uuid::new_v4();
    let mut vm = executor
        .launch_agent(firecracker::FirecrackerAgentLaunchSpec {
            agent_id,
            command: "/usr/bin/env".to_string(),
            args: vec!["python3".to_string(), script.display().to_string()],
            env: HashMap::new(),
            vcpu_count: None,
            mem_size_mib: None,
            network: None,
            network_policy: None,
            jailer: None,
            launch_timeout: Duration::from_secs(2),
        })
        .await
        .expect("firecracker vm launch should succeed");

    let payload = json!({
        "rpc": "daemon.ping",
        "agent_id": agent_id,
        "body": {"message": "hello-vsock"}
    });
    let response = vm
        .roundtrip_json(&payload)
        .await
        .expect("vsock roundtrip should succeed");

    assert_eq!(response["status"], json!("ok"));
    assert_eq!(response["transport"], json!("vsock-simulated"));
    assert_eq!(response["echo"], payload);

    vm.shutdown()
        .await
        .expect("firecracker vm shutdown should succeed");
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(test)]
#[tokio::test]
async fn invoke_skill_denied_writes_audit() {
    let db_path = std::env::temp_dir().join(format!(
        "agentd-daemon-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));

    let mut host = mcp::McpHost::new();
    host.start_declared_servers(&[mcp_valid_stdio_server("mcp-fs", "fs.read_file")])
        .await
        .expect("mcp host should start configured server");
    let state = RuntimeState::with_lifecycle_and_agent_card_root_and_mcp(
        "disabled",
        LifecycleManager::new(CgroupManager::new("/tmp/agentd-cgroup", "agentd")),
        PathBuf::from(default_agent_card_root()),
        Arc::new(Mutex::new(host)),
    );

    let create_response = handle_rpc_request(
        JsonRpcRequest::new(
            json!(8901),
            "CreateAgent",
            json!({
                "name": "invoke-deny-agent",
                "model": "claude-4-sonnet",
                "permission_policy": "ask",
                "denied_tools": ["mcp.fs.read_file"],
            }),
        ),
        store.clone(),
        state.clone(),
        OneApiConfig::default(),
    )
    .await;
    assert!(
        create_response.error.is_none(),
        "create should succeed: {create_response:?}"
    );
    let agent_id = create_response
        .result
        .expect("create result should exist")
        .get("agent")
        .expect("agent field should exist")
        .get("id")
        .expect("id field should exist")
        .as_str()
        .expect("agent id should be string")
        .to_string();
    let parsed_agent_id = uuid::Uuid::parse_str(&agent_id).expect("agent id should be valid uuid");

    let invoke_response = handle_rpc_request(
        JsonRpcRequest::new(
            json!(8902),
            "InvokeSkill",
            json!({
                "agent_id": agent_id,
                "server": "mcp-fs",
                "tool": "fs.read_file",
                "args": {
                    "path": ".env"
                }
            }),
        ),
        store.clone(),
        state.clone(),
        OneApiConfig::default(),
    )
    .await;

    let denied_error = invoke_response
        .error
        .expect("invoke skill should be denied by policy");
    assert_eq!(denied_error.code, -32016);
    assert!(denied_error.message.contains("policy.deny"));

    let events = store
        .get_audit_events(parsed_agent_id)
        .await
        .expect("audit query should succeed");
    let deny_event = events
        .iter()
        .find(|event| event.event_type == EventType::ToolDenied)
        .expect("tool denied event should exist");
    assert_eq!(deny_event.payload.message.as_deref(), Some("policy.deny"));
    assert_eq!(
        deny_event.payload.tool_name.as_deref(),
        Some("mcp.fs.read_file")
    );
    assert_eq!(
        deny_event.payload.metadata["replay"]["input"]["args"]["path"],
        json!(".env")
    );

    let mcp_host = state.mcp_host();
    mcp_host
        .lock()
        .await
        .stop_all()
        .await
        .expect("mcp host stop should succeed");
    cleanup_sqlite_files(&db_path);
}

#[cfg(test)]
#[tokio::test]
async fn firecracker_launch_timeout_returns_stable_error() {
    let root = temp_firecracker_runtime_dir();
    let executor = build_firecracker_executor_for_test(&root);
    let agent_id = uuid::Uuid::new_v4();
    let pid_file = root.join("vm.pid");

    let command = format!("echo $$ > \"{}\"; sleep 5", pid_file.display());
    let err = executor
        .launch_agent(firecracker::FirecrackerAgentLaunchSpec {
            agent_id,
            command: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), command],
            env: HashMap::new(),
            vcpu_count: None,
            mem_size_mib: None,
            network: None,
            network_policy: None,
            jailer: None,
            launch_timeout: Duration::from_millis(200),
        })
        .await
        .expect_err("vm launch should timeout when vsock is not connected");

    match err {
        AgentError::Runtime(message) => {
            assert!(
                message.contains("firecracker launch timeout"),
                "unexpected timeout error message: {message}"
            );
        }
        other => panic!("expected runtime timeout error, got: {other}"),
    }

    let socket_path = executor.vsock_path_for_agent(agent_id);
    assert!(
        !socket_path.exists(),
        "launch timeout should clean stale vsock socket"
    );

    if let Ok(pid_text) = std::fs::read_to_string(&pid_file) {
        if let Ok(pid) = pid_text.trim().parse::<u32>() {
            firecracker_wait_process_exit(pid).await;
            assert!(
                !PathBuf::from(format!("/proc/{pid}")).exists(),
                "launch timeout should not leave orphan vm process"
            );
        }
    }

    let _ = std::fs::remove_dir_all(root);
}

#[cfg(test)]
#[tokio::test]
async fn untrusted_agent_uses_firecracker_runtime() {
    let db_path = std::env::temp_dir().join(format!(
        "agentd-daemon-task21-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let cgroup_root =
        std::env::temp_dir().join(format!("agentd-task21-cgroup-{}", uuid::Uuid::new_v4()));
    let firecracker_root = temp_firecracker_runtime_dir();
    let script = firecracker_echo_script_path();
    assert!(script.exists(), "vsock echo script should exist");

    let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
    let firecracker_executor = Arc::new(build_firecracker_executor_for_test(&firecracker_root));
    let state = RuntimeState::with_lifecycle_and_agent_card_root_and_mcp_and_firecracker(
        "disabled",
        LifecycleManager::new(CgroupManager::new(&cgroup_root, "agentd")),
        PathBuf::from(default_agent_card_root()),
        Arc::new(Mutex::new(McpHost::new())),
        Some(firecracker_executor),
    );

    let create_response = handle_rpc_request(
        JsonRpcRequest::new(
            json!(21001),
            "CreateAgent",
            json!({
                "name": "task21-untrusted-runtime",
                "model": "claude-4-sonnet",
            }),
        ),
        store.clone(),
        state.clone(),
        OneApiConfig::default(),
    )
    .await;
    assert!(
        create_response.error.is_none(),
        "create should succeed: {create_response:?}"
    );
    let agent_id = create_response
        .result
        .expect("create result should exist")
        .get("agent")
        .expect("agent field should exist")
        .get("id")
        .expect("id field should exist")
        .as_str()
        .expect("agent id should be string")
        .to_string();

    let start_response = handle_rpc_request(
        JsonRpcRequest::new(
            json!(21002),
            "StartManagedAgent",
            json!({
                "agent_id": agent_id,
                "command": "/usr/bin/env",
                "args": ["python3", script.display().to_string()],
                "restart_max_attempts": 0,
                "restart_backoff_secs": 0,
                "trust_level": "untrusted",
                "network_policy": "allow",
            }),
        ),
        store.clone(),
        state.clone(),
        OneApiConfig::default(),
    )
    .await;
    assert!(
        start_response.error.is_none(),
        "start should succeed: {start_response:?}"
    );
    assert_eq!(
        start_response
            .result
            .expect("start result should exist")
            .get("runtime")
            .expect("runtime field should exist"),
        "firecracker"
    );

    let stop_response = handle_rpc_request(
        JsonRpcRequest::new(
            json!(21003),
            "StopManagedAgent",
            json!({"agent_id": agent_id}),
        ),
        store,
        state,
        OneApiConfig::default(),
    )
    .await;
    assert!(
        stop_response.error.is_none(),
        "stop should succeed: {stop_response:?}"
    );

    let _ = std::fs::remove_dir_all(cgroup_root);
    let _ = std::fs::remove_dir_all(firecracker_root);
    cleanup_sqlite_files(&db_path);
}

#[cfg(test)]
#[tokio::test]
async fn jailer_policy_blocks_forbidden_network() {
    let root = temp_firecracker_runtime_dir();
    let executor = build_firecracker_executor_for_test(&root);

    let err = executor
        .launch_agent(firecracker::FirecrackerAgentLaunchSpec {
            agent_id: uuid::Uuid::new_v4(),
            command: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "echo should-not-run".to_string()],
            env: HashMap::new(),
            vcpu_count: None,
            mem_size_mib: None,
            network: Some(firecracker::FirecrackerNetworkConfig::default()),
            network_policy: Some(firecracker::NetworkIsolationPolicy::DenyAll),
            jailer: Some(firecracker::JailerConfig::default()),
            launch_timeout: Duration::from_secs(1),
        })
        .await
        .expect_err("deny-all network policy should block firecracker launch");

    match err {
        AgentError::Permission(message) => {
            assert!(
                message.contains("jailer network policy denied outbound access"),
                "unexpected error message: {message}"
            );
        }
        other => panic!("expected permission error, got: {other}"),
    }

    let _ = std::fs::remove_dir_all(root);
}

#[cfg(test)]
#[tokio::test]
async fn a2a_state_machine_valid_transitions() {
    let state = RuntimeState::new("disabled");
    let created = state
        .create_a2a_task(CreateA2ATaskRequest {
            agent_id: None,
            input: json!({"prompt": "hello"}),
        })
        .await;

    let working = state
        .transition_a2a_task(created.id, A2ATaskState::Working, None, None, json!({}))
        .await
        .expect("submitted -> working should be valid");
    assert_eq!(working.state, A2ATaskState::Working);

    let waiting = state
        .transition_a2a_task(
            created.id,
            A2ATaskState::InputRequired,
            None,
            None,
            json!({"hint": "need user input"}),
        )
        .await
        .expect("working -> input-required should be valid");
    assert_eq!(waiting.state, A2ATaskState::InputRequired);

    let resumed = state
        .transition_a2a_task(created.id, A2ATaskState::Working, None, None, json!({}))
        .await
        .expect("input-required -> working should be valid");
    assert_eq!(resumed.state, A2ATaskState::Working);

    let completed = state
        .transition_a2a_task(
            created.id,
            A2ATaskState::Completed,
            Some(json!({"result": "done"})),
            None,
            json!({}),
        )
        .await
        .expect("working -> completed should be valid");
    assert_eq!(completed.state, A2ATaskState::Completed);
}

#[cfg(test)]
#[tokio::test]
async fn a2a_state_machine_rejects_completed_to_working() {
    let state = RuntimeState::new("disabled");
    let created = state
        .create_a2a_task(CreateA2ATaskRequest {
            agent_id: None,
            input: json!({"prompt": "hello"}),
        })
        .await;

    let _ = state
        .transition_a2a_task(created.id, A2ATaskState::Working, None, None, json!({}))
        .await
        .expect("submitted -> working should be valid");
    let _ = state
        .transition_a2a_task(
            created.id,
            A2ATaskState::Completed,
            Some(json!({"result": "done"})),
            None,
            json!({}),
        )
        .await
        .expect("working -> completed should be valid");

    let err = state
        .transition_a2a_task(created.id, A2ATaskState::Working, None, None, json!({}))
        .await
        .expect_err("completed -> working should be rejected");
    assert!(
        err.to_string().contains("invalid a2a state transition"),
        "unexpected error: {err}"
    );
}

#[cfg(test)]
#[tokio::test]
async fn a2a_server_task_crud_and_stream() {
    let db_path = std::env::temp_dir().join(format!(
        "agentd-daemon-task22-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
    let state = RuntimeState::new("disabled");
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind health server listener");
    let bind_addr = listener
        .local_addr()
        .expect("resolve health server local address");

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server_task = tokio::spawn(health_server(
        listener,
        bind_addr,
        store,
        state.clone(),
        OneApiConfig::default(),
        shutdown_rx,
    ));

    let mut create_conn = tokio::net::TcpStream::connect(bind_addr)
        .await
        .expect("connect a2a create endpoint");
    let create_body = json!({
        "input": {"prompt": "ping"}
    })
    .to_string();
    let create_req = format!(
        "POST /a2a/tasks HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        create_body.len(),
        create_body
    );
    create_conn
        .write_all(create_req.as_bytes())
        .await
        .expect("send create request");
    let mut create_resp = Vec::new();
    create_conn
        .read_to_end(&mut create_resp)
        .await
        .expect("read create response");
    let create_text = String::from_utf8(create_resp).expect("create response should be utf8");
    assert!(create_text.starts_with("HTTP/1.1 201 Created"));
    let create_payload = create_text
        .split("\r\n\r\n")
        .nth(1)
        .expect("create response body should exist");
    let created_json: Value =
        serde_json::from_str(create_payload).expect("create response body should be valid json");
    let task_id = created_json["task"]["id"]
        .as_str()
        .expect("task id should be present")
        .to_string();

    let mut get_conn = tokio::net::TcpStream::connect(bind_addr)
        .await
        .expect("connect a2a get endpoint");
    let get_req = format!(
        "GET /a2a/tasks/{task_id} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    );
    get_conn
        .write_all(get_req.as_bytes())
        .await
        .expect("send get request");
    let mut get_resp = Vec::new();
    get_conn
        .read_to_end(&mut get_resp)
        .await
        .expect("read get response");
    let get_text = String::from_utf8(get_resp).expect("get response should be utf8");
    assert!(get_text.starts_with("HTTP/1.1 200 OK"));

    let mut stream_conn = tokio::net::TcpStream::connect(bind_addr)
        .await
        .expect("connect a2a stream endpoint");
    let stream_req = format!(
        "GET /a2a/stream?task_id={task_id} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    );
    stream_conn
        .write_all(stream_req.as_bytes())
        .await
        .expect("send stream request");
    let mut stream_resp = Vec::new();
    stream_conn
        .read_to_end(&mut stream_resp)
        .await
        .expect("read stream response");
    let stream_text = String::from_utf8(stream_resp).expect("stream response should be utf8");
    assert!(stream_text.starts_with("HTTP/1.1 200 OK"));
    assert!(stream_text.contains("\"state\":\"submitted\""));
    assert!(stream_text.contains("\"state\":\"working\""));
    assert!(stream_text.contains("\"state\":\"completed\""));

    let _ = shutdown_tx.send(true);
    let _ = server_task.await;
    cleanup_sqlite_files(&db_path);
}

#[cfg(test)]
#[tokio::test]
async fn ws_bridge_forwards_rpc_and_stream() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::{connect_async, tungstenite::Message};

    let db_path = std::env::temp_dir().join(format!(
        "agentd-daemon-task27-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
    let state = RuntimeState::new("disabled");
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind health server listener");
    let bind_addr = listener
        .local_addr()
        .expect("resolve health server local address");

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server_task = tokio::spawn(health_server(
        listener,
        bind_addr,
        store,
        state.clone(),
        OneApiConfig::default(),
        shutdown_rx,
    ));

    let (mut socket, _) = connect_async(format!("ws://{bind_addr}/ws"))
        .await
        .expect("connect websocket bridge");

    socket
        .send(Message::Text(
            json!({
                "jsonrpc": "2.0",
                "id": 9001,
                "method": "GetHealth",
                "params": {}
            })
            .to_string(),
        ))
        .await
        .expect("send health rpc over ws");

    let mut health_result_seen = false;
    for _ in 0..3 {
        let next = tokio::time::timeout(Duration::from_millis(800), socket.next())
            .await
            .expect("ws health timeout");
        let Some(message) = next else {
            break;
        };
        let message = message.expect("receive ws health response");
        let Message::Text(payload) = message else {
            continue;
        };
        let decoded: Value =
            serde_json::from_str(payload.as_ref()).expect("decode ws json response");
        if decoded.get("id") == Some(&json!(9001)) {
            assert!(
                decoded.get("error").is_none(),
                "rpc should succeed: {decoded}"
            );
            assert!(
                decoded["result"].get("status").is_some(),
                "health result should include status"
            );
            health_result_seen = true;
            break;
        }
    }
    assert!(
        health_result_seen,
        "expected health rpc response over websocket"
    );

    let created = state
        .create_a2a_task(CreateA2ATaskRequest {
            agent_id: None,
            input: json!({"prompt": "ws stream"}),
        })
        .await;
    tokio::spawn(drive_a2a_task_lifecycle(state.clone(), created.id));

    socket
        .send(Message::Text(
            json!({
                "jsonrpc": "2.0",
                "id": 9002,
                "method": "A2A.SubscribeStream",
                "params": {
                    "task_id": created.id,
                }
            })
            .to_string(),
        ))
        .await
        .expect("subscribe task stream over ws");

    let mut subscribe_ack_seen = false;
    let mut streamed_states = Vec::new();
    for _ in 0..12 {
        let next = tokio::time::timeout(Duration::from_millis(800), socket.next())
            .await
            .expect("ws stream timeout");
        let Some(message) = next else {
            break;
        };
        let message = message.expect("receive ws stream message");
        let Message::Text(payload) = message else {
            continue;
        };
        let decoded: Value = serde_json::from_str(payload.as_ref()).expect("decode ws stream json");

        if decoded.get("id") == Some(&json!(9002)) {
            subscribe_ack_seen = true;
            continue;
        }

        if decoded.get("method") == Some(&json!("A2A.StreamEvent")) {
            let state_value = decoded["params"]["state"]
                .as_str()
                .expect("stream event state should be string")
                .to_string();
            streamed_states.push(state_value.clone());
            if state_value == "completed" {
                break;
            }
        }
    }

    assert!(subscribe_ack_seen, "expected subscribe ack over websocket");
    assert!(
        streamed_states.iter().any(|state| state == "working"),
        "expected working state in stream: {streamed_states:?}"
    );
    assert!(
        streamed_states.iter().any(|state| state == "completed"),
        "expected completed state in stream: {streamed_states:?}"
    );

    let _ = socket.close(None).await;
    let _ = shutdown_tx.send(true);
    let _ = server_task.await;
    cleanup_sqlite_files(&db_path);
}

#[cfg(test)]
#[tokio::test]
async fn a2a_client_discovers_remote_card() {
    let db_path = std::env::temp_dir().join(format!(
        "agentd-daemon-task23-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
    let state = RuntimeState::new("disabled");

    let created = handle_rpc_request(
        JsonRpcRequest::new(
            json!(1),
            "CreateAgent",
            json!({
                "name": "remote-card-agent",
                "model": "claude-4-sonnet",
            }),
        ),
        store.clone(),
        state.clone(),
        OneApiConfig::default(),
    )
    .await;
    assert!(
        created.error.is_none(),
        "create should succeed: {created:?}"
    );

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test health listener");
    let bind_addr = listener
        .local_addr()
        .expect("resolve listener socket address");
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server_task = tokio::spawn(health_server(
        listener,
        bind_addr,
        store,
        state,
        OneApiConfig::default(),
        shutdown_rx,
    ));

    let client = A2AHttpClient::new();
    let card = client
        .discover_agent(&format!("http://{bind_addr}"))
        .await
        .expect("discover remote agent card");
    assert!(!card.agent_id.is_empty());
    assert_eq!(card.name, "remote-card-agent");
    assert!(!card.version.is_empty());
    assert_eq!(card.model, "claude-4-sonnet");
    assert_eq!(card.provider, "one-api");
    assert_eq!(card.capabilities["protocol"], json!("a2a-compatible"));

    let _ = shutdown_tx.send(true);
    let _ = server_task.await;
    cleanup_sqlite_files(&db_path);
}

#[cfg(test)]
#[tokio::test]
async fn orchestrator_splits_and_aggregates_tasks() {
    let db_path = std::env::temp_dir().join(format!(
        "agentd-daemon-task24-orchestrator-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
    let state = RuntimeState::new("disabled");

    let result = orchestrate_task_with_delegate(
        &state,
        store.as_ref(),
        OrchestrateTaskParams {
            parent_agent_id: None,
            input: json!({ "goal": "ship task 24" }),
            subtasks: Some(vec![
                json!({"work": "decompose"}),
                json!({"work": "delegate"}),
                json!({"work": "aggregate"}),
            ]),
            delegate_agent_ids: vec!["agent-a".to_string(), "agent-b".to_string()],
            retry_limit: Some(1),
        },
        |agent_id, child_input, child_index, _attempt| {
            Ok(json!({
                "agent_id": agent_id,
                "child_index": child_index,
                "work": child_input["work"],
            }))
        },
    )
    .await
    .expect("orchestrate task should succeed");

    assert_eq!(result.state, A2ATaskState::Completed);
    assert_eq!(result.children.len(), 3);
    assert_eq!(result.children[0].agent_id, "agent-a");
    assert_eq!(result.children[1].agent_id, "agent-b");
    assert_eq!(result.children[2].agent_id, "agent-a");

    let aggregated = result
        .aggregated_output
        .get("results")
        .and_then(Value::as_array)
        .expect("aggregated results should be an array");
    assert_eq!(aggregated.len(), 3);
    assert_eq!(aggregated[0]["agent_id"], json!("agent-a"));
    assert_eq!(aggregated[1]["agent_id"], json!("agent-b"));
    assert_eq!(aggregated[2]["agent_id"], json!("agent-a"));

    cleanup_sqlite_files(&db_path);
}

#[cfg(test)]
#[tokio::test]
async fn orchestrator_retries_failed_child_once() {
    let db_path = std::env::temp_dir().join(format!(
        "agentd-daemon-task24-orchestrator-retry-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
    let state = RuntimeState::new("disabled");

    let mut attempts_by_child: HashMap<usize, u8> = HashMap::new();
    let result = orchestrate_task_with_delegate(
        &state,
        store.as_ref(),
        OrchestrateTaskParams {
            parent_agent_id: None,
            input: json!({ "subtasks": [{"work": "ok"}, {"work": "flaky"}] }),
            subtasks: None,
            delegate_agent_ids: vec!["agent-a".to_string(), "agent-b".to_string()],
            retry_limit: Some(1),
        },
        |_agent_id, child_input, child_index, _attempt| {
            let entry = attempts_by_child.entry(child_index).or_insert(0);
            *entry = entry.saturating_add(1);

            if child_input["work"] == json!("flaky") && *entry == 1 {
                return Err(AgentError::Runtime(
                    "temporary child execution failure".to_string(),
                ));
            }

            Ok(json!({
                "work": child_input["work"],
                "attempts_seen": *entry,
            }))
        },
    )
    .await
    .expect("orchestrator should recover from one retryable failure");

    assert_eq!(result.state, A2ATaskState::Completed);
    assert_eq!(result.children.len(), 2);
    assert_eq!(result.children[1].state, A2ATaskState::Completed);
    assert_eq!(result.children[1].attempts, 2);
    assert_eq!(
        result.aggregated_output["summary"]["retried_children"],
        json!(1)
    );

    cleanup_sqlite_files(&db_path);
}

#[cfg(test)]
#[tokio::test]
async fn mdns_peer_discovery_finds_remote_agent() {
    let db_path = std::env::temp_dir().join(format!(
        "agentd-daemon-task25-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
    let state = RuntimeState::new("disabled");
    state
        .set_lan_discovery_for_test(Arc::new(StaticLanDiscovery {
            records: vec![DiscoveryAgentRecord {
                agent_id: "remote-agent-1".to_string(),
                name: "remote-agent".to_string(),
                model: "claude-4-sonnet".to_string(),
                provider: "one-api".to_string(),
                endpoint: "http://10.0.0.2:8080".to_string(),
                source: "lan".to_string(),
                health: "ready".to_string(),
            }],
        }))
        .await;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test health listener");
    let bind_addr = listener
        .local_addr()
        .expect("resolve listener socket address");
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server_task = tokio::spawn(health_server(
        listener,
        bind_addr,
        store,
        state,
        OneApiConfig::default(),
        shutdown_rx,
    ));

    let mut conn = tokio::net::TcpStream::connect(bind_addr)
        .await
        .expect("connect discover endpoint");
    let req = "GET /discover HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    conn.write_all(req.as_bytes())
        .await
        .expect("send discover request");
    let mut resp = Vec::new();
    conn.read_to_end(&mut resp)
        .await
        .expect("read discover response");
    let text = String::from_utf8(resp).expect("discover response should be utf8");
    assert!(text.starts_with("HTTP/1.1 200 OK"));
    let body = text
        .split("\r\n\r\n")
        .nth(1)
        .expect("discover response body should exist");
    let payload: Value =
        serde_json::from_str(body).expect("discover response body should be valid json");
    assert_eq!(payload["lan"][0]["agent_id"], json!("remote-agent-1"));
    assert_eq!(payload["lan"][0]["source"], json!("lan"));

    let _ = shutdown_tx.send(true);
    let _ = server_task.await;
    cleanup_sqlite_files(&db_path);
}

#[cfg(test)]
#[tokio::test]
async fn semantic_migration_l1_continues_workflow() {
    let source_db_path = std::env::temp_dir().join(format!(
        "agentd-daemon-task26-l1-source-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let target_db_path = std::env::temp_dir().join(format!(
        "agentd-daemon-task26-l1-target-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let source_store =
        Arc::new(SqliteStore::new(&source_db_path).expect("initialize source store"));
    let target_store =
        Arc::new(SqliteStore::new(&target_db_path).expect("initialize target store"));
    let source_state = RuntimeState::new("disabled");
    let target_state = RuntimeState::new("disabled");

    let source_created = handle_rpc_request(
        JsonRpcRequest::new(
            json!(260001),
            "CreateAgent",
            json!({
                "name": "task26-source-l1",
                "model": "claude-4-sonnet",
            }),
        ),
        source_store.clone(),
        source_state.clone(),
        OneApiConfig::default(),
    )
    .await;
    assert!(
        source_created.error.is_none(),
        "source create should succeed: {source_created:?}"
    );
    let source_agent_id = source_created
        .result
        .expect("source create result should exist")["agent"]["id"]
        .as_str()
        .expect("source agent id should be string")
        .to_string();

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind target listener");
    let bind_addr = listener
        .local_addr()
        .expect("resolve target listener address");
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server_task = tokio::spawn(health_server(
        listener,
        bind_addr,
        target_store,
        target_state,
        OneApiConfig::default(),
        shutdown_rx,
    ));

    let migration_response = handle_rpc_request(
        JsonRpcRequest::new(
            json!(260002),
            "MigrateContext",
            json!({
                "source_agent_id": source_agent_id,
                "target_base_url": format!("http://{bind_addr}"),
                "messages": [
                    {
                        "id": "msg-root",
                        "parent_id": Value::Null,
                        "role": "user",
                        "content": "remember project codename atlas and prepare migration checklist"
                    },
                    {
                        "id": "msg-head",
                        "parent_id": "msg-root",
                        "role": "assistant",
                        "content": "atlas checklist captured; continue workflow on next device"
                    }
                ],
                "head_id": "msg-head",
                "key_files": ["README.md", "crates/agentd-daemon/src/main.rs"],
                "include_snapshot": false
            }),
        ),
        source_store.clone(),
        source_state,
        OneApiConfig::default(),
    )
    .await;
    assert!(
        migration_response.error.is_none(),
        "migration should succeed: {migration_response:?}"
    );

    let result = migration_response
        .result
        .expect("migration result should exist");
    assert_eq!(result["migration_level"], json!("l1"));
    assert_eq!(result["source_session_state"], json!("migrated"));
    assert_eq!(result["target_state"], json!("completed"));

    let resume_prompt = result["target_output"]["migration_restore"]["resume_prompt"]
        .as_str()
        .expect("resume_prompt should be string");
    assert!(
        resume_prompt.contains("atlas"),
        "resume prompt should include migrated key fact: {resume_prompt}"
    );
    assert!(
        resume_prompt.contains("context summary"),
        "resume prompt should include summary marker: {resume_prompt}"
    );

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string");
    let persisted = source_store
        .get_context_session_snapshot(session_id)
        .await
        .expect("load source migration snapshot")
        .expect("source migration snapshot should exist");
    assert_eq!(persisted.migration_state, "migrated");

    let _ = shutdown_tx.send(true);
    let _ = server_task.await;
    cleanup_sqlite_files(&source_db_path);
    cleanup_sqlite_files(&target_db_path);
}

#[cfg(test)]
#[tokio::test]
async fn snapshot_migration_l2_roundtrip() {
    let source_db_path = std::env::temp_dir().join(format!(
        "agentd-daemon-task26-l2-source-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let target_db_path = std::env::temp_dir().join(format!(
        "agentd-daemon-task26-l2-target-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let source_store =
        Arc::new(SqliteStore::new(&source_db_path).expect("initialize source store"));
    let target_store =
        Arc::new(SqliteStore::new(&target_db_path).expect("initialize target store"));
    let source_state = RuntimeState::new("disabled");
    let target_state = RuntimeState::new("disabled");

    let source_created = handle_rpc_request(
        JsonRpcRequest::new(
            json!(260101),
            "CreateAgent",
            json!({
                "name": "task26-source-l2",
                "model": "claude-4-sonnet",
            }),
        ),
        source_store.clone(),
        source_state.clone(),
        OneApiConfig::default(),
    )
    .await;
    assert!(
        source_created.error.is_none(),
        "source create should succeed: {source_created:?}"
    );
    let source_agent_id = source_created
        .result
        .expect("source create result should exist")["agent"]["id"]
        .as_str()
        .expect("source agent id should be string")
        .to_string();

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind target listener");
    let bind_addr = listener
        .local_addr()
        .expect("resolve target listener address");
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server_task = tokio::spawn(health_server(
        listener,
        bind_addr,
        target_store,
        target_state,
        OneApiConfig::default(),
        shutdown_rx,
    ));

    let migration_response = handle_rpc_request(
        JsonRpcRequest::new(
            json!(260102),
            "MigrateContext",
            json!({
                "source_agent_id": source_agent_id,
                "target_base_url": format!("http://{bind_addr}"),
                "messages": [
                    {
                        "id": "root",
                        "parent_id": Value::Null,
                        "role": "system",
                        "content": "boot"
                    },
                    {
                        "id": "user-1",
                        "parent_id": "root",
                        "role": "user",
                        "content": "review migration notes"
                    },
                    {
                        "id": "assistant-1",
                        "parent_id": "user-1",
                        "role": "assistant",
                        "content": "notes captured"
                    }
                ],
                "head_id": "assistant-1",
                "tool_results_cache": {
                    "call-1": {"status": "ok"},
                    "call-2": {"status": "cached"}
                },
                "working_directory": {
                    "README.md": "# migrated",
                    "notes/todo.txt": "finish l2"
                },
                "include_snapshot": true
            }),
        ),
        source_store.clone(),
        source_state,
        OneApiConfig::default(),
    )
    .await;
    assert!(
        migration_response.error.is_none(),
        "l2 migration should succeed: {migration_response:?}"
    );

    let result = migration_response
        .result
        .expect("l2 migration result should exist");
    assert_eq!(result["migration_level"], json!("l2"));
    assert_eq!(result["target_state"], json!("completed"));
    assert_eq!(
        result["target_output"]["migration_restore"]["snapshot"]["message_count"],
        json!(3)
    );
    assert_eq!(
        result["target_output"]["migration_restore"]["snapshot"]["tool_cache_entries"],
        json!(2)
    );

    let files = result["target_output"]["migration_restore"]["snapshot"]["working_directory_files"]
        .as_array()
        .expect("working_directory_files should be array")
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    assert!(
        files.contains(&"README.md") && files.contains(&"notes/todo.txt"),
        "restored snapshot should include working directory files: {files:?}"
    );

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string");
    let persisted = source_store
        .get_context_session_snapshot(session_id)
        .await
        .expect("load source migration snapshot")
        .expect("source migration snapshot should exist");
    assert_eq!(persisted.migration_state, "migrated");
    assert_eq!(persisted.messages.len(), 3);
    assert_eq!(
        persisted
            .tool_results_cache
            .as_object()
            .map(|value| value.len())
            .unwrap_or(0),
        2
    );
    assert_eq!(
        persisted.working_directory.get("README.md"),
        Some(&"# migrated".to_string())
    );

    let _ = shutdown_tx.send(true);
    let _ = server_task.await;
    cleanup_sqlite_files(&source_db_path);
    cleanup_sqlite_files(&target_db_path);
}

#[cfg(test)]
#[tokio::test]
async fn migration_failure_rolls_back_source_session() {
    let source_db_path = std::env::temp_dir().join(format!(
        "agentd-daemon-task26-rollback-source-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let source_store =
        Arc::new(SqliteStore::new(&source_db_path).expect("initialize source store"));
    let source_state = RuntimeState::new("disabled");

    let source_created = handle_rpc_request(
        JsonRpcRequest::new(
            json!(260201),
            "CreateAgent",
            json!({
                "name": "task26-source-rollback",
                "model": "claude-4-sonnet",
            }),
        ),
        source_store.clone(),
        source_state.clone(),
        OneApiConfig::default(),
    )
    .await;
    assert!(
        source_created.error.is_none(),
        "source create should succeed: {source_created:?}"
    );
    let source_agent_id = source_created
        .result
        .expect("source create result should exist")["agent"]["id"]
        .as_str()
        .expect("source agent id should be string")
        .to_string();

    let session_id = format!("session-task26-rollback-{}", uuid::Uuid::new_v4());
    let migration_response = handle_rpc_request(
        JsonRpcRequest::new(
            json!(260202),
            "MigrateContext",
            json!({
                "source_agent_id": source_agent_id,
                "target_base_url": "http://127.0.0.1:9",
                "session_id": session_id,
                "messages": [
                    {
                        "id": "rollback-root",
                        "parent_id": Value::Null,
                        "role": "user",
                        "content": "must remain runnable on failure"
                    }
                ],
                "include_snapshot": false
            }),
        ),
        source_store.clone(),
        source_state,
        OneApiConfig::default(),
    )
    .await;
    assert!(migration_response.error.is_some(), "migration should fail");
    assert!(
        migration_response
            .error
            .as_ref()
            .map(|err| err.message.contains("context migration failed"))
            .unwrap_or(false),
        "failure should include context migration error: {migration_response:?}"
    );

    let persisted = source_store
        .get_context_session_snapshot(&session_id)
        .await
        .expect("load persisted source snapshot after failure")
        .expect("snapshot should persist for rollback");
    assert_eq!(persisted.migration_state, "active");
    assert_eq!(persisted.messages.len(), 1);

    cleanup_sqlite_files(&source_db_path);
}

async fn health_server(
    listener: TcpListener,
    bind_addr: SocketAddr,
    store: Arc<SqliteStore>,
    state: RuntimeState,
    one_api_config: OneApiConfig,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!(%bind_addr, "Health endpoint listening");

    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    info!("Health server received shutdown signal");
                    break;
                }
            }
            accepted = listener.accept() => {
                let (mut stream, _) = accepted?;

                let mut buf = [0_u8; 8192];
                let read = stream.read(&mut buf).await?;
                let request = String::from_utf8_lossy(&buf[..read]).to_string();
                let request_line = request.lines().next().unwrap_or("");
                let mut request_parts = request_line.split_whitespace();
                let method = request_parts.next().unwrap_or("");
                let raw_path = request_parts.next().unwrap_or("/");
                let (path, query) = split_path_and_query(raw_path);

                if ws_bridge::is_ws_upgrade_request(method, path, &request) {
                    ws_bridge::serve_ws_bridge(
                        stream,
                        &request,
                        store.clone(),
                        state.clone(),
                        one_api_config.clone(),
                    )
                    .await?;
                    continue;
                }

                if method == "GET" && path == "/.well-known/agent.json" {
                    let response = match store.list_agents().await {
                        Ok(agents) => {
                            if let Some(profile) = agents.into_iter().next() {
                                json_http_response("200 OK", &build_a2a_agent_card(&profile))?
                            } else {
                                json_http_response(
                                    "404 Not Found",
                                    &json!({"error": "no registered agents"}),
                                )?
                            }
                        }
                        Err(err) => json_http_response(
                            "500 Internal Server Error",
                            &json!({"error": format!("load agent card failed: {err}")}),
                        )?,
                    };
                    stream.write_all(response.as_bytes()).await?;
                    let _ = stream.shutdown().await;
                    continue;
                }

                if method == "POST" && path == "/registry/agents" {
                    let parsed = serde_json::from_str::<RegistryRegisterRequest>(request_body(&request));
                    let payload = match parsed {
                        Ok(value) => value,
                        Err(err) => {
                            let response = json_http_response(
                                "400 Bad Request",
                                &json!({"error": format!("invalid registry payload: {err}")}),
                            )?;
                            stream.write_all(response.as_bytes()).await?;
                            let _ = stream.shutdown().await;
                            continue;
                        }
                    };

                    let entry = RegistryAgentEntry {
                        agent_id: payload.agent_id,
                        name: payload.name,
                        model: payload.model,
                        provider: payload.provider,
                        endpoint: payload.endpoint,
                        health: payload.health.unwrap_or_else(|| "unknown".to_string()),
                        updated_at: Utc::now().to_rfc3339(),
                    };
                    state.upsert_registry_agent(entry.clone()).await;
                    let response = json_http_response("201 Created", &json!({"agent": entry}))?;
                    stream.write_all(response.as_bytes()).await?;
                    let _ = stream.shutdown().await;
                    continue;
                }

                if method == "GET" && path == "/registry/agents" {
                    let agents = state.list_registry_agents().await;
                    let response = json_http_response("200 OK", &json!({"agents": agents}))?;
                    stream.write_all(response.as_bytes()).await?;
                    let _ = stream.shutdown().await;
                    continue;
                }

                if method == "GET" && path == "/discover" {
                    let mut errors = Vec::new();

                    let lan = match state.discover_lan_agents().await {
                        Ok(records) => records,
                        Err(err) => {
                            errors.push(format!("lan discovery failed: {err}"));
                            Vec::new()
                        }
                    };

                    let registry = if let Some(registry_url) = query_param(query, "registry_url") {
                        match fetch_remote_registry_agents(&registry_url).await {
                            Ok(records) => records,
                            Err(err) => {
                                errors.push(format!("registry query failed: {err}"));
                                state.list_registry_agents().await
                            }
                        }
                    } else {
                        state.list_registry_agents().await
                    };

                    let registry = registry
                        .iter()
                        .map(registry_entry_to_discovery)
                        .collect::<Vec<_>>();
                    let response = json_http_response(
                        "200 OK",
                        &json!({
                            "lan": lan,
                            "registry": registry,
                            "errors": errors,
                        }),
                    )?;
                    stream.write_all(response.as_bytes()).await?;
                    let _ = stream.shutdown().await;
                    continue;
                }

                if method == "GET" && path == "/a2a/stream" {
                    let Some(task_id_raw) = query_param(query, "task_id") else {
                        let response = json_http_response(
                            "400 Bad Request",
                            &json!({"error": "missing task_id"}),
                        )?;
                        stream.write_all(response.as_bytes()).await?;
                        let _ = stream.shutdown().await;
                        continue;
                    };
                    let task_id = match uuid::Uuid::parse_str(&task_id_raw) {
                        Ok(task_id) => task_id,
                        Err(err) => {
                            let response = json_http_response(
                                "400 Bad Request",
                                &json!({"error": format!("invalid task_id: {err}")}),
                            )?;
                            stream.write_all(response.as_bytes()).await?;
                            let _ = stream.shutdown().await;
                            continue;
                        }
                    };

                    let Some(task) = state.get_a2a_task(task_id).await else {
                        let response = json_http_response(
                            "404 Not Found",
                            &json!({"error": "a2a task not found"}),
                        )?;
                        stream.write_all(response.as_bytes()).await?;
                        let _ = stream.shutdown().await;
                        continue;
                    };

                    let sse_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n";
                    stream.write_all(sse_headers.as_bytes()).await?;

                    let initial = A2ATaskEvent {
                        task_id: task.id,
                        state: task.state,
                        lifecycle_state: task.state.to_agent_lifecycle_state(),
                        timestamp: Utc::now(),
                        payload: json!({"task": task}),
                    };
                    let encoded_initial = serde_json::to_string(&initial)?;
                    stream
                        .write_all(format!("event: task\ndata: {encoded_initial}\n\n").as_bytes())
                        .await?;

                    let mut subscription = state.subscribe_a2a_stream();
                    let stream_deadline = tokio::time::Instant::now() + Duration::from_secs(3);
                    while tokio::time::Instant::now() < stream_deadline {
                        match tokio::time::timeout(Duration::from_millis(800), subscription.recv()).await {
                            Ok(Ok(event)) if event.task_id == task_id => {
                                let encoded = serde_json::to_string(&event)?;
                                stream
                                    .write_all(format!("event: task\ndata: {encoded}\n\n").as_bytes())
                                    .await?;
                                if matches!(event.state, A2ATaskState::Completed | A2ATaskState::Failed | A2ATaskState::Canceled) {
                                    break;
                                }
                            }
                            Ok(Ok(_)) => {}
                            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
                            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) | Err(_) => break,
                        }
                    }

                    let _ = stream.shutdown().await;
                    continue;
                }

                let response = if method == "POST" && path == "/rpc" {
                    let body = request_body(&request);
                    match serde_json::from_str::<JsonRpcRequest>(body) {
                        Ok(rpc_request) => {
                            let rpc_response = handle_rpc_request(
                                rpc_request,
                                store.clone(),
                                state.clone(),
                                one_api_config.clone(),
                            )
                            .await;
                            json_http_response("200 OK", &serde_json::to_value(&rpc_response).unwrap_or(json!({"error":"serialize failed"})))?
                        }
                        Err(err) => {
                            let error_response = JsonRpcResponse::error(
                                json!(null),
                                -32700,
                                format!("parse error: {err}"),
                            );
                            json_http_response("200 OK", &serde_json::to_value(&error_response).unwrap_or(json!({"error":"serialize failed"})))?
                        }
                    }
                } else if method == "GET" && (path == "/health") {
                    let storage_status = if store.health_check().is_ok() {
                        "ready"
                    } else {
                        "degraded"
                    };
                    let one_api_status = state.one_api_status().await;
                    let overall_status = if storage_status == "ready"
                        && matches!(one_api_status.as_str(), "ready" | "disabled")
                    {
                        "ok"
                    } else {
                        "degraded"
                    };

                    json_http_response("200 OK", &json!({
                        "status": overall_status,
                        "subsystems": {
                            "daemon": "ready",
                            "protocol": "ready",
                            "storage": storage_status,
                            "one_api": one_api_status,
                        }
                    }))?
                } else if method == "POST" && path == "/a2a/tasks" {
                    let parsed = serde_json::from_str::<CreateA2ATaskRequest>(request_body(&request));
                    let payload = match parsed {
                        Ok(payload) => payload,
                        Err(err) => {
                            let response = json_http_response(
                                "400 Bad Request",
                                &json!({"error": format!("invalid a2a task payload: {err}")}),
                            )?;
                            stream.write_all(response.as_bytes()).await?;
                            let _ = stream.shutdown().await;
                            continue;
                        }
                    };
                    let created = state.create_a2a_task(payload).await;
                    tokio::spawn(drive_a2a_task_lifecycle(state.clone(), created.id));
                    json_http_response("201 Created", &json!({"task": created}))?
                } else if method == "GET" && path.starts_with("/a2a/tasks/") {
                    let task_id_raw = path.trim_start_matches("/a2a/tasks/");
                    let task_id = match uuid::Uuid::parse_str(task_id_raw) {
                        Ok(task_id) => task_id,
                        Err(err) => {
                            let response = json_http_response(
                                "400 Bad Request",
                                &json!({"error": format!("invalid task_id: {err}")}),
                            )?;
                            stream.write_all(response.as_bytes()).await?;
                            let _ = stream.shutdown().await;
                            continue;
                        }
                    };

                    match state.get_a2a_task(task_id).await {
                        Some(task) => json_http_response("200 OK", &json!({"task": task}))?,
                        None => json_http_response("404 Not Found", &json!({"error": "a2a task not found"}))?,
                    }
                } else {
                    json_http_response("404 Not Found", &json!({"error":"not found"}))?
                };

                stream.write_all(response.as_bytes()).await?;
                let _ = stream.shutdown().await;
            }
        }
    }

    Ok(())
}

fn json_http_response(status_line: &str, body: &Value) -> Result<String, serde_json::Error> {
    let encoded = serde_json::to_string(body)?;
    Ok(format!(
        "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        encoded.len(),
        encoded
    ))
}

fn split_path_and_query(path: &str) -> (&str, Option<&str>) {
    match path.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (path, None),
    }
}

fn query_param(query: Option<&str>, key: &str) -> Option<String> {
    let query = query?;
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.to_string())
}

fn request_body(request: &str) -> &str {
    request.split("\r\n\r\n").nth(1).unwrap_or("")
}

async fn drive_a2a_task_lifecycle(state: RuntimeState, task_id: uuid::Uuid) {
    let task_input = state
        .get_a2a_task(task_id)
        .await
        .map(|task| task.input)
        .unwrap_or_else(|| json!({}));
    let restored_context = match restore_task_migration_context(&task_input) {
        Ok(context) => context,
        Err(err) => {
            let _ = state
                .transition_a2a_task(
                    task_id,
                    A2ATaskState::Failed,
                    None,
                    Some(err.to_string()),
                    json!({"migration_restore_error": err.to_string()}),
                )
                .await;
            return;
        }
    };

    tokio::time::sleep(Duration::from_millis(50)).await;
    let _ = state
        .transition_a2a_task(task_id, A2ATaskState::Working, None, None, json!({}))
        .await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let completion_output = if let Some(context) = restored_context.clone() {
        json!({
            "result": "ok",
            "migration_restore": context,
        })
    } else {
        json!({"result": "ok"})
    };
    let completion_payload = restored_context
        .map(|context| json!({"migration_restore": context}))
        .unwrap_or_else(|| json!({}));

    let _ = state
        .transition_a2a_task(
            task_id,
            A2ATaskState::Completed,
            Some(completion_output),
            None,
            completion_payload,
        )
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

    #[derive(Clone, Copy)]
    struct UsageEvidence<'a> {
        provider_request_id: Option<&'a str>,
        usage_source: Option<&'a str>,
        transport_mode: Option<&'a str>,
    }

    impl<'a> UsageEvidence<'a> {
        fn provider_real(provider_request_id: &'a str) -> Self {
            Self {
                provider_request_id: Some(provider_request_id),
                usage_source: Some("provider"),
                transport_mode: Some("real"),
            }
        }
    }

    fn test_db_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("agentd-daemon-test-{}.sqlite", Uuid::new_v4()))
    }

    fn create_agent_request(name: &str, model: &str) -> JsonRpcRequest {
        JsonRpcRequest::new(
            json!(1),
            "CreateAgent",
            json!({
                "name": name,
                "model": model,
            }),
        )
    }

    fn get_usage_request(agent_id: &str) -> JsonRpcRequest {
        JsonRpcRequest::new(
            json!(5),
            "GetUsage",
            json!({
                "agent_id": agent_id,
            }),
        )
    }

    fn get_agent_request(agent_id: &str, audit_limit: Option<usize>) -> JsonRpcRequest {
        JsonRpcRequest::new(
            json!(205),
            "GetAgent",
            json!({
                "agent_id": agent_id,
                "audit_limit": audit_limit,
            }),
        )
    }

    fn delete_agent_request(agent_id: &str) -> JsonRpcRequest {
        JsonRpcRequest::new(
            json!(206),
            "DeleteAgent",
            json!({
                "agent_id": agent_id,
            }),
        )
    }

    fn get_usage_window_request(agent_id: &str, window: &str) -> JsonRpcRequest {
        JsonRpcRequest::new(
            json!(105),
            "GetUsage",
            json!({
                "agent_id": agent_id,
                "window": window,
            }),
        )
    }

    fn record_usage_request(
        agent_id: &str,
        model_name: &str,
        input_tokens: u64,
        output_tokens: u64,
        cost_usd: f64,
        evidence: UsageEvidence<'_>,
    ) -> JsonRpcRequest {
        JsonRpcRequest::new(
            json!(6),
            "RecordUsage",
            json!({
                "agent_id": agent_id,
                "model_name": model_name,
                "input_tokens": input_tokens,
                "output_tokens": output_tokens,
                "cost_usd": cost_usd,
                "provider_request_id": evidence.provider_request_id,
                "usage_source": evidence.usage_source,
                "transport_mode": evidence.transport_mode,
            }),
        )
    }

    fn authorize_tool_request(
        tool: &str,
        global_rules: Value,
        profile_rules: Value,
        session_overrides: Value,
    ) -> JsonRpcRequest {
        JsonRpcRequest::new(
            json!(8),
            "AuthorizeTool",
            json!({
                "tool": tool,
                "global_rules": global_rules,
                "profile_rules": profile_rules,
                "session_overrides": session_overrides,
            }),
        )
    }

    fn start_managed_agent_request(
        agent_id: &str,
        command: &str,
        args: Vec<&str>,
        restart_max_attempts: u32,
    ) -> JsonRpcRequest {
        JsonRpcRequest::new(
            json!(9),
            "StartManagedAgent",
            json!({
                "agent_id": agent_id,
                "command": command,
                "args": args,
                "restart_max_attempts": restart_max_attempts,
                "restart_backoff_secs": 0,
                "cpu_weight": 100,
                "memory_high": "64M",
                "memory_max": "128M",
            }),
        )
    }

    fn stop_managed_agent_request(agent_id: &str) -> JsonRpcRequest {
        JsonRpcRequest::new(
            json!(10),
            "StopManagedAgent",
            json!({
                "agent_id": agent_id,
            }),
        )
    }

    fn list_audit_events_request(agent_id: &str, limit: Option<usize>) -> JsonRpcRequest {
        JsonRpcRequest::new(
            json!(15),
            "ListAuditEvents",
            json!({
                "agent_id": agent_id,
                "limit": limit,
            }),
        )
    }

    fn managed_test_state(root: &std::path::Path) -> RuntimeState {
        RuntimeState::with_lifecycle_and_agent_card_root(
            "disabled",
            LifecycleManager::new(CgroupManager::new(root, "agentd")),
            root.join("agent-cards"),
        )
    }

    async fn create_ready_agent_id(
        store: Arc<SqliteStore>,
        state: RuntimeState,
        name: &str,
    ) -> String {
        let response = handle_rpc_request(
            create_agent_request(name, "claude-4-sonnet"),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;
        assert!(
            response.error.is_none(),
            "create should succeed: {response:?}"
        );
        response
            .result
            .expect("create result should exist")
            .get("agent")
            .expect("agent field should exist")
            .get("id")
            .expect("id field should exist")
            .as_str()
            .expect("agent id should be string")
            .to_string()
    }

    #[tokio::test]
    async fn create_agent_returns_a2a_card_path_and_persists_card_file() {
        let db_path = test_db_path();
        let card_root = std::env::temp_dir().join(format!("agentd-card-test-{}", Uuid::new_v4()));
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::with_lifecycle_and_agent_card_root(
            "disabled",
            LifecycleManager::new(CgroupManager::new("/tmp/agentd-cgroup", "agentd")),
            card_root.clone(),
        );

        let response = handle_rpc_request(
            create_agent_request("card-agent", "claude-4-sonnet"),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;
        assert!(
            response.error.is_none(),
            "create should succeed: {response:?}"
        );

        let result = response.result.expect("create result should exist");
        let card_path = result["agent_card_path"]
            .as_str()
            .expect("agent_card_path should be string")
            .to_string();
        let card_content =
            std::fs::read_to_string(&card_path).expect("agent card should be written");
        let card_json: Value =
            serde_json::from_str(&card_content).expect("agent card should be valid json");

        assert_eq!(card_json["name"], json!("card-agent"));
        assert_eq!(card_json["model"], json!("claude-4-sonnet"));
        assert!(
            card_json.get("agent_id").and_then(Value::as_str).is_some(),
            "agent card should contain agent_id"
        );

        let _ = std::fs::remove_dir_all(card_root);
        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn get_agent_returns_profile_and_recent_audit_events() {
        let db_path = test_db_path();
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::new("disabled");

        let created = handle_rpc_request(
            create_agent_request("inspect-agent", "claude-4-sonnet"),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            created.error.is_none(),
            "create should succeed: {created:?}"
        );
        let agent_id = created
            .result
            .expect("create result should exist")
            .get("agent")
            .expect("agent field should exist")
            .get("id")
            .expect("id field should exist")
            .as_str()
            .expect("agent id should be string")
            .to_string();

        let usage_record = handle_rpc_request(
            record_usage_request(
                &agent_id,
                "claude-4-sonnet",
                10,
                5,
                0.02,
                UsageEvidence::provider_real("req-inspect-1"),
            ),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            usage_record.error.is_none(),
            "usage record should succeed: {usage_record:?}"
        );

        let inspected = handle_rpc_request(
            get_agent_request(&agent_id, Some(3)),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;
        assert!(
            inspected.error.is_none(),
            "inspect should succeed: {inspected:?}"
        );

        let result = inspected.result.expect("inspect result should exist");
        assert_eq!(result["profile"]["id"], json!(agent_id));
        assert!(result["profile"]["model"].is_object());
        assert!(result["profile"]["permissions"].is_object());
        assert!(result["profile"]["budget"].is_object());
        assert!(result["audit_events"].is_array());

        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn get_agent_returns_not_found_for_unknown_id() {
        let db_path = test_db_path();
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::new("disabled");

        let response = handle_rpc_request(
            get_agent_request(&Uuid::new_v4().to_string(), None),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;

        let err = response.error.expect("unknown agent inspect should fail");
        assert_eq!(err.code, -32010);
        assert!(err.message.contains("agent not found"));

        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn delete_agent_removes_agent_from_list() {
        let db_path = test_db_path();
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::new("disabled");

        let created = handle_rpc_request(
            create_agent_request("delete-agent", "claude-4-sonnet"),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            created.error.is_none(),
            "create should succeed: {created:?}"
        );
        let agent_id = created
            .result
            .expect("create result should exist")
            .get("agent")
            .expect("agent field should exist")
            .get("id")
            .expect("id field should exist")
            .as_str()
            .expect("agent id should be string")
            .to_string();

        let delete_response = handle_rpc_request(
            delete_agent_request(&agent_id),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            delete_response.error.is_none(),
            "delete should succeed: {delete_response:?}"
        );
        let delete_result = delete_response.result.expect("delete result should exist");
        assert_eq!(delete_result["success"], json!(true));

        let list_response = handle_rpc_request(
            JsonRpcRequest::new(json!(207), "ListAgents", json!({})),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;
        assert!(
            list_response.error.is_none(),
            "list should succeed: {list_response:?}"
        );
        let listed = list_response
            .result
            .expect("list result should exist")
            .get("agents")
            .and_then(Value::as_array)
            .expect("agents should be array")
            .clone();
        assert!(!listed.iter().any(|agent| agent["id"] == json!(agent_id)));

        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn delete_agent_returns_not_found_for_unknown_id() {
        let db_path = test_db_path();
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::new("disabled");

        let response = handle_rpc_request(
            delete_agent_request(&Uuid::new_v4().to_string()),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;

        let err = response.error.expect("unknown agent delete should fail");
        assert_eq!(err.code, -32010);
        assert!(err.message.contains("agent not found"));

        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn create_agent_is_idempotent_and_list_returns_ready_state() {
        let db_path = test_db_path();
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::new("disabled");
        let one_api_config = OneApiConfig::default();

        let first = handle_rpc_request(
            create_agent_request("e2e-agent", "claude-4-sonnet"),
            store.clone(),
            state.clone(),
            one_api_config.clone(),
        )
        .await;
        assert!(
            first.error.is_none(),
            "first create should succeed: {first:?}"
        );
        let first_result = first.result.expect("first create result should exist");
        assert_eq!(first_result["idempotent"], json!(false));
        assert_eq!(first_result["agent"]["status"], json!("ready"));
        let first_agent_id = first_result["agent"]["id"]
            .as_str()
            .expect("first agent id should be string")
            .to_string();

        let second = handle_rpc_request(
            create_agent_request("e2e-agent", "claude-4-sonnet"),
            store.clone(),
            state.clone(),
            one_api_config,
        )
        .await;
        assert!(
            second.error.is_none(),
            "idempotent create should succeed: {second:?}"
        );
        let second_result = second.result.expect("second create result should exist");
        assert_eq!(second_result["idempotent"], json!(true));
        assert_eq!(second_result["agent"]["id"], json!(first_agent_id));
        assert_eq!(second_result["agent"]["status"], json!("ready"));

        let list = handle_rpc_request(
            JsonRpcRequest::new(json!(2), "ListAgents", json!({})),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;
        assert!(list.error.is_none(), "list should succeed: {list:?}");
        let listed_agents = list
            .result
            .expect("list result should exist")
            .get("agents")
            .expect("agents field should exist")
            .as_array()
            .expect("agents should be array")
            .clone();
        assert_eq!(listed_agents.len(), 1);
        assert_eq!(listed_agents[0]["status"], json!("ready"));

        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn create_agent_records_failed_state_when_one_api_provisioning_fails() {
        let db_path = test_db_path();
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::new("degraded");
        let one_api_config = OneApiConfig {
            enabled: true,
            management_enabled: true,
            management_base_url: "http://127.0.0.1:9".to_string(),
            management_timeout_secs: 1,
            management_retries: 1,
            management_retry_backoff_secs: 0,
            ..OneApiConfig::default()
        };

        let first = handle_rpc_request(
            create_agent_request("failing-agent", "claude-4-sonnet"),
            store.clone(),
            state.clone(),
            one_api_config.clone(),
        )
        .await;
        let first_error = first.error.expect("first create should fail");
        assert_eq!(first_error.code, -32014);
        assert!(
            first_error.message.contains("one-api provisioning failed"),
            "unexpected error: {}",
            first_error.message
        );

        let list_after_failure = handle_rpc_request(
            JsonRpcRequest::new(json!(3), "ListAgents", json!({})),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            list_after_failure.error.is_none(),
            "list after failure should succeed: {list_after_failure:?}"
        );
        let failed_agents = list_after_failure
            .result
            .expect("list result should exist")
            .get("agents")
            .expect("agents field should exist")
            .as_array()
            .expect("agents should be array")
            .clone();
        assert_eq!(failed_agents.len(), 1);
        assert_eq!(failed_agents[0]["status"], json!("failed"));
        assert!(failed_agents[0]["failure_reason"]
            .as_str()
            .expect("failure_reason should be present")
            .contains("one-api provisioning failed"));

        let second = handle_rpc_request(
            create_agent_request("failing-agent", "claude-4-sonnet"),
            store.clone(),
            state,
            one_api_config,
        )
        .await;
        let second_error = second
            .error
            .expect("repeated create should fail idempotently");
        assert_eq!(second_error.code, -32014);

        let final_list = handle_rpc_request(
            JsonRpcRequest::new(json!(4), "ListAgents", json!({})),
            store,
            RuntimeState::new("disabled"),
            OneApiConfig::default(),
        )
        .await;
        let final_agents = final_list
            .result
            .expect("final list result should exist")
            .get("agents")
            .expect("agents field should exist")
            .as_array()
            .expect("agents should be array")
            .clone();
        assert_eq!(final_agents.len(), 1);

        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn usage_query_and_quota_enforcement_work() {
        let db_path = test_db_path();
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::new("disabled");

        let created = handle_rpc_request(
            JsonRpcRequest::new(
                json!(7),
                "CreateAgent",
                json!({
                    "name": "quota-agent",
                    "model": "claude-4-sonnet",
                    "token_budget": 100,
                }),
            ),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            created.error.is_none(),
            "create should succeed: {created:?}"
        );
        let created_agent_id = created
            .result
            .expect("create result should exist")
            .get("agent")
            .expect("agent should exist")
            .get("id")
            .expect("agent id should exist")
            .as_str()
            .expect("agent id should be string")
            .to_string();

        let initial_usage = handle_rpc_request(
            get_usage_request(&created_agent_id),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            initial_usage.error.is_none(),
            "initial usage query should succeed: {initial_usage:?}"
        );
        let initial = initial_usage.result.expect("initial usage result");
        assert_eq!(initial["total_tokens"], json!(0));

        let record_ok = handle_rpc_request(
            record_usage_request(
                &created_agent_id,
                "claude-4-sonnet",
                60,
                30,
                0.15,
                UsageEvidence::provider_real("req-usage-1"),
            ),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            record_ok.error.is_none(),
            "usage record under budget should succeed: {record_ok:?}"
        );

        let audit_response = handle_rpc_request(
            list_audit_events_request(&created_agent_id, Some(20)),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            audit_response.error.is_none(),
            "audit query should succeed: {audit_response:?}"
        );
        let audit_events = audit_response
            .result
            .expect("audit result should exist")
            .get("events")
            .and_then(Value::as_array)
            .expect("events should be array")
            .clone();
        let usage_audit = audit_events
            .iter()
            .find(|event| {
                event["event_type"] == json!("ToolInvoked")
                    && event["payload"]["message"] == json!("usage recorded")
            })
            .expect("usage audit event should exist");
        assert_eq!(
            usage_audit["payload"]["metadata"]["provider_request_id"],
            json!("req-usage-1")
        );
        assert_eq!(
            usage_audit["payload"]["metadata"]["usage_source"],
            json!("provider")
        );
        assert_eq!(
            usage_audit["payload"]["metadata"]["transport_mode"],
            json!("real")
        );

        let over_budget = handle_rpc_request(
            record_usage_request(
                &created_agent_id,
                "claude-4-sonnet",
                20,
                5,
                0.05,
                UsageEvidence::provider_real("req-usage-2"),
            ),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        let over_budget_error = over_budget
            .error
            .expect("usage over budget should return error");
        assert_eq!(over_budget_error.code, -32015);
        assert!(
            over_budget_error.message.contains("llm.quota_exceeded"),
            "unexpected over budget message: {}",
            over_budget_error.message
        );

        let final_usage = handle_rpc_request(
            get_usage_request(&created_agent_id),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            final_usage.error.is_none(),
            "final usage query should succeed: {final_usage:?}"
        );
        let summary = final_usage.result.expect("final usage result");
        assert_eq!(summary["input_tokens"], json!(60));
        assert_eq!(summary["output_tokens"], json!(30));
        assert_eq!(summary["total_tokens"], json!(90));
        assert_eq!(
            summary["model_cost_breakdown"][0]["model_name"],
            json!("claude-4-sonnet")
        );

        for window in ["1h", "24h", "7d"] {
            let window_usage = handle_rpc_request(
                get_usage_window_request(&created_agent_id, window),
                store.clone(),
                state.clone(),
                OneApiConfig::default(),
            )
            .await;
            assert!(
                window_usage.error.is_none(),
                "window usage query should succeed for {window}: {window_usage:?}"
            );
            let window_summary = window_usage.result.expect("window usage summary");
            assert_eq!(window_summary["total_tokens"], json!(90));
            assert!(window_summary.get("total_cost_usd").is_some());
            assert!(window_summary.get("model_cost_breakdown").is_some());
        }

        let invalid_window_usage = handle_rpc_request(
            get_usage_window_request(&created_agent_id, "2d"),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;
        let invalid_window_error = invalid_window_usage
            .error
            .expect("invalid window should fail");
        assert_eq!(invalid_window_error.code, -32602);
        assert!(invalid_window_error
            .message
            .contains("unsupported usage window"));

        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn record_usage_rejects_missing_provider_reconciliation_fields() {
        let db_path = test_db_path();
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::new("disabled");

        let created = handle_rpc_request(
            JsonRpcRequest::new(
                json!(91),
                "CreateAgent",
                json!({
                    "name": "reconcile-agent",
                    "model": "claude-4-sonnet",
                }),
            ),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            created.error.is_none(),
            "create should succeed: {created:?}"
        );
        let agent_id = created
            .result
            .expect("create result should exist")
            .get("agent")
            .expect("agent should exist")
            .get("id")
            .expect("agent id should exist")
            .as_str()
            .expect("agent id should be string")
            .to_string();

        let missing_request_id = handle_rpc_request(
            record_usage_request(
                &agent_id,
                "claude-4-sonnet",
                10,
                5,
                0.03,
                UsageEvidence {
                    provider_request_id: None,
                    usage_source: Some("provider"),
                    transport_mode: Some("real"),
                },
            ),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;
        let error = missing_request_id
            .error
            .expect("missing provider request id should fail");
        assert_eq!(error.code, -32602);
        assert!(error.message.contains("MISSING_PROVIDER_REQUEST_ID"));

        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn create_agent_is_idempotent_over_ten_retries() {
        let db_path = test_db_path();
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::new("disabled");

        for _ in 0..10 {
            let response = handle_rpc_request(
                create_agent_request("idempotent-agent", "claude-4-sonnet"),
                store.clone(),
                state.clone(),
                OneApiConfig::default(),
            )
            .await;
            assert!(
                response.error.is_none(),
                "create should succeed: {response:?}"
            );
        }

        let list = handle_rpc_request(
            JsonRpcRequest::new(json!(201), "ListAgents", json!({})),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;
        assert!(list.error.is_none(), "list should succeed: {list:?}");
        let listed_agents = list
            .result
            .expect("list result should exist")
            .get("agents")
            .expect("agents field should exist")
            .as_array()
            .expect("agents should be array")
            .clone();
        let matched = listed_agents
            .iter()
            .filter(|agent| {
                agent["name"] == json!("idempotent-agent")
                    && agent["model"]["model_name"] == json!("claude-4-sonnet")
            })
            .count();
        assert_eq!(
            matched, 1,
            "idempotent retries should not duplicate entities"
        );

        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn usage_recording_succeeds_for_hundred_collection_cycles() {
        let db_path = test_db_path();
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::new("disabled");

        let created = handle_rpc_request(
            JsonRpcRequest::new(
                json!(202),
                "CreateAgent",
                json!({
                    "name": "collector-agent",
                    "model": "claude-4-sonnet",
                    "token_budget": 1000000,
                }),
            ),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            created.error.is_none(),
            "create should succeed: {created:?}"
        );
        let agent_id = created
            .result
            .expect("create result should exist")
            .get("agent")
            .expect("agent should exist")
            .get("id")
            .expect("agent id should exist")
            .as_str()
            .expect("agent id should be string")
            .to_string();

        for cycle in 0..100 {
            let record = handle_rpc_request(
                record_usage_request(
                    &agent_id,
                    "claude-4-sonnet",
                    5,
                    3,
                    0.01,
                    UsageEvidence::provider_real("req-collector"),
                ),
                store.clone(),
                state.clone(),
                OneApiConfig::default(),
            )
            .await;
            assert!(
                record.error.is_none(),
                "usage collection cycle {cycle} should succeed: {record:?}"
            );
        }

        let usage = handle_rpc_request(
            get_usage_request(&agent_id),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;
        assert!(usage.error.is_none(), "usage should succeed: {usage:?}");
        let summary = usage.result.expect("usage result should exist");
        assert_eq!(summary["total_tokens"], json!(800));
        assert_eq!(summary["input_tokens"], json!(500));
        assert_eq!(summary["output_tokens"], json!(300));

        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn authorize_tool_returns_stable_policy_deny_error_code() {
        let db_path = test_db_path();
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::new("disabled");

        let response = handle_rpc_request(
            authorize_tool_request(
                "read:secrets.env",
                json!([
                    {"pattern": "read:*", "decision": "allow"}
                ]),
                json!([
                    {"pattern": "read:*.env", "decision": "deny"}
                ]),
                json!({
                    "ask_tools": [],
                    "allow_tools": [],
                    "deny_tools": []
                }),
            ),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;

        let err = response.error.expect("authorize should deny");
        assert_eq!(err.code, -32016);
        assert!(err.message.contains("policy.deny"));
        assert!(err.message.contains("matched_rule=read:*.env"));
        assert!(err.message.contains("source_layer=agent_profile"));

        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn authorize_tool_returns_explanation_for_non_deny_decision() {
        let db_path = test_db_path();
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::new("disabled");

        let response = handle_rpc_request(
            authorize_tool_request(
                "bash:rm",
                json!([
                    {"pattern": "bash:*", "decision": "allow"}
                ]),
                json!([
                    {"pattern": "bash:rm", "decision": "ask"}
                ]),
                json!({
                    "ask_tools": ["bash:rm"],
                    "allow_tools": [],
                    "deny_tools": []
                }),
            ),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;

        assert!(response.error.is_none(), "authorize should not deny");
        let result = response.result.expect("result should exist");
        assert_eq!(result["decision"], json!("ask"));
        assert_eq!(result["matched_rule"], json!("bash:rm"));
        assert_eq!(result["source_layer"], json!("session_override"));

        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn create_agent_with_denied_tools_enforces_policy_for_lite_runtime_tool() {
        let db_path = test_db_path();
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::new("disabled");

        let create_response = handle_rpc_request(
            JsonRpcRequest::new(
                json!(981),
                "CreateAgent",
                json!({
                    "name": "lite-deny-agent",
                    "model": "claude-4-sonnet",
                    "permission_policy": "ask",
                    "denied_tools": ["builtin.lite.echo"],
                }),
            ),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            create_response.error.is_none(),
            "create should succeed: {create_response:?}"
        );
        let agent_id = create_response
            .result
            .expect("create result should exist")
            .get("agent")
            .expect("agent field should exist")
            .get("id")
            .expect("id field should exist")
            .as_str()
            .expect("agent id should be string")
            .to_string();

        let denied = handle_rpc_request(
            JsonRpcRequest::new(
                json!(982),
                "AuthorizeTool",
                json!({
                    "agent_id": agent_id,
                    "tool": "builtin.lite.echo",
                    "global_rules": [],
                    "profile_rules": [],
                    "session_overrides": {
                        "allow_tools": [],
                        "ask_tools": [],
                        "deny_tools": [],
                    }
                }),
            ),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;

        let denied_error = denied.error.expect("authorize should deny configured tool");
        assert_eq!(denied_error.code, -32016);
        assert!(denied_error.message.contains("policy.deny"));
        assert!(denied_error
            .message
            .contains("matched_rule=builtin.lite.echo"));

        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn authorize_mcp_tool_allow_forwards() {
        let db_path = test_db_path();
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::new("disabled");

        let create = handle_rpc_request(
            JsonRpcRequest::new(
                json!(983),
                "CreateAgent",
                json!({
                    "name": "mcp-allow-gateway",
                    "model": "claude-4-sonnet",
                    "permission_policy": "allow",
                }),
            ),
            store.clone(),
            state,
            OneApiConfig::default(),
        )
        .await;
        assert!(create.error.is_none(), "create should succeed: {create:?}");
        let agent_id = Uuid::parse_str(
            create
                .result
                .expect("create result should exist")
                .get("agent")
                .expect("agent should exist")
                .get("id")
                .expect("id should exist")
                .as_str()
                .expect("id should be string"),
        )
        .expect("agent id should be valid uuid");

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_forward = call_count.clone();
        let audit_context = build_audit_context(&json!(983));
        let outcome = authorize_mcp_tool_before_forward(
            &store,
            &audit_context,
            agent_id,
            "mcp.fs.read_file",
            json!({"path": "README.md"}),
            move |payload| {
                call_count_forward.fetch_add(1, Ordering::SeqCst);
                json!({"forwarded": true, "payload": payload})
            },
        )
        .await
        .expect("allow should succeed");

        assert!(outcome.forwarded);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
        assert_eq!(outcome.decision.decision, PolicyDecision::Allow);
        assert!(outcome.decision.reason.contains("policy.allow"));

        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn authorize_mcp_tool_deny_blocks_forward() {
        let db_path = test_db_path();
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::new("disabled");

        let create = handle_rpc_request(
            JsonRpcRequest::new(
                json!(984),
                "CreateAgent",
                json!({
                    "name": "mcp-deny-gateway",
                    "model": "claude-4-sonnet",
                    "permission_policy": "ask",
                    "denied_tools": ["mcp.fs.read_file"],
                }),
            ),
            store.clone(),
            state,
            OneApiConfig::default(),
        )
        .await;
        assert!(create.error.is_none(), "create should succeed: {create:?}");
        let agent_id = Uuid::parse_str(
            create
                .result
                .expect("create result should exist")
                .get("agent")
                .expect("agent should exist")
                .get("id")
                .expect("id should exist")
                .as_str()
                .expect("id should be string"),
        )
        .expect("agent id should be valid uuid");

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_forward = call_count.clone();
        let audit_context = build_audit_context(&json!(984));
        let outcome = authorize_mcp_tool_before_forward(
            &store,
            &audit_context,
            agent_id,
            "mcp.fs.read_file",
            json!({"path": ".env"}),
            move |_| {
                call_count_forward.fetch_add(1, Ordering::SeqCst);
                json!({"forwarded": true})
            },
        )
        .await
        .expect("deny should still return decision payload");

        assert!(!outcome.forwarded);
        assert_eq!(outcome.downstream, None);
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
        assert_eq!(outcome.decision.decision, PolicyDecision::Deny);
        assert!(outcome.decision.reason.contains("policy.deny"));

        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn authorize_mcp_tool_writes_audit_event() {
        let db_path = test_db_path();
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::new("disabled");

        let create = handle_rpc_request(
            JsonRpcRequest::new(
                json!(985),
                "CreateAgent",
                json!({
                    "name": "mcp-audit-gateway",
                    "model": "claude-4-sonnet",
                    "permission_policy": "ask",
                    "denied_tools": ["mcp.fs.read_file"],
                }),
            ),
            store.clone(),
            state,
            OneApiConfig::default(),
        )
        .await;
        assert!(create.error.is_none(), "create should succeed: {create:?}");
        let agent_id = Uuid::parse_str(
            create
                .result
                .expect("create result should exist")
                .get("agent")
                .expect("agent should exist")
                .get("id")
                .expect("id should exist")
                .as_str()
                .expect("id should be string"),
        )
        .expect("agent id should be valid uuid");

        let audit_context = build_audit_context(&json!(985));
        let outcome = authorize_mcp_tool_before_forward(
            &store,
            &audit_context,
            agent_id,
            "mcp.fs.read_file",
            json!({"path": ".env", "encoding": "utf-8"}),
            |_| json!({"forwarded": true}),
        )
        .await
        .expect("deny should produce auditable decision");

        let events = store
            .get_audit_events(agent_id)
            .await
            .expect("audit query should succeed");
        let deny_event = events
            .iter()
            .find(|event| event.event_type == EventType::ToolDenied)
            .expect("tool denied event should exist");

        assert_eq!(deny_event.payload.message.as_deref(), Some("policy.deny"));
        assert_eq!(
            deny_event.payload.tool_name.as_deref(),
            Some("mcp.fs.read_file")
        );
        assert_eq!(
            deny_event.payload.metadata["trace_id"],
            json!(outcome.decision.trace_id)
        );
        assert!(deny_event.payload.metadata["reason"]
            .as_str()
            .expect("reason should exist")
            .contains("policy.deny"));
        assert_eq!(
            deny_event.payload.metadata["replay"]["tool"],
            json!("mcp.fs.read_file")
        );
        assert_eq!(
            deny_event.payload.metadata["replay"]["input"]["path"],
            json!(".env")
        );

        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn approval_queue_resolve_roundtrip() {
        let db_path = test_db_path();
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::new("disabled");

        let create = handle_rpc_request(
            JsonRpcRequest::new(
                json!(9901),
                "CreateAgent",
                json!({
                    "name": "approval-queue-agent",
                    "model": "claude-4-sonnet",
                    "permission_policy": "ask",
                }),
            ),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(create.error.is_none(), "create should succeed: {create:?}");
        let agent_id = create
            .result
            .expect("create result should exist")
            .get("agent")
            .expect("agent field should exist")
            .get("id")
            .expect("id field should exist")
            .as_str()
            .expect("agent id should be string")
            .to_string();

        let ask = handle_rpc_request(
            JsonRpcRequest::new(
                json!(9902),
                "AuthorizeMcpTool",
                json!({
                    "agent_id": agent_id,
                    "tool": "mcp.fs.read_file",
                    "payload": {"path": "README.md"},
                }),
            ),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            ask.error.is_none(),
            "authorize mcp tool should return pending outcome: {ask:?}"
        );
        assert_eq!(
            ask.result.as_ref().expect("authorize result should exist")["decision"]["decision"],
            json!(PolicyDecision::Ask)
        );

        let list = handle_rpc_request(
            JsonRpcRequest::new(
                json!(9903),
                "ListApprovalQueue",
                json!({"agent_id": agent_id}),
            ),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            list.error.is_none(),
            "list approval queue should succeed: {list:?}"
        );
        let approvals = list.result.expect("list result should exist")["approvals"]
            .as_array()
            .expect("approvals should be array")
            .clone();
        assert_eq!(approvals.len(), 1);
        let approval_id = approvals[0]["id"]
            .as_str()
            .expect("approval id should be string")
            .to_string();

        let resolve = handle_rpc_request(
            JsonRpcRequest::new(
                json!(9904),
                "ResolveApproval",
                json!({
                    "agent_id": agent_id,
                    "approval_id": approval_id,
                    "decision": "deny",
                }),
            ),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            resolve.error.is_none(),
            "resolve approval should succeed: {resolve:?}"
        );
        assert_eq!(
            resolve.result.expect("resolve result should exist")["decision"],
            json!("deny")
        );

        let list_after = handle_rpc_request(
            JsonRpcRequest::new(
                json!(9905),
                "ListApprovalQueue",
                json!({"agent_id": agent_id}),
            ),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;
        assert!(
            list_after.error.is_none(),
            "list approval queue after resolve should succeed: {list_after:?}"
        );
        assert!(
            list_after.result.expect("list after result should exist")["approvals"]
                .as_array()
                .expect("approvals should be array")
                .is_empty(),
            "approval queue should be empty after resolve"
        );

        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn managed_agent_lifecycle_rpc_start_list_stop() {
        let db_path = test_db_path();
        let cgroup_root =
            std::env::temp_dir().join(format!("agentd-managed-test-{}", Uuid::new_v4()));
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = managed_test_state(&cgroup_root);

        let agent_id = Uuid::parse_str(
            &create_ready_agent_id(store.clone(), state.clone(), "managed-rpc-start-stop").await,
        )
        .expect("created agent id should be valid uuid");
        let start_response = handle_rpc_request(
            start_managed_agent_request(&agent_id.to_string(), "/bin/sh", vec!["-c", "sleep 1"], 0),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            start_response.error.is_none(),
            "start managed agent should succeed: {start_response:?}"
        );

        tokio::time::sleep(Duration::from_millis(100)).await;
        let list_response = handle_rpc_request(
            JsonRpcRequest::new(json!(11), "ListManagedAgents", json!({})),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            list_response.error.is_none(),
            "list managed agents should succeed: {list_response:?}"
        );
        let listed = list_response
            .result
            .expect("list response result should exist")
            .get("agents")
            .expect("agents field should exist")
            .as_array()
            .expect("agents should be array")
            .clone();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["agent_id"], json!(agent_id.to_string()));

        let stop_response = handle_rpc_request(
            stop_managed_agent_request(&agent_id.to_string()),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;
        assert!(
            stop_response.error.is_none(),
            "stop managed agent should succeed: {stop_response:?}"
        );
        assert_eq!(
            stop_response
                .result
                .expect("stop result should exist")
                .get("state")
                .expect("state field should exist"),
            "stopped"
        );

        let _ = std::fs::remove_dir_all(cgroup_root);
        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn managed_agent_lifecycle_emits_restart_and_oom_events() {
        let db_path = test_db_path();
        let cgroup_root =
            std::env::temp_dir().join(format!("agentd-managed-oom-{}", Uuid::new_v4()));
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = managed_test_state(&cgroup_root);

        let agent_id = Uuid::parse_str(
            &create_ready_agent_id(store.clone(), state.clone(), "managed-rpc-restart-oom").await,
        )
        .expect("created agent id should be valid uuid");
        let start_response = handle_rpc_request(
            start_managed_agent_request(
                &agent_id.to_string(),
                "/bin/sh",
                vec!["-c", "sleep 0.2; exit 1"],
                1,
            ),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            start_response.error.is_none(),
            "start response should succeed"
        );

        tokio::time::sleep(Duration::from_millis(50)).await;
        let memory_events_path = cgroup_root
            .join("agentd")
            .join(agent_id.to_string())
            .join("memory.events");
        std::fs::write(memory_events_path, "oom 2\noom_kill 1\n").expect("simulate oom events");

        tokio::time::sleep(Duration::from_millis(700)).await;
        let events_response = handle_rpc_request(
            JsonRpcRequest::new(
                json!(12),
                "ListLifecycleEvents",
                json!({
                    "limit": 20,
                }),
            ),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            events_response.error.is_none(),
            "events query should succeed"
        );
        let events = events_response
            .result
            .expect("events result should exist")
            .get("events")
            .expect("events field should exist")
            .as_array()
            .expect("events should be array")
            .clone();
        assert!(events
            .iter()
            .any(|event| event["event_type"] == json!("cgroup.oom")));
        assert!(events
            .iter()
            .any(|event| event["event_type"] == json!("agent.restarting")));

        let _ = handle_rpc_request(
            stop_managed_agent_request(&agent_id.to_string()),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;

        let _ = std::fs::remove_dir_all(cgroup_root);
        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn subscribe_events_returns_next_cursor_after_lifecycle_event() {
        let db_path = test_db_path();
        let cgroup_root =
            std::env::temp_dir().join(format!("agentd-managed-subscribe-{}", Uuid::new_v4()));
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = managed_test_state(&cgroup_root);

        let agent_id = Uuid::parse_str(
            &create_ready_agent_id(store.clone(), state.clone(), "managed-rpc-subscribe").await,
        )
        .expect("created agent id should be valid uuid");
        let start_response = handle_rpc_request(
            start_managed_agent_request(
                &agent_id.to_string(),
                "/bin/sh",
                vec!["-c", "sleep 0.2"],
                0,
            ),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            start_response.error.is_none(),
            "start response should succeed: {start_response:?}"
        );

        let subscribe_response = handle_rpc_request(
            JsonRpcRequest::new(
                json!(21),
                "SubscribeEvents",
                json!({
                    "limit": 10,
                    "wait_timeout_secs": 2,
                }),
            ),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;

        assert!(
            subscribe_response.error.is_none(),
            "subscribe should succeed: {subscribe_response:?}"
        );

        let result = subscribe_response
            .result
            .expect("subscribe result should exist");
        let events = result
            .get("events")
            .expect("events should exist")
            .as_array()
            .expect("events should be array")
            .clone();
        assert!(
            !events.is_empty(),
            "subscribe should return at least one event"
        );
        assert!(
            result
                .get("next_cursor")
                .and_then(|cursor| cursor.as_str())
                .is_some(),
            "next_cursor should be returned"
        );

        let _ = handle_rpc_request(
            stop_managed_agent_request(&agent_id.to_string()),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;

        let _ = std::fs::remove_dir_all(cgroup_root);
        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn managed_agent_start_validates_agent_id_and_command() {
        let db_path = test_db_path();
        let cgroup_root =
            std::env::temp_dir().join(format!("agentd-managed-validate-{}", Uuid::new_v4()));
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = managed_test_state(&cgroup_root);

        let invalid_id_response = handle_rpc_request(
            JsonRpcRequest::new(
                json!(13),
                "StartManagedAgent",
                json!({
                    "agent_id": "not-a-uuid",
                    "command": "/bin/sh",
                    "args": ["-c", "sleep 1"],
                }),
            ),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            invalid_id_response.result.is_none(),
            "invalid agent id should fail: {invalid_id_response:?}"
        );
        let invalid_id_error = invalid_id_response
            .error
            .expect("invalid agent id should return error");
        assert_eq!(invalid_id_error.code, -32602);

        let unknown_agent_response = handle_rpc_request(
            JsonRpcRequest::new(
                json!(131),
                "StartManagedAgent",
                json!({
                    "agent_id": Uuid::new_v4().to_string(),
                    "command": "/bin/sh",
                    "args": ["-c", "sleep 1"],
                }),
            ),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        let unknown_agent_error = unknown_agent_response
            .error
            .expect("unknown agent should return error");
        assert_eq!(unknown_agent_error.code, -32010);
        assert!(unknown_agent_error
            .message
            .contains("query agent for managed lifecycle failed"));

        let existing_agent_id = create_ready_agent_id(
            store.clone(),
            state.clone(),
            "managed-rpc-validate-empty-command",
        )
        .await;

        let empty_command_response = handle_rpc_request(
            JsonRpcRequest::new(
                json!(14),
                "StartManagedAgent",
                json!({
                    "agent_id": existing_agent_id,
                    "command": "   ",
                    "args": ["-c", "sleep 1"],
                }),
            ),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;
        assert!(
            empty_command_response.result.is_none(),
            "empty command should fail: {empty_command_response:?}"
        );
        let empty_command_error = empty_command_response
            .error
            .expect("empty command should return error");
        assert_eq!(empty_command_error.code, -32017);
        assert!(empty_command_error
            .message
            .contains("managed agent command must be non-empty"));

        let _ = std::fs::remove_dir_all(cgroup_root);
        cleanup_sqlite_files(&db_path);
    }

    #[tokio::test]
    async fn create_agent_persists_audit_event_and_query_returns_it() {
        let db_path = test_db_path();
        let store = Arc::new(SqliteStore::new(&db_path).expect("initialize sqlite store"));
        let state = RuntimeState::new("disabled");

        let create = handle_rpc_request(
            create_agent_request("audit-create-agent", "claude-4-sonnet"),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(create.error.is_none(), "create should succeed: {create:?}");
        let result = create.result.expect("create result should exist");
        let agent_id = result["agent"]["id"]
            .as_str()
            .expect("agent id should be string")
            .to_string();

        let events_response = handle_rpc_request(
            list_audit_events_request(&agent_id, Some(20)),
            store,
            state,
            OneApiConfig::default(),
        )
        .await;
        assert!(
            events_response.error.is_none(),
            "list audit events should succeed: {events_response:?}"
        );
        let events = events_response
            .result
            .expect("events result should exist")
            .get("events")
            .expect("events field should exist")
            .as_array()
            .expect("events should be array")
            .clone();
        assert!(!events.is_empty(), "audit events should not be empty");
        assert!(events
            .iter()
            .any(|event| event["event_type"] == json!("AgentCreated")));
        let created_event = events
            .iter()
            .find(|event| event["event_type"] == json!("AgentCreated"))
            .expect("should contain AgentCreated event");
        assert!(created_event
            .get("event_id")
            .and_then(Value::as_str)
            .is_some());
        assert!(created_event
            .get("trace_id")
            .and_then(Value::as_str)
            .is_some());
        assert!(created_event
            .get("session_id")
            .and_then(Value::as_str)
            .is_some());
        assert_eq!(created_event["severity"], json!("info"));

        cleanup_sqlite_files(&db_path);
    }
}

#[derive(Debug, Deserialize)]
struct CreateAgentParams {
    name: String,
    model: String,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    token_budget: Option<u64>,
    #[serde(default)]
    max_tokens: Option<u32>,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    permission_policy: Option<String>,
    #[serde(default)]
    allowed_tools: Vec<String>,
    #[serde(default)]
    denied_tools: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct GetUsageParams {
    agent_id: String,
    #[serde(default)]
    window: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecordUsageParams {
    agent_id: String,
    model_name: String,
    input_tokens: u64,
    output_tokens: u64,
    #[serde(default)]
    cost_usd: f64,
    #[serde(default)]
    provider_request_id: Option<String>,
    #[serde(default)]
    usage_source: Option<String>,
    #[serde(default)]
    transport_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PolicyRuleInput {
    pattern: String,
    decision: PolicyDecision,
}

#[derive(Debug, Clone, Serialize)]
struct McpGatewayForwardResult {
    decision: PolicyGatewayDecision,
    forwarded: bool,
    downstream: Option<Value>,
    matched_rule: Option<String>,
    source_layer: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthorizeToolParams {
    tool: String,
    #[serde(default)]
    global_rules: Vec<PolicyRuleInput>,
    #[serde(default)]
    profile_rules: Vec<PolicyRuleInput>,
    #[serde(default)]
    session_overrides: Option<SessionPolicyOverrides>,
    #[serde(default)]
    agent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthorizeMcpToolParams {
    agent_id: String,
    tool: String,
    #[serde(default)]
    payload: Value,
}

#[derive(Debug, Deserialize)]
struct ListAvailableToolsParams {
    agent_id: String,
}

#[derive(Debug, Deserialize)]
struct InvokeSkillParams {
    agent_id: String,
    server: String,
    tool: String,
    #[serde(default)]
    args: Value,
}

#[derive(Debug, Deserialize)]
struct OnboardMcpServerParams {
    name: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    transport: Option<String>,
    #[serde(default)]
    trust_level: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MigrateContextRpcParams {
    source_agent_id: String,
    target_base_url: String,
    #[serde(default)]
    target_agent_id: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    key_files: Vec<String>,
    #[serde(default)]
    messages: Vec<Value>,
    #[serde(default)]
    head_id: Option<String>,
    #[serde(default)]
    tool_results_cache: Value,
    #[serde(default)]
    working_directory: BTreeMap<String, String>,
    #[serde(default)]
    include_snapshot: bool,
}

#[derive(Debug, Deserialize)]
struct ListApprovalQueueParams {
    agent_id: String,
}

#[derive(Debug, Deserialize)]
struct ResolveApprovalParams {
    agent_id: String,
    approval_id: String,
    decision: String,
}

#[derive(Debug, Deserialize)]
struct StartManagedAgentParams {
    agent_id: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    restart_max_attempts: Option<u32>,
    #[serde(default)]
    restart_backoff_secs: Option<u64>,
    #[serde(default)]
    cpu_weight: Option<u64>,
    #[serde(default)]
    memory_high: Option<String>,
    #[serde(default)]
    memory_max: Option<String>,
    #[serde(default)]
    trust_level: Option<String>,
    #[serde(default)]
    network_policy: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StopManagedAgentParams {
    agent_id: String,
}

#[derive(Debug, Deserialize)]
struct ListLifecycleEventsParams {
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SubscribeEventsParams {
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    wait_timeout_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ListAuditEventsParams {
    agent_id: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct GetAgentParams {
    agent_id: String,
    #[serde(default)]
    audit_limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct DeleteAgentParams {
    agent_id: String,
}

#[derive(Debug, Deserialize)]
struct AgentProfileFile {
    agent: AgentFileSection,
    llm: LlmFileSection,
    policy: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct AgentFileSection {
    name: String,
}

#[derive(Debug, Deserialize)]
struct LlmFileSection {
    models: Vec<String>,
    #[serde(default)]
    token_budget_daily: Option<u64>,
    #[serde(default)]
    fallback_model: Option<String>,
    #[serde(default)]
    provider: Option<String>,
}

fn convert_rule_inputs(name: &str, inputs: Vec<PolicyRuleInput>) -> PolicyLayer {
    PolicyLayer {
        name: name.to_string(),
        rules: inputs
            .into_iter()
            .map(|rule| PolicyRule {
                pattern: rule.pattern,
                decision: rule.decision,
            })
            .collect(),
    }
}

fn profile_to_policy_layer(profile: &AgentProfile) -> PolicyLayer {
    let mut rules = Vec::with_capacity(
        1 + profile.permissions.allowed_tools.len() + profile.permissions.denied_tools.len(),
    );

    let default_decision = match profile.permissions.policy {
        PermissionPolicy::Allow => PolicyDecision::Allow,
        PermissionPolicy::Ask => PolicyDecision::Ask,
        PermissionPolicy::Deny => PolicyDecision::Deny,
    };
    rules.push(PolicyRule {
        pattern: "*".to_string(),
        decision: default_decision,
    });

    for pattern in &profile.permissions.allowed_tools {
        rules.push(PolicyRule {
            pattern: pattern.clone(),
            decision: PolicyDecision::Allow,
        });
    }
    for pattern in &profile.permissions.denied_tools {
        rules.push(PolicyRule {
            pattern: pattern.clone(),
            decision: PolicyDecision::Deny,
        });
    }

    PolicyLayer {
        name: "agent_profile".to_string(),
        rules,
    }
}

fn evaluate_policy_with_rego_dir(
    policy_dir: &Path,
    global_layer: PolicyLayer,
    profile_layer: PolicyLayer,
    session_layer: PolicyLayer,
    policy_input: &PolicyInputContext,
) -> Result<PolicyEvaluation, AgentError> {
    let engine = RegorusPolicyEngine::from_policy_dir(
        PolicyEngineLayers::new(global_layer, profile_layer, session_layer),
        policy_dir.to_path_buf(),
    )?;
    Ok(engine.evaluate(policy_input))
}

#[cfg(test)]
fn temp_rego_policy_dir() -> PathBuf {
    std::env::temp_dir().join(format!("agentd-rego-policy-test-{}", uuid::Uuid::new_v4()))
}

#[cfg(test)]
fn ask_only_layer(name: &str) -> PolicyLayer {
    PolicyLayer {
        name: name.to_string(),
        rules: vec![PolicyRule {
            pattern: "*".to_string(),
            decision: PolicyDecision::Ask,
        }],
    }
}

#[cfg(test)]
fn test_policy_input(tool: &str) -> PolicyInputContext {
    PolicyInputContext {
        agent: PolicyAgentContext {
            id: Some("agent-rego-test".to_string()),
            trust_level: Some("ask".to_string()),
        },
        tool: PolicyToolContext {
            name: tool.to_string(),
        },
        resource: PolicyResourceContext { uri: None },
        time: PolicyTimeContext {
            timestamp_rfc3339: Some("2026-03-04T17:00:00Z".to_string()),
        },
        request_meta: BTreeMap::new(),
    }
}

#[cfg(test)]
#[test]
fn rego_policy_loaded_and_evaluated() {
    let dir = temp_rego_policy_dir();
    std::fs::create_dir_all(&dir).expect("create rego test dir");
    std::fs::write(
        dir.join("allow.rego"),
        r#"
package agentd.policy
import rego.v1

default allow := false
default deny := false

allow if {
  input.tool.name == "mcp.fs.read_file"
}
"#,
    )
    .expect("write allow policy");

    let evaluation = evaluate_policy_with_rego_dir(
        &dir,
        ask_only_layer("global"),
        ask_only_layer("agent_profile"),
        ask_only_layer("session_override"),
        &test_policy_input("mcp.fs.read_file"),
    )
    .expect("rego policy should evaluate");

    assert_eq!(evaluation.decision, PolicyDecision::Allow);
    assert_eq!(
        evaluation.source_layer.as_deref(),
        Some("rego:data.agentd.policy")
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(test)]
#[test]
fn rego_deny_returns_explanation_path() {
    let dir = temp_rego_policy_dir();
    std::fs::create_dir_all(&dir).expect("create rego test dir");
    std::fs::write(
        dir.join("deny.rego"),
        r#"
package agentd.policy
import rego.v1

default allow := false
default deny := false

deny if {
  startswith(input.tool.name, "mcp.fs.")
}

explain := "data.agentd.policy.deny[mcp.fs]" if {
  deny
}
"#,
    )
    .expect("write deny policy");

    let evaluation = evaluate_policy_with_rego_dir(
        &dir,
        ask_only_layer("global"),
        ask_only_layer("agent_profile"),
        ask_only_layer("session_override"),
        &test_policy_input("mcp.fs.read_file"),
    )
    .expect("rego policy should evaluate");

    assert_eq!(evaluation.decision, PolicyDecision::Deny);
    assert_eq!(
        evaluation.matched_rule.as_deref(),
        Some("data.agentd.policy.deny[mcp.fs]")
    );

    let gateway = evaluation.to_gateway_decision("trace-rego-deny");
    assert!(gateway.reason.contains("data.agentd.policy.deny[mcp.fs]"));

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(test)]
#[test]
fn rego_hot_reload_without_restart() {
    let dir = temp_rego_policy_dir();
    std::fs::create_dir_all(&dir).expect("create rego test dir");
    std::fs::write(
        dir.join("hot-reload.rego"),
        r#"
package agentd.policy
import rego.v1

default allow := false
default deny := false

allow if {
  input.tool.name == "mcp.fs.read_file"
}
"#,
    )
    .expect("write allow policy");

    let engine = RegorusPolicyEngine::from_policy_dir(
        PolicyEngineLayers::new(
            ask_only_layer("global"),
            ask_only_layer("agent_profile"),
            ask_only_layer("session_override"),
        ),
        &dir,
    )
    .expect("create regorus engine");

    let first = engine.evaluate(&test_policy_input("mcp.fs.read_file"));
    assert_eq!(first.decision, PolicyDecision::Allow);

    std::fs::write(
        dir.join("hot-reload.rego"),
        r#"
package agentd.policy
import rego.v1

default allow := false
default deny := false

deny if {
  input.tool.name == "mcp.fs.read_file"
}

explain := "data.agentd.policy.deny[mcp.fs.read_file]" if {
  deny
}
"#,
    )
    .expect("rewrite policy for deny");

    let second = engine.evaluate(&test_policy_input("mcp.fs.read_file"));
    assert_eq!(second.decision, PolicyDecision::Deny);
    assert_eq!(
        second.matched_rule.as_deref(),
        Some("data.agentd.policy.deny[mcp.fs.read_file]")
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(test)]
#[test]
fn deny_explain_contains_rule_path() {
    let dir = temp_rego_policy_dir();
    std::fs::create_dir_all(&dir).expect("create rego test dir");
    std::fs::write(
        dir.join("deny-explain.rego"),
        r#"
package agentd.policy
import rego.v1

default allow := false
default deny := false

deny if {
  startswith(input.tool.name, "mcp.fs.")
}

explain := "data.agentd.policy.deny[mcp.fs]" if {
  deny
}
"#,
    )
    .expect("write deny policy");

    let evaluation = evaluate_policy_with_rego_dir(
        &dir,
        ask_only_layer("global"),
        ask_only_layer("agent_profile"),
        ask_only_layer("session_override"),
        &test_policy_input("mcp.fs.read_file"),
    )
    .expect("rego policy should evaluate");
    let gateway = evaluation.to_gateway_decision("trace-rego-explain");

    assert_eq!(evaluation.decision, PolicyDecision::Deny);
    assert!(gateway
        .reason
        .contains("matched_rule=data.agentd.policy.deny[mcp.fs]"));
    assert!(gateway.reason.contains("input_snapshot="));

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(test)]
#[test]
fn rego_reload_bad_policy_keeps_previous_engine() {
    let dir = temp_rego_policy_dir();
    std::fs::create_dir_all(&dir).expect("create rego test dir");
    std::fs::write(
        dir.join("reload-fallback.rego"),
        r#"
package agentd.policy
import rego.v1

default allow := false
default deny := false

allow if {
  input.tool.name == "mcp.fs.read_file"
}
"#,
    )
    .expect("write allow policy");

    let engine = RegorusPolicyEngine::from_policy_dir(
        PolicyEngineLayers::new(
            ask_only_layer("global"),
            ask_only_layer("agent_profile"),
            ask_only_layer("session_override"),
        ),
        &dir,
    )
    .expect("create regorus engine");

    let before = engine.evaluate(&test_policy_input("mcp.fs.read_file"));
    assert_eq!(before.decision, PolicyDecision::Allow);

    std::fs::write(
        dir.join("reload-fallback.rego"),
        r#"
package agentd.policy
import rego.v1

allow if {
  input.tool.name ==
}
"#,
    )
    .expect("write invalid policy");

    let after = engine.evaluate(&test_policy_input("mcp.fs.read_file"));
    assert_eq!(after.decision, PolicyDecision::Allow);

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(test)]
#[test]
fn rego_invalid_policy_compile_error() {
    let dir = temp_rego_policy_dir();
    std::fs::create_dir_all(&dir).expect("create rego test dir");
    std::fs::write(
        dir.join("broken.rego"),
        r#"
package agentd.policy
import rego.v1

allow if {
  input.tool.name ==
}
"#,
    )
    .expect("write broken policy");

    let err = evaluate_policy_with_rego_dir(
        &dir,
        ask_only_layer("global"),
        ask_only_layer("agent_profile"),
        ask_only_layer("session_override"),
        &test_policy_input("mcp.fs.read_file"),
    )
    .expect_err("invalid rego should fail on compile");

    let err_msg = err.to_string();
    assert!(err_msg.contains("broken.rego"));
    assert!(err_msg.contains("compile rego policy"));

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(test)]
#[test]
fn toml_to_rego_equivalence_suite() {
    let profile_path = PathBuf::from("equivalence.toml");
    let profile_content = r#"
[agent]
name = "equivalence-agent"

[llm]
models = ["gpt-4.1-mini"]

[policy]
"*" = "ask"
"mcp.fs.read:*" = "allow"
"mcp.fs.read:*.env" = "deny"
"mcp.shell.execute" = "deny"
"mcp.search.ripgrep" = "allow"
"mcp.net.fetch" = "allow"
"#;

    let profile = parse_agent_profile_file(&profile_path, profile_content)
        .expect("parse profile should work");
    let profile_layer = profile_to_policy_layer(&profile);

    let global_layer = PolicyLayer {
        name: "global".to_string(),
        rules: vec![
            PolicyRule {
                pattern: "*".to_string(),
                decision: PolicyDecision::Ask,
            },
            PolicyRule {
                pattern: "mcp.search.*".to_string(),
                decision: PolicyDecision::Deny,
            },
        ],
    };
    let session_layer = SessionPolicyOverrides {
        allow_tools: vec!["mcp.search.ripgrep".to_string()],
        ask_tools: vec!["mcp.fs.read:*.txt".to_string()],
        deny_tools: vec!["mcp.net.fetch".to_string()],
    }
    .into_layer();

    let dir = temp_rego_policy_dir();
    std::fs::create_dir_all(&dir).expect("create rego test dir");

    let tools = [
        "mcp.fs.read:notes.txt",
        "mcp.fs.read:secret.env",
        "mcp.shell.execute",
        "mcp.search.ripgrep",
        "mcp.net.fetch",
        "mcp.git.status",
    ];
    let mut mismatches = Vec::new();

    for tool in tools {
        let expected =
            PolicyLayer::evaluate_tool(&global_layer, &profile_layer, &session_layer, tool);
        let actual = evaluate_policy_with_rego_dir(
            &dir,
            global_layer.clone(),
            profile_layer.clone(),
            session_layer.clone(),
            &test_policy_input(tool),
        )
        .expect("rego evaluation should succeed");

        if actual.decision != expected.decision
            || actual.matched_rule != expected.matched_rule
            || actual.source_layer != expected.source_layer
        {
            mismatches.push(json!({
                "tool": tool,
                "expected": {
                    "decision": expected.decision,
                    "matched_rule": expected.matched_rule,
                    "source_layer": expected.source_layer,
                },
                "actual": {
                    "decision": actual.decision,
                    "matched_rule": actual.matched_rule,
                    "source_layer": actual.source_layer,
                }
            }));
        }
    }

    let total = tools.len();
    let matched = total.saturating_sub(mismatches.len());
    let report = json!({
        "suite": "toml_to_rego_equivalence_suite",
        "total": total,
        "matched": matched,
        "parity_percent": (matched as f64 / total as f64) * 100.0,
        "mismatches": mismatches,
    });

    assert!(
        report["mismatches"]
            .as_array()
            .expect("mismatches should be array")
            .is_empty(),
        "toml->rego diff report:\n{}",
        serde_json::to_string_pretty(&report).expect("report serialization should succeed")
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(test)]
#[test]
fn toml_policy_legacy_behavior_unchanged() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let profile_path = manifest_dir.join("../../configs/agents/example.toml");
    let content = std::fs::read_to_string(&profile_path).expect("read example profile");
    let profile = parse_agent_profile_file(&profile_path, &content)
        .expect("parse example profile should work");

    let global_layer = ask_only_layer("global");
    let profile_layer = profile_to_policy_layer(&profile);
    let session_layer = SessionPolicyOverrides {
        allow_tools: vec![],
        ask_tools: vec![],
        deny_tools: vec![],
    }
    .into_layer();

    let dir = temp_rego_policy_dir();
    std::fs::create_dir_all(&dir).expect("create rego test dir");

    for tool in [
        "edit",
        "web_fetch",
        "read:secrets.env",
        "read:docs.md",
        "bash",
    ] {
        let expected =
            PolicyLayer::evaluate_tool(&global_layer, &profile_layer, &session_layer, tool);
        let actual = evaluate_policy_with_rego_dir(
            &dir,
            global_layer.clone(),
            profile_layer.clone(),
            session_layer.clone(),
            &test_policy_input(tool),
        )
        .expect("rego evaluation should succeed");

        assert_eq!(
            actual.decision, expected.decision,
            "decision mismatch for {tool}"
        );
        assert_eq!(
            actual.matched_rule, expected.matched_rule,
            "matched_rule mismatch for {tool}"
        );
        assert_eq!(
            actual.source_layer, expected.source_layer,
            "source_layer mismatch for {tool}"
        );
    }

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(test)]
#[test]
fn toml_to_rego_rejects_unsupported_construct() {
    let profile_path = PathBuf::from("unsupported-policy.toml");
    let content = r#"
[agent]
name = "unsupported-agent"

[llm]
models = ["gpt-4.1-mini"]

[policy]
"*" = "ask"
"mcp.shell.execute\nunsafe" = "deny"
"#;

    let profile =
        parse_agent_profile_file(&profile_path, content).expect("parse profile should work");
    let profile_layer = profile_to_policy_layer(&profile);

    let dir = temp_rego_policy_dir();
    std::fs::create_dir_all(&dir).expect("create rego test dir");

    let err = evaluate_policy_with_rego_dir(
        &dir,
        ask_only_layer("global"),
        profile_layer,
        ask_only_layer("session_override"),
        &test_policy_input("mcp.shell.execute"),
    )
    .expect_err("unsupported policy key should be rejected");

    let err_msg = err.to_string();
    assert!(err_msg.contains("unsupported toml policy key"));
    assert!(err_msg.contains("mcp.shell.execute\\nunsafe"));

    let _ = std::fs::remove_dir_all(dir);
}

fn parse_permission_policy(value: &str) -> Result<PermissionPolicy, AgentError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "allow" => Ok(PermissionPolicy::Allow),
        "ask" => Ok(PermissionPolicy::Ask),
        "deny" => Ok(PermissionPolicy::Deny),
        _ => Err(AgentError::InvalidInput(format!(
            "unsupported permission_policy: {}",
            value
        ))),
    }
}

fn parse_profile_policy(
    policy: &BTreeMap<String, String>,
    profile_path: &Path,
) -> Result<(PermissionPolicy, Vec<String>, Vec<String>), AgentError> {
    let default_raw = policy.get("*").ok_or_else(|| {
        AgentError::InvalidInput(format!(
            "profile {} missing policy.* default rule",
            profile_path.display()
        ))
    })?;
    let default_policy = parse_permission_policy(default_raw).map_err(|err| {
        AgentError::InvalidInput(format!(
            "profile {} policy.* error: {err}",
            profile_path.display()
        ))
    })?;

    let mut allowed_tools = Vec::new();
    let mut denied_tools = Vec::new();

    for (pattern, decision_raw) in policy {
        if pattern == "*" {
            continue;
        }

        let decision = parse_permission_policy(decision_raw).map_err(|err| {
            AgentError::InvalidInput(format!(
                "profile {} policy.{pattern} error: {err}",
                profile_path.display()
            ))
        })?;

        match decision {
            PermissionPolicy::Allow => allowed_tools.push(pattern.clone()),
            PermissionPolicy::Deny => denied_tools.push(pattern.clone()),
            PermissionPolicy::Ask => {
                if default_policy != PermissionPolicy::Ask {
                    return Err(AgentError::InvalidInput(format!(
                        "profile {} policy.{pattern}=ask is unsupported when policy.* != ask",
                        profile_path.display()
                    )));
                }
            }
        }
    }

    Ok((default_policy, allowed_tools, denied_tools))
}

fn parse_agent_profile_file(
    profile_path: &Path,
    content: &str,
) -> Result<AgentProfile, Box<dyn std::error::Error>> {
    let parsed: AgentProfileFile = toml::from_str(content).map_err(|err| {
        AgentError::InvalidInput(format!(
            "profile {} parse failed: {err}",
            profile_path.display()
        ))
    })?;

    if parsed.agent.name.trim().is_empty() {
        return Err(AgentError::InvalidInput(format!(
            "profile {} agent.name must be non-empty",
            profile_path.display()
        ))
        .into());
    }
    if parsed.llm.models.is_empty() {
        return Err(AgentError::InvalidInput(format!(
            "profile {} llm.models must contain at least one model",
            profile_path.display()
        ))
        .into());
    }

    if let Some(fallback_model) = parsed.llm.fallback_model.as_ref() {
        if !parsed
            .llm
            .models
            .iter()
            .any(|model| model == fallback_model)
        {
            return Err(AgentError::InvalidInput(format!(
                "profile {} llm.fallback_model must exist in llm.models",
                profile_path.display()
            ))
            .into());
        }
    }

    let model_name = parsed
        .llm
        .fallback_model
        .clone()
        .unwrap_or_else(|| parsed.llm.models[0].clone());
    let provider = parsed
        .llm
        .provider
        .clone()
        .unwrap_or_else(|| "one-api".to_string());

    let (default_policy, allowed_tools, denied_tools) =
        parse_profile_policy(&parsed.policy, profile_path)?;

    let mut profile = AgentProfile::new(
        parsed.agent.name,
        ModelConfig {
            provider,
            model_name,
            max_tokens: None,
            temperature: None,
        },
    );
    profile.budget.token_limit = parsed.llm.token_budget_daily;
    profile.permissions.policy = default_policy;
    profile.permissions.allowed_tools = allowed_tools;
    profile.permissions.denied_tools = denied_tools;

    Ok(profile)
}

fn load_agent_profiles(
    profiles_dir: &Path,
) -> Result<Vec<AgentProfile>, Box<dyn std::error::Error>> {
    if !profiles_dir.exists() {
        info!(profiles_dir = %profiles_dir.display(), "Agent profile directory not found, skipping profile load");
        return Ok(Vec::new());
    }

    if !profiles_dir.is_dir() {
        return Err(AgentError::InvalidInput(format!(
            "agent profile path is not a directory: {}",
            profiles_dir.display()
        ))
        .into());
    }

    let mut profile_paths = std::fs::read_dir(profiles_dir)?
        .filter_map(|entry_result| entry_result.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("toml"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    profile_paths.sort();

    let mut profiles = Vec::with_capacity(profile_paths.len());
    for profile_path in profile_paths {
        let content = std::fs::read_to_string(&profile_path).map_err(|err| {
            AgentError::InvalidInput(format!(
                "profile {} read failed: {err}",
                profile_path.display()
            ))
        })?;
        let profile = parse_agent_profile_file(&profile_path, &content)?;
        profiles.push(profile);
    }

    Ok(profiles)
}

#[cfg(test)]
mod profile_loader_tests {
    use super::*;

    fn temp_profiles_dir() -> PathBuf {
        std::env::temp_dir().join(format!("agentd-profiles-test-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn load_agent_profiles_accepts_example_schema() {
        let dir = temp_profiles_dir();
        std::fs::create_dir_all(&dir).expect("create temp profiles dir");
        let file = dir.join("example.toml");
        std::fs::write(
            &file,
            r#"
[agent]
name = "sample"

[llm]
models = ["claude-4-sonnet", "gpt-4.1-mini"]
token_budget_daily = 500000
fallback_model = "gpt-4.1-mini"

[policy]
"*" = "ask"
bash = "ask"
edit = "allow"
web_fetch = "deny"
"#,
        )
        .expect("write valid profile");

        let profiles = load_agent_profiles(&dir).expect("profiles should load");
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "sample");
        assert_eq!(profiles[0].model.model_name, "gpt-4.1-mini");
        assert_eq!(profiles[0].budget.token_limit, Some(500000));
        assert_eq!(profiles[0].permissions.policy, PermissionPolicy::Ask);
        assert!(profiles[0]
            .permissions
            .allowed_tools
            .contains(&"edit".to_string()));
        assert!(profiles[0]
            .permissions
            .denied_tools
            .contains(&"web_fetch".to_string()));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_agent_profiles_returns_error_for_invalid_profile() {
        let dir = temp_profiles_dir();
        std::fs::create_dir_all(&dir).expect("create temp profiles dir");
        let file = dir.join("invalid.toml");
        std::fs::write(
            &file,
            r#"
[agent]
name = "bad"

[llm]
models = ["claude-4-sonnet"]

[policy]
edit = "allow"
"#,
        )
        .expect("write invalid profile");

        let err = load_agent_profiles(&dir).expect_err("invalid profile should fail");
        let err_msg = err.to_string();
        assert!(err_msg.contains("invalid.toml"));
        assert!(err_msg.contains("policy.*"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_agent_profiles_returns_empty_for_missing_dir() {
        let dir = temp_profiles_dir();
        let profiles = load_agent_profiles(&dir).expect("missing dir should be skipped");
        assert!(profiles.is_empty());
    }
}

fn build_a2a_agent_card(profile: &AgentProfile) -> Value {
    json!({
        "agent_id": profile.id.to_string(),
        "name": profile.name,
        "version": "0.1.0",
        "model": profile.model.model_name,
        "provider": profile.model.provider,
        "capabilities": {
            "protocol": "a2a-compatible",
            "tools": {
                "allowed": profile.permissions.allowed_tools,
                "denied": profile.permissions.denied_tools,
                "default_policy": format!("{:?}", profile.permissions.policy).to_ascii_lowercase()
            }
        }
    })
}

fn persist_agent_card(root: &Path, profile: &AgentProfile) -> Result<PathBuf, AgentError> {
    let card_path = root.join(profile.id.to_string()).join("agent.json");
    if let Some(parent_dir) = card_path.parent() {
        std::fs::create_dir_all(parent_dir).map_err(|err| {
            AgentError::Runtime(format!("create agent card directory failed: {err}"))
        })?;
    }

    let card_json = build_a2a_agent_card(profile);
    let card_content = serde_json::to_string_pretty(&card_json)
        .map_err(|err| AgentError::Runtime(format!("serialize agent card failed: {err}")))?;
    std::fs::write(&card_path, card_content)
        .map_err(|err| AgentError::Runtime(format!("write agent card failed: {err}")))?;

    Ok(card_path)
}

fn request_id_to_session_suffix(id: &Value) -> String {
    match id {
        Value::String(value) => {
            if value.trim().is_empty() {
                "empty".to_string()
            } else {
                value.clone()
            }
        }
        Value::Number(number) => number.to_string(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Null => "null".to_string(),
        Value::Array(_) | Value::Object(_) => format!("json-{}", uuid::Uuid::new_v4()),
    }
}

fn build_audit_context(request_id: &Value) -> AuditContext {
    let session_id = format!("rpc-{}", request_id_to_session_suffix(request_id));
    let trace_id = format!("trace-{session_id}");
    AuditContext::new(trace_id, session_id, EventSeverity::Info)
}

async fn record_audit_event(
    store: &Arc<SqliteStore>,
    context: &AuditContext,
    agent_id: uuid::Uuid,
    event_type: EventType,
    result: EventResult,
    payload: EventPayload,
) {
    let event = AuditEvent::new_with_context(
        agent_id,
        event_type,
        payload,
        result.clone(),
        context.with_severity(EventSeverity::from_result(&result)),
    );

    if let Err(err) = store.append_audit_event(event).await {
        warn!(
            %err,
            %agent_id,
            trace_id = %context.trace_id,
            session_id = %context.session_id,
            "persist audit event failed"
        );
    }
}

fn is_pending_approval_event(event: &AuditEvent) -> bool {
    event.event_type == EventType::ToolInvoked
        && event.result == EventResult::Pending
        && event.payload.message.as_deref() == Some("policy.ask")
}

fn is_approval_resolution_event(event: &AuditEvent) -> bool {
    matches!(
        event.event_type,
        EventType::ToolApproved | EventType::ToolDenied
    ) && event
        .payload
        .metadata
        .get("approval_id")
        .and_then(Value::as_str)
        .is_some()
}

fn pending_approval_items(events: &[AuditEvent]) -> Vec<Value> {
    let mut resolved_ids = HashSet::new();
    for event in events {
        if is_approval_resolution_event(event) {
            if let Some(approval_id) = event
                .payload
                .metadata
                .get("approval_id")
                .and_then(Value::as_str)
            {
                resolved_ids.insert(approval_id.to_string());
            }
        }
    }

    let mut seen = HashSet::new();
    let mut approvals = Vec::new();
    for event in events.iter().rev() {
        if !is_pending_approval_event(event) {
            continue;
        }

        let approval_id = event.id.to_string();
        if resolved_ids.contains(&approval_id) || seen.contains(&approval_id) {
            continue;
        }
        seen.insert(approval_id.clone());

        approvals.push(json!({
            "id": approval_id,
            "tool": event.payload.tool_name.clone().unwrap_or_else(|| "<unknown>".to_string()),
            "reason": event
                .payload
                .metadata
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("policy.ask"),
            "trace_id": event.trace_id,
            "requested_at": event.timestamp,
        }));
    }

    approvals
}

fn canonical_mcp_tool_name(tool: &str) -> Result<String, AgentError> {
    let normalized = tool.trim();
    if normalized.is_empty() {
        return Err(AgentError::InvalidInput(
            "tool must be non-empty".to_string(),
        ));
    }

    if normalized.starts_with("mcp.") {
        return Ok(normalized.to_string());
    }

    Ok(format!("mcp.{normalized}"))
}

fn parse_onboard_mcp_transport(value: Option<&str>) -> Result<mcp::McpTransport, AgentError> {
    let Some(raw) = value else {
        return Ok(mcp::McpTransport::Stdio);
    };

    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "stdio" => Ok(mcp::McpTransport::Stdio),
        _ => Err(AgentError::InvalidInput(format!(
            "invalid mcp transport `{raw}` (expected stdio)"
        ))),
    }
}

fn parse_onboard_mcp_trust_level(value: Option<&str>) -> Result<mcp::McpTrustLevel, AgentError> {
    match value {
        Some(raw) => mcp::parse_trust_level(raw),
        None => Ok(mcp::McpTrustLevel::Community),
    }
}

async fn evaluate_mcp_tool_policy(
    store: &Arc<SqliteStore>,
    audit_context: &AuditContext,
    agent_id: uuid::Uuid,
    tool: &str,
) -> Result<(PolicyEvaluation, PolicyGatewayDecision), AgentError> {
    let profile = store.get_agent(agent_id).await?;
    let global_layer = PolicyLayer {
        name: "global".to_string(),
        rules: vec![],
    };
    let profile_layer = profile_to_policy_layer(&profile);
    let session_layer = SessionPolicyOverrides {
        allow_tools: vec![],
        ask_tools: vec![],
        deny_tools: vec![],
    }
    .into_layer();

    let mut request_meta = BTreeMap::new();
    request_meta.insert("trace_id".to_string(), audit_context.trace_id.clone());
    let policy_input = PolicyInputContext {
        agent: PolicyAgentContext {
            id: Some(agent_id.to_string()),
            trust_level: Some(format!("{:?}", profile.permissions.policy).to_ascii_lowercase()),
        },
        tool: PolicyToolContext {
            name: tool.to_string(),
        },
        resource: PolicyResourceContext { uri: None },
        time: PolicyTimeContext {
            timestamp_rfc3339: Some(Utc::now().to_rfc3339()),
        },
        request_meta,
    };
    policy_input.validate()?;

    let evaluation = evaluate_policy_with_rego_dir(
        Path::new("policies"),
        global_layer,
        profile_layer,
        session_layer,
        &policy_input,
    )?;
    let gateway_decision = evaluation.to_gateway_decision(audit_context.trace_id.clone());
    Ok((evaluation, gateway_decision))
}

async fn authorize_mcp_tool_before_forward<F>(
    store: &Arc<SqliteStore>,
    audit_context: &AuditContext,
    agent_id: uuid::Uuid,
    tool: &str,
    forward_payload: Value,
    forward: F,
) -> Result<McpGatewayForwardResult, AgentError>
where
    F: FnOnce(&Value) -> Value,
{
    let tool = canonical_mcp_tool_name(tool)?;
    let (evaluation, gateway_decision) =
        evaluate_mcp_tool_policy(store, audit_context, agent_id, &tool).await?;

    let mut metadata = json!({
        "matched_rule": evaluation.matched_rule.clone(),
        "source_layer": evaluation.source_layer.clone(),
        "reason": gateway_decision.reason.clone(),
        "trace_id": gateway_decision.trace_id.clone(),
    });

    let (event_type, result, message, forwarded, downstream) = match gateway_decision.decision {
        PolicyDecision::Allow => {
            let downstream = forward(&forward_payload);
            (
                EventType::ToolApproved,
                EventResult::Success,
                "policy.allow",
                true,
                Some(downstream),
            )
        }
        PolicyDecision::Ask => (
            EventType::ToolInvoked,
            EventResult::Pending,
            "policy.ask",
            false,
            None,
        ),
        PolicyDecision::Deny => {
            let replay = serde_json::to_value(PolicyReplayReference::new(
                &tool,
                forward_payload,
                gateway_decision.reason.clone(),
                gateway_decision.trace_id.clone(),
            ))
            .map_err(|err| {
                AgentError::Runtime(format!("serialize policy replay reference failed: {err}"))
            })?;
            metadata["replay"] = replay;

            (
                EventType::ToolDenied,
                EventResult::Failure,
                "policy.deny",
                false,
                None,
            )
        }
    };

    record_audit_event(
        store,
        audit_context,
        agent_id,
        event_type,
        result,
        EventPayload {
            tool_name: Some(tool.clone()),
            message: Some(message.to_string()),
            metadata,
        },
    )
    .await;

    Ok(McpGatewayForwardResult {
        decision: gateway_decision,
        forwarded,
        downstream,
        matched_rule: evaluation.matched_rule,
        source_layer: evaluation.source_layer,
    })
}

#[derive(Debug, Clone)]
struct OneApiProvisioned {
    token_id: String,
    access_token: String,
    channel_id: Option<String>,
}

fn extract_string_value(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(v) = value.get(*key) {
            if let Some(s) = v.as_str() {
                return Some(s.to_string());
            }
            if let Some(i) = v.as_i64() {
                return Some(i.to_string());
            }
            if let Some(u) = v.as_u64() {
                return Some(u.to_string());
            }
        }
    }
    None
}

fn with_base_url(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

async fn request_with_retry(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    body: Value,
    api_key: Option<&str>,
    retries: u32,
    backoff_secs: u64,
) -> Result<Value, AgentError> {
    let attempts = retries.max(1);
    for attempt in 1..=attempts {
        let mut request = client
            .request(method.clone(), url)
            .header("Content-Type", "application/json")
            .json(&body);

        if let Some(key) = api_key {
            request = request.header("Authorization", format!("Bearer {key}"));
        }

        match request.send().await {
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.map_err(|err| {
                    AgentError::Runtime(format!("read one-api response body failed: {err}"))
                })?;

                if !status.is_success() {
                    if attempt < attempts {
                        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                        continue;
                    }
                    return Err(AgentError::Runtime(format!(
                        "one-api management request failed with status {status}: {text}"
                    )));
                }

                let parsed: Value =
                    serde_json::from_str(&text).unwrap_or_else(|_| json!({"raw": text}));
                return Ok(parsed);
            }
            Err(err) => {
                if attempt < attempts {
                    tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                    continue;
                }
                return Err(AgentError::Runtime(format!(
                    "one-api management request failed after retries: {err}"
                )));
            }
        }
    }

    Err(AgentError::Runtime(
        "one-api management request exhausted without result".to_string(),
    ))
}

async fn provision_one_api(
    config: &OneApiConfig,
    profile: &AgentProfile,
    idempotency_key: &str,
) -> Result<OneApiProvisioned, AgentError> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(config.management_timeout_secs))
        .build()
        .map_err(|err| {
            AgentError::Config(format!("build one-api management client failed: {err}"))
        })?;

    let token_url = with_base_url(&config.management_base_url, &config.create_token_path);
    let quota_value: i64 = match profile.budget.token_limit {
        Some(limit) => i64::try_from(limit)
            .map_err(|err| AgentError::InvalidInput(format!("token_limit overflow: {err}")))?,
        None => -1,
    };
    let token_body = json!({
        "name": format!("agentd-{}", profile.name),
        "idempotency_key": idempotency_key,
        "remain_quota": quota_value,
        "unlimited_quota": profile.budget.token_limit.is_none(),
        "model_limits": [profile.model.model_name.clone()],
    });

    let token_resp = request_with_retry(
        &client,
        reqwest::Method::POST,
        &token_url,
        token_body,
        config.management_api_key.as_deref(),
        config.management_retries,
        config.management_retry_backoff_secs,
    )
    .await?;

    let token_data = token_resp.get("data").unwrap_or(&token_resp);
    let token_id = extract_string_value(token_data, &["id", "token_id"]).ok_or_else(|| {
        AgentError::Runtime("one-api create token response missing token id".to_string())
    })?;
    let access_token =
        extract_string_value(token_data, &["key", "token", "value"]).ok_or_else(|| {
            AgentError::Runtime("one-api create token response missing access token".to_string())
        })?;

    let channel_id = if config.provision_channel {
        let channel_url = with_base_url(&config.management_base_url, &config.create_channel_path);
        let channel_body = json!({
            "name": format!("agentd-{}", profile.name),
            "idempotency_key": idempotency_key,
            "key": access_token,
            "models": profile.model.model_name,
        });

        let channel_resp = request_with_retry(
            &client,
            reqwest::Method::POST,
            &channel_url,
            channel_body,
            config.management_api_key.as_deref(),
            config.management_retries,
            config.management_retry_backoff_secs,
        )
        .await?;
        let channel_data = channel_resp.get("data").unwrap_or(&channel_resp);
        extract_string_value(channel_data, &["id", "channel_id"])
    } else {
        None
    };

    Ok(OneApiProvisioned {
        token_id,
        access_token,
        channel_id,
    })
}

fn derive_trust_level_from_profile(profile: &AgentProfile) -> TrustLevel {
    match profile.permissions.policy {
        PermissionPolicy::Allow => TrustLevel::Builtin,
        PermissionPolicy::Ask => TrustLevel::Verified,
        PermissionPolicy::Deny => TrustLevel::Untrusted,
    }
}

fn resolve_trust_level(
    profile: &AgentProfile,
    explicit_trust_level: Option<&str>,
) -> Result<TrustLevel, AgentError> {
    if let Some(raw) = explicit_trust_level {
        return TrustLevel::parse(raw);
    }

    Ok(derive_trust_level_from_profile(profile))
}

fn parse_network_isolation_policy(
    trust_level: TrustLevel,
    raw_network_policy: Option<&str>,
) -> Result<firecracker::NetworkIsolationPolicy, AgentError> {
    if let Some(raw) = raw_network_policy {
        let normalized = raw.trim().to_ascii_lowercase();
        return match normalized.as_str() {
            "allow" | "allow_all" => Ok(firecracker::NetworkIsolationPolicy::AllowAll),
            "deny" | "deny_all" => Ok(firecracker::NetworkIsolationPolicy::DenyAll),
            _ => Err(AgentError::InvalidInput(format!(
                "invalid network_policy `{raw}` (expected allow|deny)"
            ))),
        };
    }

    let _ = trust_level;
    Ok(firecracker::NetworkIsolationPolicy::AllowAll)
}

async fn handle_rpc_request(
    request: JsonRpcRequest,
    store: Arc<SqliteStore>,
    state: RuntimeState,
    one_api_config: OneApiConfig,
) -> JsonRpcResponse {
    let audit_context = build_audit_context(&request.id);
    match request.method.as_str() {
        "GetHealth" | "management.GetHealth" => {
            let storage_status = if store.health_check().is_ok() {
                "ready"
            } else {
                "degraded"
            };
            let one_api_status = state.one_api_status().await;
            let overall_status = if storage_status == "ready"
                && matches!(one_api_status.as_str(), "ready" | "disabled")
            {
                "ok"
            } else {
                "degraded"
            };

            JsonRpcResponse::success(
                request.id,
                json!({
                    "status": overall_status,
                    "subsystems": {
                        "daemon": "ready",
                        "protocol": "ready",
                        "storage": storage_status,
                        "one_api": one_api_status,
                    }
                }),
            )
        }
        "AuthorizeTool" | "management.AuthorizeTool" => {
            let params = match serde_json::from_value::<AuthorizeToolParams>(request.params) {
                Ok(params) => params,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid authorize params: {err}"),
                    )
                }
            };

            if params.tool.trim().is_empty() {
                return JsonRpcResponse::error(request.id, -32602, "tool must be non-empty");
            }

            let global_layer = convert_rule_inputs("global", params.global_rules);
            let mut profile_layer = convert_rule_inputs("agent_profile", params.profile_rules);
            let mut evaluated_agent_id: Option<uuid::Uuid> = None;
            let mut evaluated_trust_level: Option<String> = None;

            if let Some(agent_id) = params.agent_id.clone() {
                let parsed_agent_id = match uuid::Uuid::parse_str(&agent_id) {
                    Ok(agent_id) => agent_id,
                    Err(err) => {
                        return JsonRpcResponse::error(
                            request.id,
                            -32602,
                            format!("invalid agent_id: {err}"),
                        )
                    }
                };
                evaluated_agent_id = Some(parsed_agent_id);

                let profile = match store.get_agent(parsed_agent_id).await {
                    Ok(profile) => profile,
                    Err(err) => {
                        return JsonRpcResponse::error(
                            request.id,
                            -32010,
                            format!("query agent for policy evaluation failed: {err}"),
                        )
                    }
                };
                evaluated_trust_level =
                    Some(format!("{:?}", profile.permissions.policy).to_ascii_lowercase());
                profile_layer = profile_to_policy_layer(&profile);
            }

            let session_layer = params
                .session_overrides
                .unwrap_or(SessionPolicyOverrides {
                    allow_tools: vec![],
                    ask_tools: vec![],
                    deny_tools: vec![],
                })
                .into_layer();

            let mut request_meta = BTreeMap::new();
            request_meta.insert("trace_id".to_string(), audit_context.trace_id.clone());
            let policy_input = PolicyInputContext {
                agent: PolicyAgentContext {
                    id: params.agent_id.clone(),
                    trust_level: evaluated_trust_level,
                },
                tool: PolicyToolContext {
                    name: params.tool.clone(),
                },
                resource: PolicyResourceContext { uri: None },
                time: PolicyTimeContext {
                    timestamp_rfc3339: Some(Utc::now().to_rfc3339()),
                },
                request_meta,
            };
            if let Err(err) = policy_input.validate() {
                return JsonRpcResponse::error(request.id, -32602, err.to_string());
            }

            let evaluation = match evaluate_policy_with_rego_dir(
                Path::new("policies"),
                global_layer,
                profile_layer,
                session_layer,
                &policy_input,
            ) {
                Ok(evaluation) => evaluation,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32021,
                        format!("evaluate policy with rego failed: {err}"),
                    )
                }
            };
            let gateway_decision = evaluation.to_gateway_decision(audit_context.trace_id.clone());

            if evaluation.decision == PolicyDecision::Deny {
                if let Some(agent_id) = evaluated_agent_id {
                    let replay = match serde_json::to_value(PolicyReplayReference::new(
                        params.tool.clone(),
                        json!({
                            "agent_id": params.agent_id.clone(),
                            "mode": "authorize_tool",
                        }),
                        gateway_decision.reason.clone(),
                        gateway_decision.trace_id.clone(),
                    )) {
                        Ok(replay) => replay,
                        Err(err) => {
                            return JsonRpcResponse::error(
                                request.id,
                                -32021,
                                format!("serialize policy replay reference failed: {err}"),
                            )
                        }
                    };
                    record_audit_event(
                        &store,
                        &audit_context,
                        agent_id,
                        EventType::ToolDenied,
                        EventResult::Failure,
                        EventPayload {
                            tool_name: Some(params.tool.clone()),
                            message: Some("policy.deny".to_string()),
                            metadata: json!({
                                "matched_rule": evaluation.matched_rule.clone(),
                                "source_layer": evaluation.source_layer.clone(),
                                "reason": gateway_decision.reason.clone(),
                                "trace_id": gateway_decision.trace_id.clone(),
                                "replay": replay,
                            }),
                        },
                    )
                    .await;
                }
                return JsonRpcResponse::error(request.id, -32016, gateway_decision.reason.clone());
            }

            if let Some(agent_id) = evaluated_agent_id {
                let (event_type, result, message) = match evaluation.decision {
                    PolicyDecision::Allow => (
                        EventType::ToolApproved,
                        EventResult::Success,
                        "policy.allow",
                    ),
                    PolicyDecision::Ask => {
                        (EventType::ToolInvoked, EventResult::Pending, "policy.ask")
                    }
                    PolicyDecision::Deny => {
                        (EventType::ToolDenied, EventResult::Failure, "policy.deny")
                    }
                };
                record_audit_event(
                    &store,
                    &audit_context,
                    agent_id,
                    event_type,
                    result,
                    EventPayload {
                        tool_name: Some(params.tool.clone()),
                        message: Some(message.to_string()),
                        metadata: json!({
                            "matched_rule": evaluation.matched_rule.clone(),
                            "source_layer": evaluation.source_layer.clone(),
                            "reason": gateway_decision.reason.clone(),
                            "trace_id": gateway_decision.trace_id.clone(),
                        }),
                    },
                )
                .await;
            }

            JsonRpcResponse::success(
                request.id,
                json!({
                    "tool": evaluation.tool,
                    "decision": evaluation.decision,
                    "matched_rule": evaluation.matched_rule,
                    "source_layer": evaluation.source_layer,
                    "reason": gateway_decision.reason,
                    "trace_id": gateway_decision.trace_id,
                }),
            )
        }
        "ListAvailableTools" | "management.ListAvailableTools" => {
            let params = match serde_json::from_value::<ListAvailableToolsParams>(request.params) {
                Ok(params) => params,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid list available tools params: {err}"),
                    )
                }
            };

            let agent_id = match uuid::Uuid::parse_str(&params.agent_id) {
                Ok(agent_id) => agent_id,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid agent_id: {err}"),
                    )
                }
            };

            let available_tools = {
                let mcp_host = state.mcp_host();
                let mut host = mcp_host.lock().await;
                if let Err(err) = host.refresh_health() {
                    return JsonRpcResponse::error(
                        request.id,
                        -32022,
                        format!("refresh mcp health failed: {err}"),
                    );
                }
                host.list_available_tools()
            };

            let mut tools = Vec::new();
            for available_tool in available_tools {
                let policy_tool_name = match canonical_mcp_tool_name(&available_tool.tool_name) {
                    Ok(name) => name,
                    Err(err) => return JsonRpcResponse::error(request.id, -32602, err.to_string()),
                };

                let (evaluation, gateway_decision) = match evaluate_mcp_tool_policy(
                    &store,
                    &audit_context,
                    agent_id,
                    &policy_tool_name,
                )
                .await
                {
                    Ok(result) => result,
                    Err(AgentError::NotFound(message)) => {
                        return JsonRpcResponse::error(request.id, -32010, message)
                    }
                    Err(err) => {
                        return JsonRpcResponse::error(
                            request.id,
                            -32021,
                            format!("evaluate tool policy failed: {err}"),
                        )
                    }
                };

                if evaluation.decision == PolicyDecision::Deny {
                    continue;
                }

                tools.push(json!({
                    "server": available_tool.server_id,
                    "tool": available_tool.tool_name,
                    "policy_tool": policy_tool_name,
                    "trust_level": format!("{:?}", available_tool.trust_level).to_ascii_lowercase(),
                    "health": format!("{:?}", available_tool.health).to_ascii_lowercase(),
                    "decision": evaluation.decision,
                    "reason": gateway_decision.reason,
                    "trace_id": gateway_decision.trace_id,
                }));
            }

            JsonRpcResponse::success(
                request.id,
                json!({
                    "agent_id": params.agent_id,
                    "tools": tools,
                }),
            )
        }
        "InvokeSkill" | "management.InvokeSkill" => {
            let params = match serde_json::from_value::<InvokeSkillParams>(request.params) {
                Ok(params) => params,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid invoke skill params: {err}"),
                    )
                }
            };

            if params.server.trim().is_empty() || params.tool.trim().is_empty() {
                return JsonRpcResponse::error(
                    request.id,
                    -32602,
                    "server and tool must be non-empty",
                );
            }

            let agent_id = match uuid::Uuid::parse_str(&params.agent_id) {
                Ok(agent_id) => agent_id,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid agent_id: {err}"),
                    )
                }
            };

            let resolved_tool = {
                let mcp_host = state.mcp_host();
                let mut host = mcp_host.lock().await;
                if let Err(err) = host.refresh_health() {
                    return JsonRpcResponse::error(
                        request.id,
                        -32022,
                        format!("refresh mcp health failed: {err}"),
                    );
                }
                host.list_available_tools()
                    .into_iter()
                    .find(|tool| tool.server_id == params.server && tool.tool_name == params.tool)
            };

            let Some(resolved_tool) = resolved_tool else {
                return JsonRpcResponse::error(
                    request.id,
                    -32023,
                    format!(
                        "requested skill not available: server={} tool={}",
                        params.server, params.tool
                    ),
                );
            };

            let policy_tool_name = match canonical_mcp_tool_name(&resolved_tool.tool_name) {
                Ok(name) => name,
                Err(err) => return JsonRpcResponse::error(request.id, -32602, err.to_string()),
            };
            let payload = json!({
                "server": resolved_tool.server_id,
                "tool": resolved_tool.tool_name,
                "args": params.args,
            });

            let forward_server = resolved_tool.server_id.clone();
            let forward_tool = resolved_tool.tool_name.clone();
            let outcome = match authorize_mcp_tool_before_forward(
                &store,
                &audit_context,
                agent_id,
                &policy_tool_name,
                payload,
                move |forward_payload| {
                    json!({
                        "status": "forwarded",
                        "transport": "mcp-skill-gateway",
                        "server": forward_server,
                        "tool": forward_tool,
                        "request": forward_payload,
                    })
                },
            )
            .await
            {
                Ok(outcome) => outcome,
                Err(AgentError::NotFound(message)) => {
                    return JsonRpcResponse::error(request.id, -32010, message)
                }
                Err(AgentError::InvalidInput(message)) => {
                    return JsonRpcResponse::error(request.id, -32602, message)
                }
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32022,
                        format!("invoke skill failed: {err}"),
                    )
                }
            };

            if outcome.decision.decision == PolicyDecision::Deny {
                return JsonRpcResponse::error(request.id, -32016, outcome.decision.reason);
            }

            JsonRpcResponse::success(
                request.id,
                json!({
                    "server": resolved_tool.server_id,
                    "tool": resolved_tool.tool_name,
                    "policy_tool": policy_tool_name,
                    "forwarded": outcome.forwarded,
                    "decision": outcome.decision,
                    "matched_rule": outcome.matched_rule,
                    "source_layer": outcome.source_layer,
                    "downstream": outcome.downstream,
                }),
            )
        }
        "AuthorizeMcpTool" | "management.AuthorizeMcpTool" => {
            let params = match serde_json::from_value::<AuthorizeMcpToolParams>(request.params) {
                Ok(params) => params,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid authorize mcp tool params: {err}"),
                    )
                }
            };

            let agent_id = match uuid::Uuid::parse_str(&params.agent_id) {
                Ok(agent_id) => agent_id,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid agent_id: {err}"),
                    )
                }
            };

            let payload = params.payload;
            let tool_name = params.tool;
            let outcome = match authorize_mcp_tool_before_forward(
                &store,
                &audit_context,
                agent_id,
                &tool_name,
                payload,
                |forward_payload| {
                    json!({
                        "status": "forwarded",
                        "transport": "mcp-gateway",
                        "request": forward_payload,
                    })
                },
            )
            .await
            {
                Ok(outcome) => outcome,
                Err(AgentError::NotFound(message)) => {
                    return JsonRpcResponse::error(request.id, -32010, message)
                }
                Err(AgentError::InvalidInput(message)) => {
                    return JsonRpcResponse::error(request.id, -32602, message)
                }
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32022,
                        format!("authorize mcp tool failed: {err}"),
                    )
                }
            };

            if outcome.decision.decision == PolicyDecision::Deny {
                return JsonRpcResponse::error(request.id, -32016, outcome.decision.reason);
            }

            JsonRpcResponse::success(request.id, json!(outcome))
        }
        "ListMcpServers" | "management.ListMcpServers" => {
            let servers = {
                let mcp_host = state.mcp_host();
                let mut host = mcp_host.lock().await;
                if let Err(err) = host.refresh_health() {
                    return JsonRpcResponse::error(
                        request.id,
                        -32022,
                        format!("refresh mcp health failed: {err}"),
                    );
                }
                host.list_servers()
            };

            JsonRpcResponse::success(
                request.id,
                json!({
                    "servers": servers
                        .into_iter()
                        .map(|entry| {
                            json!({
                                "server": entry.server_id,
                                "capabilities": entry.capabilities,
                                "trust_level": format!("{:?}", entry.trust_level).to_ascii_lowercase(),
                                "health": format!("{:?}", entry.health).to_ascii_lowercase(),
                            })
                        })
                        .collect::<Vec<_>>(),
                }),
            )
        }
        "OnboardMcpServer" | "management.OnboardMcpServer" => {
            let params = match serde_json::from_value::<OnboardMcpServerParams>(request.params) {
                Ok(params) => params,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid onboard mcp server params: {err}"),
                    )
                }
            };

            let name = params.name.trim();
            let command = params.command.trim();
            if name.is_empty() || command.is_empty() {
                return JsonRpcResponse::error(
                    request.id,
                    -32602,
                    "name and command must be non-empty",
                );
            }

            let transport = match parse_onboard_mcp_transport(params.transport.as_deref()) {
                Ok(transport) => transport,
                Err(AgentError::InvalidInput(message)) => {
                    return JsonRpcResponse::error(request.id, -32602, message)
                }
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32027,
                        format!("resolve onboard transport failed: {err}"),
                    )
                }
            };
            let trust_level = match parse_onboard_mcp_trust_level(params.trust_level.as_deref()) {
                Ok(level) => level,
                Err(AgentError::InvalidInput(message)) => {
                    return JsonRpcResponse::error(request.id, -32602, message)
                }
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32027,
                        format!("resolve onboard trust level failed: {err}"),
                    )
                }
            };

            let onboarded = {
                let mcp_host = state.mcp_host();
                let mut host = mcp_host.lock().await;
                host.onboard_server(mcp::McpServerConfig {
                    name: name.to_string(),
                    command: command.to_string(),
                    args: params.args,
                    transport,
                    trust_level,
                })
                .await
            };

            match onboarded {
                Ok(entry) => JsonRpcResponse::success(
                    request.id,
                    json!({
                        "status": "onboarded",
                        "server": {
                            "server": entry.server_id,
                            "capabilities": entry.capabilities,
                            "trust_level": format!("{:?}", entry.trust_level).to_ascii_lowercase(),
                            "health": format!("{:?}", entry.health).to_ascii_lowercase(),
                        }
                    }),
                ),
                Err(AgentError::InvalidInput(message)) => {
                    JsonRpcResponse::error(request.id, -32602, message)
                }
                Err(err) => JsonRpcResponse::error(
                    request.id,
                    -32027,
                    format!("onboard mcp server failed: {err}"),
                ),
            }
        }
        "OrchestrateTask" | "management.OrchestrateTask" => {
            let params = match serde_json::from_value::<OrchestrateTaskParams>(request.params) {
                Ok(params) => params,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid orchestrate task params: {err}"),
                    )
                }
            };

            let orchestrated = match orchestrate_task_with_delegate(
                &state,
                store.as_ref(),
                params,
                |agent_id, child_input, child_index, attempt| {
                    Ok(json!({
                        "agent_id": agent_id,
                        "child_index": child_index,
                        "attempt": attempt,
                        "result": child_input,
                    }))
                },
            )
            .await
            {
                Ok(result) => result,
                Err(AgentError::InvalidInput(message)) => {
                    return JsonRpcResponse::error(request.id, -32602, message)
                }
                Err(AgentError::NotFound(message)) => {
                    return JsonRpcResponse::error(request.id, -32010, message)
                }
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32024,
                        format!("orchestrate task failed: {err}"),
                    )
                }
            };

            JsonRpcResponse::success(request.id, json!(orchestrated))
        }
        "MigrateContext" | "management.MigrateContext" => {
            let params = match serde_json::from_value::<MigrateContextRpcParams>(request.params) {
                Ok(params) => params,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid migrate context params: {err}"),
                    )
                }
            };

            let migration = migrate_context_to_target(
                store.as_ref(),
                MigrateContextParams {
                    source_agent_id: params.source_agent_id,
                    target_base_url: params.target_base_url,
                    target_agent_id: params.target_agent_id,
                    session_id: params.session_id,
                    key_files: params.key_files,
                    messages: params.messages,
                    head_id: params.head_id,
                    tool_results_cache: params.tool_results_cache,
                    working_directory: params.working_directory,
                    include_snapshot: params.include_snapshot,
                },
            )
            .await;

            match migration {
                Ok(result) => JsonRpcResponse::success(request.id, result),
                Err(AgentError::InvalidInput(message)) => {
                    JsonRpcResponse::error(request.id, -32602, message)
                }
                Err(AgentError::NotFound(message)) => {
                    JsonRpcResponse::error(request.id, -32010, message)
                }
                Err(err) => JsonRpcResponse::error(
                    request.id,
                    -32026,
                    format!("context migration failed: {err}"),
                ),
            }
        }
        "StartManagedAgent" | "management.StartManagedAgent" => {
            let params = match serde_json::from_value::<StartManagedAgentParams>(request.params) {
                Ok(params) => params,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid start managed agent params: {err}"),
                    )
                }
            };

            let agent_id = match uuid::Uuid::parse_str(&params.agent_id) {
                Ok(agent_id) => agent_id,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid agent_id: {err}"),
                    )
                }
            };

            let profile = match store.get_agent(agent_id).await {
                Ok(profile) => profile,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32010,
                        format!("query agent for managed lifecycle failed: {err}"),
                    )
                }
            };

            let trust_level = match resolve_trust_level(&profile, params.trust_level.as_deref()) {
                Ok(level) => level,
                Err(AgentError::InvalidInput(message)) => {
                    return JsonRpcResponse::error(request.id, -32602, message);
                }
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32017,
                        format!("resolve trust level failed: {err}"),
                    );
                }
            };

            let network_policy =
                match parse_network_isolation_policy(trust_level, params.network_policy.as_deref())
                {
                    Ok(policy) => policy,
                    Err(AgentError::InvalidInput(message)) => {
                        return JsonRpcResponse::error(request.id, -32602, message);
                    }
                    Err(err) => {
                        return JsonRpcResponse::error(
                            request.id,
                            -32017,
                            format!("resolve network policy failed: {err}"),
                        );
                    }
                };

            let runtime = match trust_level {
                TrustLevel::Builtin | TrustLevel::Verified => ManagedRuntimeSpec::Cgroup,
                TrustLevel::Community | TrustLevel::Untrusted => {
                    let Some(executor) = state.firecracker_executor() else {
                        return JsonRpcResponse::error(
                            request.id,
                            -32017,
                            "firecracker runtime requested but executor is not configured",
                        );
                    };

                    ManagedRuntimeSpec::Firecracker(FirecrackerRuntimeSpec {
                        executor,
                        vcpu_count: None,
                        mem_size_mib: None,
                        network: None,
                        network_policy: Some(network_policy),
                        jailer: Some(firecracker::JailerConfig::default()),
                        launch_timeout: Duration::from_secs(3),
                    })
                }
            };

            let limits = CgroupResourceLimits {
                cpu_weight: params.cpu_weight.unwrap_or(100),
                memory_high: params.memory_high.unwrap_or_else(|| "256M".to_string()),
                memory_max: params.memory_max.unwrap_or_else(|| "512M".to_string()),
            };
            let lifecycle_spec = ManagedAgentSpec {
                agent_id,
                command: params.command,
                args: params.args,
                env: params.env,
                restart_max_attempts: params.restart_max_attempts.unwrap_or(3),
                restart_backoff_secs: params.restart_backoff_secs.unwrap_or(1),
                limits,
                runtime,
            };

            match state.lifecycle().start_agent(lifecycle_spec).await {
                Ok(snapshot) => {
                    record_audit_event(
                        &store,
                        &audit_context,
                        agent_id,
                        EventType::AgentStarted,
                        EventResult::Success,
                        EventPayload {
                            tool_name: None,
                            message: Some("managed lifecycle start".to_string()),
                            metadata: json!({
                                "pid": snapshot.pid,
                                "restart_count": snapshot.restart_count,
                                "cgroup_path": snapshot.cgroup_path,
                                "runtime": snapshot.runtime,
                                "trust_level": trust_level.as_str(),
                                "network_policy": params.network_policy,
                            }),
                        },
                    )
                    .await;

                    JsonRpcResponse::success(request.id, json!(snapshot))
                }
                Err(err) => {
                    record_audit_event(
                        &store,
                        &audit_context,
                        agent_id,
                        EventType::Error,
                        EventResult::Failure,
                        EventPayload {
                            tool_name: None,
                            message: Some("managed lifecycle start failed".to_string()),
                            metadata: json!({
                                "error": err.to_string(),
                            }),
                        },
                    )
                    .await;

                    JsonRpcResponse::error(
                        request.id,
                        -32017,
                        format!("start managed agent failed: {err}"),
                    )
                }
            }
        }
        "StopManagedAgent" | "management.StopManagedAgent" => {
            let params = match serde_json::from_value::<StopManagedAgentParams>(request.params) {
                Ok(params) => params,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid stop managed agent params: {err}"),
                    )
                }
            };

            let agent_id = match uuid::Uuid::parse_str(&params.agent_id) {
                Ok(agent_id) => agent_id,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid agent_id: {err}"),
                    )
                }
            };

            if let Err(err) = store.get_agent(agent_id).await {
                return JsonRpcResponse::error(
                    request.id,
                    -32010,
                    format!("query agent for managed lifecycle failed: {err}"),
                );
            }

            match state.lifecycle().stop_agent(agent_id).await {
                Ok(snapshot) => {
                    record_audit_event(
                        &store,
                        &audit_context,
                        agent_id,
                        EventType::AgentStopped,
                        EventResult::Success,
                        EventPayload {
                            tool_name: None,
                            message: Some("managed lifecycle stop".to_string()),
                            metadata: json!({
                                "restart_count": snapshot.restart_count,
                                "state": snapshot.state,
                            }),
                        },
                    )
                    .await;

                    JsonRpcResponse::success(request.id, json!(snapshot))
                }
                Err(AgentError::NotFound(message)) => {
                    record_audit_event(
                        &store,
                        &audit_context,
                        agent_id,
                        EventType::Error,
                        EventResult::Failure,
                        EventPayload {
                            tool_name: None,
                            message: Some("managed lifecycle stop failed".to_string()),
                            metadata: json!({"reason": message}),
                        },
                    )
                    .await;
                    JsonRpcResponse::error(request.id, -32018, message)
                }
                Err(err) => {
                    record_audit_event(
                        &store,
                        &audit_context,
                        agent_id,
                        EventType::Error,
                        EventResult::Failure,
                        EventPayload {
                            tool_name: None,
                            message: Some("managed lifecycle stop failed".to_string()),
                            metadata: json!({"error": err.to_string()}),
                        },
                    )
                    .await;
                    JsonRpcResponse::error(
                        request.id,
                        -32018,
                        format!("stop managed agent failed: {err}"),
                    )
                }
            }
        }
        "ListManagedAgents" | "management.ListManagedAgents" => {
            let snapshots = state.lifecycle().list_agents().await;
            JsonRpcResponse::success(
                request.id,
                json!({
                    "agents": snapshots,
                }),
            )
        }
        "ListLifecycleEvents" | "management.ListLifecycleEvents" => {
            let params = serde_json::from_value::<ListLifecycleEventsParams>(request.params)
                .unwrap_or(ListLifecycleEventsParams { limit: None });
            let events = state.lifecycle().list_events(params.limit).await;
            JsonRpcResponse::success(
                request.id,
                json!({
                    "events": events,
                }),
            )
        }
        "SubscribeEvents" | "management.SubscribeEvents" => {
            let params = serde_json::from_value::<SubscribeEventsParams>(request.params).unwrap_or(
                SubscribeEventsParams {
                    cursor: None,
                    limit: Some(100),
                    wait_timeout_secs: Some(5),
                },
            );
            let wait_timeout_secs = params.wait_timeout_secs.unwrap_or(5);

            let mut events = state
                .lifecycle()
                .list_events_since(params.cursor.as_deref(), params.limit)
                .await;

            if events.is_empty() && wait_timeout_secs > 0 {
                let mut subscription = state.lifecycle().subscribe_events();
                let wait_result = tokio::time::timeout(
                    Duration::from_secs(wait_timeout_secs),
                    subscription.recv(),
                )
                .await;

                match wait_result {
                    Ok(Ok(_)) => {
                        events = state
                            .lifecycle()
                            .list_events_since(params.cursor.as_deref(), params.limit)
                            .await;
                    }
                    Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {
                        events = state
                            .lifecycle()
                            .list_events_since(None, params.limit)
                            .await;
                    }
                    Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) | Err(_) => {}
                }
            }

            let next_cursor = events.last().map(|event| event.event_id.clone());
            JsonRpcResponse::success(
                request.id,
                json!({
                    "events": events,
                    "next_cursor": next_cursor,
                }),
            )
        }
        "ListAuditEvents" | "management.ListAuditEvents" => {
            let params = match serde_json::from_value::<ListAuditEventsParams>(request.params) {
                Ok(params) => params,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid list audit events params: {err}"),
                    )
                }
            };
            let agent_id = match uuid::Uuid::parse_str(&params.agent_id) {
                Ok(agent_id) => agent_id,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid agent_id: {err}"),
                    )
                }
            };

            match store.get_audit_events(agent_id).await {
                Ok(all_events) => {
                    let limit = params
                        .limit
                        .unwrap_or(all_events.len())
                        .min(all_events.len());
                    let events = all_events.into_iter().take(limit).collect::<Vec<_>>();
                    JsonRpcResponse::success(request.id, json!({"events": events}))
                }
                Err(err) => JsonRpcResponse::error(
                    request.id,
                    -32019,
                    format!("list audit events failed: {err}"),
                ),
            }
        }
        "ListApprovalQueue" | "management.ListApprovalQueue" => {
            let params = match serde_json::from_value::<ListApprovalQueueParams>(request.params) {
                Ok(params) => params,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid list approval queue params: {err}"),
                    )
                }
            };

            let agent_id = match uuid::Uuid::parse_str(&params.agent_id) {
                Ok(agent_id) => agent_id,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid agent_id: {err}"),
                    )
                }
            };

            match store.get_audit_events(agent_id).await {
                Ok(events) => {
                    let approvals = pending_approval_items(&events);
                    JsonRpcResponse::success(
                        request.id,
                        json!({
                            "agent_id": params.agent_id,
                            "approvals": approvals,
                        }),
                    )
                }
                Err(err) => JsonRpcResponse::error(
                    request.id,
                    -32019,
                    format!("list approval queue failed: {err}"),
                ),
            }
        }
        "ResolveApproval" | "management.ResolveApproval" => {
            let params = match serde_json::from_value::<ResolveApprovalParams>(request.params) {
                Ok(params) => params,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid resolve approval params: {err}"),
                    )
                }
            };

            let agent_id = match uuid::Uuid::parse_str(&params.agent_id) {
                Ok(agent_id) => agent_id,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid agent_id: {err}"),
                    )
                }
            };

            let decision = params.decision.trim().to_ascii_lowercase();
            if !matches!(decision.as_str(), "approve" | "deny") {
                return JsonRpcResponse::error(
                    request.id,
                    -32602,
                    "decision must be approve or deny",
                );
            }

            let events = match store.get_audit_events(agent_id).await {
                Ok(events) => events,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32019,
                        format!("resolve approval query failed: {err}"),
                    )
                }
            };

            let already_resolved = events.iter().any(|event| {
                is_approval_resolution_event(event)
                    && event
                        .payload
                        .metadata
                        .get("approval_id")
                        .and_then(Value::as_str)
                        == Some(params.approval_id.as_str())
            });
            if already_resolved {
                return JsonRpcResponse::error(
                    request.id,
                    -32025,
                    format!("approval already resolved: {}", params.approval_id),
                );
            }

            let pending = events
                .iter()
                .find(|event| {
                    event.id.to_string() == params.approval_id && is_pending_approval_event(event)
                })
                .cloned();

            let Some(pending) = pending else {
                return JsonRpcResponse::error(
                    request.id,
                    -32024,
                    format!("approval not found: {}", params.approval_id),
                );
            };

            let (event_type, result, message) = match decision.as_str() {
                "approve" => (
                    EventType::ToolApproved,
                    EventResult::Success,
                    "approval.approve",
                ),
                "deny" => (EventType::ToolDenied, EventResult::Failure, "approval.deny"),
                _ => unreachable!("decision validated above"),
            };

            record_audit_event(
                &store,
                &audit_context,
                agent_id,
                event_type,
                result,
                EventPayload {
                    tool_name: pending.payload.tool_name.clone(),
                    message: Some(message.to_string()),
                    metadata: json!({
                        "approval_id": params.approval_id,
                        "requested_trace_id": pending.trace_id,
                        "requested_event_id": pending.id,
                        "requested_reason": pending.payload.metadata.get("reason").cloned().unwrap_or(json!("policy.ask")),
                    }),
                },
            )
            .await;

            JsonRpcResponse::success(
                request.id,
                json!({
                    "agent_id": agent_id,
                    "approval_id": params.approval_id,
                    "decision": decision,
                    "resolved": true,
                }),
            )
        }
        "GetAgent" | "management.GetAgent" => {
            let params = match serde_json::from_value::<GetAgentParams>(request.params) {
                Ok(params) => params,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid get agent params: {err}"),
                    )
                }
            };

            let agent_id = match uuid::Uuid::parse_str(&params.agent_id) {
                Ok(agent_id) => agent_id,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid agent_id: {err}"),
                    )
                }
            };

            let profile = match store.get_agent(agent_id).await {
                Ok(profile) => profile,
                Err(AgentError::NotFound(message)) => {
                    return JsonRpcResponse::error(request.id, -32010, message)
                }
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32010,
                        format!("get agent failed: {err}"),
                    )
                }
            };

            let audit_events = match store.get_audit_events(agent_id).await {
                Ok(mut events) => {
                    let limit = params.audit_limit.unwrap_or(10).min(events.len());
                    events.truncate(limit);
                    events
                }
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32019,
                        format!("get agent audit summary failed: {err}"),
                    )
                }
            };

            JsonRpcResponse::success(
                request.id,
                json!({
                    "profile": profile,
                    "audit_events": audit_events,
                }),
            )
        }
        "DeleteAgent" | "management.DeleteAgent" => {
            let params = match serde_json::from_value::<DeleteAgentParams>(request.params) {
                Ok(params) => params,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid delete agent params: {err}"),
                    )
                }
            };

            let agent_id = match uuid::Uuid::parse_str(&params.agent_id) {
                Ok(agent_id) => agent_id,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid agent_id: {err}"),
                    )
                }
            };

            if let Err(err) = store.get_agent(agent_id).await {
                return match err {
                    AgentError::NotFound(message) => {
                        JsonRpcResponse::error(request.id, -32010, message)
                    }
                    _ => JsonRpcResponse::error(
                        request.id,
                        -32010,
                        format!("query agent before delete failed: {err}"),
                    ),
                };
            }

            let managed_agents = state.lifecycle().list_agents().await;
            if managed_agents
                .iter()
                .any(|agent| agent.agent_id == agent_id)
            {
                return JsonRpcResponse::error(
                    request.id,
                    -32020,
                    format!(
                        "cannot delete running managed agent: {} (stop it first)",
                        agent_id
                    ),
                );
            }

            record_audit_event(
                &store,
                &audit_context,
                agent_id,
                EventType::AgentStopped,
                EventResult::Success,
                EventPayload {
                    tool_name: None,
                    message: Some("agent deleted".to_string()),
                    metadata: json!({
                        "action": "delete_agent",
                    }),
                },
            )
            .await;

            match store.delete_agent(agent_id).await {
                Ok(()) => JsonRpcResponse::success(
                    request.id,
                    json!({
                        "success": true,
                        "agent_id": agent_id,
                    }),
                ),
                Err(AgentError::NotFound(message)) => {
                    JsonRpcResponse::error(request.id, -32010, message)
                }
                Err(err) => JsonRpcResponse::error(
                    request.id,
                    -32011,
                    format!("delete agent failed: {err}"),
                ),
            }
        }
        "ListAgents" | "management.ListAgents" => match store.list_agents().await {
            Ok(agents) => JsonRpcResponse::success(
                request.id,
                json!({
                    "agents": agents
                }),
            ),
            Err(err) => {
                JsonRpcResponse::error(request.id, -32010, format!("list agents failed: {err}"))
            }
        },
        "GetUsage" | "management.GetUsage" => {
            let params = match serde_json::from_value::<GetUsageParams>(request.params) {
                Ok(params) => params,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid get usage params: {err}"),
                    )
                }
            };

            let agent_id = match uuid::Uuid::parse_str(&params.agent_id) {
                Ok(agent_id) => agent_id,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid agent_id: {err}"),
                    )
                }
            };

            let usage_result = if let Some(window) = params.window.as_deref() {
                match UsageWindow::parse(window) {
                    Ok(window) => store.get_usage_in_window(agent_id, window).await,
                    Err(err) => {
                        return JsonRpcResponse::error(
                            request.id,
                            -32602,
                            format!("invalid get usage params: {err}"),
                        )
                    }
                }
            } else {
                store.get_usage(agent_id).await
            };

            match usage_result {
                Ok(usage) => JsonRpcResponse::success(request.id, json!(usage)),
                Err(err) => {
                    JsonRpcResponse::error(request.id, -32012, format!("get usage failed: {err}"))
                }
            }
        }
        "RecordUsage" | "management.RecordUsage" => {
            let params = match serde_json::from_value::<RecordUsageParams>(request.params) {
                Ok(params) => params,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid record usage params: {err}"),
                    )
                }
            };

            let provider_request_id = match params.provider_request_id.as_deref() {
                Some(value) if !value.trim().is_empty() => value.trim().to_string(),
                _ => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        "MISSING_PROVIDER_REQUEST_ID",
                    )
                }
            };

            let usage_source = match params.usage_source.as_deref() {
                Some("provider") => "provider".to_string(),
                Some("estimated") => "estimated".to_string(),
                Some(_) => {
                    return JsonRpcResponse::error(request.id, -32602, "INVALID_USAGE_SOURCE")
                }
                None => return JsonRpcResponse::error(request.id, -32602, "MISSING_USAGE_SOURCE"),
            };

            let transport_mode = match params.transport_mode.as_deref() {
                Some("real") => "real".to_string(),
                Some("simulated") => "simulated".to_string(),
                Some(_) => {
                    return JsonRpcResponse::error(request.id, -32602, "INVALID_TRANSPORT_MODE")
                }
                None => {
                    return JsonRpcResponse::error(request.id, -32602, "MISSING_TRANSPORT_MODE")
                }
            };

            let agent_id = match uuid::Uuid::parse_str(&params.agent_id) {
                Ok(agent_id) => agent_id,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid agent_id: {err}"),
                    )
                }
            };

            let profile = match store.get_agent(agent_id).await {
                Ok(profile) => profile,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32010,
                        format!("query agent for usage failed: {err}"),
                    )
                }
            };

            let day = Utc::now().date_naive().format("%Y-%m-%d").to_string();
            let current_day_total = match store.get_daily_total_tokens(agent_id, &day).await {
                Ok(total) => total,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32012,
                        format!("query current day usage failed: {err}"),
                    )
                }
            };

            let delta_input = match i64::try_from(params.input_tokens) {
                Ok(value) => value,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("input_tokens overflow: {err}"),
                    )
                }
            };
            let delta_output = match i64::try_from(params.output_tokens) {
                Ok(value) => value,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("output_tokens overflow: {err}"),
                    )
                }
            };
            let delta_total = match delta_input.checked_add(delta_output) {
                Some(value) => value,
                None => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        "input_tokens + output_tokens overflow",
                    )
                }
            };

            if let Some(limit) = profile.budget.token_limit {
                let limit_i64 = match i64::try_from(limit) {
                    Ok(v) => v,
                    Err(err) => {
                        return JsonRpcResponse::error(
                            request.id,
                            -32603,
                            format!("token budget overflow: {err}"),
                        )
                    }
                };

                if current_day_total.saturating_add(delta_total) > limit_i64 {
                    record_audit_event(
                        &store,
                        &audit_context,
                        agent_id,
                        EventType::BudgetExceeded,
                        EventResult::Failure,
                        EventPayload {
                            tool_name: Some("llm.request".to_string()),
                            message: Some("llm.quota_exceeded".to_string()),
                            metadata: json!({
                                "day": day,
                                "current_day_total": current_day_total,
                                "requested_tokens": delta_total,
                                "token_budget": limit_i64,
                            }),
                        },
                    )
                    .await;
                    return JsonRpcResponse::error(
                        request.id,
                        -32015,
                        format!(
                            "llm.quota_exceeded: daily token budget {} exceeded by requested {} tokens",
                            limit_i64, delta_total
                        ),
                    );
                }
            }

            match store
                .record_usage(
                    agent_id,
                    &params.model_name,
                    delta_input,
                    delta_output,
                    params.cost_usd,
                )
                .await
            {
                Ok(usage) => {
                    record_audit_event(
                        &store,
                        &audit_context,
                        agent_id,
                        EventType::ToolInvoked,
                        EventResult::Success,
                        EventPayload {
                            tool_name: Some("llm.request".to_string()),
                            message: Some("usage recorded".to_string()),
                            metadata: json!({
                                "model_name": params.model_name,
                                "input_tokens": delta_input,
                                "output_tokens": delta_output,
                                "cost_usd": params.cost_usd,
                                "provider_request_id": provider_request_id,
                                "usage_source": usage_source,
                                "transport_mode": transport_mode,
                            }),
                        },
                    )
                    .await;
                    JsonRpcResponse::success(request.id, json!(usage))
                }
                Err(err) => JsonRpcResponse::error(
                    request.id,
                    -32012,
                    format!("record usage failed: {err}"),
                ),
            }
        }
        "CreateAgent" | "management.CreateAgent" => {
            let params = match serde_json::from_value::<CreateAgentParams>(request.params) {
                Ok(params) => params,
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        format!("invalid create params: {err}"),
                    )
                }
            };

            if params.name.trim().is_empty() || params.model.trim().is_empty() {
                return JsonRpcResponse::error(
                    request.id,
                    -32602,
                    "name and model must be non-empty",
                );
            }

            let provider = params.provider.unwrap_or_else(|| "one-api".to_string());
            let mut profile = AgentProfile::new(
                params.name,
                ModelConfig {
                    provider: provider.clone(),
                    model_name: params.model,
                    max_tokens: params.max_tokens,
                    temperature: params.temperature,
                },
            );
            profile.budget.token_limit = params.token_budget;
            if let Some(permission_policy_value) = params.permission_policy.as_deref() {
                match parse_permission_policy(permission_policy_value) {
                    Ok(permission_policy) => {
                        profile.permissions.policy = permission_policy;
                    }
                    Err(err) => {
                        return JsonRpcResponse::error(
                            request.id,
                            -32602,
                            format!("invalid create params: {err}"),
                        );
                    }
                }
            }
            if !params.allowed_tools.is_empty() {
                profile.permissions.allowed_tools = params.allowed_tools.clone();
            }
            if !params.denied_tools.is_empty() {
                profile.permissions.denied_tools = params.denied_tools.clone();
            }

            let idempotency_key =
                format!("{}:{}:{}", profile.name, provider, profile.model.model_name);
            let _guard = state.create_agent_lock.lock().await;

            match store
                .get_agent_by_identity(&profile.name, &provider, &profile.model.model_name)
                .await
            {
                Ok(Some(existing_agent)) => {
                    if existing_agent.status == AgentLifecycleState::Failed {
                        let reason = existing_agent
                            .failure_reason
                            .clone()
                            .unwrap_or_else(|| "unknown failure".to_string());
                        return JsonRpcResponse::error(
                            request.id,
                            -32014,
                            format!("agent provisioning failed: {reason}"),
                        );
                    }

                    let mut result = json!({
                        "agent": existing_agent,
                        "idempotent": true,
                    });

                    match store.get_mapping_by_idempotency_key(&idempotency_key).await {
                        Ok(Some(mapping)) => {
                            if let Some(result_obj) = result.as_object_mut() {
                                result_obj.insert(
                                    "one_api".to_string(),
                                    json!({
                                        "token_id": mapping.one_api_token_id,
                                        "channel_id": mapping.one_api_channel_id,
                                    }),
                                );
                            }
                        }
                        Ok(None) => {}
                        Err(err) => {
                            return JsonRpcResponse::error(
                                request.id,
                                -32011,
                                format!("query idempotent mapping failed: {err}"),
                            );
                        }
                    }

                    record_audit_event(
                        &store,
                        &audit_context,
                        existing_agent.id,
                        EventType::AgentCreated,
                        EventResult::Success,
                        EventPayload {
                            tool_name: None,
                            message: Some("idempotent create agent hit".to_string()),
                            metadata: json!({
                                "idempotent": true,
                                "provider": provider,
                                "model": existing_agent.model.model_name,
                            }),
                        },
                    )
                    .await;

                    let card_path =
                        match persist_agent_card(&state.agent_card_root, &existing_agent) {
                            Ok(path) => path,
                            Err(err) => {
                                return JsonRpcResponse::error(
                                    request.id,
                                    -32018,
                                    format!("persist agent card failed: {err}"),
                                )
                            }
                        };
                    if let Some(result_obj) = result.as_object_mut() {
                        result_obj.insert(
                            "agent_card_path".to_string(),
                            json!(card_path.to_string_lossy().to_string()),
                        );
                    }

                    return JsonRpcResponse::success(request.id, result);
                }
                Ok(None) => {}
                Err(err) => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32011,
                        format!("query idempotent agent failed: {err}"),
                    );
                }
            }

            if let Err(err) = store.create_agent(profile.clone()).await {
                record_audit_event(
                    &store,
                    &audit_context,
                    profile.id,
                    EventType::Error,
                    EventResult::Failure,
                    EventPayload {
                        tool_name: None,
                        message: Some("create agent failed".to_string()),
                        metadata: json!({"error": err.to_string()}),
                    },
                )
                .await;
                return JsonRpcResponse::error(
                    request.id,
                    -32011,
                    format!("create agent failed: {err}"),
                );
            }

            let provisioned = if one_api_config.enabled
                && one_api_config.management_enabled
                && provider == "one-api"
            {
                match provision_one_api(&one_api_config, &profile, &idempotency_key).await {
                    Ok(result) => Some(result),
                    Err(err) => {
                        let reason = format!("one-api provisioning failed: {err}");
                        record_audit_event(
                            &store,
                            &audit_context,
                            profile.id,
                            EventType::Error,
                            EventResult::Failure,
                            EventPayload {
                                tool_name: None,
                                message: Some("one-api provisioning failed".to_string()),
                                metadata: json!({"error": err.to_string()}),
                            },
                        )
                        .await;
                        if let Err(update_err) = store
                            .update_agent_state(
                                profile.id,
                                AgentLifecycleState::Failed,
                                Some(reason.clone()),
                            )
                            .await
                        {
                            return JsonRpcResponse::error(
                                request.id,
                                -32014,
                                format!(
                                    "{reason}; additionally failed to persist failed state: {update_err}"
                                ),
                            );
                        }

                        return JsonRpcResponse::error(request.id, -32014, reason);
                    }
                }
            } else {
                None
            };

            if let Some(one_api) = provisioned.clone() {
                let mapping = OneApiMapping {
                    agent_id: profile.id,
                    idempotency_key,
                    one_api_token_id: one_api.token_id.clone(),
                    one_api_access_token: one_api.access_token,
                    one_api_channel_id: one_api.channel_id.clone(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };

                if let Err(err) = store.save_mapping(mapping).await {
                    let reason = format!("persist one-api mapping failed: {err}");
                    record_audit_event(
                        &store,
                        &audit_context,
                        profile.id,
                        EventType::Error,
                        EventResult::Failure,
                        EventPayload {
                            tool_name: None,
                            message: Some("persist one-api mapping failed".to_string()),
                            metadata: json!({"error": err.to_string()}),
                        },
                    )
                    .await;
                    if let Err(update_err) = store
                        .update_agent_state(
                            profile.id,
                            AgentLifecycleState::Failed,
                            Some(reason.clone()),
                        )
                        .await
                    {
                        return JsonRpcResponse::error(
                            request.id,
                            -32014,
                            format!(
                                "{reason}; additionally failed to persist failed state: {update_err}"
                            ),
                        );
                    }

                    return JsonRpcResponse::error(request.id, -32014, reason);
                }
            }

            match store
                .update_agent_state(profile.id, AgentLifecycleState::Ready, None)
                .await
            {
                Ok(ready_agent) => {
                    let card_path = match persist_agent_card(&state.agent_card_root, &ready_agent) {
                        Ok(path) => path,
                        Err(err) => {
                            return JsonRpcResponse::error(
                                request.id,
                                -32018,
                                format!("persist agent card failed: {err}"),
                            )
                        }
                    };
                    record_audit_event(
                        &store,
                        &audit_context,
                        ready_agent.id,
                        EventType::AgentCreated,
                        EventResult::Success,
                        EventPayload {
                            tool_name: None,
                            message: Some("agent created".to_string()),
                            metadata: json!({
                                "idempotent": false,
                                "provider": ready_agent.model.provider,
                                "model": ready_agent.model.model_name,
                            }),
                        },
                    )
                    .await;

                    if let Some(one_api) = provisioned {
                        JsonRpcResponse::success(
                            request.id,
                            json!({
                                "agent": ready_agent,
                                "idempotent": false,
                                "agent_card_path": card_path.to_string_lossy().to_string(),
                                "one_api": {
                                    "token_id": one_api.token_id,
                                    "channel_id": one_api.channel_id,
                                }
                            }),
                        )
                    } else {
                        JsonRpcResponse::success(
                            request.id,
                            json!({
                                "agent": ready_agent,
                                "idempotent": false,
                                "agent_card_path": card_path.to_string_lossy().to_string()
                            }),
                        )
                    }
                }
                Err(err) => {
                    record_audit_event(
                        &store,
                        &audit_context,
                        profile.id,
                        EventType::Error,
                        EventResult::Failure,
                        EventPayload {
                            tool_name: None,
                            message: Some("mark agent ready failed".to_string()),
                            metadata: json!({"error": err.to_string()}),
                        },
                    )
                    .await;
                    JsonRpcResponse::error(
                        request.id,
                        -32011,
                        format!("mark agent ready failed: {err}"),
                    )
                }
            }
        }
        _ => JsonRpcResponse::error(request.id, -32601, "method not found"),
    }
}

async fn protocol_server(
    socket_path: String,
    store: Arc<SqliteStore>,
    state: RuntimeState,
    one_api_config: OneApiConfig,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if Path::new(&socket_path).exists() {
        std::fs::remove_file(&socket_path)?;
    }

    let listener = UnixListener::bind(&socket_path)?;
    info!(socket_path = %socket_path, "UDS JSON-RPC endpoint listening");

    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    info!(socket_path = %socket_path, "Protocol server received shutdown signal");
                    break;
                }
            }
            accepted = listener.accept() => {
                let (mut stream, _) = accepted?;
                let mut request_bytes = Vec::new();
                stream.read_to_end(&mut request_bytes).await?;

                let request: Result<JsonRpcRequest, _> = serde_json::from_slice(&request_bytes);
                let response = match request {
                    Ok(request) if request.jsonrpc == "2.0" => {
                        handle_rpc_request(request, store.clone(), state.clone(), one_api_config.clone()).await
                    }
                    Ok(request) => JsonRpcResponse::error(request.id, -32600, "invalid jsonrpc version"),
                    Err(err) => {
                        warn!(%err, "Invalid JSON-RPC request payload");
                        JsonRpcResponse::error(json!(null), -32700, "parse error")
                    }
                };

                let payload = serde_json::to_vec(&response)?;
                stream.write_all(&payload).await?;
                let _ = stream.shutdown().await;
            }
        }
    }

    if Path::new(&socket_path).exists() {
        std::fs::remove_file(&socket_path)?;
    }

    Ok(())
}

async fn probe_one_api_health(
    client: &reqwest::Client,
    health_url: &str,
    startup_timeout: Duration,
) -> bool {
    let check_interval = Duration::from_millis(500);
    let started = tokio::time::Instant::now();

    while started.elapsed() < startup_timeout {
        match client.get(health_url).send().await {
            Ok(resp) if resp.status().is_success() => return true,
            Ok(resp) => {
                warn!(status = %resp.status(), health_url, "One-API health probe returned non-success status");
            }
            Err(err) => {
                warn!(%err, health_url, "One-API health probe failed");
            }
        }
        tokio::time::sleep(check_interval).await;
    }

    false
}

fn spawn_one_api(config: &OneApiConfig) -> Result<Child, std::io::Error> {
    let mut command = Command::new(&config.command);
    command
        .args(&config.args)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    command.spawn()
}

async fn stop_one_api_child(child: &mut Child) {
    if let Some(pid) = child.id() {
        info!(pid, "Stopping managed One-API child process");
        if let Err(err) = child.kill().await {
            warn!(%err, pid, "Failed to kill One-API child process");
        }
    }

    if let Err(err) = child.wait().await {
        warn!(%err, "Failed waiting for One-API child process to exit");
    }
}

async fn start_one_api_until_ready(
    config: &OneApiConfig,
    client: &reqwest::Client,
    state: &RuntimeState,
) -> Result<Child, Box<dyn std::error::Error + Send + Sync>> {
    let startup_timeout = Duration::from_secs(config.startup_timeout_secs);
    let mut child = spawn_one_api(config)?;
    let pid = child.id().unwrap_or(0);
    info!(pid, command = %config.command, health_url = %config.health_url, "Started One-API child process");

    if probe_one_api_health(client, &config.health_url, startup_timeout).await {
        state.set_one_api_status("ready").await;
        return Ok(child);
    }

    state.set_one_api_status("degraded").await;
    stop_one_api_child(&mut child).await;
    Err(format!(
        "One-API did not become ready within {} seconds",
        config.startup_timeout_secs
    )
    .into())
}

async fn one_api_supervisor(
    config: OneApiConfig,
    state: RuntimeState,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()?;

    let mut child = start_one_api_until_ready(&config, &client, &state).await?;

    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    info!("One-API supervisor received shutdown signal");
                    state.set_one_api_status("stopping").await;
                    stop_one_api_child(&mut child).await;
                    state.set_one_api_status("stopped").await;
                    break;
                }
            }
            exit_status = child.wait() => {
                match exit_status {
                    Ok(status) => {
                        warn!(%status, "Managed One-API process exited unexpectedly");
                    }
                    Err(err) => {
                        warn!(%err, "Managed One-API process wait failed");
                    }
                }

                state.set_one_api_status("degraded").await;

                let mut recovered = false;
                for attempt in 1..=config.restart_max_attempts {
                    if *shutdown.borrow() {
                        break;
                    }

                    info!(attempt, max_attempts = config.restart_max_attempts, "Attempting One-API restart");
                    tokio::time::sleep(Duration::from_secs(config.restart_backoff_secs)).await;

                    match start_one_api_until_ready(&config, &client, &state).await {
                        Ok(new_child) => {
                            child = new_child;
                            recovered = true;
                            info!(attempt, "One-API restart succeeded");
                            break;
                        }
                        Err(err) => {
                            warn!(%err, attempt, "One-API restart attempt failed");
                        }
                    }
                }

                if !recovered {
                    warn!("One-API restart attempts exhausted; service remains degraded");
                    tokio::time::sleep(Duration::from_secs(config.restart_backoff_secs)).await;
                    match start_one_api_until_ready(&config, &client, &state).await {
                        Ok(new_child) => {
                            child = new_child;
                            info!("One-API recovered after cooldown restart attempt");
                        }
                        Err(err) => {
                            warn!(%err, "One-API cooldown restart attempt failed; continuing supervision loop");
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_max_level(if args.verbose {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .init();

    let mut config = load_config(&args.config)?;
    if let Some(health_host) = args.health_host {
        config.daemon.health_host = health_host;
    }
    if let Some(health_port) = args.health_port {
        config.daemon.health_port = health_port;
    }
    if let Some(db_path) = args.db_path {
        config.daemon.db_path = db_path;
    }
    if let Some(one_api_enabled) = args.one_api_enabled {
        config.one_api.enabled = one_api_enabled;
    }

    let bind_addr: SocketAddr = format!(
        "{}:{}",
        config.daemon.health_host, config.daemon.health_port
    )
    .parse()?;

    info!("Starting agentd daemon");
    info!(config_path = %args.config, "Loaded daemon config");

    let loaded_profiles = load_agent_profiles(Path::new(&config.daemon.agent_profiles_dir))?;
    if loaded_profiles.is_empty() {
        info!("No agent profiles loaded");
    } else {
        let profile_names = loaded_profiles
            .iter()
            .map(|profile| profile.name.clone())
            .collect::<Vec<_>>();
        info!(
            profiles_dir = %config.daemon.agent_profiles_dir,
            count = loaded_profiles.len(),
            names = ?profile_names,
            "Loaded agent profiles"
        );
    }

    let mcp_server_configs = load_mcp_server_configs(Path::new(&config.daemon.mcp_servers_dir))?;
    if mcp_server_configs.is_empty() {
        info!("No MCP server configs loaded");
    } else {
        let mcp_server_names = mcp_server_configs
            .iter()
            .map(|server| server.name.clone())
            .collect::<Vec<_>>();
        info!(
            configs_dir = %config.daemon.mcp_servers_dir,
            count = mcp_server_configs.len(),
            names = ?mcp_server_names,
            "Loaded MCP server configs"
        );
    }

    let store = Arc::new(SqliteStore::new(Path::new(&config.daemon.db_path))?);
    let lifecycle_manager = LifecycleManager::new(CgroupManager::new(
        config.daemon.cgroup_root.clone(),
        config.daemon.cgroup_parent.clone(),
    ));
    let mcp_host = Arc::new(Mutex::new(McpHost::new()));
    {
        let mut host = mcp_host.lock().await;
        host.start_declared_servers(&mcp_server_configs).await?;
        let mcp_health = host.refresh_health()?;
        let available_tools = host.list_available_tools();
        info!(
            total_servers = mcp_health.total,
            healthy_servers = mcp_health.healthy,
            degraded_servers = mcp_health.degraded,
            unreachable_servers = mcp_health.unreachable,
            available_tools = available_tools.len(),
            "MCP host lifecycle initialized"
        );
    }

    let state = RuntimeState::with_lifecycle_and_agent_card_root_and_mcp(
        if config.one_api.enabled {
            "starting"
        } else {
            "disabled"
        },
        lifecycle_manager,
        PathBuf::from(config.daemon.agent_card_root.clone()),
        mcp_host.clone(),
    );

    let health_listener = TcpListener::bind(bind_addr).await?;
    let mdns_daemon = match start_mdns_advertisement(bind_addr) {
        Ok(daemon) => {
            info!(%bind_addr, service_type = AGENTD_MDNS_SERVICE_TYPE, "mDNS advertisement registered");
            Some(daemon)
        }
        Err(err) => {
            warn!(%err, %bind_addr, "mDNS advertisement disabled due to setup error");
            None
        }
    };

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let health_task = tokio::spawn(health_server(
        health_listener,
        bind_addr,
        store.clone(),
        state.clone(),
        config.one_api.clone(),
        shutdown_rx,
    ));
    let protocol_task = tokio::spawn(protocol_server(
        config.daemon.socket_path.clone(),
        store,
        state.clone(),
        config.one_api.clone(),
        shutdown_tx.subscribe(),
    ));

    let one_api_task = if config.one_api.enabled {
        Some(tokio::spawn(one_api_supervisor(
            config.one_api.clone(),
            state,
            shutdown_tx.subscribe(),
        )))
    } else {
        info!("One-API supervisor disabled by configuration");
        None
    };

    notify_systemd("READY=1");
    info!("Systemd READY=1 notification sent");

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT, initiating graceful shutdown");
        }
        _ = sigterm.recv() => {
            info!("Received SIGTERM, initiating graceful shutdown");
        }
    }

    let _ = shutdown_tx.send(true);

    let timeout_secs = config.daemon.shutdown_timeout_secs;
    let health_shutdown_result = timeout(Duration::from_secs(timeout_secs), health_task).await;
    match health_shutdown_result {
        Ok(join_result) => match join_result {
            Ok(Ok(())) => {
                info!(timeout_secs, "Health server shut down gracefully");
            }
            Ok(Err(err)) => {
                error!(%err, timeout_secs, "Health server exited with error during shutdown");
            }
            Err(err) => {
                error!(%err, timeout_secs, "Health server task join failed during shutdown");
            }
        },
        Err(_) => {
            warn!(timeout_secs, "Health server graceful shutdown timed out");
        }
    }

    let protocol_shutdown_result = timeout(Duration::from_secs(timeout_secs), protocol_task).await;
    match protocol_shutdown_result {
        Ok(join_result) => match join_result {
            Ok(Ok(())) => {
                info!(timeout_secs, "Protocol server shut down gracefully");
            }
            Ok(Err(err)) => {
                error!(%err, timeout_secs, "Protocol server exited with error during shutdown");
            }
            Err(err) => {
                error!(%err, timeout_secs, "Protocol server task join failed during shutdown");
            }
        },
        Err(_) => {
            warn!(timeout_secs, "Protocol server graceful shutdown timed out");
        }
    }

    if let Some(task) = one_api_task {
        let one_api_shutdown_result = timeout(Duration::from_secs(timeout_secs), task).await;
        match one_api_shutdown_result {
            Ok(join_result) => match join_result {
                Ok(Ok(())) => {
                    info!(timeout_secs, "One-API supervisor shut down gracefully");
                }
                Ok(Err(err)) => {
                    error!(%err, timeout_secs, "One-API supervisor exited with error during shutdown");
                }
                Err(err) => {
                    error!(%err, timeout_secs, "One-API supervisor task join failed during shutdown");
                }
            },
            Err(_) => {
                warn!(
                    timeout_secs,
                    "One-API supervisor graceful shutdown timed out"
                );
            }
        }
    }

    let mut host = mcp_host.lock().await;
    match host.stop_all().await {
        Ok(()) => {
            info!("MCP host servers shut down gracefully");
        }
        Err(err) => {
            warn!(%err, "MCP host shutdown encountered errors");
        }
    }

    if let Some(daemon) = mdns_daemon {
        if let Err(err) = daemon.shutdown() {
            warn!(%err, "mDNS daemon shutdown encountered errors");
        } else {
            info!(
                service_type = AGENTD_MDNS_SERVICE_TYPE,
                "mDNS daemon shut down"
            );
        }
    }

    notify_systemd("STOPPING=1");
    info!("Daemon shutdown sequence finished");

    Ok(())
}
