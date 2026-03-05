import {
  createUIMessageStream,
  createUIMessageStreamResponse,
  type UIMessage,
} from 'ai';

export const maxDuration = 60;

const DAEMON_URL = process.env.AGENTD_DAEMON_URL ?? 'http://127.0.0.1:7000';

type RunAgentRpcResult = {
  output?: string;
  usage?: {
    input_tokens?: number;
    output_tokens?: number;
  };
};

type JsonRpcResponse<T> = {
  jsonrpc: string;
  id: unknown;
  result?: T;
  error?: {
    code: number;
    message: string;
    data?: unknown;
  };
};

function extractLatestUserInput(messages: UIMessage[]): string {
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    const message = messages[i];
    if (message.role !== 'user') continue;

    const textParts = message.parts
      .filter((part) => part.type === 'text')
      .map((part) => part.text)
      .join('')
      .trim();
    if (textParts) return textParts;

  }

  return '';
}

function buildSingleTextStreamResponse(text: string) {
  const stream = createUIMessageStream({
    execute: ({ writer }) => {
      const textId = `text-${Date.now()}`;
      writer.write({ type: 'start' });
      writer.write({ type: 'start-step' });
      writer.write({ type: 'text-start', id: textId });
      writer.write({ type: 'text-delta', id: textId, delta: text });
      writer.write({ type: 'text-end', id: textId });
      writer.write({ type: 'finish-step' });
      writer.write({ type: 'finish', finishReason: 'stop' });
    },
  });

  return createUIMessageStreamResponse({ stream });
}

export async function POST(req: Request) {
  const {
    messages,
    model: modelId,
    agentId,
  }: {
    messages: UIMessage[];
    model?: string;
    agentId?: string;
  } = await req.json();

  const selectedModel = modelId ?? 'gpt-5.3-codex';
  const input = extractLatestUserInput(messages);
  if (!input) {
    return buildSingleTextStreamResponse('Please provide a message to run.');
  }

  const response = await fetch(`${DAEMON_URL}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id: Date.now(),
      method: 'RunAgent',
      params: {
        input,
        model: selectedModel,
        ...(agentId ? { agent_id: agentId } : {}),
        stream: false,
      },
    }),
  });

  const json: JsonRpcResponse<RunAgentRpcResult> = await response.json();
  if (json.error) {
    const message = `RunAgent failed (${json.error.code}): ${json.error.message}`;
    return buildSingleTextStreamResponse(message);
  }

  const output = json.result?.output?.trim();
  if (!output) {
    return buildSingleTextStreamResponse('RunAgent returned an empty response.');
  }

  return buildSingleTextStreamResponse(output);
}
