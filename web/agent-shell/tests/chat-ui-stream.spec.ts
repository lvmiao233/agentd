import assert from 'node:assert/strict';
import { consumeChatUiStream } from '../lib/chat-ui-stream.mjs';

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

export async function run() {
  const deltas = [];
  const toolStarts = [];
  const tools = [];
  const toolOutputs = [];
  const finishes = [];

  await consumeChatUiStream(
    new Response(
      createReadableStream([
        'data: {"type":"start"}\n',
        'data: {"type":"text-delta","delta":"Hel"}\n',
        'data: {"type":"text-delta","delta":"lo"}\n',
        'data: {"type":"tool-input-start","toolCallId":"call-1","toolName":"lookup"}\n',
        'data: {"type":"tool-input-available","toolCallId":"call-1","toolName":"lookup","input":{"path":"README.md"}}\n',
        'data: {"type":"tool-output-error","toolCallId":"call-1","errorText":"denied"}\n',
        'data: {"type":"finish","finishReason":"stop"}\n',
      ]),
    ),
    {
      onAssistantDelta: (delta) => deltas.push(delta),
      onToolInputStart: (event) => toolStarts.push(event),
      onToolInput: (event) => tools.push(event),
      onToolOutput: (event) => toolOutputs.push(event),
      onFinish: (finishReason) => finishes.push(finishReason),
    },
  );

  assert.deepEqual(deltas, ['Hel', 'lo']);
  assert.deepEqual(toolStarts, [
    {
      toolCallId: 'call-1',
      toolName: 'lookup',
    },
  ]);
  assert.deepEqual(tools, [
    {
      toolCallId: 'call-1',
      toolName: 'lookup',
      input: { path: 'README.md' },
    },
  ]);
  assert.deepEqual(toolOutputs, [
    {
      toolCallId: 'call-1',
      output: undefined,
      errorText: 'denied',
    },
  ]);
  assert.deepEqual(finishes, ['stop']);
}
