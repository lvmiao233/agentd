const LOCAL_DAEMON_HTTP_URL = 'http://127.0.0.1:7000';
const LOCAL_DAEMON_WS_URL = 'ws://127.0.0.1:7000/ws';

function isLoopbackHostname(hostname: string): boolean {
  return hostname === 'localhost' || hostname === '127.0.0.1' || hostname === '::1';
}

function trimTrailingSlash(value: string): string {
  return value.endsWith('/') ? value.slice(0, -1) : value;
}

function wsUrlFromHttpBase(httpBaseUrl: string): string {
  const url = new URL(httpBaseUrl);
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  url.pathname = '/ws';
  url.search = '';
  url.hash = '';
  return url.toString();
}

function resolveDaemonHttpUrl(): string {
  const configuredUrl =
    process.env.NEXT_PUBLIC_AGENTD_DAEMON_URL ?? process.env.AGENTD_DAEMON_URL;
  if (configuredUrl) {
    return trimTrailingSlash(configuredUrl);
  }

  if (typeof window !== 'undefined') {
    const { protocol, hostname, host } = window.location;
    if (!isLoopbackHostname(hostname)) {
      return `${protocol}//${host}`;
    }
  }

  return LOCAL_DAEMON_HTTP_URL;
}

function resolveDaemonWsUrl(): string {
  const configuredWsUrl =
    process.env.NEXT_PUBLIC_AGENTD_DAEMON_WS_URL ?? process.env.AGENTD_DAEMON_WS_URL;
  if (configuredWsUrl) {
    return configuredWsUrl;
  }

  const configuredHttpUrl =
    process.env.NEXT_PUBLIC_AGENTD_DAEMON_URL ?? process.env.AGENTD_DAEMON_URL;
  if (configuredHttpUrl) {
    return wsUrlFromHttpBase(trimTrailingSlash(configuredHttpUrl));
  }

  if (typeof window !== 'undefined') {
    const { protocol, hostname, host } = window.location;
    if (!isLoopbackHostname(hostname)) {
      return `${protocol === 'https:' ? 'wss:' : 'ws:'}//${host}/ws`;
    }
  }

  return LOCAL_DAEMON_WS_URL;
}

let rpcIdCounter = 0;
function nextId(): number {
  return ++rpcIdCounter;
}

export class DaemonRpcError extends Error {
  constructor(
    public code: number,
    message: string,
    public data?: unknown,
  ) {
    super(message);
    this.name = 'DaemonRpcError';
  }
}

type JsonRpcResponse<T = unknown> = {
  jsonrpc: string;
  id: unknown;
  result?: T;
  error?: { code: number; message: string; data?: unknown };
};

