function normalizeToolName(rawTool) {
  if (typeof rawTool !== 'string' || rawTool.trim().length === 0) {
    return null;
  }
  return rawTool.startsWith('mcp.') ? rawTool : `mcp.${rawTool}`;
}

export function summarizeDashboardState({ agents = [], events = [] } = {}) {
  const runningCount = agents.filter((agent) => agent.status === 'running').length;
  const degradedCount = agents.filter(
    (agent) => agent.status === 'degraded' || agent.runnable === false
  ).length;

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
  currentServers = [],
  onboardingError = null,
} = {}) {
  const normalizedTools = currentTools
    .map((tool) => ({
      ...tool,
      policy_tool: normalizeToolName(tool?.policy_tool ?? tool?.tool ?? ''),
    }))
    .filter((tool) => typeof tool.policy_tool === 'string');

  const normalizedServerTools = currentServers.flatMap((server) => {
    if (!server || typeof server !== 'object') {
      return [];
    }

    const serverId = typeof server.server === 'string' ? server.server : 'unknown';
    const capabilities = Array.isArray(server.capabilities) ? server.capabilities : [];
    return capabilities
      .filter((capability) => typeof capability === 'string' && capability.trim().length > 0)
      .map((capability) => ({
        server: serverId,
        policy_tool: normalizeToolName(capability),
      }))
      .filter((tool) => typeof tool.policy_tool === 'string');
  });

  const allTools = [...normalizedTools, ...normalizedServerTools];

  const builtinTools = allTools.filter(
    (tool) =>
      tool.server === 'mcp-fs' ||
      tool.server === 'mcp-search' ||
      tool.server === 'mcp-shell' ||
      tool.server === 'mcp-git'
  );
  const healthyServers = currentServers.filter((server) => server?.health === 'healthy').length;

  return {
    onboardingStatus: onboardingError
      ? 'failed'
      : currentServers.length > 0 || allTools.length > 0
        ? 'onboarded'
        : 'idle',
    onboardingError,
    builtinToolsIntact: builtinTools.length > 0,
    exposedTools: allTools.map((tool) => tool.policy_tool),
    healthyServerCount: healthyServers,
    serverCount: currentServers.length,
  };
}
