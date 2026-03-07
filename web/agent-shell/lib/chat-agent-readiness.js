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

export function buildChatAgentUnavailableMessage(agent) {
  if (!agent) {
    return 'No runnable agent is available. Create or start a ready agent first.';
  }

  if (typeof agent.runnable_reason === 'string' && agent.runnable_reason.trim()) {
    return agent.runnable_reason;
  }

  return `Agent ${agent.name} is ${agent.status} and cannot run chat requests yet. Select a ready agent first.`;
}
