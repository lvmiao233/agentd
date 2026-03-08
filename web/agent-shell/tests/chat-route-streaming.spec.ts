import assert from 'node:assert/strict';
import {
  buildConversationInput,
  handleChatPost,
  normalizeChatMessages,
} from '../lib/chat-route-handler.mjs';

function createReadableStream(chunks) {
  const encoder = new TextEncoder();
  return new ReadableStream({
    start(controller) {
      for (const chunk of chunks) {
        controller.enqueue(encoder.encode(chunk));
      }
      controller.close();
    },
  });
}

async function readResponseEvents(response) {
  assert.ok(response.body, 'response body should exist');
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let raw = '';

  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    raw += decoder.decode(value, { stream: true });
  }
  raw += decoder.decode();

  return raw
    .split('\n')
    .filter((line) => line.startsWith('data: '))
    .map((line) => line.slice(6))
    .map((payload) => (payload === '[DONE]' ? payload : JSON.parse(payload)));
}

export async function run() {
  assert.equal(
    buildConversationInput([
      {
        id: 'msg-1',
        role: 'user',
        parts: [{ type: 'text', text: '分析' }, { type: 'text', text: ' main.rs' }],
      },
      {
        id: 'msg-2',
        role: 'assistant',
        parts: [{ type: 'text', text: '好的' }],
      },
    ]),
    '[user]\n分析 main.rs\n\n[assistant]\n好的',
    'conversation input should match route formatting'
  );

  assert.equal(
    buildConversationInput([
      {
        id: 'msg-attach-1',
        role: 'user',
        parts: [
          { type: 'text', text: 'Please review the attached file.' },
          {
            type: 'file',
            filename: 'demo.tsx',
            mediaType: 'text/plain',
            url: 'data:text/plain;base64,ZXhwb3J0IGRlZmF1bHQgZnVuY3Rpb24gRGVtbygpIHsgcmV0dXJuIDxkaXY+SGk8L2Rpdj47IH0=',
          },
        ],
      },
    ]),
    [
      '[user]',
      'Please review the attached file.',
      '[attachment]',
      'filename: demo.tsx',
      'media_type: text/plain',
      'content:',
      '```tsx',
      'export default function Demo() { return <div>Hi</div>; }',
      '```',
    ].join('\n'),
    'conversation input should inline text attachments as prompt context'
  );

  const regenerateMessages = [
    {
      id: 'msg-a',
      role: 'user',
      parts: [{ type: 'text', text: 'first prompt' }],
    },
    {
      id: 'msg-b',
      role: 'assistant',
      parts: [{ type: 'text', text: 'first answer' }],
    },
    {
      id: 'msg-c',
      role: 'user',
      parts: [{ type: 'text', text: 'follow up' }],
    },
    {
      id: 'msg-d',
      role: 'assistant',
      parts: [{ type: 'text', text: 'second answer' }],
    },
  ];

  assert.deepEqual(
    normalizeChatMessages(regenerateMessages, 'regenerate-message', 'msg-d').map(
      (message) => message.id,
    ),
    ['msg-a', 'msg-b', 'msg-c'],
    'regenerate should trim the selected assistant message from the request context'
  );

  assert.deepEqual(
    normalizeChatMessages(regenerateMessages, 'regenerate-message').map(
      (message) => message.id,
    ),
    ['msg-a', 'msg-b', 'msg-c'],
    'regenerate without a message id should trim the last assistant response'
  );

  const fetchCalls = [];
  const successResponse = await handleChatPost(
    new Request('http://local.test/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          messages: [
            {
            id: 'msg-1',
            role: 'user',
            parts: [
              { type: 'text', text: '分析' },
              { type: 'text', text: ' main.rs' },
              {
                type: 'file',
                filename: 'notes.md',
                mediaType: 'text/markdown',
                url: 'data:text/markdown;base64,IyBOb3RlcwpDb25zaWRlciB0aGUgUlBDIGhhbmRsZXIu',
              },
            ],
          },
          ],
          model: 'gpt-test-model',
          agentId: 'agent-42',
          sessionId: 'web-agent-42',
          runtime: 'agent-lite',
        }),
      }),
    {
      daemonUrl: 'http://daemon.test:7000',
      fetchImpl: async (url, init) => {
        fetchCalls.push({ url, init });
        return new Response(
          createReadableStream([
            'data: {"result":{"llm":{"ou',
            'tput":"Hel',
            'lo"}}}\n',
            '\n',
            'data: {"result":{"status":"working","tool":{"calls":[{"id":"call_1","function":{"name":"lookup","arguments":"{\\"path\\":\\"/tmp/a\\"}"}}]}}}\n\n',
            ': keepalive\n',
            'event: message\n',
            'data: [DO',
            'NE]\n\n',
          ]),
          {
            status: 200,
            headers: { 'Content-Type': 'text/event-stream' },
          }
        );
      },
    }
  );

  assert.equal(fetchCalls.length, 1, 'route helper should issue one daemon request');
  assert.equal(fetchCalls[0].url, 'http://daemon.test:7000/rpc');
  assert.equal(fetchCalls[0].init.method, 'POST');
  const requestPayload = JSON.parse(fetchCalls[0].init.body);
  assert.equal(requestPayload.method, 'RunAgent');
  assert.deepEqual(requestPayload.params, {
    input: [
      '[user]',
      '分析 main.rs',
      '[attachment]',
      'filename: notes.md',
      'media_type: text/markdown',
      'content:',
      '```md',
      '# Notes',
      'Consider the RPC handler.',
      '```',
    ].join('\n'),
    model: 'gpt-test-model',
    agent_id: 'agent-42',
    session_id: 'web-agent-42',
    runtime: 'agent-lite',
    stream: true,
  });

  const fallbackSessionFetchCalls = [];
  await handleChatPost(
    new Request('http://local.test/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        id: 'chat-fallback-1',
        messages: [
          {
            id: 'msg-session-1',
            role: 'user',
            parts: [{ type: 'text', text: 'session fallback' }],
          },
        ],
        agentId: 'agent-fallback',
      }),
    }),
    {
      daemonUrl: 'http://daemon.test:7000',
      fetchImpl: async (_url, init) => {
        fallbackSessionFetchCalls.push(JSON.parse(init.body));
        return new Response(createReadableStream(['data: [DONE]\n\n']), {
          status: 200,
          headers: { 'Content-Type': 'text/event-stream' },
        });
      },
    }
  );

  assert.equal(
    fallbackSessionFetchCalls[0].params.session_id,
    'chat-fallback-1',
    'route should fall back to chat id when sessionId is omitted'
  );

  const regenerateFetchCalls = [];
  await handleChatPost(
    new Request('http://local.test/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        messages: regenerateMessages,
        trigger: 'regenerate-message',
        messageId: 'msg-d',
      }),
    }),
    {
      daemonUrl: 'http://daemon.test:7000',
      fetchImpl: async (_url, init) => {
        regenerateFetchCalls.push(JSON.parse(init.body));
        return new Response(createReadableStream(['data: [DONE]\n\n']), {
          status: 200,
          headers: { 'Content-Type': 'text/event-stream' },
        });
      },
    }
  );

  assert.equal(
    regenerateFetchCalls[0].params.input,
    '[user]\nfirst prompt\n\n[assistant]\nfirst answer\n\n[user]\nfollow up',
    'regenerate requests should exclude the assistant message being regenerated'
  );

  const successEvents = await readResponseEvents(successResponse);
  const successPayloads = successEvents.filter((event) => event !== '[DONE]');
  assert.deepEqual(
    successPayloads.map((event) => event.type),
    [
      'start',
      'start-step',
      'text-start',
      'text-delta',
      'tool-input-available',
      'text-end',
      'finish-step',
      'finish',
    ],
    'successful route stream should bridge daemon frames into UI stream events'
  );
  assert.equal(successPayloads[3].delta, 'Hello', 'fragmented daemon text should be reassembled');
  assert.deepEqual(successPayloads[4], {
    type: 'tool-input-available',
    toolCallId: 'call_1',
    toolName: 'lookup',
    input: { path: '/tmp/a' },
  });
  assert.equal(successPayloads[7].finishReason, 'stop', '[DONE] should yield stop finish reason');
  assert.equal(successEvents.at(-1), '[DONE]', 'UI response should terminate with [DONE]');

  const toolFirstResponse = await handleChatPost(
    new Request('http://local.test/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        messages: [
          {
            id: 'msg-tool-first',
            role: 'user',
            parts: [{ type: 'text', text: 'read README and answer' }],
          },
        ],
        model: 'gpt-test-model',
        agentId: 'agent-42',
        sessionId: 'web-agent-42',
        runtime: 'agent-lite',
      }),
    }),
    {
      daemonUrl: 'http://daemon.test:7000',
      fetchImpl: async () =>
        new Response(
          createReadableStream([
            'data: {"result":{"status":"working","tool":{"calls":[{"id":"call_tool_first","name":"mcp.fs.read_file","phase":"input-start"}]}}}\n\n',
            'data: {"result":{"status":"working","tool":{"calls":[{"id":"call_tool_first","name":"mcp.fs.read_file","arguments":"{\\"path\\":\\"README.md\\"}"}]}}}\n\n',
            'data: {"result":{"llm":{"output":"agentd"}}}\n\n',
            'data: [DONE]\n\n',
          ]),
          {
            status: 200,
            headers: { 'Content-Type': 'text/event-stream' },
          }
        ),
    }
  );
  const toolFirstEvents = await readResponseEvents(toolFirstResponse);
  const toolFirstPayloads = toolFirstEvents.filter((event) => event !== '[DONE]');
  assert.deepEqual(
    toolFirstPayloads.map((event) => event.type),
    [
      'start',
      'start-step',
      'tool-input-start',
      'tool-input-available',
      'text-start',
      'text-delta',
      'text-end',
      'finish-step',
      'finish',
    ],
    'tool-first daemon frames should preserve tool ordering before assistant text starts'
  );

  const toolLifecycleResponse = await handleChatPost(
    new Request('http://local.test/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        messages: [
          {
            id: 'msg-tool-life',
            role: 'user',
            parts: [{ type: 'text', text: 'trace tool lifecycle' }],
          },
        ],
      }),
    }),
    {
      daemonUrl: 'http://daemon.test:7000',
      fetchImpl: async () =>
        new Response(
          createReadableStream([
            'data: {"result":{"status":"working","tool":{"calls":[{"id":"call_life","name":"mcp.fs.read_file","phase":"input-start"}]}}}\n\n',
            'data: {"result":{"status":"working","tool":{"calls":[{"id":"call_life","name":"mcp.fs.read_file","arguments":"{\\"path\\":\\"README.md\\"}"}]}}}\n\n',
            'data: {"result":{"status":"working","tool":{"calls":[{"id":"call_life","output":{"content":"ok"}}]}}}\n\n',
            'data: [DONE]\n\n',
          ]),
          {
            status: 200,
            headers: { 'Content-Type': 'text/event-stream' },
          }
        ),
    }
  );
  const toolLifecycleEvents = await readResponseEvents(toolLifecycleResponse);
  const toolLifecyclePayloads = toolLifecycleEvents.filter((event) => event !== '[DONE]');
  assert.deepEqual(
    toolLifecyclePayloads.map((event) => event.type),
    [
      'start',
      'start-step',
      'tool-input-start',
      'tool-input-available',
      'tool-output-available',
      'finish-step',
      'finish',
    ],
    'tool lifecycle frames should preserve preparing, running, and completed phases'
  );

  const toolErrorResponse = await handleChatPost(
    new Request('http://local.test/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        messages: [
          {
            id: 'msg-tool-error',
            role: 'user',
            parts: [{ type: 'text', text: 'trace tool error lifecycle' }],
          },
        ],
      }),
    }),
    {
      daemonUrl: 'http://daemon.test:7000',
      fetchImpl: async () =>
        new Response(
          createReadableStream([
            'data: {"result":{"status":"working","tool":{"calls":[{"id":"call_error","name":"mcp.fs.read_file","phase":"input-start"}]}}}\n\n',
            'data: {"result":{"status":"working","tool":{"calls":[{"id":"call_error","name":"mcp.fs.read_file","arguments":"{\\"path\\":\\"README.md\\"}"}]}}}\n\n',
            'data: {"result":{"status":"working","tool":{"calls":[{"id":"call_error","error":{"message":"permission denied"}}]}}}\n\n',
            'data: [DONE]\n\n',
          ]),
          {
            status: 200,
            headers: { 'Content-Type': 'text/event-stream' },
          }
        ),
    }
  );
  const toolErrorEvents = await readResponseEvents(toolErrorResponse);
  const toolErrorPayloads = toolErrorEvents.filter((event) => event !== '[DONE]');
  assert.deepEqual(
    toolErrorPayloads.map((event) => event.type),
    [
      'start',
      'start-step',
      'tool-input-start',
      'tool-input-available',
      'tool-output-error',
      'finish-step',
      'finish',
    ],
    'tool errors should surface as error-state tool events instead of completed outputs'
  );

  const truncatedResponse = await handleChatPost(
    new Request('http://local.test/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        messages: [
          {
            id: 'msg-truncated',
            role: 'user',
            parts: [{ type: 'text', text: 'check truncated stream' }],
          },
        ],
      }),
    }),
    {
      daemonUrl: 'http://daemon.test:7000',
      fetchImpl: async () =>
        new Response(createReadableStream(['data: {"result":{"llm":{"output":"partial"}}}']), {
          status: 200,
          headers: { 'Content-Type': 'text/event-stream' },
        }),
    }
  );
  const truncatedEvents = await readResponseEvents(truncatedResponse);
  const truncatedPayloads = truncatedEvents.filter((event) => event !== '[DONE]');
  assert.deepEqual(
    truncatedPayloads.filter((event) => event.type === 'text-delta').map((event) => event.delta),
    ['partial', 'RunAgent stream ended before a terminal event.'],
    'truncated streams should preserve partial text and surface a terminal error hint'
  );
  assert.equal(
    truncatedPayloads.at(-1).finishReason,
    'error',
    'truncated streams should finish as error instead of stop'
  );

  const remoteDefaultFetchCalls = [];
  await handleChatPost(
    new Request('https://shell.example/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        messages: [
          {
            id: 'msg-origin-1',
            role: 'user',
            parts: [{ type: 'text', text: '检查远程默认 origin' }],
          },
        ],
      }),
    }),
    {
      fetchImpl: async (url) => {
        remoteDefaultFetchCalls.push(url);
        return new Response(createReadableStream(['data: [DONE]\n\n']), {
          status: 200,
          headers: { 'Content-Type': 'text/event-stream' },
        });
      },
    }
  );
  assert.equal(
    remoteDefaultFetchCalls[0],
    'https://shell.example/rpc',
    'non-loopback requests should default to request origin when daemonUrl is not provided'
  );

  const localDefaultFetchCalls = [];
  await handleChatPost(
    new Request('http://127.0.0.1:4173/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        messages: [
          {
            id: 'msg-origin-2',
            role: 'user',
            parts: [{ type: 'text', text: '检查本地默认 origin' }],
          },
        ],
      }),
    }),
    {
      fetchImpl: async (url) => {
        localDefaultFetchCalls.push(url);
        return new Response(createReadableStream(['data: [DONE]\n\n']), {
          status: 200,
          headers: { 'Content-Type': 'text/event-stream' },
        });
      },
    }
  );
  assert.equal(
    localDefaultFetchCalls[0],
    'http://127.0.0.1:7000/rpc',
    'loopback requests should preserve the local daemon default when daemonUrl is not provided'
  );

  const zeroHostFetchCalls = [];
  await handleChatPost(
    new Request('http://0.0.0.0:4173/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        messages: [
          {
            id: 'msg-origin-3',
            role: 'user',
            parts: [{ type: 'text', text: '检查 0.0.0.0 默认 origin' }],
          },
        ],
      }),
    }),
    {
      fetchImpl: async (url) => {
        zeroHostFetchCalls.push(url);
        return new Response(createReadableStream(['data: [DONE]\n\n']), {
          status: 200,
          headers: { 'Content-Type': 'text/event-stream' },
        });
      },
    }
  );
  assert.equal(
    zeroHostFetchCalls[0],
    'http://127.0.0.1:7000/rpc',
    '0.0.0.0 shell requests should also resolve to the local daemon default'
  );

  const failedResponse = await handleChatPost(
    new Request('http://local.test/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        messages: [
          {
            id: 'msg-3',
            role: 'user',
            parts: [{ type: 'text', text: '重试' }],
          },
        ],
      }),
    }),
    {
      fetchImpl: async () =>
        new Response(
          createReadableStream([
            'data: {"result":{"llm":{"output":"Partial"}}}\n\n',
            'data: {"error":{"message":"upstream overloaded"},"status":"failed"}\n\n',
          ]),
          {
            status: 200,
            headers: { 'Content-Type': 'text/event-stream' },
          }
        ),
    }
  );

  const failedEvents = await readResponseEvents(failedResponse);
  const failedPayloads = failedEvents.filter((event) => event !== '[DONE]');
  const failedTextDeltas = failedPayloads.filter((event) => event.type === 'text-delta');
  assert.deepEqual(
    failedTextDeltas.map((event) => event.delta),
    ['Partial', 'RunAgent failed: upstream overloaded'],
    'failed daemon stream should preserve visible text and error message'
  );
  assert.equal(
    failedPayloads.at(-1).finishReason,
    'error',
    'failed daemon stream should surface error finish reason through route response'
  );

  let emptyInputFetchCalls = 0;
  const emptyInputResponse = await handleChatPost(
    new Request('http://local.test/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        messages: [
          {
            id: 'msg-4',
            role: 'user',
            parts: [{ type: 'text', text: '   ' }],
          },
        ],
      }),
    }),
    {
      fetchImpl: async () => {
        emptyInputFetchCalls += 1;
        throw new Error('empty input should not call fetch');
      },
    }
  );

  const emptyInputEvents = await readResponseEvents(emptyInputResponse);
  const emptyInputPayloads = emptyInputEvents.filter((event) => event !== '[DONE]');
  assert.equal(emptyInputFetchCalls, 0, 'empty input should short-circuit before daemon request');
  assert.equal(
    emptyInputPayloads.find((event) => event.type === 'text-delta')?.delta,
    'Please provide a message to run.',
    'empty input should return the route fallback text'
  );
  assert.equal(
    emptyInputPayloads.at(-1).finishReason,
    'stop',
    'empty input fallback should finish successfully'
  );

  const transportFailureResponse = await handleChatPost(
    new Request('http://local.test/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        messages: [
          {
            id: 'msg-5',
            role: 'user',
            parts: [{ type: 'text', text: '检查 transport' }],
          },
        ],
      }),
    }),
    {
      fetchImpl: async () =>
        new Response('gateway timeout', {
          status: 504,
          headers: { 'Content-Type': 'text/plain' },
        }),
    }
  );

  const transportFailureEvents = await readResponseEvents(transportFailureResponse);
  const transportFailurePayloads = transportFailureEvents.filter((event) => event !== '[DONE]');
  assert.equal(
    transportFailurePayloads.find((event) => event.type === 'text-delta')?.delta,
    'RunAgent HTTP transport failed (504).',
    'transport failure should surface route fallback text'
  );
  assert.equal(
    transportFailurePayloads.at(-1).finishReason,
    'stop',
    'transport failure fallback should finish as stop'
  );

  const rejectedTransportResponse = await handleChatPost(
    new Request('http://local.test/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        messages: [
          {
            id: 'msg-6',
            role: 'user',
            parts: [{ type: 'text', text: '检查 daemon 连接失败' }],
          },
        ],
      }),
    }),
    {
      fetchImpl: async () => {
        throw new TypeError('connection refused');
      },
    }
  );

  const rejectedTransportEvents = await readResponseEvents(rejectedTransportResponse);
  const rejectedTransportPayloads = rejectedTransportEvents.filter((event) => event !== '[DONE]');
  assert.equal(
    rejectedTransportPayloads.find((event) => event.type === 'text-delta')?.delta,
    'RunAgent HTTP transport failed (connection refused).',
    'transport exceptions should degrade into fallback text instead of throwing'
  );
  assert.equal(
    rejectedTransportPayloads.at(-1).finishReason,
    'stop',
    'transport exception fallback should finish as stop'
  );

  const emptyStreamResponse = await handleChatPost(
    new Request('http://local.test/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        messages: [
          {
            id: 'msg-7',
            role: 'user',
            parts: [{ type: 'text', text: '检查空流' }],
          },
        ],
      }),
    }),
    {
      fetchImpl: async () =>
        new Response(createReadableStream(['data: {"result":{"status":"completed"}}\n\n']), {
          status: 200,
          headers: { 'Content-Type': 'text/event-stream' },
        }),
    }
  );

  const emptyStreamEvents = await readResponseEvents(emptyStreamResponse);
  const emptyStreamPayloads = emptyStreamEvents.filter((event) => event !== '[DONE]');
  assert.equal(
    emptyStreamPayloads.find((event) => event.type === 'text-delta')?.delta,
    'RunAgent returned an empty streaming response.',
    'terminal stream with no visible chunks should degrade into empty-stream fallback text'
  );
  assert.equal(
    emptyStreamPayloads.at(-1).finishReason,
    'stop',
    'empty stream fallback should finish as stop'
  );
}
