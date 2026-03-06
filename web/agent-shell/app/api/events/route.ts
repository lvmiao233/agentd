import { NextResponse } from 'next/server';
import { getAgentWithAudit, listAgents } from '@/lib/daemon-rpc';

function normalizeEvent(event: Record<string, unknown>) {
  const payload =
    event.payload && typeof event.payload === 'object'
      ? (event.payload as Record<string, unknown>)
      : {};
  return {
    id: typeof event.event_id === 'string' ? event.event_id : String(event.id ?? 'unknown'),
    event_type:
      typeof event.event_type === 'string' ? event.event_type : String(event.type ?? 'unknown'),
    agent_id: typeof event.agent_id === 'string' ? event.agent_id : undefined,
    severity: typeof event.severity === 'string' ? event.severity : 'info',
    result: typeof event.result === 'string' ? event.result : 'success',
    tool_name: typeof payload.tool_name === 'string' ? payload.tool_name : undefined,
    message: typeof payload.message === 'string' ? payload.message : undefined,
    metadata:
      payload.metadata && typeof payload.metadata === 'object'
        ? (payload.metadata as Record<string, unknown>)
        : payload,
    created_at:
      typeof event.timestamp === 'string'
        ? event.timestamp
        : String(event.created_at ?? new Date().toISOString()),
  };
}

export async function GET(req: Request) {
  const { searchParams } = new URL(req.url);
  const limit = searchParams.get('limit')
    ? Number(searchParams.get('limit'))
    : 50;

  try {
    const agents = await listAgents();
    const auditResults = await Promise.all(
      (agents ?? []).map(async (agent) => getAgentWithAudit(agent.agent_id, limit)),
    );
    const events = auditResults
      .flatMap((result) => result.audit_events ?? [])
      .map((event) => normalizeEvent(event as Record<string, unknown>))
      .sort((left, right) => right.created_at.localeCompare(left.created_at))
      .slice(0, limit);

    return NextResponse.json({
      events,
      next_cursor: events.at(-1)?.id ?? null,
    });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : 'daemon unreachable' },
      { status: 502 },
    );
  }
}
