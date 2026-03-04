use agentd_core::error::AgentError;
use agentd_core::profile::TrustLevel;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum McpTransport {
    Stdio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum McpServerHealth {
    Unknown,
    Healthy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct McpServerConfig {
    pub(crate) name: String,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) transport: McpTransport,
    pub(crate) trust_level: TrustLevel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct McpRegistryEntry {
    pub(crate) server_id: String,
    pub(crate) capabilities: Vec<String>,
    pub(crate) trust_level: TrustLevel,
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

    pub(crate) fn upsert(&mut self, entry: McpRegistryEntry) {
        self.entries.insert(entry.server_id.clone(), entry);
    }

    pub(crate) fn get(&self, server_id: &str) -> Option<&McpRegistryEntry> {
        self.entries.get(server_id)
    }

    pub(crate) fn remove(&mut self, server_id: &str) -> Option<McpRegistryEntry> {
        self.entries.remove(server_id)
    }

    pub(crate) fn list(&self) -> Vec<&McpRegistryEntry> {
        self.entries.values().collect()
    }
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

pub(crate) fn parse_trust_level(raw: &str) -> Result<TrustLevel, AgentError> {
    TrustLevel::parse(raw)
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

    let trust_level = parse_trust_level(&require_non_empty_string(
        server.trust_level,
        "server.trust_level",
        config_path,
    )?)?;

    Ok(McpServerConfig {
        name: require_non_empty_string(server.name, "server.name", config_path)?,
        command: require_non_empty_string(server.command, "server.command", config_path)?,
        args,
        transport: parse_transport(server.transport, config_path)?,
        trust_level,
    })
}

pub(crate) fn load_mcp_server_configs(
    configs_dir: &Path,
) -> Result<Vec<McpServerConfig>, AgentError> {
    if !configs_dir.exists() {
        return Ok(Vec::new());
    }

    if !configs_dir.is_dir() {
        return Err(AgentError::InvalidInput(format!(
            "mcp config path is not a directory: {}",
            configs_dir.display()
        )));
    }

    let mut config_paths = std::fs::read_dir(configs_dir)
        .map_err(|err| AgentError::Runtime(format!("read_dir failed: {err}")))?
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
            AgentError::Runtime(format!(
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

    #[test]
    fn mcp_registry_roundtrip_entry() {
        let mut registry = McpRegistry::new();
        registry.upsert(McpRegistryEntry {
            server_id: "mcp-search".to_string(),
            capabilities: vec!["search.query".to_string(), "search.fetch".to_string()],
            trust_level: TrustLevel::Verified,
            health: McpServerHealth::Healthy,
        });

        let entry = registry.get("mcp-search").expect("entry should exist");
        assert_eq!(entry.server_id, "mcp-search");
        assert_eq!(entry.trust_level, TrustLevel::Verified);
        assert_eq!(registry.list().len(), 1);

        let digest = entry.capabilities.join(",");
        assert_eq!(digest, "search.query,search.fetch");

        let removed = registry.remove("mcp-search");
        assert!(removed.is_some());
        assert!(registry.get("mcp-search").is_none());

        registry.upsert(McpRegistryEntry {
            server_id: "mcp-git".to_string(),
            capabilities: vec!["git.status".to_string()],
            trust_level: TrustLevel::Community,
            health: McpServerHealth::Unknown,
        });
        assert_eq!(registry.list().len(), 1);
    }

    #[test]
    fn mcp_registry_rejects_unknown_trust() {
        let err = parse_trust_level("unknown").expect_err("unknown trust level should fail");
        let message = err.to_string();
        assert!(message.contains("invalid trust_level"));
    }
}
