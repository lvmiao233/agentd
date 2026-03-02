use serde::{Deserialize, Serialize};

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
}
