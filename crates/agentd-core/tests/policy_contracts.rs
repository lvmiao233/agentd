use std::collections::BTreeMap;

use agentd_core::policy::{
    LayeredPolicyEngine, PolicyAgentContext, PolicyDecision, PolicyEngine, PolicyEngineLayers,
    PolicyInputContext, PolicyLayer, PolicyResourceContext, PolicyRule, PolicyTimeContext,
    PolicyToolContext,
};

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
            timestamp_rfc3339: Some("2026-03-04T17:00:00Z".to_string()),
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
            name: "  ".to_string(),
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
