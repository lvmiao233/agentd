import assert from 'node:assert/strict';
import { consumeRunAgentStream } from '../lib/run-agent-stream-reader.mjs';

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
  const writes = [];
  const writer = {
    write(chunk) {
      writes.push(chunk);
    },
  };

  const fragmentedOutcome = await consumeRunAgentStream({
    responseBody: createReadableStream([
      'data: {"result":{"llm":{"ou',
      'tput":"Hel',
      'lo"}}}\n',
      '\n',
      'data: {"result":{"status":"working","tool":{"calls":[{"id":"call_start","name":"lookup","phase":"input-start"}]}}}\n\n',
      'data: {"result":{"status":"working","tool":{"calls":[{"id":"call_1","function":{"name":"lookup","arguments":"{\\"path\\":\\"/tmp/a\\"}"}}]}}}\n\n',
      'data: {"result":{"status":"completed","type":"done"}}\n\n',
    ]),
    textId: 'text-fragmented',
    writer,
  });

  assert.equal(fragmentedOutcome.emitted, true, 'fragmented stream should emit content');
  assert.equal(fragmentedOutcome.terminalReached, true, 'completed frame should terminate stream');
  assert.equal(fragmentedOutcome.finishReason, 'stop', 'completed frame should map to stop finish reason');
  assert.deepEqual(writes[0], {
    type: 'text-delta',
    id: 'text-fragmented',
    delta: 'Hello',
  });
  assert.deepEqual(writes[1], {
    type: 'tool-input-start',
    toolCallId: 'call_start',
    toolName: 'lookup',
  });
  assert.deepEqual(writes[2], {
    type: 'tool-input-available',
    toolCallId: 'call_1',
    toolName: 'lookup',
    input: { path: '/tmp/a' },
  });

  writes.length = 0;
  const toolErrorOutcome = await consumeRunAgentStream({
    responseBody: createReadableStream([
      'data: {"result":{"status":"working","tool":{"calls":[{"id":"call_error","name":"lookup","arguments":"{\\"path\\":\\"/tmp/secret\\"}","error":{"message":"denied"}}]}}}\n\n',
    ]),
    textId: 'text-tool-error',
    writer,
  });

  assert.equal(toolErrorOutcome.emitted, true, 'tool error stream should emit content');
  assert.equal(toolErrorOutcome.terminalReached, false, 'tool error frame itself should not terminate stream');
  assert.deepEqual(writes[0], {
    type: 'tool-input-available',
    toolCallId: 'call_error',
    toolName: 'lookup',
    input: { path: '/tmp/secret' },
  });
  assert.deepEqual(writes[1], {
    type: 'tool-output-error',
    toolCallId: 'call_error',
    errorText: 'denied',
  });

  writes.length = 0;
  const doneMarkerOutcome = await consumeRunAgentStream({
    responseBody: createReadableStream([': keepalive\n', 'event: message\n', 'data: [DO', 'NE]\n\n']),
    textId: 'text-done',
    writer,
  });

  assert.equal(doneMarkerOutcome.emitted, false, '[DONE] should not emit text');
  assert.equal(doneMarkerOutcome.terminalReached, true, '[DONE] should terminate stream');
  assert.equal(doneMarkerOutcome.finishReason, 'stop', '[DONE] should map to stop finish reason');
  assert.equal(writes.length, 0, '[DONE] should not produce chunks');

  writes.length = 0;
  const failedOutcome = await consumeRunAgentStream({
    responseBody: createReadableStream([
      'data: {"error":{"message":"upstream overloaded"},"status":"failed"}\n\n',
    ]),
    textId: 'text-failed',
    writer,
  });

  assert.equal(failedOutcome.emitted, true, 'failed frame should emit visible text');
  assert.equal(failedOutcome.terminalReached, true, 'failed frame should terminate stream');
  assert.equal(failedOutcome.finishReason, 'error', 'failed frame should map to error finish reason');
  assert.deepEqual(writes[0], {
    type: 'text-delta',
    id: 'text-failed',
    delta: 'RunAgent failed: upstream overloaded',
  });

  writes.length = 0;
  const rawJsonOutcome = await consumeRunAgentStream({
    responseBody: createReadableStream([
      '{"result":{"llm":{"output":"raw"}}}\n',
      '{"result":{"status":"completed"}}\n',
    ]),
    textId: 'text-raw-json',
    writer,
  });

  assert.equal(rawJsonOutcome.emitted, true, 'raw json newline stream should emit content');
  assert.equal(rawJsonOutcome.terminalReached, true, 'raw completed json frame should terminate stream');
  assert.equal(rawJsonOutcome.finishReason, 'stop', 'raw completed frame should map to stop finish reason');
  assert.deepEqual(writes[0], {
    type: 'text-delta',
    id: 'text-raw-json',
    delta: 'raw',
  });

  writes.length = 0;
  const daemonFrameOutcome = await consumeRunAgentStream({
    responseBody: createReadableStream([
      'data: {"result":{"llm":{"output":"daemon"}}}\n',
      'data: {"result":{"status":"completed"}}\n',
    ]),
    textId: 'text-daemon',
    writer,
  });

  assert.equal(daemonFrameOutcome.emitted, true, 'daemon newline-delimited frames should emit content');
  assert.equal(daemonFrameOutcome.terminalReached, true, 'daemon completed frame should terminate stream');
  assert.equal(daemonFrameOutcome.finishReason, 'stop', 'daemon completed frame should map to stop finish reason');
  assert.deepEqual(writes[0], {
    type: 'text-delta',
    id: 'text-daemon',
    delta: 'daemon',
  });

  writes.length = 0;
  const multilineSseOutcome = await consumeRunAgentStream({
    responseBody: createReadableStream([
      'data: {"result":\n',
      'data: {"llm":{"output":"multiline"}}}\n',
      '\n',
      'data: {"result":{"status":"completed"}}\n\n',
    ]),
    textId: 'text-multiline-sse',
    writer,
  });

  assert.equal(multilineSseOutcome.emitted, true, 'multi-line SSE event should emit content');
  assert.equal(
    multilineSseOutcome.terminalReached,
    true,
    'multi-line SSE stream should terminate on completed frame'
  );
  assert.equal(
    multilineSseOutcome.finishReason,
    'stop',
    'multi-line SSE completed frame should map to stop finish reason'
  );
  assert.deepEqual(writes[0], {
    type: 'text-delta',
    id: 'text-multiline-sse',
    delta: 'multiline',
  });

  writes.length = 0;
  const trailingOutcome = await consumeRunAgentStream({
    responseBody: createReadableStream(['data: {"result":{"llm":{"output":"tail"}}}']),
    textId: 'text-trailing',
    writer,
  });

  assert.equal(trailingOutcome.emitted, true, 'trailing event should emit on stream end');
  assert.equal(trailingOutcome.terminalReached, false, 'non-terminal trailing event should stay non-terminal');
  assert.equal(trailingOutcome.finishReason, null, 'non-terminal stream should not emit finish reason');
  assert.deepEqual(writes[0], {
    type: 'text-delta',
    id: 'text-trailing',
    delta: 'tail',
  });
}
