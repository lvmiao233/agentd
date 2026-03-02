use agentd_core::profile::ModelConfig;
use agentd_core::AgentProfile;
use agentd_protocol::{JsonRpcRequest, JsonRpcResponse};
use agentd_store::{AgentStore, SqliteStore};
use clap::Parser;
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;
use std::os::unix::net::UnixDatagram;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UnixListener};
use tokio::process::{Child, Command};
use tokio::sync::{watch, RwLock};
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

#[derive(Debug, Clone, Deserialize)]
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

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            daemon: DaemonConfig::default(),
            one_api: OneApiConfig::default(),
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

#[derive(Debug, Clone)]
struct RuntimeState {
    one_api_status: Arc<RwLock<String>>,
}

impl RuntimeState {
    fn new(initial_status: &str) -> Self {
        Self {
            one_api_status: Arc::new(RwLock::new(initial_status.to_string())),
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
    let target = if socket_path.starts_with('@') {
        format!("\0{}", &socket_path[1..])
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
                        body.as_bytes().len(),
                        body
                    )
                } else {
                    let body = "{\"error\":\"not found\"}";
                    format!(
                        "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.as_bytes().len(),
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

async fn handle_rpc_request(
    request: JsonRpcRequest,
    store: Arc<SqliteStore>,
    state: RuntimeState,
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

            let mut profile = AgentProfile::new(
                params.name,
                ModelConfig {
                    provider: params.provider.unwrap_or_else(|| "one-api".to_string()),
                    model_name: params.model,
                    max_tokens: params.max_tokens,
                    temperature: params.temperature,
                },
            );
            profile.budget.token_limit = params.token_budget;

            match store.create_agent(profile.clone()).await {
                Ok(created) => JsonRpcResponse::success(
                    request.id,
                    json!({
                        "agent": created
                    }),
                ),
                Err(err) => JsonRpcResponse::error(
                    request.id,
                    -32011,
                    format!("create agent failed: {err}"),
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
                        handle_rpc_request(request, store.clone(), state.clone()).await
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
                warn!(timeout_secs, "One-API supervisor graceful shutdown timed out");
            }
        }
    }

    notify_systemd("STOPPING=1");
    info!("Daemon shutdown sequence finished");

    Ok(())
}
