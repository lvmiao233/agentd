import { NextResponse } from 'next/server';
import { getUsage, listAgents } from '@/lib/daemon-rpc';

export async function GET(req: Request) {
  const { searchParams } = new URL(req.url);
  const agentId = searchParams.get('agent_id');
  const window = searchParams.get('window') ?? undefined;

  try {
    if (agentId) {
      const result = await getUsage(agentId, window);
      return NextResponse.json(result);
    }

    const agents = await listAgents();
    const usage = await Promise.all(
      (agents ?? []).map(async (a) => {
        const totals = await getUsage(a.agent_id, window);
        return {
          agent_id: a.agent_id,
          name: a.name,
          model: a.model,
          input_tokens: totals.total_input_tokens,
          output_tokens: totals.total_output_tokens,
          total_tokens: totals.total_input_tokens + totals.total_output_tokens,
          cost_usd: totals.total_cost_usd,
        };
      }),
    );

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
