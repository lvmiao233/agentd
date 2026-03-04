import { NextResponse } from 'next/server';
import { daemonRpc } from '@/lib/daemon-fetch';

export async function GET() {
  try {
    const result = await daemonRpc('ListMcpServers');
    return NextResponse.json(result);
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
    const result = await daemonRpc('OnboardMcpServer', body);
    return NextResponse.json(result);
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : 'onboard failed' },
      { status: 502 },
    );
  }
}
