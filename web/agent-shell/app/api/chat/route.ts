import {
  createUIMessageStream,
  createUIMessageStreamResponse,
  type UIMessage,
} from 'ai';

export const maxDuration = 60;

const DAEMON_URL = process.env.AGENTD_DAEMON_URL ?? 'http://127.0.0.1:7000';

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

type RunAgentStreamPayload = Record<string, unknown>;

type RunAgentToolCall = {
  id: string;
  name: string;
  argumentsText: string;
};

function buildConversationInput(messages: UIMessage[]): string {
  const normalized = messages
    .map((message) => {
      const content = message.parts
        .filter((part) => part.type === 'text')
        .map((part) => part.text)
        .join('')
        .trim();
      if (!content) return '';
      return `[${message.role}]\n${content}`;
    })
    .filter(Boolean);

  return normalized.join('\n\n').trim();
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

function normalizeStreamPayload(frame: RunAgentStreamPayload): RunAgentStreamPayload {
  const maybeResult = frame.result;
  if (maybeResult && typeof maybeResult === 'object') {
    return maybeResult as RunAgentStreamPayload;
  }
  return frame;
}

function extractStreamError(frame: RunAgentStreamPayload): string | null {
  const payload = normalizeStreamPayload(frame);
  const error = payload.error;
  if (typeof error === 'string' && error.trim()) return error;
  if (error && typeof error === 'object') {
    const message = (error as Record<string, unknown>).message;
    if (typeof message === 'string' && message.trim()) return message;
  }
  const message = payload.message;
  if (typeof message === 'string' && message.trim()) return message;
  const status = payload.status;
  if (status === 'failed' || status === 'blocked') {
    return 'RunAgent streaming failed.';
  }
  return null;
}

function extractStreamText(frame: RunAgentStreamPayload): string {
  const payload = normalizeStreamPayload(frame);
  const llm = payload.llm;
  if (llm && typeof llm === 'object') {
    const output = (llm as Record<string, unknown>).output;
    if (typeof output === 'string' && output.length > 0) {
      return output;
    }
  }
  for (const field of ['delta', 'token', 'text', 'content', 'output']) {
    const value = payload[field];
    if (typeof value === 'string' && value.length > 0) {
      return value;
    }
  }
  return '';
}

function extractStreamToolCalls(frame: RunAgentStreamPayload): RunAgentToolCall[] {
  const payload = normalizeStreamPayload(frame);

  const toolContainer = payload.tool;
  if (!toolContainer || typeof toolContainer !== 'object') {
    return [];
  }

  const calls = (toolContainer as Record<string, unknown>).calls;
  if (!Array.isArray(calls)) {
    return [];
  }

  return calls
    .map((call, index) => {
      if (!call || typeof call !== 'object') return null;
      const raw = call as Record<string, unknown>;

      const idValue = raw.id;
      const id = typeof idValue === 'string' && idValue.trim() ? idValue : `call-${index}`;

      const functionValue = raw.function;
      let name = 'unknown_tool';
      let argumentsText = '';

      if (functionValue && typeof functionValue === 'object') {
        const fn = functionValue as Record<string, unknown>;
        const nameValue = fn.name;
        if (typeof nameValue === 'string' && nameValue.trim()) {
          name = nameValue;
        }

        const argumentsValue = fn.arguments;
        if (typeof argumentsValue === 'string') {
          argumentsText = argumentsValue;
        }
      }

      return {
        id,
        name,
        argumentsText,
      };
    })
    .filter((entry): entry is RunAgentToolCall => entry !== null);
}

function formatToolCallDelta(calls: RunAgentToolCall[]): string {
  if (calls.length === 0) return '';

  const lines = calls.map((call) => {
    const rawArgs = call.argumentsText.trim();
    if (!rawArgs) {
      return `\n[tool] ${call.name}`;
    }

    const compactArgs = rawArgs.replaceAll(/\s+/g, ' ');
    const preview =
      compactArgs.length > 160 ? `${compactArgs.slice(0, 160)}…` : compactArgs;
    return `\n[tool] ${call.name} ${preview}`;
  });

  return lines.join('');
}

function isTerminalStreamFrame(frame: RunAgentStreamPayload): boolean {
  const payload = normalizeStreamPayload(frame);
  const status = payload.status;
  if (status === 'completed' || status === 'done' || status === 'failed' || status === 'blocked') {
    return true;
  }
  const kind = payload.type ?? payload.event ?? payload.kind;
  return kind === 'done' || kind === 'completed' || kind === 'finish' || kind === 'finished';
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
  const input = buildConversationInput(messages);
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
        stream: true,
      },
    }),
  });

  if (!response.ok || !response.body) {
    return buildSingleTextStreamResponse(`RunAgent HTTP transport failed (${response.status}).`);
  }
  const responseBody = response.body;

  const stream = createUIMessageStream({
    execute: async ({ writer }) => {
      const textId = `text-${Date.now()}`;
      writer.write({ type: 'start' });
      writer.write({ type: 'start-step' });
      writer.write({ type: 'text-start', id: textId });

      let emitted = false;
      const reader = responseBody.getReader();
      const decoder = new TextDecoder();
      let pending = '';
      let terminalReached = false;

      const handleLine = (lineRaw: string) => {
        let line = lineRaw.trim();
        if (!line) return;
        if (line.startsWith('data:')) {
          line = line.slice(5).trim();
        }
        if (!line) return;

        let parsed: RunAgentStreamPayload | null = null;
        try {
          parsed = JSON.parse(line) as RunAgentStreamPayload;
        } catch {
          writer.write({ type: 'text-delta', id: textId, delta: line });
          emitted = true;
          return;
        }

        const errorMessage = extractStreamError(parsed);
        if (errorMessage) {
          writer.write({
            type: 'text-delta',
            id: textId,
            delta: `RunAgent failed: ${errorMessage}`,
          });
          emitted = true;
          terminalReached = true;
          return;
        }

        const chunk = extractStreamText(parsed);
        if (chunk) {
          writer.write({ type: 'text-delta', id: textId, delta: chunk });
          emitted = true;
        }

        const toolCalls = extractStreamToolCalls(parsed);
        if (toolCalls.length > 0) {
          const toolDelta = formatToolCallDelta(toolCalls);
          if (toolDelta) {
            writer.write({ type: 'text-delta', id: textId, delta: toolDelta });
            emitted = true;
          }
        }

        if (isTerminalStreamFrame(parsed)) {
          terminalReached = true;
        }
      };

      while (!terminalReached) {
        const { done, value } = await reader.read();
        if (done) break;
        pending += decoder.decode(value, { stream: true });

        let newlineIndex = pending.indexOf('\n');
        while (newlineIndex >= 0) {
          const line = pending.slice(0, newlineIndex);
          pending = pending.slice(newlineIndex + 1);
          handleLine(line);
          if (terminalReached) break;
          newlineIndex = pending.indexOf('\n');
        }
      }

      const trailing = pending.trim();
      if (!terminalReached && trailing) {
        handleLine(trailing);
      }

      if (!emitted) {
        writer.write({
          type: 'text-delta',
          id: textId,
          delta: 'RunAgent returned an empty streaming response.',
        });
      }

      writer.write({ type: 'text-end', id: textId });
      writer.write({ type: 'finish-step' });
      writer.write({ type: 'finish', finishReason: 'stop' });
    },
  });

  return createUIMessageStreamResponse({ stream });
}
