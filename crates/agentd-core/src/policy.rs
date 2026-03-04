use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::{Deserialize, Serialize};

use crate::error::AgentError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecision {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyRule {
    pub pattern: String,
    pub decision: PolicyDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyLayer {
    pub name: String,
    pub rules: Vec<PolicyRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPolicyOverrides {
    pub allow_tools: Vec<String>,
    pub ask_tools: Vec<String>,
    pub deny_tools: Vec<String>,
}

impl SessionPolicyOverrides {
    pub fn into_layer(self) -> PolicyLayer {
        let mut rules = Vec::with_capacity(
            self.allow_tools.len() + self.ask_tools.len() + self.deny_tools.len(),
        );

        for pattern in self.allow_tools {
            rules.push(PolicyRule {
                pattern,
                decision: PolicyDecision::Allow,
            });
        }
        for pattern in self.ask_tools {
            rules.push(PolicyRule {
                pattern,
                decision: PolicyDecision::Ask,
            });
        }
        for pattern in self.deny_tools {
            rules.push(PolicyRule {
                pattern,
                decision: PolicyDecision::Deny,
            });
        }

        PolicyLayer {
            name: "session_override".to_string(),
            rules,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyEvaluation {
    pub tool: String,
    pub decision: PolicyDecision,
    pub matched_rule: Option<String>,
    pub source_layer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyGatewayDecision {
    pub decision: PolicyDecision,
    pub reason: String,
    pub trace_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyAgentContext {
    pub id: Option<String>,
    pub trust_level: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyToolContext {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyResourceContext {
    pub uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyTimeContext {
    pub timestamp_rfc3339: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyInputContext {
    pub agent: PolicyAgentContext,
    pub tool: PolicyToolContext,
    pub resource: PolicyResourceContext,
    pub time: PolicyTimeContext,
    pub request_meta: BTreeMap<String, String>,
}

impl PolicyInputContext {
    pub fn validate(&self) -> Result<(), AgentError> {
        if self.tool.name.trim().is_empty() {
            return Err(AgentError::InvalidInput(
                "policy input missing required field tool".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyEngineLayers {
    pub global: PolicyLayer,
    pub agent_profile: PolicyLayer,
    pub session_override: PolicyLayer,
}

impl PolicyEngineLayers {
    pub fn new(
        global: PolicyLayer,
        agent_profile: PolicyLayer,
        session_override: PolicyLayer,
    ) -> Self {
        Self {
            global,
            agent_profile,
            session_override,
        }
    }
}

pub trait PolicyEngine {
    fn evaluate(&self, input: &PolicyInputContext) -> PolicyEvaluation;
    fn load(&mut self, layers: PolicyEngineLayers) -> Result<(), AgentError>;
    fn load_policy_layers(
        &mut self,
        global: PolicyLayer,
        agent_profile: PolicyLayer,
        session_override: PolicyLayer,
    ) -> Result<(), AgentError> {
        self.load(PolicyEngineLayers::new(
            global,
            agent_profile,
            session_override,
        ))
    }
    fn reload(&mut self) -> Result<(), AgentError>;
    fn explain(&self, input: &PolicyInputContext) -> String;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegorusQueryPaths {
    pub allow: String,
    pub deny: String,
    pub explain: String,
}

impl Default for RegorusQueryPaths {
    fn default() -> Self {
        Self {
            allow: "data.agentd.policy.allow".to_string(),
            deny: "data.agentd.policy.deny".to_string(),
            explain: "data.agentd.policy.explain".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct RegorusPolicyEngine {
    layered: LayeredPolicyEngine,
    policy_dir: PathBuf,
    policy_files: Vec<PathBuf>,
    query_paths: RegorusQueryPaths,
    rego_engine: Option<Mutex<regorus::Engine>>,
}

impl RegorusPolicyEngine {
    pub fn from_policy_dir(
        layers: PolicyEngineLayers,
        policy_dir: impl Into<PathBuf>,
    ) -> Result<Self, AgentError> {
        let policy_dir = policy_dir.into();
        let layered = LayeredPolicyEngine::new(
            layers.global.clone(),
            layers.agent_profile.clone(),
            layers.session_override.clone(),
        );
        let policy_files = discover_rego_files(&policy_dir)?;
        let rego_engine = if policy_files.is_empty() {
            None
        } else {
            Some(Mutex::new(build_regorus_engine(&policy_files)?))
        };

        Ok(Self {
            layered,
            policy_dir,
            policy_files,
            query_paths: RegorusQueryPaths::default(),
            rego_engine,
        })
    }

    fn evaluate_with_regorus(
        &self,
        input: &PolicyInputContext,
    ) -> Result<PolicyEvaluation, AgentError> {
        let Some(rego_engine) = &self.rego_engine else {
            return Ok(self.layered.evaluate(input));
        };

        let input_json = serde_json::to_string(input)
            .map_err(|err| AgentError::Runtime(format!("serialize policy input failed: {err}")))?;

        let mut engine = rego_engine
            .lock()
            .map_err(|_| AgentError::Runtime("regorus engine lock poisoned".to_string()))?;

        engine
            .set_input_json(&input_json)
            .map_err(|err| AgentError::Runtime(format!("set rego input failed: {err}")))?;

        let deny = engine
            .eval_bool_query(self.query_paths.deny.clone(), false)
            .map_err(|err| AgentError::Runtime(format!("evaluate deny query failed: {err}")))?;
        let allow = engine
            .eval_bool_query(self.query_paths.allow.clone(), false)
            .map_err(|err| AgentError::Runtime(format!("evaluate allow query failed: {err}")))?;

        let explanation = match engine.eval_rule(self.query_paths.explain.clone()) {
            Ok(value) if value != regorus::Value::Undefined => {
                if let Ok(explain) = value.as_string() {
                    Some(explain.as_ref().to_string())
                } else {
                    Some(value.to_string())
                }
            }
            _ => None,
        };

        let (decision, matched_rule) = if deny {
            (PolicyDecision::Deny, explanation)
        } else if allow {
            (PolicyDecision::Allow, explanation)
        } else {
            (PolicyDecision::Ask, explanation)
        };

        Ok(PolicyEvaluation {
            tool: input.tool.name.clone(),
            decision,
            matched_rule,
            source_layer: Some("rego:data.agentd.policy".to_string()),
        })
    }
}

#[derive(Debug, Clone)]
pub struct LayeredPolicyEngine {
    global: PolicyLayer,
    agent_profile: PolicyLayer,
    session_override: PolicyLayer,
}

impl LayeredPolicyEngine {
    pub fn new(
        global: PolicyLayer,
        agent_profile: PolicyLayer,
        session_override: PolicyLayer,
    ) -> Self {
        Self {
            global,
            agent_profile,
            session_override,
        }
    }
}

impl PolicyEngine for LayeredPolicyEngine {
    fn evaluate(&self, input: &PolicyInputContext) -> PolicyEvaluation {
        PolicyLayer::evaluate_tool(
            &self.global,
            &self.agent_profile,
            &self.session_override,
            &input.tool.name,
        )
    }

    fn load(&mut self, layers: PolicyEngineLayers) -> Result<(), AgentError> {
        self.global = layers.global;
        self.agent_profile = layers.agent_profile;
        self.session_override = layers.session_override;
        Ok(())
    }

    fn reload(&mut self) -> Result<(), AgentError> {
        Ok(())
    }

    fn explain(&self, input: &PolicyInputContext) -> String {
        let evaluation = self.evaluate(input);
        format!(
            "policy.explain: tool={} decision={:?} matched_rule={} source_layer={}",
            evaluation.tool,
            evaluation.decision,
            evaluation.matched_rule.as_deref().unwrap_or("<none>"),
            evaluation.source_layer.as_deref().unwrap_or("<none>")
        )
    }
}

impl PolicyEngine for RegorusPolicyEngine {
    fn evaluate(&self, input: &PolicyInputContext) -> PolicyEvaluation {
        match self.evaluate_with_regorus(input) {
            Ok(evaluation) => evaluation,
            Err(err) => PolicyEvaluation {
                tool: input.tool.name.clone(),
                decision: PolicyDecision::Ask,
                matched_rule: Some(format!("regorus.error: {err}")),
                source_layer: Some("rego:data.agentd.policy".to_string()),
            },
        }
    }

    fn load(&mut self, layers: PolicyEngineLayers) -> Result<(), AgentError> {
        self.layered =
            LayeredPolicyEngine::new(layers.global, layers.agent_profile, layers.session_override);
        Ok(())
    }

    fn reload(&mut self) -> Result<(), AgentError> {
        self.policy_files = discover_rego_files(&self.policy_dir)?;
        self.rego_engine = if self.policy_files.is_empty() {
            None
        } else {
            Some(Mutex::new(build_regorus_engine(&self.policy_files)?))
        };
        Ok(())
    }

    fn explain(&self, input: &PolicyInputContext) -> String {
        let evaluation = self.evaluate(input);
        format!(
            "policy.explain: tool={} decision={:?} matched_rule={} source_layer={}",
            evaluation.tool,
            evaluation.decision,
            evaluation.matched_rule.as_deref().unwrap_or("<none>"),
            evaluation.source_layer.as_deref().unwrap_or("<none>")
        )
    }
}

fn discover_rego_files(policy_dir: &Path) -> Result<Vec<PathBuf>, AgentError> {
    if !policy_dir.exists() {
        return Ok(Vec::new());
    }
    if !policy_dir.is_dir() {
        return Err(AgentError::InvalidInput(format!(
            "policy dir is not a directory: {}",
            policy_dir.display()
        )));
    }

    let mut paths = fs::read_dir(policy_dir)
        .map_err(|err| {
            AgentError::Runtime(format!(
                "read policy directory {} failed: {err}",
                policy_dir.display()
            ))
        })?
        .filter_map(|entry| entry.ok().map(|value| value.path()))
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("rego"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn build_regorus_engine(policy_files: &[PathBuf]) -> Result<regorus::Engine, AgentError> {
    let mut engine = regorus::Engine::new();
    for path in policy_files {
        engine.add_policy_from_file(path).map_err(|err| {
            AgentError::Config(format!(
                "compile rego policy {} failed: {err}",
                path.display()
            ))
        })?;
    }
    Ok(engine)
}

impl PolicyEvaluation {
    pub fn to_gateway_decision(&self, trace_id: impl Into<String>) -> PolicyGatewayDecision {
        let matched_rule = self.matched_rule.as_deref().unwrap_or("<none>").to_string();
        let source_layer = self.source_layer.as_deref().unwrap_or("<none>").to_string();
        let reason = match self.decision {
            PolicyDecision::Allow => format!(
                "policy.allow: tool={} matched_rule={} source_layer={}",
                self.tool, matched_rule, source_layer
            ),
            PolicyDecision::Ask => format!(
                "policy.ask: tool={} matched_rule={} source_layer={}",
                self.tool, matched_rule, source_layer
            ),
            PolicyDecision::Deny => format!(
                "policy.deny: tool={} matched_rule={} source_layer={}",
                self.tool, matched_rule, source_layer
            ),
        };

        PolicyGatewayDecision {
            decision: self.decision,
            reason,
            trace_id: trace_id.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuleMatch {
    decision: PolicyDecision,
    layer_index: usize,
    layer_name: String,
    pattern: String,
    specificity: usize,
    rule_index: usize,
}

impl PolicyLayer {
    pub fn evaluate_tool(
        global: &PolicyLayer,
        agent_profile: &PolicyLayer,
        session_override: &PolicyLayer,
        tool: &str,
    ) -> PolicyEvaluation {
        let layers = [global, agent_profile, session_override];
        let mut matches = Vec::new();

        for (layer_index, layer) in layers.iter().enumerate() {
            for (rule_index, rule) in layer.rules.iter().enumerate() {
                if wildcard_matches(&rule.pattern, tool) {
                    matches.push(RuleMatch {
                        decision: rule.decision,
                        layer_index,
                        layer_name: layer.name.clone(),
                        pattern: rule.pattern.clone(),
                        specificity: rule
                            .pattern
                            .chars()
                            .filter(|c| *c != '*' && *c != '?')
                            .count(),
                        rule_index,
                    });
                }
            }
        }

        let winner = pick_winner(&matches);
        match winner {
            Some(matched) => PolicyEvaluation {
                tool: tool.to_string(),
                decision: matched.decision,
                matched_rule: Some(matched.pattern),
                source_layer: Some(matched.layer_name),
            },
            None => PolicyEvaluation {
                tool: tool.to_string(),
                decision: PolicyDecision::Ask,
                matched_rule: None,
                source_layer: None,
            },
        }
    }
}

fn pick_winner(matches: &[RuleMatch]) -> Option<RuleMatch> {
    for decision in [
        PolicyDecision::Deny,
        PolicyDecision::Ask,
        PolicyDecision::Allow,
    ] {
        let mut candidates: Vec<&RuleMatch> =
            matches.iter().filter(|m| m.decision == decision).collect();
        if candidates.is_empty() {
            continue;
        }

        candidates.sort_by(|a, b| {
            b.layer_index
                .cmp(&a.layer_index)
                .then_with(|| b.specificity.cmp(&a.specificity))
                .then_with(|| a.rule_index.cmp(&b.rule_index))
        });
        return candidates.first().map(|m| (*m).clone());
    }
    None
}

fn wildcard_matches(pattern: &str, text: &str) -> bool {
    let p = pattern.as_bytes();
    let t = text.as_bytes();

    let mut pi = 0usize;
    let mut ti = 0usize;
    let mut star_idx: Option<usize> = None;
    let mut match_idx = 0usize;

    while ti < t.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star_idx = Some(pi);
            pi += 1;
            match_idx = ti;
        } else if let Some(star) = star_idx {
            pi = star + 1;
            match_idx += 1;
            ti = match_idx;
        } else {
            return false;
        }
    }

    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }

    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_matching_works() {
        assert!(wildcard_matches("read:*", "read:file"));
        assert!(wildcard_matches("read:*.env", "read:secrets.env"));
        assert!(!wildcard_matches("read:*.env", "read:secrets.txt"));
        assert!(wildcard_matches("exec:?sh", "exec:bsh"));
    }

    #[test]
    fn deny_takes_precedence_over_allow() {
        let global = PolicyLayer {
            name: "global".to_string(),
            rules: vec![PolicyRule {
                pattern: "read:*".to_string(),
                decision: PolicyDecision::Allow,
            }],
        };
        let profile = PolicyLayer {
            name: "profile".to_string(),
            rules: vec![PolicyRule {
                pattern: "read:*.env".to_string(),
                decision: PolicyDecision::Deny,
            }],
        };
        let session = PolicyLayer {
            name: "session_override".to_string(),
            rules: vec![],
        };

        let evaluation = PolicyLayer::evaluate_tool(&global, &profile, &session, "read:.env");
        assert_eq!(evaluation.decision, PolicyDecision::Deny);
        assert_eq!(evaluation.matched_rule.as_deref(), Some("read:*.env"));
        assert_eq!(evaluation.source_layer.as_deref(), Some("profile"));
    }

    #[test]
    fn layer_priority_prefers_session_when_decision_ties() {
        let global = PolicyLayer {
            name: "global".to_string(),
            rules: vec![PolicyRule {
                pattern: "bash:*".to_string(),
                decision: PolicyDecision::Ask,
            }],
        };
        let profile = PolicyLayer {
            name: "profile".to_string(),
            rules: vec![PolicyRule {
                pattern: "bash:rm".to_string(),
                decision: PolicyDecision::Ask,
            }],
        };
        let session = SessionPolicyOverrides {
            allow_tools: vec![],
            ask_tools: vec!["bash:rm".to_string()],
            deny_tools: vec![],
        }
        .into_layer();

        let evaluation = PolicyLayer::evaluate_tool(&global, &profile, &session, "bash:rm");
        assert_eq!(evaluation.decision, PolicyDecision::Ask);
        assert_eq!(evaluation.source_layer.as_deref(), Some("session_override"));
    }

    #[test]
    fn gateway_decision_contains_reason_and_trace_id() {
        let evaluation = PolicyEvaluation {
            tool: "mcp.fs.read_file".to_string(),
            decision: PolicyDecision::Deny,
            matched_rule: Some("mcp.fs.*".to_string()),
            source_layer: Some("agent_profile".to_string()),
        };

        let gateway = evaluation.to_gateway_decision("trace-rpc-8");
        assert_eq!(gateway.decision, PolicyDecision::Deny);
        assert_eq!(gateway.trace_id, "trace-rpc-8");
        assert!(gateway.reason.contains("policy.deny"));
        assert!(gateway.reason.contains("matched_rule=mcp.fs.*"));
        assert!(gateway.reason.contains("source_layer=agent_profile"));
    }

    #[test]
    fn policy_engine_trait_contract() {
        let global = PolicyLayer {
            name: "global".to_string(),
            rules: vec![PolicyRule {
                pattern: "mcp.*".to_string(),
                decision: PolicyDecision::Ask,
            }],
        };
        let profile = PolicyLayer {
            name: "agent_profile".to_string(),
            rules: vec![PolicyRule {
                pattern: "mcp.fs.read_file".to_string(),
                decision: PolicyDecision::Deny,
            }],
        };
        let session = PolicyLayer {
            name: "session_override".to_string(),
            rules: vec![],
        };

        let mut engine: Box<dyn PolicyEngine> = Box::new(LayeredPolicyEngine::new(
            global.clone(),
            profile.clone(),
            session.clone(),
        ));
        let input = PolicyInputContext {
            agent: PolicyAgentContext {
                id: Some("agent-1".to_string()),
                trust_level: Some("ask".to_string()),
            },
            tool: PolicyToolContext {
                name: "mcp.fs.read_file".to_string(),
            },
            resource: PolicyResourceContext {
                uri: Some(".env".to_string()),
            },
            time: PolicyTimeContext {
                timestamp_rfc3339: None,
            },
            request_meta: BTreeMap::new(),
        };

        let evaluation = engine.evaluate(&input);
        assert_eq!(evaluation.decision, PolicyDecision::Deny);

        engine
            .load(PolicyEngineLayers::new(global, profile, session))
            .expect("load should succeed");
        engine.reload().expect("reload should succeed");

        let explain = engine.explain(&input);
        assert!(explain.contains("policy.explain"));
        assert!(explain.contains("decision=Deny"));
    }

    #[test]
    fn policy_input_context_roundtrip() {
        let mut request_meta = BTreeMap::new();
        request_meta.insert("trace_id".to_string(), "trace-101".to_string());
        request_meta.insert("request_id".to_string(), "rpc-7".to_string());

        let input = PolicyInputContext {
            agent: PolicyAgentContext {
                id: Some("agent-7".to_string()),
                trust_level: Some("allow".to_string()),
            },
            tool: PolicyToolContext {
                name: "mcp.git.status".to_string(),
            },
            resource: PolicyResourceContext {
                uri: Some("repo:/work".to_string()),
            },
            time: PolicyTimeContext {
                timestamp_rfc3339: Some("2026-03-04T17:00:00Z".to_string()),
            },
            request_meta,
        };

        let encoded = serde_json::to_string(&input).expect("serialize input should succeed");
        let decoded: PolicyInputContext =
            serde_json::from_str(&encoded).expect("deserialize input should succeed");

        assert_eq!(decoded, input);
    }

    #[test]
    fn policy_input_context_missing_tool_rejected() {
        let input = PolicyInputContext {
            agent: PolicyAgentContext {
                id: Some("agent-9".to_string()),
                trust_level: None,
            },
            tool: PolicyToolContext {
                name: "   ".to_string(),
            },
            resource: PolicyResourceContext { uri: None },
            time: PolicyTimeContext {
                timestamp_rfc3339: None,
            },
            request_meta: BTreeMap::new(),
        };

        let err = input
            .validate()
            .expect_err("missing tool should be rejected");
        assert!(err.to_string().contains("missing required field tool"));
    }
}
