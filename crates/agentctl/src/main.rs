use agentd_protocol::{
    A2ATask, A2ATaskEvent, CreateA2ATaskRequest, JsonRpcRequest, JsonRpcResponse,
};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::{sleep, Duration};
use tracing::info;

mod tui;

type DynError = Box<dyn std::error::Error>;

#[derive(Parser, Debug)]
#[command(name = "agentctl")]
#[command(about = "CLI for agentd daemon")]
struct Cli {
    #[arg(long, default_value = "/tmp/agentd.sock")]
    socket_path: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Health {
        #[arg(long)]
        json: bool,
    },
    Usage {
        agent_id: String,
        #[arg(long)]
        window: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Discover {
        #[arg(long)]
        url: String,
        #[arg(long)]
        registry_url: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Agent {
        #[command(subcommand)]
        command: Box<AgentCommands>,
    },
    A2a {
        #[command(subcommand)]
        command: Box<A2aCommands>,
    },
}

#[derive(Subcommand, Debug)]
enum AgentCommands {
    List {
        #[arg(long)]
        json: bool,
    },
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        model: String,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        token_budget: Option<u64>,
        #[arg(long)]
        max_tokens: Option<u32>,
        #[arg(long)]
        temperature: Option<f32>,
        #[arg(long)]
        permission_policy: Option<String>,
        #[arg(long = "allow-tool")]
        allow_tools: Vec<String>,
        #[arg(long = "deny-tool")]
        deny_tools: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    Shell,
    Inspect {
        #[arg(long)]
        agent_id: String,
        #[arg(long)]
        audit_limit: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    Delete {
        #[arg(long)]
        agent_id: String,
        #[arg(long)]
        json: bool,
    },
    Run {
        #[arg(long)]
        builtin: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value = "claude-4-sonnet")]
        model: String,
        #[arg(long, default_value = "builtin.lite.echo")]
        tool: String,
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        command: Option<String>,
        #[arg(long, value_delimiter = ' ')]
        args: Vec<String>,
        #[arg(long)]
        restart_max_attempts: Option<u32>,
        #[arg(long)]
        restart_backoff_secs: Option<u64>,
        #[arg(long)]
        cpu_weight: Option<u64>,
        #[arg(long)]
        memory_high: Option<String>,
        #[arg(long)]
        memory_max: Option<String>,
        #[arg(value_name = "PROMPT")]
        prompt: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Stop {
        #[arg(long)]
        agent_id: String,
        #[arg(long)]
        json: bool,
    },
    Ps {
        #[arg(long)]
        json: bool,
    },
    Events {
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        follow: bool,
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long, default_value_t = 5)]
        reconnect_delay_secs: u64,
        #[arg(long)]
        json: bool,
    },
    Audit {
        #[arg(long)]
        agent_id: String,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
enum A2aCommands {
    Discover {
        #[arg(long)]
        url: String,
        #[arg(long)]
        json: bool,
    },
    Send {
        #[arg(long)]
        target: String,
        #[arg(long)]
        input: String,
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Status {
        #[arg(long)]
        target: String,
        #[arg(long)]
        task_id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone)]
struct AgentctlA2AClient {
    http: reqwest::Client,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct A2AAgentCard {
    agent_id: String,
    name: String,
    version: String,
    model: String,
    provider: String,
    #[serde(default)]
    capabilities: Value,
}

impl AgentctlA2AClient {
    fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .no_proxy()
                .build()
                .expect("build reqwest client without proxy"),
        }
    }

    async fn discover_agent(&self, base_url: &str) -> Result<A2AAgentCard, DynError> {
        let url = format!("{}/.well-known/agent.json", normalize_base_url(base_url)?);
        let response = self.http.get(url).send().await?;
        let response = response.error_for_status()?;
        Ok(response.json::<A2AAgentCard>().await?)
    }

    async fn create_task(
        &self,
        target: &str,
        payload: CreateA2ATaskRequest,
    ) -> Result<A2ATask, DynError> {
        let url = format!("{}/a2a/tasks", normalize_base_url(target)?);
        let response = self.http.post(url).json(&payload).send().await?;
        let response = response.error_for_status()?;
        let body = response.json::<serde_json::Value>().await?;
        let task = serde_json::from_value::<A2ATask>(body["task"].clone())
            .map_err(|err| format!("invalid create task response: {err}"))?;
        Ok(task)
    }

    async fn get_task(&self, target: &str, task_id: &str) -> Result<A2ATask, DynError> {
        let url = format!(
            "{}/a2a/tasks/{}",
            normalize_base_url(target)?,
            task_id.trim()
        );
        let response = self.http.get(url).send().await?;
        let response = response.error_for_status()?;
        let body = response.json::<serde_json::Value>().await?;
        let task = serde_json::from_value::<A2ATask>(body["task"].clone())
            .map_err(|err| format!("invalid get task response: {err}"))?;
        Ok(task)
    }

    async fn stream_task(
        &self,
        target: &str,
        task_id: &str,
    ) -> Result<Vec<A2ATaskEvent>, DynError> {
        let url = format!(
            "{}/a2a/stream?task_id={}",
            normalize_base_url(target)?,
            task_id.trim()
        );
        let response = self.http.get(url).send().await?;
        let response = response.error_for_status()?;
        let text = response.text().await?;
        let mut events = Vec::new();
        for line in text.lines() {
            if let Some(payload) = line.strip_prefix("data: ") {
                if payload.trim().is_empty() {
                    continue;
                }
                let event = serde_json::from_str::<A2ATaskEvent>(payload)
                    .map_err(|err| format!("invalid stream event payload: {err}"))?;
                events.push(event);
            }
        }
        Ok(events)
    }
}

fn normalize_base_url(input: &str) -> Result<String, DynError> {
    let trimmed = input.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("target url must be non-empty".into());
    }
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(
            format!("target url must start with http:// or https://, got: {trimmed}").into(),
        );
    }
    Ok(trimmed.to_string())
}

