import assert from 'node:assert/strict';
import {
  buildUsageBars,
  evaluateThirdPartyOnboarding,
  summarizeDashboardState,
} from '../lib/dashboard-events-model.mjs';

export async function run() {
  const dashboard = summarizeDashboardState({
    agents: [
      { id: 'agent-dev-01', status: 'running' },
      { id: 'agent-review-02', status: 'running' },
      { id: 'agent-search-03', status: 'degraded' },
      { id: 'agent-codex-04', status: 'ready', runnable: false },
    ],
    events: [{ type: 'ToolInvoked' }],
  });

  assert.equal(dashboard.agentCount, 4, 'agent-count-card should show numeric value');
  assert.equal(dashboard.runningCount, 2, 'running agent count should be tracked');
  assert.equal(
    dashboard.degradedCount,
    2,
    'degraded agent count should include unrunnable agents even when lifecycle status is ready'
  );
  assert.equal(dashboard.latestEventType, 'ToolInvoked', 'latest event should be visible');

  const bars = buildUsageBars([640, 720, 690, 810, 760, 880, 830]);
  assert.equal(bars.length, 7, 'token-chart should build one bar per usage window');
  assert.ok(
    bars.every((bar) => bar.heightPercent > 0 && bar.heightPercent <= 100),
    'token-chart bar heights should be normalized between 1 and 100'
  );

  const zeroBars = buildUsageBars([0, 0, 0]);
  assert.deepEqual(
    zeroBars,
    [],
    'token-chart should stay empty when filtered usage totals are all zero'
  );

  const failure = evaluateThirdPartyOnboarding({
    onboardingError: 'initialize handshake failed for mcp-figma',
    currentTools: [
      {
        server: 'mcp-fs',
        tool: 'fs.read_file',
        policy_tool: 'mcp.fs.read_file',
      },
    ],
  });

  assert.equal(failure.onboardingStatus, 'failed');
  assert.equal(
    failure.builtinToolsIntact,
    true,
    'third-party handshake failure should not remove builtin tools'
  );
  assert.ok(
    failure.exposedTools.includes('mcp.fs.read_file'),
    'builtin tool should still be exposed after onboarding failure'
  );
}
