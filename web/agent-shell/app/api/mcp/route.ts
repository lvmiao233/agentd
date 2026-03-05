import { NextResponse } from 'next/server';
import { listMcpServers, onboardMcpServer } from '@/lib/daemon-rpc';

export async function GET() {
  try {
    const servers = await listMcpServers();
    return NextResponse.json({ servers });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : 'daemon unreachable' },
      { status: 502 },
    );
  }
}

export async function POST(req: Request) {
  try {
    const body = await req.json();
    const result = await onboardMcpServer(body);
    return NextResponse.json(result);
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : 'onboard failed' },
      { status: 502 },
    );
  }
}
