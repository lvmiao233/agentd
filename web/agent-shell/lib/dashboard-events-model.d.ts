export declare function summarizeDashboardState(params?: {
  agents?: Array<{ status?: string }>;
  events?: Array<{ type?: string }>;
}): {
  agentCount: number;
  runningCount: number;
  degradedCount: number;
  latestEventType: string;
};

export declare function buildUsageBars(tokensByWindow?: number[]): Array<{
  value: number;
  heightPercent: number;
}>;

export declare function evaluateThirdPartyOnboarding(params?: {
  currentTools?: Array<{ server?: string; tool?: string; policy_tool?: string }>;
  currentServers?: Array<{
    server?: string;
    health?: string;
    capabilities?: string[];
  }>;
  onboardingError?: string | null;
}): {
  onboardingStatus: 'idle' | 'onboarded' | 'failed';
  onboardingError: string | null;
  builtinToolsIntact: boolean;
  exposedTools: Array<string | null>;
  healthyServerCount: number;
  serverCount: number;
};
