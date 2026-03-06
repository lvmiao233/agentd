import {
  createUIMessageStream,
  createUIMessageStreamResponse,
} from 'ai';
import { consumeRunAgentStream } from './run-agent-stream-reader.mjs';

const DEFAULT_DAEMON_URL = process.env.AGENTD_DAEMON_URL ?? 'http://127.0.0.1:7000';

export function buildConversationInput(messages) {
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

export function buildSingleTextStreamResponse(text) {
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

export async function handleChatPost(
  req,
  { fetchImpl = fetch, daemonUrl = DEFAULT_DAEMON_URL } = {}
) {
  const {
    messages,
    model: modelId,
    agentId,
  } = await req.json();

  const selectedModel = modelId ?? 'gpt-5.3-codex';
  const input = buildConversationInput(messages);
  if (!input) {
    return buildSingleTextStreamResponse('Please provide a message to run.');
  }

  const response = await fetchImpl(`${daemonUrl}/rpc`, {
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

      const { emitted, finishReason } = await consumeRunAgentStream({
        responseBody,
        textId,
        writer,
      });

      if (!emitted) {
        writer.write({
          type: 'text-delta',
          id: textId,
          delta: 'RunAgent returned an empty streaming response.',
        });
      }

      writer.write({ type: 'text-end', id: textId });
      writer.write({ type: 'finish-step' });
      writer.write({ type: 'finish', finishReason: finishReason ?? 'stop' });
    },
  });

  return createUIMessageStreamResponse({ stream });
}
