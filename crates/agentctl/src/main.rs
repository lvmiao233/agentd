use agentd_protocol::{JsonRpcRequest, JsonRpcResponse};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
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
        },
    }

    Ok(())
}
