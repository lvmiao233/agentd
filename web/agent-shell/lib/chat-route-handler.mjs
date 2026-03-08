import {
  createUIMessageStream,
  createUIMessageStreamResponse,
} from 'ai';
import { buildAttachmentPromptSection } from './chat-attachments.js';
import { consumeRunAgentStream } from './run-agent-stream-reader.mjs';

const LOCAL_DAEMON_URL = 'http://127.0.0.1:7000';

function isLoopbackHostname(hostname) {
  return hostname === 'localhost' || hostname === '127.0.0.1' || hostname === '0.0.0.0' || hostname === '::1';
}

function resolveDaemonUrl(req, daemonUrlOverride) {
  if (typeof daemonUrlOverride === 'string' && daemonUrlOverride.trim()) {
    return daemonUrlOverride.replace(/\/$/, '');
  }

  if (typeof process.env.AGENTD_DAEMON_URL === 'string' && process.env.AGENTD_DAEMON_URL.trim()) {
    return process.env.AGENTD_DAEMON_URL.replace(/\/$/, '');
  }

  const requestUrl = new URL(req.url);
  if (isLoopbackHostname(requestUrl.hostname)) {
    return LOCAL_DAEMON_URL;
  }

  return requestUrl.origin;
}

export function buildConversationInput(messages) {
  const normalized = messages
    .map((message) => {
      const content = message.parts
        .filter((part) => part.type === 'text')
        .map((part) => part.text)
        .join('')
        .trim();
      const attachmentSection = buildAttachmentPromptSection(message.parts);

      if (!content && !attachmentSection) {
        return '';
      }

      return [
        `[${message.role}]`,
        content || null,
        attachmentSection || null,
      ]
        .filter(Boolean)
        .join('\n');
    })
    .filter(Boolean);

  return normalized.join('\n\n').trim();
}

export function normalizeChatMessages(messages, trigger, messageId) {
  if (!Array.isArray(messages)) {
    return [];
  }

  if (trigger !== 'regenerate-message') {
    return messages;
  }

  if (typeof messageId === 'string' && messageId.trim()) {
    const messageIndex = messages.findIndex((message) => message.id === messageId);
    if (messageIndex >= 0) {
      return messages.slice(0, messageIndex);
    }
  }

  const lastAssistantIndex = [...messages]
    .map((message, index) => ({ index, role: message?.role }))
    .reverse()
    .find((message) => message.role === 'assistant')?.index;

  if (typeof lastAssistantIndex === 'number') {
    return messages.slice(0, lastAssistantIndex);
  }

  return messages;
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

function describeTransportFailure(error) {
  if (error instanceof Error) {
    const message = error.message.trim();
    if (message) {
      return `RunAgent HTTP transport failed (${message}).`;
    }
  }
  return 'RunAgent HTTP transport failed.';
}

export async function handleChatPost(
  req,
  { fetchImpl = fetch, daemonUrl } = {}
) {
  const {
    id,
    messages,
    trigger,
    messageId,
    model: modelId,
    agentId,
    sessionId,
    runtime,
  } = await req.json();

  const selectedModel = modelId ?? 'gpt-5.3-codex';
  const normalizedMessages = normalizeChatMessages(messages, trigger, messageId);
  const resolvedSessionId =
    typeof sessionId === 'string' && sessionId.trim()
      ? sessionId
      : typeof id === 'string' && id.trim()
        ? id
        : typeof agentId === 'string' && agentId.trim()
          ? `web-${agentId}`
          : undefined;
  const input = buildConversationInput(normalizedMessages);
  const resolvedDaemonUrl = resolveDaemonUrl(req, daemonUrl);
  if (!input) {
    return buildSingleTextStreamResponse('Please provide a message to run.');
  }

  let response;
  try {
    response = await fetchImpl(`${resolvedDaemonUrl}/rpc`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      signal: req.signal,
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: Date.now(),
        method: 'RunAgent',
        params: {
          input,
          model: selectedModel,
          ...(agentId ? { agent_id: agentId } : {}),
          ...(resolvedSessionId ? { session_id: resolvedSessionId } : {}),
          ...(runtime ? { runtime } : {}),
          stream: true,
        },
      }),
    });
  } catch (error) {
    return buildSingleTextStreamResponse(describeTransportFailure(error));
  }

  if (!response.ok || !response.body) {
    return buildSingleTextStreamResponse(`RunAgent HTTP transport failed (${response.status}).`);
  }
  const responseBody = response.body;

  const stream = createUIMessageStream({
    execute: async ({ writer }) => {
      const textId = `text-${Date.now()}`;
      let textStarted = false;
      writer.write({ type: 'start' });
      writer.write({ type: 'start-step' });

      const streamingWriter = {
        write(chunk) {
          if (chunk?.type === 'text-delta' && !textStarted) {
            writer.write({ type: 'text-start', id: textId });
            textStarted = true;
          }
          writer.write(chunk);
        },
      };

      const { emitted, terminalReached, finishReason } = await consumeRunAgentStream({
        responseBody,
        textId,
        writer: streamingWriter,
      });

      if (!emitted) {
        if (!textStarted) {
          writer.write({ type: 'text-start', id: textId });
          textStarted = true;
        }
        writer.write({
          type: 'text-delta',
          id: textId,
          delta: 'RunAgent returned an empty streaming response.',
        });
      }

      if (emitted && !terminalReached) {
        if (!textStarted) {
          writer.write({ type: 'text-start', id: textId });
          textStarted = true;
        }
        writer.write({
          type: 'text-delta',
          id: textId,
          delta: 'RunAgent stream ended before a terminal event.',
        });
      }

      if (textStarted) {
        writer.write({ type: 'text-end', id: textId });
      }
      writer.write({ type: 'finish-step' });
      writer.write({
        type: 'finish',
        finishReason: terminalReached ? finishReason ?? 'stop' : 'error',
      });
    },
  });

  return createUIMessageStreamResponse({ stream });
}
