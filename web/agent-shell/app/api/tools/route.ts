import { NextResponse } from 'next/server';
import {
  authorizeTool,
  DaemonRpcError,
  listAgents,
  listAvailableTools,
} from '@/lib/daemon-rpc';

export async function GET(req: Request) {
  const { searchParams } = new URL(req.url);
  const requestedAgentId = searchParams.get('agent_id') ?? undefined;

  try {
    const agents = await listAgents();
    const agentId = requestedAgentId ?? agents[0]?.agent_id;

    if (!agentId) {
      return NextResponse.json({
        agent_id: null,
        agents: [],
        tools: [],
      });
    }

    const tools = await listAvailableTools(agentId);
    return NextResponse.json({
      agent_id: agentId,
      agents: agents.map((agent) => ({
        agent_id: agent.agent_id,
        name: agent.name,
        model: agent.model,
      })),
      tools,
    });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : 'daemon unreachable' },
      { status: 502 },
    );
  }
}

export async function POST(req: Request) {
  try {
    const body = (await req.json()) as {
      agent_id?: string;
      tool?: string;
    };
    if (!body.agent_id?.trim() || !body.tool?.trim()) {
      return NextResponse.json(
        { error: 'agent_id and tool are required' },
        { status: 400 },
      );
    }

    const result = await authorizeTool({
      agent_id: body.agent_id.trim(),
      tool: body.tool.trim(),
    });
    return NextResponse.json(result);
  } catch (err) {
    if (
      err instanceof DaemonRpcError &&
      err.code === -32016 &&
      err.message.startsWith('policy.deny:')
    ) {
      return NextResponse.json({
        decision: 'deny',
        reason: err.message,
      });
    }
    return NextResponse.json(
      { error: err instanceof Error ? err.message : 'tool authorization failed' },
      { status: 502 },
    );
  }
}
