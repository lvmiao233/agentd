import { NextResponse } from 'next/server';
import { getHealth } from '@/lib/daemon-rpc';

export async function GET() {
  try {
    const health = await getHealth();
    return NextResponse.json(health);
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
