use std::{
    collections::hash_map::DefaultHasher,
    collections::BTreeMap,
    fs,
    hash::{Hash, Hasher},
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
    pub input_snapshot: Option<String>,
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
    pub transpiled_allow: String,
    pub transpiled_deny: String,
    pub transpiled_explain: String,
    pub transpiled_source_layer: String,
}

impl Default for RegorusQueryPaths {
    fn default() -> Self {
        Self {
            allow: "data.agentd.policy.allow".to_string(),
            deny: "data.agentd.policy.deny".to_string(),
            explain: "data.agentd.policy.explain".to_string(),
            transpiled_allow: "data.agentd.transpiled.allow".to_string(),
            transpiled_deny: "data.agentd.transpiled.deny".to_string(),
            transpiled_explain: "data.agentd.transpiled.explain".to_string(),
            transpiled_source_layer: "data.agentd.transpiled.source_layer".to_string(),
        }
    }
}

const REGO_POLICY_SOURCE: &str = "rego:data.agentd.policy";
const REGO_TRANSPILED_SOURCE: &str = "rego:data.agentd.transpiled";
const TRANSPILED_POLICY_PATH: &str = "__generated__/toml-transpiled.rego";

#[derive(Debug)]
pub struct RegorusPolicyEngine {
    layered: LayeredPolicyEngine,
    policy_dir: PathBuf,
    policy_files: Mutex<Vec<PathBuf>>,
    policy_fingerprints: Mutex<BTreeMap<PathBuf, PolicyFileFingerprint>>,
    query_paths: RegorusQueryPaths,
    rego_engine: Option<Mutex<regorus::Engine>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PolicyFileFingerprint {
    size: u64,
    modified_unix_secs: i64,
    content_hash: u64,
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
        let policy_fingerprints = collect_policy_fingerprints(&policy_files)?;
        let transpiled_policy = transpile_policy_layers_to_rego(
            &layers.global,
            &layers.agent_profile,
            &layers.session_override,
        )?;
        let generated_modules = vec![(TRANSPILED_POLICY_PATH.to_string(), transpiled_policy)];
        let rego_engine = Some(Mutex::new(build_regorus_engine(
            &policy_files,
            &generated_modules,
        )?));

        Ok(Self {
            layered,
            policy_dir,
            policy_files: Mutex::new(policy_files),
            policy_fingerprints: Mutex::new(policy_fingerprints),
            query_paths: RegorusQueryPaths::default(),
            rego_engine,
        })
    }

    fn maybe_reload_if_policy_changed(&self) -> Result<(), AgentError> {
        let next_policy_files = discover_rego_files(&self.policy_dir)?;
        let next_fingerprints = collect_policy_fingerprints(&next_policy_files)?;

        let changed = {
            let current_policy_files = self
                .policy_files
                .lock()
                .map_err(|_| AgentError::Runtime("policy files lock poisoned".to_string()))?;
            let current_fingerprints = self.policy_fingerprints.lock().map_err(|_| {
                AgentError::Runtime("policy file fingerprints lock poisoned".to_string())
            })?;
            *current_policy_files != next_policy_files || *current_fingerprints != next_fingerprints
        };

        if !changed {
            return Ok(());
        }

        let transpiled_policy = transpile_policy_layers_to_rego(
            &self.layered.global,
            &self.layered.agent_profile,
            &self.layered.session_override,
        )?;
        let generated_modules = vec![(TRANSPILED_POLICY_PATH.to_string(), transpiled_policy)];
        let next_engine = build_regorus_engine(&next_policy_files, &generated_modules)?;

        let Some(rego_engine) = &self.rego_engine else {
            return Err(AgentError::Runtime(
                "regorus engine not initialized".to_string(),
            ));
        };

        let mut current_engine = rego_engine
            .lock()
            .map_err(|_| AgentError::Runtime("regorus engine lock poisoned".to_string()))?;
        *current_engine = next_engine;

        {
            let mut current_policy_files = self
                .policy_files
                .lock()
                .map_err(|_| AgentError::Runtime("policy files lock poisoned".to_string()))?;
            *current_policy_files = next_policy_files;
        }
        {
            let mut current_fingerprints = self.policy_fingerprints.lock().map_err(|_| {
                AgentError::Runtime("policy file fingerprints lock poisoned".to_string())
            })?;
            *current_fingerprints = next_fingerprints;
        }

        Ok(())
    }

