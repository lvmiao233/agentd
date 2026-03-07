import { NextResponse } from 'next/server';
import { createAgent, listAgents } from '@/lib/daemon-rpc';

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

export async function POST(req: Request) {
  try {
    const body = (await req.json()) as {
      name?: string;
      model?: string;
      provider?: string;
      permission_policy?: string;
      allowed_tools?: string[];
      denied_tools?: string[];
    };
    if (!body.name?.trim() || !body.model?.trim()) {
      return NextResponse.json(
        { error: 'name and model are required' },
        { status: 400 },
      );
    }

    const agent = await createAgent({
      name: body.name.trim(),
      model: body.model.trim(),
      provider: body.provider?.trim(),
      permission_policy: body.permission_policy?.trim(),
      allowed_tools: body.allowed_tools ?? [],
      denied_tools: body.denied_tools ?? [],
    });
    return NextResponse.json({ agent });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : 'create agent failed' },
      { status: 502 },
    );
  }
}
