import { NextResponse } from 'next/server';
import { subscribeEvents } from '@/lib/daemon-rpc';

export async function GET(req: Request) {
  const { searchParams } = new URL(req.url);
  const cursor = searchParams.get('cursor') ?? undefined;
  const limit = searchParams.get('limit')
    ? Number(searchParams.get('limit'))
    : 50;

  try {
    const result = await subscribeEvents({
      cursor,
      limit,
      wait_timeout_secs: 0,
    });
    return NextResponse.json(result);
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : 'daemon unreachable' },
      { status: 502 },
    );
  }
}
