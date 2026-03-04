use agentd_core::AgentError;
use serde::Deserialize;
use std::path::Path;
use tracing::info;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum McpTransport {
    Stdio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum McpTrustLevel {
    Builtin,
    Verified,
    Community,
    Untrusted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct McpServerConfig {
    pub(crate) name: String,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) transport: McpTransport,
    pub(crate) trust_level: McpTrustLevel,
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

fn parse_trust_level(
    value: Option<String>,
    config_path: &Path,
) -> Result<McpTrustLevel, AgentError> {
    let raw = require_non_empty_string(value, "server.trust_level", config_path)?;
    match raw.as_str() {
        "builtin" => Ok(McpTrustLevel::Builtin),
        "verified" => Ok(McpTrustLevel::Verified),
        "community" => Ok(McpTrustLevel::Community),
        "untrusted" => Ok(McpTrustLevel::Untrusted),
        _ => Err(AgentError::InvalidInput(format!(
            "mcp config {} invalid trust_level `{raw}` (expected: builtin|verified|community|untrusted)",
            config_path.display()
        ))),
    }
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
        trust_level: parse_trust_level(server.trust_level, config_path)?,
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
