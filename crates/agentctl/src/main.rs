use agentd_protocol::{A2ATask, CreateA2ATaskRequest, JsonRpcRequest, JsonRpcResponse};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::{sleep, Duration};
use tracing::info;

mod tui;

type DynError = Box<dyn std::error::Error>;

#[derive(Debug, Clone, serde::Deserialize)]
struct DiscoveryRecord {
    agent_id: String,
    name: String,
    endpoint: String,
}

#[derive(Parser, Debug)]
#[command(name = "agentctl")]
#[command(about = "CLI for agentd daemon")]
struct Cli {
    #[arg(long, default_value = "/tmp/agentd.sock")]
    socket_path: String,

    #[command(subcommand)]
    command: Commands,
}

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand, Debug)]
enum Commands {
    Health {
        #[arg(long)]
        json: bool,
    },
    Migrate {
        #[arg(long)]
        source_agent_id: String,
        #[arg(long)]
        target_base_url: Option<String>,
        #[arg(long)]
        discover_url: Option<String>,
        #[arg(long)]
        registry_url: Option<String>,
        #[arg(long)]
        target_agent_id: Option<String>,
        #[arg(long)]
        target_name: Option<String>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long = "key-file")]
        key_files: Vec<String>,
        #[arg(long = "message")]
        messages: Vec<String>,
        #[arg(long)]
        head_id: Option<String>,
        #[arg(long)]
        tool_cache_json: Option<String>,
        #[arg(long = "working-file")]
        working_files: Vec<String>,
        #[arg(long)]
        include_snapshot: bool,
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
    Shell {
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        model: Option<String>,
    },
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
        task_id: uuid::Uuid,
    ) -> Result<Vec<agentd_protocol::A2ATaskEvent>, DynError> {
        let url = format!(
            "{}/a2a/stream?task_id={}",
            normalize_base_url(target)?,
            task_id
        );
        let response = self.http.get(url).send().await?;
        let response = response.error_for_status()?;
        let body = response.text().await?;

        let mut events = Vec::new();
        for line in body.lines() {
            if let Some(payload) = line.strip_prefix("data: ") {
                if payload.trim().is_empty() {
                    continue;
                }
                events.push(serde_json::from_str(payload)?);
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

fn build_migration_messages(
    specs: &[String],
    explicit_head_id: Option<&str>,
) -> Result<(Vec<Value>, Option<String>), DynError> {
    let mut messages = Vec::with_capacity(specs.len());
    let mut previous_id: Option<String> = None;

    for (index, spec) in specs.iter().enumerate() {
        let (role, content) = spec
            .split_once(':')
            .ok_or_else(|| format!("invalid --message value `{spec}` (expected role:content)"))?;
        let role = role.trim();
        let content = content.trim();
        if role.is_empty() || content.is_empty() {
            return Err(format!(
                "invalid --message value `{spec}` (role/content must be non-empty)"
            )
            .into());
        }

        let message_id = format!("msg-{}", index + 1);
        messages.push(json!({
            "id": message_id,
            "parent_id": previous_id,
            "role": role,
            "content": content,
        }));
        previous_id = Some(format!("msg-{}", index + 1));
    }

    let head_id = explicit_head_id.map(ToString::to_string).or(previous_id);
    Ok((messages, head_id))
}

fn parse_tool_cache_json(raw: Option<&str>) -> Result<Value, DynError> {
    match raw {
        Some(raw) => Ok(serde_json::from_str(raw)?),
        None => Ok(json!({})),
    }
}

fn parse_working_files(
    entries: &[String],
) -> Result<std::collections::BTreeMap<String, String>, DynError> {
    let mut files = std::collections::BTreeMap::new();
    for entry in entries {
        let (path, content) = entry
            .split_once('=')
            .ok_or_else(|| format!("invalid --working-file `{entry}` (expected path=content)"))?;
        if path.trim().is_empty() {
            return Err(
                format!("invalid --working-file `{entry}` (path must be non-empty)").into(),
            );
        }
        files.insert(path.trim().to_string(), content.to_string());
    }
    Ok(files)
}

fn select_migration_target_endpoint(
    report: &Value,
    target_agent_id: Option<&str>,
    target_name: Option<&str>,
) -> Result<String, DynError> {
    let mut records = Vec::<DiscoveryRecord>::new();
    for section in ["lan", "registry"] {
        if let Some(items) = report.get(section).and_then(Value::as_array) {
            for item in items {
                records.push(serde_json::from_value(item.clone())?);
            }
        }
    }

    let selected = if let Some(agent_id) = target_agent_id.filter(|value| !value.trim().is_empty())
    {
        records
            .into_iter()
            .find(|record| record.agent_id == agent_id)
    } else if let Some(name) = target_name.filter(|value| !value.trim().is_empty()) {
        records.into_iter().find(|record| record.name == name)
    } else if records.len() == 1 {
        records.into_iter().next()
    } else {
        None
    };

    let selected = selected.ok_or_else(|| -> DynError {
        if target_agent_id.is_some() {
            Box::<dyn std::error::Error>::from(format!(
                "no discovered agent matched target-agent-id {:?}",
                target_agent_id
            ))
        } else if target_name.is_some() {
            Box::<dyn std::error::Error>::from(format!(
                "no discovered agent matched target-name {:?}",
                target_name
            ))
        } else {
            Box::<dyn std::error::Error>::from(
                "multiple discovered agents found; pass --target-agent-id or --target-name",
            )
        }
    })?;

    normalize_base_url(&selected.endpoint)
}

async fn resolve_migration_target_base_url(
    target_base_url: Option<&str>,
    discover_url: Option<&str>,
    registry_url: Option<&str>,
    target_agent_id: Option<&str>,
    target_name: Option<&str>,
) -> Result<String, DynError> {
    if let Some(target_base_url) = target_base_url {
        return normalize_base_url(target_base_url);
    }

    let discover_url =
        discover_url.ok_or("either --target-base-url or --discover-url is required")?;
    let report = fetch_discovery_report(discover_url, registry_url).await?;
    select_migration_target_endpoint(&report, target_agent_id, target_name)
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
            "permission_policy": "allow",
            "allowed_tools": [tool],
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
    mut shell_runner: impl FnMut(&str, Option<&str>, Option<&str>) -> Result<(), DynError>,
) -> Result<(), DynError> {
    match cli.command {
        Commands::Health { json } => {
            info!(socket_path = %cli.socket_path, "Calling GetHealth over UDS JSON-RPC");
            let response = call_rpc(&cli.socket_path, "GetHealth", json!({})).await?;
            print_response(response, json)?;
        }
        Commands::Migrate {
            source_agent_id,
            target_base_url,
            discover_url,
            registry_url,
            target_agent_id,
            target_name,
            session_id,
            key_files,
            messages,
            head_id,
            tool_cache_json,
            working_files,
            include_snapshot,
            json,
        } => {
            let (messages, head_id) = build_migration_messages(&messages, head_id.as_deref())?;
            let tool_results_cache = parse_tool_cache_json(tool_cache_json.as_deref())?;
            let working_directory = parse_working_files(&working_files)?;
            let target_base_url = resolve_migration_target_base_url(
                target_base_url.as_deref(),
                discover_url.as_deref(),
                registry_url.as_deref(),
                target_agent_id.as_deref(),
                target_name.as_deref(),
            )
            .await?;
            let response = call_rpc(
                &cli.socket_path,
                "MigrateContext",
                json!({
                    "source_agent_id": source_agent_id,
                    "target_base_url": target_base_url,
                    "target_agent_id": target_agent_id,
                    "session_id": session_id,
                    "key_files": key_files,
                    "messages": messages,
                    "head_id": head_id,
                    "tool_results_cache": tool_results_cache,
                    "working_directory": working_directory,
                    "include_snapshot": include_snapshot,
                }),
            )
            .await?;
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

                    let stream = client.stream_task(&target, task.id).await.ok();
                    let status = client.get_task(&target, &task.id.to_string()).await?;
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&json!({
                                "task": task,
                                "stream": stream,
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
            AgentCommands::Shell { agent_id, model } => {
                info!(
                    socket_path = %cli.socket_path,
                    agent_id = ?agent_id,
                    model = ?model,
                    "Launching interactive agent shell TUI"
                );
                shell_runner(&cli.socket_path, agent_id.as_deref(), model.as_deref())?;
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

    run_cli(cli, tui::run).await
}

#[cfg(test)]
#[tokio::test]
async fn shell_command_routes_to_tui() {
    use std::sync::Arc;
    use std::sync::Mutex;

    let cli = Cli::try_parse_from(["agentctl", "agent", "shell"]).expect("cli args should parse");
    let shell_args = Arc::new(Mutex::new(None::<(String, Option<String>, Option<String>)>));
    let shell_args_for_closure = Arc::clone(&shell_args);

    let result = run_cli(cli, move |socket_path, agent_id, model| {
        let mut guard = shell_args_for_closure
            .lock()
            .expect("shell args mutex should not be poisoned");
        *guard = Some((
            socket_path.to_string(),
            agent_id.map(ToString::to_string),
            model.map(ToString::to_string),
        ));
        Ok(())
    })
    .await;

    assert!(result.is_ok());
    let guard = shell_args
        .lock()
        .expect("shell args mutex should not be poisoned");
    let captured = guard.clone().expect("shell runner should be called");
    assert_eq!(captured.0, "/tmp/agentd.sock");
    assert_eq!(captured.1, None);
    assert_eq!(captured.2, None);
}

#[cfg(test)]
#[tokio::test]
async fn shell_command_passes_initial_context_to_tui() {
    use std::sync::Arc;
    use std::sync::Mutex;

    let cli = Cli::try_parse_from([
        "agentctl",
        "agent",
        "shell",
        "--agent-id",
        "agent-123",
        "--model",
        "gpt-5.3-codex",
    ])
    .expect("cli args should parse");
    let shell_args = Arc::new(Mutex::new(None::<(String, Option<String>, Option<String>)>));
    let shell_args_for_closure = Arc::clone(&shell_args);

    let result = run_cli(cli, move |socket_path, agent_id, model| {
        let mut guard = shell_args_for_closure
            .lock()
            .expect("shell args mutex should not be poisoned");
        *guard = Some((
            socket_path.to_string(),
            agent_id.map(ToString::to_string),
            model.map(ToString::to_string),
        ));
        Ok(())
    })
    .await;

    assert!(result.is_ok());
    let guard = shell_args
        .lock()
        .expect("shell args mutex should not be poisoned");
    let captured = guard.clone().expect("shell runner should be called");
    assert_eq!(captured.1.as_deref(), Some("agent-123"));
    assert_eq!(captured.2.as_deref(), Some("gpt-5.3-codex"));
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
        "/agent", "/usage", "/events", "/tools", "/compact", "/model", "/approve", "/deny", "/session",
    ] {
        assert!(commands.contains(&required));
    }
}

#[cfg(test)]
#[test]
fn approval_queue_roundtrip() {
    assert!(tui::approval_queue_roundtrip_probe());
}

#[cfg(test)]
#[test]
fn build_migration_messages_chains_parent_ids() {
    let (messages, head_id) = build_migration_messages(
        &[
            "user:first prompt".to_string(),
            "assistant:second reply".to_string(),
        ],
        None,
    )
    .expect("message specs should parse");

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["id"], json!("msg-1"));
    assert_eq!(messages[0]["parent_id"], Value::Null);
    assert_eq!(messages[1]["parent_id"], json!("msg-1"));
    assert_eq!(head_id, Some("msg-2".to_string()));
}

#[cfg(test)]
#[test]
fn select_migration_target_endpoint_prefers_named_match() {
    let report = json!({
        "lan": [
            {
                "agent_id": "lan-a",
                "name": "alpha",
                "endpoint": "http://10.0.0.2:8080",
                "source": "lan"
            }
        ],
        "registry": [
            {
                "agent_id": "reg-b",
                "name": "beta",
                "endpoint": "https://registry.example.com/agents/reg-b",
                "source": "registry"
            }
        ]
    });

    let endpoint = select_migration_target_endpoint(&report, None, Some("beta"))
        .expect("target name should resolve");
    assert_eq!(endpoint, "https://registry.example.com/agents/reg-b");
}

#[cfg(test)]
#[tokio::test]
async fn migrate_command_sends_rpc_request() {
    let socket_path = std::env::temp_dir().join(format!(
        "agentctl-migrate-test-{}.sock",
        uuid::Uuid::new_v4()
    ));
    let listener = tokio::net::UnixListener::bind(&socket_path).expect("bind test unix socket");

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept rpc connection");
        let mut buf = Vec::new();
        stream
            .read_to_end(&mut buf)
            .await
            .expect("read rpc request");
        let request: JsonRpcRequest = serde_json::from_slice(&buf).expect("decode rpc request");
        assert_eq!(request.method, "MigrateContext");
        assert_eq!(request.params["source_agent_id"], json!("agent-1"));
        assert_eq!(
            request.params["target_base_url"],
            json!("http://127.0.0.1:18085")
        );
        assert_eq!(request.params["messages"][0]["role"], json!("user"));
        assert_eq!(request.params["messages"][1]["parent_id"], json!("msg-1"));
        assert_eq!(
            request.params["working_directory"]["README.md"],
            json!("hello")
        );
        assert_eq!(request.params["include_snapshot"], json!(true));

        let response = JsonRpcResponse::success(
            json!(1),
            json!({"migration_level": "l1", "target_state": "completed"}),
        );
        let encoded = serde_json::to_vec(&response).expect("encode rpc response");
        stream
            .write_all(&encoded)
            .await
            .expect("write rpc response");
    });

    let cli = Cli::try_parse_from([
        "agentctl",
        "--socket-path",
        socket_path.to_str().expect("socket path should be utf8"),
        "migrate",
        "--source-agent-id",
        "agent-1",
        "--target-base-url",
        "http://127.0.0.1:18085",
        "--message",
        "user:first prompt",
        "--message",
        "assistant:second reply",
        "--key-file",
        "README.md",
        "--working-file",
        "README.md=hello",
        "--tool-cache-json",
        "{\"last_tool\":\"search\"}",
        "--include-snapshot",
        "--json",
    ])
    .expect("migrate args should parse");

    run_cli(cli, |_socket_path, _agent_id, _model| Ok(()))
        .await
        .expect("migrate command should succeed");
    server.await.expect("rpc server should finish");
    let _ = std::fs::remove_file(socket_path);
}

#[cfg(test)]
#[tokio::test]
async fn builtin_lite_create_agent_allows_requested_tool() {
    let socket_path = std::env::temp_dir().join(format!(
        "agentctl-builtin-lite-test-{}.sock",
        uuid::Uuid::new_v4()
    ));
    let listener = tokio::net::UnixListener::bind(&socket_path).expect("bind test unix socket");

    let server = tokio::spawn(async move {
        let (mut create_stream, _) = listener.accept().await.expect("accept create rpc");
        let mut create_buf = Vec::new();
        create_stream
            .read_to_end(&mut create_buf)
            .await
            .expect("read create rpc request");
        let create_request: JsonRpcRequest =
            serde_json::from_slice(&create_buf).expect("decode create rpc request");
        assert_eq!(create_request.method, "CreateAgent");
        assert_eq!(create_request.params["permission_policy"], json!("allow"));
        assert_eq!(
            create_request.params["allowed_tools"],
            json!(["builtin.lite.echo"])
        );

        let create_response = JsonRpcResponse::success(
            json!(1),
            json!({
                "agent": {"id": "agent-builtin-lite-test"}
            }),
        );
        let encoded_create = serde_json::to_vec(&create_response).expect("encode create response");
        create_stream
            .write_all(&encoded_create)
            .await
            .expect("write create rpc response");
        let _ = create_stream.shutdown().await;

        let (mut start_stream, _) = listener.accept().await.expect("accept start rpc");
        let mut start_buf = Vec::new();
        start_stream
            .read_to_end(&mut start_buf)
            .await
            .expect("read start rpc request");
        let start_request: JsonRpcRequest =
            serde_json::from_slice(&start_buf).expect("decode start rpc request");
        assert_eq!(start_request.method, "StartManagedAgent");
        assert_eq!(start_request.params["agent_id"], json!("agent-builtin-lite-test"));

        let start_response = JsonRpcResponse::success(
            json!(1),
            json!({"agent_id": "agent-builtin-lite-test", "state": "starting"}),
        );
        let encoded_start = serde_json::to_vec(&start_response).expect("encode start response");
        start_stream
            .write_all(&encoded_start)
            .await
            .expect("write start rpc response");
        let _ = start_stream.shutdown().await;
    });

    let cli = Cli::try_parse_from([
        "agentctl",
        "--socket-path",
        socket_path.to_str().expect("socket path should be utf8"),
        "agent",
        "run",
        "--builtin",
        "lite",
        "--name",
        "builtin-lite-test",
        "--model",
        "gpt-5.3-codex",
        "Reply with exactly OK",
    ])
    .expect("builtin lite args should parse");

    run_cli(cli, |_socket_path, _agent_id, _model| Ok(()))
        .await
        .expect("builtin lite command should succeed");
    server.await.expect("rpc server should finish");
    let _ = std::fs::remove_file(socket_path);
}

#[cfg(test)]
#[tokio::test]
async fn migrate_command_can_resolve_target_from_discovery() {
    let socket_path = std::env::temp_dir().join(format!(
        "agentctl-migrate-discovery-test-{}.sock",
        uuid::Uuid::new_v4()
    ));
    let listener = tokio::net::UnixListener::bind(&socket_path).expect("bind test unix socket");

    let discovery_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind discovery listener");
    let discovery_addr = discovery_listener
        .local_addr()
        .expect("resolve discovery address");
    let discovery_server = tokio::spawn(async move {
        let (mut stream, _) = discovery_listener
            .accept()
            .await
            .expect("accept discovery request");
        let mut buf = [0_u8; 8192];
        let read = stream.read(&mut buf).await.expect("read discovery request");
        let request = String::from_utf8_lossy(&buf[..read]).to_string();
        assert!(request.contains("GET /discover"));

        let body = serde_json::to_string(&json!({
            "lan": [{
                "agent_id": "target-1",
                "name": "target-node",
                "endpoint": "http://127.0.0.1:18087",
                "source": "lan"
            }],
            "registry": [],
            "errors": []
        }))
        .expect("encode discovery body");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("write discovery response");
    });

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept rpc connection");
        let mut buf = Vec::new();
        stream
            .read_to_end(&mut buf)
            .await
            .expect("read rpc request");
        let request: JsonRpcRequest = serde_json::from_slice(&buf).expect("decode rpc request");
        assert_eq!(request.method, "MigrateContext");
        assert_eq!(
            request.params["target_base_url"],
            json!("http://127.0.0.1:18087")
        );

        let response = JsonRpcResponse::success(json!(1), json!({"migration_level": "l1"}));
        let encoded = serde_json::to_vec(&response).expect("encode rpc response");
        stream
            .write_all(&encoded)
            .await
            .expect("write rpc response");
    });

    let cli = Cli::try_parse_from([
        "agentctl",
        "--socket-path",
        socket_path.to_str().expect("socket path should be utf8"),
        "migrate",
        "--source-agent-id",
        "agent-1",
        "--discover-url",
        &format!("http://{discovery_addr}"),
        "--target-name",
        "target-node",
        "--message",
        "user:first prompt",
        "--json",
    ])
    .expect("migrate discovery args should parse");

    run_cli(cli, |_socket_path, _agent_id, _model| Ok(()))
        .await
        .expect("migrate discovery command should succeed");
    discovery_server
        .await
        .expect("discovery server should finish");
    server.await.expect("rpc server should finish");
    let _ = std::fs::remove_file(socket_path);
}

#[cfg(test)]
#[test]
fn tui_multi_agent_panel_updates_on_events() {
    assert!(tui::multi_agent_panel_updates_on_events_probe());
}

#[cfg(test)]
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
        for _ in 0..5 {
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
            } else if request_line.starts_with(&format!("GET /a2a/stream?task_id={task_id} ")) {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\nevent: task\ndata: {}\n\nevent: task\ndata: {}\n\nevent: task\ndata: {}\n\n",
                    json!({
                        "task_id": task_id,
                        "state": "submitted",
                        "lifecycle_state": "creating",
                        "timestamp": now,
                        "payload": {"phase": "submitted"}
                    }),
                    json!({
                        "task_id": task_id,
                        "state": "working",
                        "lifecycle_state": "running",
                        "timestamp": now,
                        "payload": {"phase": "started"}
                    }),
                    json!({
                        "task_id": task_id,
                        "state": "completed",
                        "lifecycle_state": "stopped",
                        "timestamp": now,
                        "payload": {"phase": "completed"}
                    })
                )
                .into_bytes()
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
    run_cli(discover, |_socket_path, _agent_id, _model| Ok(()))
        .await
        .expect("discover should succeed");

    let send = Cli::try_parse_from([
        "agentctl", "a2a", "send", "--target", &base, "--input", "ping", "--json",
    ])
    .expect("send args should parse");
    run_cli(send, |_socket_path, _agent_id, _model| Ok(()))
        .await
        .expect("send should succeed");

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
    run_cli(status, |_socket_path, _agent_id, _model| Ok(()))
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
    let result = run_cli(cli, |_socket_path, _agent_id, _model| Ok(())).await;
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
    run_cli(cli, |_socket_path, _agent_id, _model| Ok(()))
        .await
        .expect("discover command should succeed");

    server.await.expect("discover mock server should finish");
}
