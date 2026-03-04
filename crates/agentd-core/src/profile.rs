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
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Verified => "verified",
            Self::Community => "community",
            Self::Untrusted => "untrusted",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, AgentError> {
        let normalized = raw.trim();

        if normalized.eq_ignore_ascii_case("builtin") {
            return Ok(Self::Builtin);
        }

        if normalized.eq_ignore_ascii_case("verified") {
            return Ok(Self::Verified);
        }

        if normalized.eq_ignore_ascii_case("community") {
            return Ok(Self::Community);
        }

        if normalized.eq_ignore_ascii_case("untrusted") {
            return Ok(Self::Untrusted);
        }

        Err(AgentError::InvalidInput(format!(
            "invalid trust_level `{normalized}` (expected: builtin|verified|community|untrusted)"
        )))
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

#[cfg(test)]
mod tests {
    use super::TrustLevel;

    #[test]
    fn trust_level_parse_accepts_canonical_values() {
        assert_eq!(
            TrustLevel::parse("builtin").expect("builtin"),
            TrustLevel::Builtin
        );
        assert_eq!(
            TrustLevel::parse("verified").expect("verified"),
            TrustLevel::Verified
        );
        assert_eq!(
            TrustLevel::parse("community").expect("community"),
            TrustLevel::Community
        );
        assert_eq!(
            TrustLevel::parse("untrusted").expect("untrusted"),
            TrustLevel::Untrusted
        );
    }

    #[test]
    fn trust_level_parse_accepts_mixed_case_with_whitespace() {
        assert_eq!(
            TrustLevel::parse("  VerIfied  ").expect("mixed-case verified"),
            TrustLevel::Verified
        );
        assert_eq!(
            TrustLevel::parse("\tunTRUSTED\n").expect("mixed-case untrusted"),
            TrustLevel::Untrusted
        );
    }

    #[test]
    fn trust_level_parse_rejects_unknown_value() {
        let err = TrustLevel::parse("partner").expect_err("invalid trust level should fail");
        assert!(err.to_string().contains("invalid trust_level `partner`"));
    }

    #[test]
    fn trust_level_as_str_matches_serialized_names() {
        assert_eq!(TrustLevel::Builtin.as_str(), "builtin");
        assert_eq!(TrustLevel::Verified.as_str(), "verified");
        assert_eq!(TrustLevel::Community.as_str(), "community");
        assert_eq!(TrustLevel::Untrusted.as_str(), "untrusted");
    }
}
