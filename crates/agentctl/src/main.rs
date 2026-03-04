use agentd_protocol::{JsonRpcRequest, JsonRpcResponse};
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
    Agent {
        #[command(subcommand)]
        command: Box<AgentCommands>,
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
    mut shell_runner: impl FnMut() -> Result<(), DynError>,
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
        Commands::Agent { command } => match *command {
            AgentCommands::List { json } => {
                info!(socket_path = %cli.socket_path, "Calling ListAgents over UDS JSON-RPC");
                let response = call_rpc(&cli.socket_path, "ListAgents", json!({})).await?;
                print_response(response, json)?;
            }
            AgentCommands::Shell => {
                info!("Launching interactive agent shell TUI");
                shell_runner()?;
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

    run_cli(cli, || tui::run()).await
}

#[cfg(test)]
#[tokio::test]
async fn shell_command_routes_to_tui() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let cli = Cli::try_parse_from(["agentctl", "agent", "shell"]).expect("cli args should parse");
    let shell_called = Arc::new(AtomicBool::new(false));
    let shell_called_for_closure = Arc::clone(&shell_called);

    let result = run_cli(cli, move || {
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
