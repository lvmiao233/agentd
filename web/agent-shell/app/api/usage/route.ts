import { NextResponse } from 'next/server';
import { daemonRpc } from '@/lib/daemon-fetch';

type AgentSummary = {
  agent_id: string;
  name: string;
  model: string;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_usd: number;
};

export async function GET(req: Request) {
  const { searchParams } = new URL(req.url);
  const agentId = searchParams.get('agent_id');

  try {
    if (agentId) {
      const result = await daemonRpc('GetUsage', { agent_id: agentId });
      return NextResponse.json(result);
    }

    const { agents } = await daemonRpc<{ agents: AgentSummary[] }>('ListAgents');
    const usage = (agents ?? []).map((a) => ({
      agent_id: a.agent_id,
      name: a.name,
      model: a.model,
      input_tokens: a.total_input_tokens,
      output_tokens: a.total_output_tokens,
      total_tokens: a.total_input_tokens + a.total_output_tokens,
      cost_usd: a.total_cost_usd,
    }));

    const totals = usage.reduce(
      (acc, u) => ({
        input_tokens: acc.input_tokens + u.input_tokens,
        output_tokens: acc.output_tokens + u.output_tokens,
        total_tokens: acc.total_tokens + u.total_tokens,
        cost_usd: acc.cost_usd + u.cost_usd,
      }),
      { input_tokens: 0, output_tokens: 0, total_tokens: 0, cost_usd: 0 },
    );

    return NextResponse.json({ agents: usage, totals });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : 'daemon unreachable' },
      { status: 502 },
    );
  }
}