    fn evaluate_with_regorus(
        &self,
        input: &PolicyInputContext,
    ) -> Result<PolicyEvaluation, AgentError> {
        let _ = self.maybe_reload_if_policy_changed();

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

        let policy_deny = eval_rule_as_bool_or_false(&mut engine, &self.query_paths.deny)?;
        let policy_allow = eval_rule_as_bool_or_false(&mut engine, &self.query_paths.allow)?;

        let transpiled_deny =
            eval_rule_as_bool_or_false(&mut engine, &self.query_paths.transpiled_deny)?;
        let transpiled_allow =
            eval_rule_as_bool_or_false(&mut engine, &self.query_paths.transpiled_allow)?;

        let policy_explanation =
            eval_rule_as_optional_string(&mut engine, &self.query_paths.explain)?;
        let transpiled_explanation =
            eval_rule_as_optional_string(&mut engine, &self.query_paths.transpiled_explain)?;
        let transpiled_source_layer =
            eval_rule_as_optional_string(&mut engine, &self.query_paths.transpiled_source_layer)?;

        let (decision, matched_rule, source_layer) = if policy_deny {
            (
                PolicyDecision::Deny,
                policy_explanation
                    .clone()
                    .or_else(|| transpiled_explanation.clone()),
                Some(REGO_POLICY_SOURCE.to_string()),
            )
        } else if transpiled_deny {
            (
                PolicyDecision::Deny,
                transpiled_explanation.clone(),
                transpiled_source_layer
                    .clone()
                    .or_else(|| Some(REGO_TRANSPILED_SOURCE.to_string())),
            )
        } else if policy_allow {
            (
                PolicyDecision::Allow,
                policy_explanation
                    .clone()
                    .or_else(|| transpiled_explanation.clone()),
                Some(REGO_POLICY_SOURCE.to_string()),
            )
        } else if transpiled_allow {
            (
                PolicyDecision::Allow,
                transpiled_explanation.clone(),
                transpiled_source_layer
                    .clone()
                    .or_else(|| Some(REGO_TRANSPILED_SOURCE.to_string())),
            )
        } else if policy_explanation.is_some() {
            (
                PolicyDecision::Ask,
                policy_explanation,
                Some(REGO_POLICY_SOURCE.to_string()),
            )
        } else {
            (
                PolicyDecision::Ask,
                transpiled_explanation,
                transpiled_source_layer,
            )
        };

        Ok(PolicyEvaluation {
            tool: input.tool.name.clone(),
            decision,
            matched_rule,
            source_layer,
            input_snapshot: Some(summarize_policy_input(input)),
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
        let mut evaluation = PolicyLayer::evaluate_tool(
            &self.global,
            &self.agent_profile,
            &self.session_override,
            &input.tool.name,
        );
        evaluation.input_snapshot = Some(summarize_policy_input(input));
        evaluation
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
            "policy.explain: tool={} decision={:?} matched_rule={} source_layer={} input_snapshot={}",
            evaluation.tool,
            evaluation.decision,
            evaluation.matched_rule.as_deref().unwrap_or("<none>"),
            evaluation.source_layer.as_deref().unwrap_or("<none>"),
            evaluation.input_snapshot.as_deref().unwrap_or("<none>")
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
                source_layer: Some(REGO_POLICY_SOURCE.to_string()),
                input_snapshot: Some(summarize_policy_input(input)),
            },
        }
    }

    fn load(&mut self, layers: PolicyEngineLayers) -> Result<(), AgentError> {
        self.layered =
            LayeredPolicyEngine::new(layers.global, layers.agent_profile, layers.session_override);
        self.reload()
    }