async fn fetch_discovery_report(
    daemon_url: &str,
    registry_url: Option<&str>,
) -> Result<Value, DynError> {
    let daemon = normalize_base_url(daemon_url)?;
    let client = reqwest::Client::builder().no_proxy().build()?;
    let mut endpoint = reqwest::Url::parse(&format!("{daemon}/discover"))?;
    if let Some(registry) = registry_url {
        endpoint
            .query_pairs_mut()
            .append_pair("registry_url", &normalize_base_url(registry)?);
    }

    let response = client.get(endpoint).send().await?;
    let response = response.error_for_status()?;
    Ok(response.json::<Value>().await?)
}

async fn call_rpc(
    socket_path: &str,
    method: &str,
    params: Value,
) -> Result<JsonRpcResponse, DynError> {
    let mut stream = UnixStream::connect(socket_path).await?;
    let request = JsonRpcRequest::new(json!(1), method, params);
    let payload = serde_json::to_vec(&request)?;

    stream.write_all(&payload).await?;
    stream.shutdown().await?;

    let mut response_payload = Vec::new();
    stream.read_to_end(&mut response_payload).await?;
    let response: JsonRpcResponse = serde_json::from_slice(&response_payload)?;

    Ok(response)
}

fn rpc_result_or_error(response: JsonRpcResponse) -> Result<Value, DynError> {
    if let Some(error) = response.error {
        return Err(format!("RPC error {}: {}", error.code, error.message).into());
    }
    Ok(response.result.unwrap_or(json!(null)))
}

fn print_response(response: JsonRpcResponse, as_json: bool) -> Result<(), DynError> {
    if let Some(error) = response.error {
        return Err(format!("RPC error {}: {}", error.code, error.message).into());
    }

    let result = response.result.unwrap_or(json!(null));
    if as_json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("{}", result);
    }

    Ok(())
}

