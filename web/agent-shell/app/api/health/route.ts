import { NextResponse } from 'next/server';
import { daemonRpc } from '@/lib/daemon-fetch';

export async function GET() {
  try {
    const result = await daemonRpc('GetHealth');
    return NextResponse.json(result);
  } catch (err) {
    return NextResponse.json(
      {
        status: 'unreachable',
        error: err instanceof Error ? err.message : 'daemon unreachable',
      },
      { status: 502 },
    );
  }
}