    fn reload(&mut self) -> Result<(), AgentError> {
        let next_policy_files = discover_rego_files(&self.policy_dir)?;
        let next_fingerprints = collect_policy_fingerprints(&next_policy_files)?;
        let transpiled_policy = transpile_policy_layers_to_rego(
            &self.layered.global,
            &self.layered.agent_profile,
            &self.layered.session_override,
        )?;
        let generated_modules = vec![(TRANSPILED_POLICY_PATH.to_string(), transpiled_policy)];
        let next_engine = build_regorus_engine(&next_policy_files, &generated_modules)?;

        self.rego_engine = Some(Mutex::new(next_engine));
        {
            let mut current_policy_files = self
                .policy_files
                .lock()
                .map_err(|_| AgentError::Runtime("policy files lock poisoned".to_string()))?;
            *current_policy_files = next_policy_files;
        }
        {
            let mut current_fingerprints = self.policy_fingerprints.lock().map_err(|_| {
                AgentError::Runtime("policy file fingerprints lock poisoned".to_string())
            })?;
            *current_fingerprints = next_fingerprints;
        }
        Ok(())
    }

    fn explain(&self, input: &PolicyInputContext) -> String {
        let evaluation = self.evaluate(input);
        format!(
            "policy.explain: tool={} decision={:?} matched_rule={} source_layer={} input_snapshot={}",
            evaluation.tool,
            evaluation.decision,
            evaluation.matched_rule.as_deref().unwrap_or("<none>"),
            evaluation.source_layer.as_deref().unwrap_or("<none>"),
            evaluation.input_snapshot.as_deref().unwrap_or("<none>")
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

fn collect_policy_fingerprints(
    policy_files: &[PathBuf],
) -> Result<BTreeMap<PathBuf, PolicyFileFingerprint>, AgentError> {
    let mut fingerprints = BTreeMap::new();

    for path in policy_files {
        let metadata = fs::metadata(path).map_err(|err| {
            AgentError::Runtime(format!(
                "read policy file metadata {} failed: {err}",
                path.display()
            ))
        })?;
        let modified_unix_secs = metadata
            .modified()
            .ok()
            .and_then(|time| {
                time.duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|duration| duration.as_secs() as i64)
            })
            .unwrap_or(0);
        let content = fs::read(path).map_err(|err| {
            AgentError::Runtime(format!(
                "read policy file content {} failed: {err}",
                path.display()
            ))
        })?;
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);

        fingerprints.insert(
            path.clone(),
            PolicyFileFingerprint {
                size: metadata.len(),
                modified_unix_secs,
                content_hash: hasher.finish(),
            },
        );
    }

    Ok(fingerprints)
}

fn summarize_policy_input(input: &PolicyInputContext) -> String {
    let request_meta_keys = if input.request_meta.is_empty() {
        "<none>".to_string()
    } else {
        input
            .request_meta
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(",")
    };

    format!(
        "agent.id={} trust_level={} tool={} resource={} request_meta_keys=[{}]",
        input.agent.id.as_deref().unwrap_or("<none>"),
        input.agent.trust_level.as_deref().unwrap_or("<none>"),
        input.tool.name,
        input.resource.uri.as_deref().unwrap_or("<none>"),
        request_meta_keys,
    )
}

fn build_regorus_engine(
    policy_files: &[PathBuf],
    generated_modules: &[(String, String)],
) -> Result<regorus::Engine, AgentError> {
    let mut engine = regorus::Engine::new();
    for path in policy_files {
        engine.add_policy_from_file(path).map_err(|err| {
            AgentError::Config(format!(
                "compile rego policy {} failed: {err}",
                path.display()
            ))
        })?;
    }
    for (path, rego) in generated_modules {
        engine
            .add_policy(path.clone(), rego.clone())
            .map_err(|err| {
                AgentError::Config(format!("compile rego policy {path} failed: {err}"))
            })?;
    }
    Ok(engine)
}

fn eval_rule_as_optional_string(
    engine: &mut regorus::Engine,
    query: &str,
) -> Result<Option<String>, AgentError> {
    match engine.eval_rule(query.to_string()) {
        Ok(value) if value != regorus::Value::Undefined => {
            if let Ok(explain) = value.as_string() {
                Ok(Some(explain.as_ref().to_string()))
            } else {
                Ok(Some(value.to_string()))
            }
        }
        Ok(_) => Ok(None),
        Err(err) => {
            let err_msg = err.to_string();
            if err_msg.contains("not a valid rule path") {
                Ok(None)
            } else {
                Err(AgentError::Runtime(format!(
                    "evaluate rego rule `{query}` failed: {err_msg}"
                )))
            }
        }
    }
}

fn eval_rule_as_bool_or_false(
    engine: &mut regorus::Engine,
    query: &str,
) -> Result<bool, AgentError> {
    let value = match engine.eval_rule(query.to_string()) {
        Ok(value) => value,
        Err(err) => {
            let err_msg = err.to_string();
            if err_msg.contains("not a valid rule path") {
                return Ok(false);
            }
            return Err(AgentError::Runtime(format!(
                "evaluate rego rule `{query}` failed: {err_msg}"
            )));
        }
    };

    if value == regorus::Value::Undefined {
        return Ok(false);
    }

    if let Ok(as_bool) = value.as_bool() {
        return Ok(*as_bool);
    }

    Err(AgentError::Runtime(format!(
        "rego rule `{query}` must evaluate to boolean, got {value}"
    )))
}

impl PolicyEvaluation {
    pub fn to_gateway_decision(&self, trace_id: impl Into<String>) -> PolicyGatewayDecision {
        let matched_rule = self.matched_rule.as_deref().unwrap_or("<none>").to_string();
        let source_layer = self.source_layer.as_deref().unwrap_or("<none>").to_string();
        let input_snapshot = self
            .input_snapshot
            .as_deref()
            .unwrap_or("<none>")
            .to_string();
        let reason = match self.decision {
            PolicyDecision::Allow => format!(
                "policy.allow: tool={} matched_rule={} source_layer={}",
                self.tool, matched_rule, source_layer
            ),
            PolicyDecision::Ask => format!(
                "policy.ask: tool={} matched_rule={} source_layer={} input_snapshot={}",
                self.tool, matched_rule, source_layer, input_snapshot
            ),
            PolicyDecision::Deny => format!(
                "policy.deny: tool={} matched_rule={} source_layer={} input_snapshot={}",
                self.tool, matched_rule, source_layer, input_snapshot
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct TranspiledRule {
    id: usize,
    decision: PolicyDecision,
    layer_index: usize,
    layer_name: String,
    pattern: String,
    specificity: usize,
    rule_index: usize,
    regex_pattern: String,
}

fn transpile_policy_layers_to_rego(
    global: &PolicyLayer,
    agent_profile: &PolicyLayer,
    session_override: &PolicyLayer,
) -> Result<String, AgentError> {
    let rules = collect_transpiled_rules(global, agent_profile, session_override)?;

    let mut rego = String::from("package agentd.transpiled\nimport rego.v1\n\n");
    if rules.is_empty() {
        return Ok(rego);
    }

    for rule in &rules {
        let regex_literal = serde_json::to_string(&rule.regex_pattern)
            .map_err(|err| AgentError::Runtime(format!("serialize regex pattern failed: {err}")))?;
        rego.push_str(&format!(
            "rule_{}_match if {{\n  regex.match({}, input.tool.name)\n}}\n\n",
            rule.id, regex_literal
        ));
    }

    for rule in &rules {
        rego.push_str(&format!(
            "rule_{}_wins if {{\n  rule_{}_match\n",
            rule.id, rule.id
        ));
        for higher in rules
            .iter()
            .filter(|candidate| has_higher_precedence(candidate, rule))
        {
            rego.push_str(&format!("  not rule_{}_match\n", higher.id));
        }
        rego.push_str("}\n\n");
    }

    for rule in &rules {
        match rule.decision {
            PolicyDecision::Allow => {
                rego.push_str(&format!("allow if rule_{}_wins\n\n", rule.id));
            }
            PolicyDecision::Deny => {
                rego.push_str(&format!("deny if rule_{}_wins\n\n", rule.id));
            }
            PolicyDecision::Ask => {}
        }

        let pattern_literal = serde_json::to_string(&rule.pattern)
            .map_err(|err| AgentError::Runtime(format!("serialize matched rule failed: {err}")))?;
        let layer_literal = serde_json::to_string(&rule.layer_name)
            .map_err(|err| AgentError::Runtime(format!("serialize layer name failed: {err}")))?;
        rego.push_str(&format!(
            "explain := {} if rule_{}_wins\n\n",
            pattern_literal, rule.id
        ));
        rego.push_str(&format!(
            "source_layer := {} if rule_{}_wins\n\n",
            layer_literal, rule.id
        ));
    }

    Ok(rego)
}

fn collect_transpiled_rules(
    global: &PolicyLayer,
    agent_profile: &PolicyLayer,
    session_override: &PolicyLayer,
) -> Result<Vec<TranspiledRule>, AgentError> {
    let layers = [global, agent_profile, session_override];
    let mut rules = Vec::new();

    for (layer_index, layer) in layers.iter().enumerate() {
        for (rule_index, rule) in layer.rules.iter().enumerate() {
            validate_transpilable_pattern(&layer.name, &rule.pattern)?;
            rules.push(TranspiledRule {
                id: rules.len(),
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
                regex_pattern: wildcard_pattern_to_regex(&rule.pattern),
            });
        }
    }

    Ok(rules)
}

fn validate_transpilable_pattern(layer_name: &str, pattern: &str) -> Result<(), AgentError> {
    let printable = pattern.escape_default().to_string();

    if pattern.trim().is_empty() {
        return Err(AgentError::InvalidInput(format!(
            "unsupported toml policy key `{printable}` in layer `{layer_name}`: empty pattern"
        )));
    }

    if pattern.contains('\n') || pattern.contains('\r') {
        return Err(AgentError::InvalidInput(format!(
            "unsupported toml policy key `{printable}` in layer `{layer_name}`: multiline pattern"
        )));
    }

    Ok(())
}

fn wildcard_pattern_to_regex(pattern: &str) -> String {
    let mut regex = String::from("^");
    for ch in pattern.chars() {
        match ch {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '\\' | '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$' => {
                regex.push('\\');
                regex.push(ch);
            }
            _ => regex.push(ch),
        }
    }
    regex.push('$');
    regex
}

fn has_higher_precedence(candidate: &TranspiledRule, current: &TranspiledRule) -> bool {
    let candidate_decision = decision_precedence_rank(candidate.decision);
    let current_decision = decision_precedence_rank(current.decision);

    if candidate_decision != current_decision {
        return candidate_decision < current_decision;
    }

    if candidate.layer_index != current.layer_index {
        return candidate.layer_index > current.layer_index;
    }

    if candidate.specificity != current.specificity {
        return candidate.specificity > current.specificity;
    }

    candidate.rule_index < current.rule_index
}

fn decision_precedence_rank(decision: PolicyDecision) -> u8 {
    match decision {
        PolicyDecision::Deny => 0,
        PolicyDecision::Allow => 1,
        PolicyDecision::Ask => 2,
    }
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
                input_snapshot: None,
            },
            None => PolicyEvaluation {
                tool: tool.to_string(),
                decision: PolicyDecision::Ask,
                matched_rule: None,
                source_layer: None,
                input_snapshot: None,
            },
        }
    }
}

fn pick_winner(matches: &[RuleMatch]) -> Option<RuleMatch> {
    for decision in [
        PolicyDecision::Deny,
        PolicyDecision::Allow,
        PolicyDecision::Ask,
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
    fn allow_takes_precedence_over_ask_for_more_specific_rule() {
        let global = PolicyLayer {
            name: "global".to_string(),
            rules: vec![PolicyRule {
                pattern: "*".to_string(),
                decision: PolicyDecision::Ask,
            }],
        };
        let profile = PolicyLayer {
            name: "profile".to_string(),
            rules: vec![PolicyRule {
                pattern: "builtin.lite.upper".to_string(),
                decision: PolicyDecision::Allow,
            }],
        };
        let session = PolicyLayer {
            name: "session_override".to_string(),
            rules: vec![],
        };

        let evaluation =
            PolicyLayer::evaluate_tool(&global, &profile, &session, "builtin.lite.upper");
        assert_eq!(evaluation.decision, PolicyDecision::Allow);
        assert_eq!(
            evaluation.matched_rule.as_deref(),
            Some("builtin.lite.upper")
        );
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
            input_snapshot: Some("agent.id=agent-1 trust_level=ask tool=mcp.fs.read_file resource=.env request_meta_keys=[trace_id]".to_string()),
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

    #[test]
    fn transpile_rejects_multiline_policy_key() {
        let global = PolicyLayer {
            name: "global".to_string(),
            rules: vec![],
        };
        let profile = PolicyLayer {
            name: "agent_profile".to_string(),
            rules: vec![PolicyRule {
                pattern: "mcp.fs.read\nsecret".to_string(),
                decision: PolicyDecision::Deny,
            }],
        };
        let session = PolicyLayer {
            name: "session_override".to_string(),
            rules: vec![],
        };

        let err = transpile_policy_layers_to_rego(&global, &profile, &session)
            .expect_err("multiline key should be rejected");
        let err_msg = err.to_string();
        assert!(err_msg.contains("unsupported toml policy key"));
        assert!(err_msg.contains("mcp.fs.read\\nsecret"));
    }

    #[test]
    fn transpiled_rego_matches_legacy_layer_resolution() {
        let global = PolicyLayer {
            name: "global".to_string(),
            rules: vec![PolicyRule {
                pattern: "*".to_string(),
                decision: PolicyDecision::Ask,
            }],
        };
        let profile = PolicyLayer {
            name: "agent_profile".to_string(),
            rules: vec![
                PolicyRule {
                    pattern: "mcp.fs.read:*".to_string(),
                    decision: PolicyDecision::Allow,
                },
                PolicyRule {
                    pattern: "mcp.fs.read:*.env".to_string(),
                    decision: PolicyDecision::Deny,
                },
            ],
        };
        let session = SessionPolicyOverrides {
            allow_tools: vec![],
            ask_tools: vec![],
            deny_tools: vec!["mcp.shell.exec".to_string()],
        }
        .into_layer();

        let transpiled = transpile_policy_layers_to_rego(&global, &profile, &session)
            .expect("transpile should succeed");
        let mut rego = regorus::Engine::new();
        rego.add_policy(TRANSPILED_POLICY_PATH.to_string(), transpiled)
            .expect("load transpiled rego should succeed");

        for tool in [
            "mcp.fs.read:notes.txt",
            "mcp.fs.read:secrets.env",
            "mcp.shell.exec",
            "mcp.git.status",
        ] {
            let legacy = PolicyLayer::evaluate_tool(&global, &profile, &session, tool);
            let input = PolicyInputContext {
                agent: PolicyAgentContext {
                    id: Some("agent-test".to_string()),
                    trust_level: Some("ask".to_string()),
                },
                tool: PolicyToolContext {
                    name: tool.to_string(),
                },
                resource: PolicyResourceContext { uri: None },
                time: PolicyTimeContext {
                    timestamp_rfc3339: None,
                },
                request_meta: BTreeMap::new(),
            };
            rego.set_input_json(
                &serde_json::to_string(&input).expect("serialize policy input should succeed"),
            )
            .expect("set rego input should succeed");

            let deny = eval_rule_as_bool_or_false(&mut rego, "data.agentd.transpiled.deny")
                .expect("evaluate deny should succeed");
            let allow = eval_rule_as_bool_or_false(&mut rego, "data.agentd.transpiled.allow")
                .expect("evaluate allow should succeed");
            let re_eval_decision = if deny {
                PolicyDecision::Deny
            } else if allow {
                PolicyDecision::Allow
            } else {
                PolicyDecision::Ask
            };

            assert_eq!(
                re_eval_decision, legacy.decision,
                "decision mismatch for tool {tool}"
            );
        }
    }
}