fn print_event(event: &Value, as_json: bool) -> Result<(), DynError> {
    if as_json {
        println!("{}", serde_json::to_string(event)?);
    } else {
        println!("{}", serde_json::to_string_pretty(event)?);
    }
    Ok(())
}

async fn follow_events(
    socket_path: &str,
    limit: Option<usize>,
    as_json: bool,
    mut cursor: Option<String>,
    reconnect_delay_secs: u64,
) -> Result<(), DynError> {
    loop {
        let response = call_rpc(
            socket_path,
            "SubscribeEvents",
            json!({
                "cursor": cursor,
                "limit": limit,
                "wait_timeout_secs": 5,
            }),
        )
        .await;

        let response = match response {
            Ok(value) => value,
            Err(err) => {
                eprintln!("event stream reconnecting after transport error: {err}");
                sleep(Duration::from_secs(reconnect_delay_secs)).await;
                continue;
            }
        };

        if let Some(error) = response.error {
            eprintln!(
                "event stream reconnecting after rpc error {}: {}",
                error.code, error.message
            );
            sleep(Duration::from_secs(reconnect_delay_secs)).await;
            continue;
        }

        let result = response.result.unwrap_or(json!({}));
        let events = result
            .get("events")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        for event in &events {
            print_event(event, as_json)?;
        }

        if let Some(next_cursor) = result.get("next_cursor").and_then(Value::as_str) {
            cursor = Some(next_cursor.to_string());
        }
    }
}

async fn run_builtin_lite(
    socket_path: &str,
    request: BuiltinLiteRequest<'_>,
    as_json: bool,
) -> Result<(), DynError> {
    let BuiltinLiteRequest {
        name,
        model,
        tool,
        prompt,
        restart_max_attempts,
        restart_backoff_secs,
        cpu_weight,
        memory_high,
        memory_max,
    } = request;

    let created = call_rpc(
        socket_path,
        "CreateAgent",
        json!({
            "name": name,
            "model": model,
        }),
    )
    .await?;
    let created_result = rpc_result_or_error(created)?;
    let agent_id = created_result
        .get("agent")
        .and_then(|agent| agent.get("id"))
        .and_then(Value::as_str)
        .ok_or("CreateAgent result missing agent.id")?
        .to_string();

    let command = "uv";
    let args = vec![
        "run".to_string(),
        "--project".to_string(),
        "python/agentd-agent-lite".to_string(),
        "agentd-agent-lite".to_string(),
        "--socket-path".to_string(),
        socket_path.to_string(),
        "--agent-id".to_string(),
        agent_id.clone(),
        "--prompt".to_string(),
        prompt.to_string(),
        "--model".to_string(),
        model.to_string(),
        "--tool".to_string(),
        tool.to_string(),
    ];

    let started = call_rpc(
        socket_path,
        "StartManagedAgent",
        json!({
            "agent_id": agent_id,
            "command": command,
            "args": args,
            "restart_max_attempts": restart_max_attempts,
            "restart_backoff_secs": restart_backoff_secs,
            "cpu_weight": cpu_weight,
            "memory_high": memory_high,
            "memory_max": memory_max,
        }),
    )
    .await?;
    let started_result = rpc_result_or_error(started)?;

    let output = json!({
        "builtin": "lite",
        "prompt": prompt,
        "agent": created_result.get("agent").cloned().unwrap_or(json!(null)),
        "managed": started_result,
    });

    if as_json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", output);
    }
    Ok(())
}

struct BuiltinLiteRequest<'a> {
    name: &'a str,
    model: &'a str,
    tool: &'a str,
    prompt: &'a str,
    restart_max_attempts: Option<u32>,
    restart_backoff_secs: Option<u64>,
    cpu_weight: Option<u64>,
    memory_high: Option<String>,
    memory_max: Option<String>,
}

