import { NextResponse } from 'next/server';
import { listAgents, listAvailableTools } from '@/lib/daemon-rpc';

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
