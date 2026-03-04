use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AgentError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub id: Uuid,
    pub name: String,
    pub model: ModelConfig,
    pub permissions: PermissionConfig,
    pub budget: BudgetConfig,
    pub status: AgentLifecycleState,
    pub failure_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentLifecycleState {
    Creating,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub model_name: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionConfig {
    pub policy: PermissionPolicy,
    pub allowed_tools: Vec<String>,
    pub denied_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PermissionPolicy {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    Builtin,
    Verified,
    Community,
    Untrusted,
}

impl TrustLevel {
    pub fn parse(raw: &str) -> Result<Self, AgentError> {
        match raw {
            "builtin" => Ok(Self::Builtin),
            "verified" => Ok(Self::Verified),
            "community" => Ok(Self::Community),
            "untrusted" => Ok(Self::Untrusted),
            _ => Err(AgentError::InvalidInput(format!(
                "invalid trust_level `{raw}` (expected: builtin|verified|community|untrusted)"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub token_limit: Option<u64>,
    pub cost_limit_usd: Option<f64>,
    pub time_limit_seconds: Option<u64>,
}

impl AgentProfile {
    pub fn new(name: String, model: ModelConfig) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            model,
            permissions: PermissionConfig {
                policy: PermissionPolicy::Ask,
                allowed_tools: vec![],
                denied_tools: vec![],
            },
            budget: BudgetConfig {
                token_limit: None,
                cost_limit_usd: None,
                time_limit_seconds: None,
            },
            status: AgentLifecycleState::Creating,
            failure_reason: None,
            created_at: now,
            updated_at: now,
        }
    }
}
