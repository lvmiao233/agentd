import { NextResponse } from 'next/server';
import { listApprovalQueue, resolveApproval } from '@/lib/daemon-rpc';

export async function GET(req: Request) {
  const { searchParams } = new URL(req.url);
  const agentId = searchParams.get('agent_id');
  if (!agentId) {
    return NextResponse.json(
      { error: 'agent_id is required' },
      { status: 400 },
    );
  }

  try {
    const approvals = await listApprovalQueue(agentId);
    return NextResponse.json({ approvals });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : 'list approvals failed' },
      { status: 502 },
    );
  }
}

export async function POST(req: Request) {
  try {
    const body = (await req.json()) as {
      agent_id?: string;
      approval_id?: string;
      decision?: 'approve' | 'deny';
    };
    if (!body.agent_id || !body.approval_id || !body.decision) {
      return NextResponse.json(
        { error: 'agent_id, approval_id, and decision are required' },
        { status: 400 },
      );
    }

    const result = await resolveApproval({
      agent_id: body.agent_id,
      approval_id: body.approval_id,
      decision: body.decision,
    });
    return NextResponse.json(result);
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : 'resolve approval failed' },
      { status: 502 },
    );
  }
}