async function rpcCall<T = unknown>(method: string, params: unknown = {}): Promise<T> {
  const body = JSON.stringify({
    jsonrpc: '2.0',
    id: nextId(),
    method,
    params,
  });

  const response = await fetch(`${resolveDaemonHttpUrl()}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body,
  });

  const json: JsonRpcResponse<T> = await response.json();
  if (json.error) {
    throw new DaemonRpcError(json.error.code, json.error.message, json.error.data);
  }
  return json.result as T;
}

// --- Agent Management ---

export type AgentProfile = {
  agent_id: string;
  name: string;
  model: string;
  provider?: string;
  status: string;
  token_budget?: number;
  max_tokens?: number;
  temperature?: number;
  permission_policy?: string;
  allowed_tools: string[];
  denied_tools: string[];
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_usd: number;
  session_count: number;
  created_at: string;
  updated_at: string;
};

type RawAgentProfile = {
  agent_id?: string;
  id?: string;
  name?: string;
  model?:
    | string
    | {
        model_name?: string;
        provider?: string;
      };
  provider?: string;
  status?: string;
  permissions?: {
    policy?: string;
    allowed_tools?: string[];
    denied_tools?: string[];
  };
  total_input_tokens?: number;
  total_output_tokens?: number;
  total_cost_usd?: number;
  session_count?: number;
  created_at?: string;
  updated_at?: string;
};

function normalizeAgentProfile(raw: RawAgentProfile): AgentProfile {
  const model =
    typeof raw.model === 'string'
      ? raw.model
      : raw.model?.model_name ?? 'unknown';
  const provider =
    typeof raw.model === 'string'
      ? raw.provider
      : raw.model?.provider ?? raw.provider;

  return {
    agent_id: raw.agent_id ?? raw.id ?? 'unknown-agent',
    name: raw.name ?? 'unknown-agent',
    model,
    provider,
    status: raw.status ?? 'unknown',
    permission_policy: raw.permissions?.policy?.toLowerCase(),
    allowed_tools: raw.permissions?.allowed_tools ?? [],
    denied_tools: raw.permissions?.denied_tools ?? [],
    total_input_tokens: raw.total_input_tokens ?? 0,
    total_output_tokens: raw.total_output_tokens ?? 0,
    total_cost_usd: raw.total_cost_usd ?? 0,
    session_count: raw.session_count ?? 0,
    created_at: raw.created_at ?? '',
    updated_at: raw.updated_at ?? '',
  };
}

export type CreateAgentParams = {
  name: string;
  model: string;
  provider?: string;
  token_budget?: number;
  max_tokens?: number;
  temperature?: number;
  permission_policy?: string;
  allowed_tools?: string[];
  denied_tools?: string[];
};

export async function createAgent(params: CreateAgentParams): Promise<AgentProfile> {
  const result = await rpcCall<{ agent?: RawAgentProfile }>('CreateAgent', params);
  return normalizeAgentProfile(result.agent ?? {});
}

export async function listAgents(): Promise<AgentProfile[]> {
  const result = await rpcCall<{ agents: RawAgentProfile[] }>('ListAgents');
  return (result.agents ?? []).map(normalizeAgentProfile);
}

export async function getAgent(agentId: string, auditLimit?: number): Promise<AgentProfile> {
  const result = await rpcCall<{ profile?: RawAgentProfile }>('GetAgent', {
    agent_id: agentId,
    ...(auditLimit !== undefined && { audit_limit: auditLimit }),
  });
  return normalizeAgentProfile(result.profile ?? {});
}

export async function getAgentWithAudit(
  agentId: string,
  auditLimit?: number,
): Promise<{ profile?: RawAgentProfile; audit_events?: RuntimeEvent[] }> {
  return rpcCall<{ profile?: RawAgentProfile; audit_events?: RuntimeEvent[] }>('GetAgent', {
    agent_id: agentId,
    ...(auditLimit !== undefined && { audit_limit: auditLimit }),
  });
}

export async function deleteAgent(agentId: string): Promise<{ deleted: boolean }> {
  const result = await rpcCall<{ deleted?: boolean; success?: boolean }>('DeleteAgent', {
    agent_id: agentId,
  });
  return { deleted: result.deleted ?? result.success ?? false };
}

// --- Usage ---

export type UsageRecord = {
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_usd: number;
  records: Array<{
    model_name: string;
    input_tokens: number;
    output_tokens: number;
    cost_usd: number;
    timestamp: string;
  }>;
};

type RawUsageRecord = {
  total_input_tokens?: number;
  total_output_tokens?: number;
  total_cost_usd?: number;
  input_tokens?: number;
  output_tokens?: number;
  records?: Array<{
    model_name: string;
    input_tokens: number;
    output_tokens: number;
    cost_usd: number;
    timestamp: string;
  }>;
  model_cost_breakdown?: Array<{
    model_name: string;
    input_tokens: number;
    output_tokens: number;
    cost_usd: number;
    total_tokens?: number;
  }>;
};

function normalizeUsageRecord(raw: RawUsageRecord): UsageRecord {
  return {
    total_input_tokens: raw.total_input_tokens ?? raw.input_tokens ?? 0,
    total_output_tokens: raw.total_output_tokens ?? raw.output_tokens ?? 0,
    total_cost_usd: raw.total_cost_usd ?? 0,
    records:
      raw.records ??
      (raw.model_cost_breakdown ?? []).map((entry) => ({
        model_name: entry.model_name,
        input_tokens: entry.input_tokens,
        output_tokens: entry.output_tokens,
        cost_usd: entry.cost_usd,
        timestamp: '',
      })),
  };
}

export async function getUsage(agentId: string, window?: string): Promise<UsageRecord> {
  const result = await rpcCall<RawUsageRecord>('GetUsage', {
    agent_id: agentId,
    ...(window && { window }),
  });
  return normalizeUsageRecord(result);
}

export async function recordUsage(params: {
  agent_id: string;
  model_name: string;
  input_tokens: number;
  output_tokens: number;
  cost_usd?: number;
}): Promise<{ recorded: boolean }> {
  return rpcCall<{ recorded: boolean }>('RecordUsage', params);
}

// --- Tool Authorization ---

export type AuthorizeResult = {
  decision: 'allow' | 'ask' | 'deny';
  matched_rule?: string;
  source_layer?: string;
  reason?: string;
  trace_id?: string;
};

export async function authorizeTool(params: {
  tool: string;
  agent_id?: string;
}): Promise<AuthorizeResult> {
  return rpcCall<AuthorizeResult>('AuthorizeTool', params);
}

// --- MCP Servers ---

export type McpServer = {
  server: string;
  trust_level: string;
  health: string;
  capabilities: string[];
};

export async function listMcpServers(): Promise<McpServer[]> {
  const result = await rpcCall<{ servers: McpServer[] }>('ListMcpServers');
  return result.servers;
}

export async function onboardMcpServer(params: {
  name: string;
  command: string;
  args?: string[];
  trust_level?: string;
}): Promise<{ status: string; server: McpServer }> {
  return rpcCall<{ status: string; server: McpServer }>('OnboardMcpServer', params);
}

// --- Available Tools ---

export type AvailableTool = {
  server: string;
  tool: string;
  policy_tool: string;
  trust_level: string;
  health: string;
  decision: string;
  reason?: string;
  trace_id?: string;
};

export async function listAvailableTools(agentId: string): Promise<AvailableTool[]> {
  const result = await rpcCall<{ tools: AvailableTool[] }>('ListAvailableTools', {
    agent_id: agentId,
  });
  return result.tools;
}

export type ApprovalItem = {
  id: string;
  tool: string;
  reason: string;
  trace_id?: string;
  requested_at: string;
};

export async function listApprovalQueue(agentId: string): Promise<ApprovalItem[]> {
  const result = await rpcCall<{ approvals: ApprovalItem[] }>('ListApprovalQueue', {
    agent_id: agentId,
  });
  return result.approvals ?? [];
}

export async function resolveApproval(params: {
  agent_id: string;
  approval_id: string;
  decision: 'approve' | 'deny';
}): Promise<{ resolved: boolean; decision: string; approval_id: string }> {
  return rpcCall<{ resolved: boolean; decision: string; approval_id: string }>(
    'ResolveApproval',
    params,
  );
}

// --- Events ---

export type RuntimeEvent = {
  id: string;
  event_type: string;
  agent_id?: string;
  severity: string;
  result: string;
  tool_name?: string;
  message?: string;
  metadata?: Record<string, unknown>;
  created_at: string;
};

export type SubscribeEventsResult = {
  events: RuntimeEvent[];
  next_cursor?: string;
};

export async function subscribeEvents(params?: {
  cursor?: string;
  limit?: number;
  wait_timeout_secs?: number;
}): Promise<SubscribeEventsResult> {
  return rpcCall<SubscribeEventsResult>('SubscribeEvents', params ?? {});
}

// --- Health ---

export type HealthStatus = {
  status: string;
  subsystems: {
    daemon: string;
    protocol: string;
    storage: string;
    one_api: string;
  };
};

export async function getHealth(): Promise<HealthStatus> {
  return rpcCall<HealthStatus>('GetHealth');
}

// --- Managed Agent ---

export async function startManagedAgent(params: {
  agent_id: string;
  command: string;
  args?: string[];
  env?: Record<string, string>;
}): Promise<{ pid: number }> {
  return rpcCall<{ pid: number }>('StartManagedAgent', params);
}

// --- WebSocket Client ---

export function createDaemonWs(): WebSocket {
  return new WebSocket(resolveDaemonWsUrl());
}

export function sendWsRpc(
  ws: WebSocket,
  method: string,
  params: unknown = {},
): number {
  const id = nextId();
  ws.send(
    JSON.stringify({
      jsonrpc: '2.0',
      id,
      method,
      params,
    }),
  );
  return id;
}
