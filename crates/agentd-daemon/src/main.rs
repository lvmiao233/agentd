use agentd_core::profile::{ModelConfig, PermissionPolicy};
use agentd_core::{
    AgentError, AgentLifecycleState, AgentProfile, PolicyDecision, PolicyLayer, PolicyRule,
    SessionPolicyOverrides,
};
use agentd_protocol::{JsonRpcRequest, JsonRpcResponse};
use agentd_store::{AgentStore, OneApiMapping, SqliteStore};
use chrono::Utc;
use clap::Parser;
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::os::unix::net::UnixDatagram;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UnixListener};
use tokio::process::{Child, Command};
use tokio::sync::{watch, Mutex, RwLock};
use tokio::time::{timeout, Duration};
use tracing::{error, info, warn};

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

#[derive(Debug, Clone)]
struct RuntimeState {
    one_api_status: Arc<RwLock<String>>,
    create_agent_lock: Arc<Mutex<()>>,
}

impl RuntimeState {
    fn new(initial_status: &str) -> Self {
        Self {
            one_api_status: Arc::new(RwLock::new(initial_status.to_string())),
            create_agent_lock: Arc::new(Mutex::new(())),
        }
    }

    async fn set_one_api_status(&self, status: &str) {
        let mut guard = self.one_api_status.write().await;
        *guard = status.to_string();
    }

    async fn one_api_status(&self) -> String {
        self.one_api_status.read().await.clone()
    }
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

async fn health_server(
    listener: TcpListener,
    bind_addr: SocketAddr,
    store: Arc<SqliteStore>,
    state: RuntimeState,
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

                let mut buf = [0_u8; 1024];
                let read = stream.read(&mut buf).await?;
                let request = String::from_utf8_lossy(&buf[..read]);
                let is_health = request.starts_with("GET /health ") || request.starts_with("GET /health?");

                let response = if is_health {
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

                    let body = serde_json::to_string(&json!({
                        "status": overall_status,
                        "subsystems": {
                            "daemon": "ready",
                            "protocol": "ready",
                            "storage": storage_status,
                            "one_api": one_api_status,
                        }
                    }))?;

                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                } else {
                    let body = "{\"error\":\"not found\"}";
                    format!(
                        "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                };

                stream.write_all(response.as_bytes()).await?;
                let _ = stream.shutdown().await;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

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

    fn record_usage_request(
        agent_id: &str,
        model_name: &str,
        input_tokens: u64,
        output_tokens: u64,
        cost_usd: f64,
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
            record_usage_request(&created_agent_id, "claude-4-sonnet", 60, 30, 0.15),
            store.clone(),
            state.clone(),
            OneApiConfig::default(),
        )
        .await;
        assert!(
            record_ok.error.is_none(),
            "usage record under budget should succeed: {record_ok:?}"
        );

        let over_budget = handle_rpc_request(
            record_usage_request(&created_agent_id, "claude-4-sonnet", 20, 5, 0.05),
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
            store,
            state,
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
}

#[derive(Debug, Deserialize)]
struct GetUsageParams {
    agent_id: String,
}

#[derive(Debug, Deserialize)]
struct RecordUsageParams {
    agent_id: String,
    model_name: String,
    input_tokens: u64,
    output_tokens: u64,
    #[serde(default)]
    cost_usd: f64,
}

#[derive(Debug, Deserialize)]
struct PolicyRuleInput {
    pattern: String,
    decision: PolicyDecision,
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

async fn handle_rpc_request(
    request: JsonRpcRequest,
    store: Arc<SqliteStore>,
    state: RuntimeState,
    one_api_config: OneApiConfig,
) -> JsonRpcResponse {
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

            if let Some(agent_id) = params.agent_id {
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

            let evaluation = PolicyLayer::evaluate_tool(
                &global_layer,
                &profile_layer,
                &session_layer,
                &params.tool,
            );

            if evaluation.decision == PolicyDecision::Deny {
                return JsonRpcResponse::error(
                    request.id,
                    -32016,
                    format!(
                        "policy.deny: tool={} matched_rule={} source_layer={}",
                        evaluation.tool,
                        evaluation
                            .matched_rule
                            .clone()
                            .unwrap_or_else(|| "<none>".to_string()),
                        evaluation
                            .source_layer
                            .clone()
                            .unwrap_or_else(|| "<none>".to_string())
                    ),
                );
            }

            JsonRpcResponse::success(request.id, json!(evaluation))
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

            match store.get_usage(agent_id).await {
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
                Ok(usage) => JsonRpcResponse::success(request.id, json!(usage)),
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
                    if let Some(one_api) = provisioned {
                        JsonRpcResponse::success(
                            request.id,
                            json!({
                                "agent": ready_agent,
                                "idempotent": false,
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
                                "idempotent": false
                            }),
                        )
                    }
                }
                Err(err) => JsonRpcResponse::error(
                    request.id,
                    -32011,
                    format!("mark agent ready failed: {err}"),
                ),
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

    let store = Arc::new(SqliteStore::new(Path::new(&config.daemon.db_path))?);
    let state = RuntimeState::new(if config.one_api.enabled {
        "starting"
    } else {
        "disabled"
    });

    let health_listener = TcpListener::bind(bind_addr).await?;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let health_task = tokio::spawn(health_server(
        health_listener,
        bind_addr,
        store.clone(),
        state.clone(),
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

    notify_systemd("STOPPING=1");
    info!("Daemon shutdown sequence finished");

    Ok(())
}
