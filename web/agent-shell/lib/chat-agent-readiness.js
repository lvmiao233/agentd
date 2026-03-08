export function isAgentRunnable(agent) {
  return agent.runnable ?? agent.status === 'ready';
}

export function choosePreferredAgent(agents, preferredModel = 'gpt-5.3-codex') {
  if (agents.length === 0) {
    return null;
  }

  return (
    agents.find((agent) => isAgentRunnable(agent) && agent.model === preferredModel) ??
    agents.find((agent) => isAgentRunnable(agent)) ??
    agents[0]
  );
}

export function chooseInitialAgentSelection(params) {
  const {
    agents,
    currentAgentId = '',
    rememberedAgentId = '',
    preferredModel = 'gpt-5.3-codex',
  } = params;

  if (!Array.isArray(agents) || agents.length === 0) {
    return null;
  }

  const byId = (agentId) =>
    typeof agentId === 'string' && agentId.trim()
      ? agents.find((candidate) => candidate.agent_id === agentId)
      : undefined;

  const currentAgent = byId(currentAgentId);
  if (currentAgent && isAgentRunnable(currentAgent)) {
    return currentAgent;
  }

  const rememberedAgent = byId(rememberedAgentId);
  if (rememberedAgent && isAgentRunnable(rememberedAgent)) {
    return rememberedAgent;
  }

  return choosePreferredAgent(agents, preferredModel) ?? rememberedAgent ?? currentAgent ?? null;
}

export function buildChatAgentUnavailableMessage(agent) {
  if (!agent) {
    return 'No runnable agent is available. Create or start a ready agent first.';
  }

  if (typeof agent.runnable_reason === 'string' && agent.runnable_reason.trim()) {
    return agent.runnable_reason;
  }

  return `Agent ${agent.name} is ${agent.status} and cannot run chat requests yet. Select a ready agent first.`;
}
