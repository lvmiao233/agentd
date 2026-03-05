const DAEMON_URL = process.env.AGENTD_DAEMON_URL ?? 'http://127.0.0.1:7000';
const DAEMON_WS_URL = process.env.AGENTD_DAEMON_WS_URL ?? 'ws://127.0.0.1:7000/ws';

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

  const response = await fetch(`${DAEMON_URL}/rpc`, {
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
  return rpcCall<AgentProfile>('CreateAgent', params);
}

export async function listAgents(): Promise<AgentProfile[]> {
  const result = await rpcCall<{ agents: AgentProfile[] }>('ListAgents');
  return result.agents;
}

export async function getAgent(agentId: string, auditLimit?: number): Promise<AgentProfile> {
  return rpcCall<AgentProfile>('GetAgent', {
    agent_id: agentId,
    ...(auditLimit !== undefined && { audit_limit: auditLimit }),
  });
}

export async function deleteAgent(agentId: string): Promise<{ deleted: boolean }> {
  return rpcCall<{ deleted: boolean }>('DeleteAgent', { agent_id: agentId });
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

export async function getUsage(agentId: string, window?: string): Promise<UsageRecord> {
  return rpcCall<UsageRecord>('GetUsage', {
    agent_id: agentId,
    ...(window && { window }),
  });
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
  return new WebSocket(DAEMON_WS_URL);
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
