import assert from 'node:assert/strict';
import {
  buildConversationInput,
  handleChatPost,
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
            parts: [{ type: 'text', text: '分析' }, { type: 'text', text: ' main.rs' }],
          },
        ],
        model: 'gpt-test-model',
        agentId: 'agent-42',
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
    input: '[user]\n分析 main.rs',
    model: 'gpt-test-model',
    agent_id: 'agent-42',
    stream: true,
  });

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
