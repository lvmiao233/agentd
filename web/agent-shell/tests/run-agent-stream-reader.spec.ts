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
      'data: {"result":{"status":"working","tool":{"calls":[{"id":"call_1","function":{"name":"lookup","arguments":"{\\"path\\":\\"/tmp/a\\"}"}}]}}}\n\n',
      'data: {"result":{"status":"completed","type":"done"}}\n\n',
    ]),
    textId: 'text-fragmented',
    writer,
  });

  assert.equal(fragmentedOutcome.emitted, true, 'fragmented stream should emit content');
  assert.equal(fragmentedOutcome.terminalReached, true, 'completed frame should terminate stream');
  assert.deepEqual(writes[0], {
    type: 'text-delta',
    id: 'text-fragmented',
    delta: 'Hello',
  });
  assert.deepEqual(writes[1], {
    type: 'tool-input-available',
    toolCallId: 'call_1',
    toolName: 'lookup',
    input: { path: '/tmp/a' },
  });

  writes.length = 0;
  const doneMarkerOutcome = await consumeRunAgentStream({
    responseBody: createReadableStream([': keepalive\n', 'event: message\n', 'data: [DO', 'NE]\n\n']),
    textId: 'text-done',
    writer,
  });

  assert.equal(doneMarkerOutcome.emitted, false, '[DONE] should not emit text');
  assert.equal(doneMarkerOutcome.terminalReached, true, '[DONE] should terminate stream');
  assert.equal(writes.length, 0, '[DONE] should not produce chunks');

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
  assert.deepEqual(writes[0], {
    type: 'text-delta',
    id: 'text-daemon',
    delta: 'daemon',
  });

  writes.length = 0;
  const trailingOutcome = await consumeRunAgentStream({
    responseBody: createReadableStream(['data: {"result":{"llm":{"output":"tail"}}}']),
    textId: 'text-trailing',
    writer,
  });

  assert.equal(trailingOutcome.emitted, true, 'trailing event should emit on stream end');
  assert.equal(trailingOutcome.terminalReached, false, 'non-terminal trailing event should stay non-terminal');
  assert.deepEqual(writes[0], {
    type: 'text-delta',
    id: 'text-trailing',
    delta: 'tail',
  });
}
