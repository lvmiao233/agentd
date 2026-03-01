use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub id: Uuid,
    pub name: String,
    pub model: ModelConfig,
    pub permissions: PermissionConfig,
    pub budget: BudgetConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
            created_at: now,
            updated_at: now,
        }
    }
}