async fn run_cli(
    cli: Cli,
    mut shell_runner: impl FnMut(&str) -> Result<(), DynError>,
) -> Result<(), DynError> {
    match cli.command {
        Commands::Health { json } => {
            info!(socket_path = %cli.socket_path, "Calling GetHealth over UDS JSON-RPC");
            let response = call_rpc(&cli.socket_path, "GetHealth", json!({})).await?;
            print_response(response, json)?;
        }
        Commands::Usage {
            agent_id,
            window,
            json,
        } => {
            info!(socket_path = %cli.socket_path, %agent_id, "Calling GetUsage over UDS JSON-RPC");
            let response = call_rpc(
                &cli.socket_path,
                "GetUsage",
                json!({
                    "agent_id": agent_id,
                    "window": window,
                }),
            )
            .await?;
            print_response(response, json)?;
        }
        Commands::Discover {
            url,
            registry_url,
            json,
        } => {
            let result = fetch_discovery_report(&url, registry_url.as_deref()).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("{}", result);
            }
        }
        Commands::A2a { command } => {
            let client = AgentctlA2AClient::new();
            match *command {
                A2aCommands::Discover { url, json } => {
                    let card = client.discover_agent(&url).await?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&card)?);
                    } else {
                        println!(
                            "agent={} model={} provider={} version={}",
                            card.name, card.model, card.provider, card.version
                        );
                    }
                }
                A2aCommands::Send {
                    target,
                    input,
                    agent_id,
                    json,
                } => {
                    let task = client
                        .create_task(
                            &target,
                            CreateA2ATaskRequest {
                                agent_id: agent_id
                                    .as_deref()
                                    .map(uuid::Uuid::parse_str)
                                    .transpose()
                                    .map_err(|err| format!("invalid --agent-id: {err}"))?,
                                input: json!(input),
                            },
                        )
                        .await?;

                    let status = client.get_task(&target, &task.id.to_string()).await?;
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&json!({
                                "task": task,
                                "status": status,
                            }))?
                        );
                    } else {
                        println!("task_id={} state={:?}", status.id, status.state);
                    }
                }
                A2aCommands::Status {
                    target,
                    task_id,
                    json,
                } => {
                    let task = client.get_task(&target, &task_id).await?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&task)?);
                    } else {
                        println!("task_id={} state={:?}", task.id, task.state);
                    }
                }
            }
        }
        Commands::Agent { command } => match *command {
            AgentCommands::List { json } => {
                info!(socket_path = %cli.socket_path, "Calling ListAgents over UDS JSON-RPC");
                let response = call_rpc(&cli.socket_path, "ListAgents", json!({})).await?;
                print_response(response, json)?;
            }
            AgentCommands::Shell => {
                info!("Launching interactive agent shell TUI");
                shell_runner(&cli.socket_path)?;
            }
            AgentCommands::Create {
                name,
                model,
                provider,
                token_budget,
                max_tokens,
                temperature,
                permission_policy,
                allow_tools,
                deny_tools,
                json,
            } => {
                info!(socket_path = %cli.socket_path, "Calling CreateAgent over UDS JSON-RPC");
                let response = call_rpc(
                    &cli.socket_path,
                    "CreateAgent",
                    json!({
                        "name": name,
                        "model": model,
                        "provider": provider,
                        "token_budget": token_budget,
                        "max_tokens": max_tokens,
                        "temperature": temperature,
                        "permission_policy": permission_policy,
                        "allowed_tools": allow_tools,
                        "denied_tools": deny_tools,
                    }),
                )
                .await?;
                print_response(response, json)?;
            }
            AgentCommands::Inspect {
                agent_id,
                audit_limit,
                json,
            } => {
                info!(socket_path = %cli.socket_path, %agent_id, "Calling GetAgent over UDS JSON-RPC");
                let response = call_rpc(
                    &cli.socket_path,
                    "GetAgent",
                    json!({
                        "agent_id": agent_id,
                        "audit_limit": audit_limit,
                    }),
                )
                .await?;
                print_response(response, json)?;
            }
            AgentCommands::Delete { agent_id, json } => {
                info!(socket_path = %cli.socket_path, %agent_id, "Calling DeleteAgent over UDS JSON-RPC");
                let response = call_rpc(
                    &cli.socket_path,
                    "DeleteAgent",
                    json!({
                        "agent_id": agent_id,
                    }),
                )
                .await?;
                print_response(response, json)?;
            }
            AgentCommands::Run {
                builtin,
                name,
                model,
                tool,
                agent_id,
                command,
                args,
                restart_max_attempts,
                restart_backoff_secs,
                cpu_weight,
                memory_high,
                memory_max,
                prompt,
                json,
            } => {
                if let Some(builtin_name) = builtin {
                    if builtin_name != "lite" {
                        return Err(format!(
                            "unsupported builtin runtime: {builtin_name} (expected: lite)"
                        )
                        .into());
                    }
                    let runtime_name = name
                        .as_deref()
                        .ok_or("--name is required when using --builtin lite")?;
                    let runtime_prompt = prompt
                        .as_deref()
                        .ok_or("prompt positional argument is required for --builtin lite")?;
                    info!(socket_path = %cli.socket_path, runtime_name, "Creating agent and starting builtin lite runtime");
                    run_builtin_lite(
                        &cli.socket_path,
                        BuiltinLiteRequest {
                            name: runtime_name,
                            model: &model,
                            tool: &tool,
                            prompt: runtime_prompt,
                            restart_max_attempts,
                            restart_backoff_secs,
                            cpu_weight,
                            memory_high,
                            memory_max,
                        },
                        json,
                    )
                    .await?;
                } else {
                    let agent_id = agent_id
                        .as_deref()
                        .ok_or("--agent-id is required when --builtin is not set")?;
                    let command = command
                        .as_deref()
                        .ok_or("--command is required when --builtin is not set")?;
                    info!(socket_path = %cli.socket_path, %agent_id, "Calling StartManagedAgent over UDS JSON-RPC");
                    let response = call_rpc(
                        &cli.socket_path,
                        "StartManagedAgent",
                        json!({
                            "agent_id": agent_id,
                            "command": command,
                            "args": args,
                            "restart_max_attempts": restart_max_attempts,
                            "restart_backoff_secs": restart_backoff_secs,
                            "cpu_weight": cpu_weight,
                            "memory_high": memory_high,
                            "memory_max": memory_max,
                        }),
                    )
                    .await?;
                    print_response(response, json)?;
                }
            }
            AgentCommands::Stop { agent_id, json } => {
                info!(socket_path = %cli.socket_path, %agent_id, "Calling StopManagedAgent over UDS JSON-RPC");
                let response = call_rpc(
                    &cli.socket_path,
                    "StopManagedAgent",
                    json!({
                        "agent_id": agent_id,
                    }),
                )
                .await?;
                print_response(response, json)?;
            }
            AgentCommands::Ps { json } => {
                info!(socket_path = %cli.socket_path, "Calling ListManagedAgents over UDS JSON-RPC");
                let response = call_rpc(&cli.socket_path, "ListManagedAgents", json!({})).await?;
                print_response(response, json)?;
            }
            AgentCommands::Events {
                limit,
                follow,
                cursor,
                reconnect_delay_secs,
                json,
            } => {
                if follow {
                    info!(socket_path = %cli.socket_path, "Calling SubscribeEvents over UDS JSON-RPC in follow mode");
                    follow_events(&cli.socket_path, limit, json, cursor, reconnect_delay_secs)
                        .await?;
                } else {
                    info!(socket_path = %cli.socket_path, "Calling ListLifecycleEvents over UDS JSON-RPC");
                    let response = call_rpc(
                        &cli.socket_path,
                        "ListLifecycleEvents",
                        json!({
                            "limit": limit,
                        }),
                    )
                    .await?;
                    print_response(response, json)?;
                }
            }
            AgentCommands::Audit {
                agent_id,
                limit,
                json,
            } => {
                info!(socket_path = %cli.socket_path, %agent_id, "Calling ListAuditEvents over UDS JSON-RPC");
                let response = call_rpc(
                    &cli.socket_path,
                    "ListAuditEvents",
                    json!({
                        "agent_id": agent_id,
                        "limit": limit,
                    }),
                )
                .await?;
                print_response(response, json)?;
            }
        },
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), DynError> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::io::stderr)
        .init();

    run_cli(cli, |socket_path| tui::run(socket_path)).await
}

