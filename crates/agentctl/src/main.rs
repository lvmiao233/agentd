use agentd_protocol::{JsonRpcRequest, JsonRpcResponse};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::{sleep, Duration};
use tracing::info;

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
        command: AgentCommands,
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
        json: bool,
    },
    Run {
        #[arg(long)]
        agent_id: String,
        #[arg(long)]
        command: String,
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
) -> Result<JsonRpcResponse, Box<dyn std::error::Error>> {
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

fn print_response(
    response: JsonRpcResponse,
    as_json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
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

fn print_event(event: &Value, as_json: bool) -> Result<(), Box<dyn std::error::Error>> {
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
) -> Result<(), Box<dyn std::error::Error>> {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

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
        Commands::Agent { command } => match command {
            AgentCommands::List { json } => {
                info!(socket_path = %cli.socket_path, "Calling ListAgents over UDS JSON-RPC");
                let response = call_rpc(&cli.socket_path, "ListAgents", json!({})).await?;
                print_response(response, json)?;
            }
            AgentCommands::Create {
                name,
                model,
                provider,
                token_budget,
                max_tokens,
                temperature,
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
                    }),
                )
                .await?;
                print_response(response, json)?;
            }
            AgentCommands::Run {
                agent_id,
                command,
                args,
                restart_max_attempts,
                restart_backoff_secs,
                cpu_weight,
                memory_high,
                memory_max,
                json,
            } => {
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
