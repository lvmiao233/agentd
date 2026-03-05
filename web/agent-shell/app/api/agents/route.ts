import { NextResponse } from 'next/server';
import { listAgents } from '@/lib/daemon-rpc';

export async function GET() {
  try {
    const agents = await listAgents();
    return NextResponse.json({ agents });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : 'daemon unreachable' },
      { status: 502 },
    );
  }
}