#[cfg(test)]
#[tokio::test]
async fn shell_command_routes_to_tui() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let cli = Cli::try_parse_from(["agentctl", "agent", "shell"]).expect("cli args should parse");
    let shell_called = Arc::new(AtomicBool::new(false));
    let shell_called_for_closure = Arc::clone(&shell_called);

    let result = run_cli(cli, move |_socket_path| {
        shell_called_for_closure.store(true, Ordering::SeqCst);
        Ok(())
    })
    .await;

    assert!(result.is_ok());
    assert!(shell_called.load(Ordering::SeqCst));
}

#[cfg(test)]
#[test]
fn tui_app_handles_quit_key() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = tui::AgentShellApp::new();
    let should_continue =
        app.handle_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(!should_continue);
}

#[cfg(test)]
#[test]
fn slash_commands_core_set_available() {
    let commands = tui::AgentShellApp::supported_slash_commands();
    for required in [
        "/usage",
        "/events",
        "/tools",
        "/compact",
        "/model",
        "/approve",
        "/deny",
        "/session",
    ] {
        assert!(commands.contains(&required));
    }
}

#[cfg(test)]
#[test]
fn approval_queue_roundtrip() {
    assert!(tui::approval_queue_roundtrip_probe());
}

fn test_json_http_response(status_line: &str, body: &Value) -> Vec<u8> {
    let encoded = serde_json::to_string(body).expect("encode response body");
    format!(
        "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        encoded.len(),
        encoded
    )
    .into_bytes()
}

