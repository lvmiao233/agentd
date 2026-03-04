function normalizeToolName(rawTool) {
  if (typeof rawTool !== 'string' || rawTool.trim().length === 0) {
    return null;
  }
  return rawTool.startsWith('mcp.') ? rawTool : `mcp.${rawTool}`;
}

export function summarizeDashboardState({ agents = [], events = [] } = {}) {
  const runningCount = agents.filter((agent) => agent.status === 'running').length;
  const degradedCount = agents.filter((agent) => agent.status === 'degraded').length;

  return {
    agentCount: agents.length,
    runningCount,
    degradedCount,
    latestEventType: events[0]?.type ?? 'none',
  };
}

export function buildUsageBars(tokensByWindow = []) {
  const safeSeries = tokensByWindow
    .map((value) => (typeof value === 'number' && value > 0 ? value : 1))
    .slice(0, 12);
  const max = Math.max(...safeSeries, 1);
  return safeSeries.map((value) => ({
    value,
    heightPercent: Math.round((value / max) * 100),
  }));
}

export function evaluateThirdPartyOnboarding({
  currentTools = [],
  onboardingError = null,
} = {}) {
  const normalizedTools = currentTools
    .map((tool) => ({
      ...tool,
      policy_tool: normalizeToolName(tool?.policy_tool ?? tool?.tool ?? ''),
    }))
    .filter((tool) => typeof tool.policy_tool === 'string');

  const builtinTools = normalizedTools.filter(
    (tool) => tool.server === 'mcp-fs' || tool.server === 'mcp-search'
  );

  return {
    onboardingStatus: onboardingError ? 'failed' : 'onboarded',
    onboardingError,
    builtinToolsIntact: builtinTools.length > 0,
    exposedTools: normalizedTools.map((tool) => tool.policy_tool),
  };
}