#[cfg(test)]
#[tokio::test]
async fn a2a_cli_discover_send_status_flow() {
    let task_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now().to_rfc3339();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock a2a listener");
    let addr = listener.local_addr().expect("resolve mock listener addr");

    let server = tokio::spawn(async move {
        for _ in 0..4 {
            let (mut stream, _) = listener.accept().await.expect("accept mock request");
            let mut buf = [0_u8; 8192];
            let read = stream.read(&mut buf).await.expect("read mock request");
            let request = String::from_utf8_lossy(&buf[..read]).to_string();
            let request_line = request.lines().next().unwrap_or_default().to_string();

            let response = if request_line.starts_with("GET /.well-known/agent.json ") {
                test_json_http_response(
                    "200 OK",
                    &json!({
                        "agent_id": task_id.to_string(),
                        "name": "mock-agent",
                        "version": "0.1.0",
                        "model": "claude-4-sonnet",
                        "provider": "builtin",
                        "capabilities": {
                            "protocol": "a2a-compatible"
                        }
                    }),
                )
            } else if request_line.starts_with("POST /a2a/tasks ") {
                test_json_http_response(
                    "201 Created",
                    &json!({
                        "task": {
                            "id": task_id,
                            "agent_id": null,
                            "state": "submitted",
                            "input": "ping",
                            "output": null,
                            "error": null,
                            "created_at": now,
                            "updated_at": now,
                        }
                    }),
                )
            } else if request_line.starts_with(&format!("GET /a2a/tasks/{task_id} ")) {
                test_json_http_response(
                    "200 OK",
                    &json!({
                        "task": {
                            "id": task_id,
                            "agent_id": null,
                            "state": "completed",
                            "input": "ping",
                            "output": {"result": "ok"},
                            "error": null,
                            "created_at": now,
                            "updated_at": now,
                        }
                    }),
                )
            } else {
                test_json_http_response("404 Not Found", &json!({"error": "not found"}))
            };

            stream
                .write_all(&response)
                .await
                .expect("write mock response");
            let _ = stream.shutdown().await;
        }
    });

    let base = format!("http://{addr}");

    let discover = Cli::try_parse_from(["agentctl", "a2a", "discover", "--url", &base, "--json"])
        .expect("discover args should parse");
    run_cli(discover, || Ok(()))
        .await
        .expect("discover should succeed");

    let send = Cli::try_parse_from([
        "agentctl", "a2a", "send", "--target", &base, "--input", "ping", "--json",
    ])
    .expect("send args should parse");
    run_cli(send, || Ok(())).await.expect("send should succeed");

    let status = Cli::try_parse_from([
        "agentctl",
        "a2a",
        "status",
        "--target",
        &base,
        "--task-id",
        &task_id.to_string(),
        "--json",
    ])
    .expect("status args should parse");
    run_cli(status, || Ok(()))
        .await
        .expect("status should succeed");

    server.await.expect("mock server should finish");
}

#[cfg(test)]
#[tokio::test]
async fn a2a_discover_handles_unreachable_remote() {
    let cli = Cli::try_parse_from([
        "agentctl",
        "a2a",
        "discover",
        "--url",
        "http://127.0.0.1:9",
        "--json",
    ])
    .expect("cli args should parse");
    let result = run_cli(cli, || Ok(())).await;
    assert!(
        result.is_err(),
        "discover should fail for unreachable remote"
    );
}

#[cfg(test)]
#[tokio::test]
async fn discover_lists_lan_and_registry_sources() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind discover mock server");
    let addr = listener
        .local_addr()
        .expect("resolve discover mock address");

    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.expect("accept discover request");
            let mut buf = [0_u8; 8192];
            let read = stream.read(&mut buf).await.expect("read discover request");
            let request = String::from_utf8_lossy(&buf[..read]).to_string();
            let request_line = request.lines().next().unwrap_or_default().to_string();
            assert!(request_line.contains("/discover"));

            let response = test_json_http_response(
                "200 OK",
                &json!({
                    "lan": [{
                        "agent_id": "lan-a",
                        "name": "lan-node",
                        "model": "claude-4-sonnet",
                        "provider": "one-api",
                        "endpoint": "http://10.0.0.2:8080",
                        "source": "lan",
                        "health": "ready"
                    }],
                    "registry": [{
                        "agent_id": "reg-b",
                        "name": "registry-node",
                        "model": "claude-4-sonnet",
                        "provider": "one-api",
                        "endpoint": "https://registry.example.com/agents/reg-b",
                        "source": "registry",
                        "health": "ready"
                    }],
                    "errors": []
                }),
            );
            stream
                .write_all(&response)
                .await
                .expect("write discover response");
            let _ = stream.shutdown().await;
        }
    });

    let base = format!("http://{addr}");
    let report = fetch_discovery_report(&base, Some(&base))
        .await
        .expect("fetch discovery report");
    assert_eq!(
        report["lan"][0]["source"],
        json!("lan"),
        "lan source should be present"
    );
    assert_eq!(
        report["registry"][0]["source"],
        json!("registry"),
        "registry source should be present"
    );

    let cli = Cli::try_parse_from([
        "agentctl",
        "discover",
        "--url",
        &base,
        "--registry-url",
        &base,
        "--json",
    ])
    .expect("discover args should parse");
    run_cli(cli, || Ok(()))
        .await
        .expect("discover command should succeed");

    server.await.expect("discover mock server should finish");
}
