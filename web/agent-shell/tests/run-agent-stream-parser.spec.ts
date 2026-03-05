import assert from 'node:assert/strict';
import { emitRunAgentStreamLine } from '../lib/run-agent-stream.mjs';

export async function run() {
  const writes = [];
  const writer = {
    write(chunk) {
      writes.push(chunk);
    },
  };

  const toolOutcome = emitRunAgentStreamLine({
    lineRaw:
      'data: {"result":{"status":"working","tool":{"calls":[{"id":"call_1","type":"function","function":{"name":"lookup","arguments":"{\\"path\\":\\"/tmp/file.txt\\"}"}}]}}}',
    textId: 'text-1',
    writer,
  });

  assert.equal(toolOutcome.emitted, true, 'tool-call frame should emit chunks');
  assert.equal(toolOutcome.terminalReached, false, 'working frame should not terminate stream');
  assert.deepEqual(writes[0], {
    type: 'tool-input-available',
    toolCallId: 'call_1',
    toolName: 'lookup',
    input: { path: '/tmp/file.txt' },
  });

  writes.length = 0;
  const fallbackArgsOutcome = emitRunAgentStreamLine({
    lineRaw:
      'data: {"result":{"status":"working","tool":{"calls":[{"id":"call_2","type":"function","function":{"name":"lookup","arguments":"not-json"}}]}}}',
    textId: 'text-2',
    writer,
  });

  assert.equal(fallbackArgsOutcome.emitted, true, 'tool-call with plain text args should still emit');
  assert.deepEqual(writes[0], {
    type: 'tool-input-available',
    toolCallId: 'call_2',
    toolName: 'lookup',
    input: 'not-json',
  });

  writes.length = 0;
  const failedOutcome = emitRunAgentStreamLine({
    lineRaw: 'data: {"error":{"message":"upstream overloaded"},"status":"failed"}',
    textId: 'text-3',
    writer,
  });

  assert.equal(failedOutcome.emitted, true, 'failed frame should emit visible error text');
  assert.equal(failedOutcome.terminalReached, true, 'failed frame must terminate stream');
  assert.deepEqual(writes[0], {
    type: 'text-delta',
    id: 'text-3',
    delta: 'RunAgent failed: upstream overloaded',
  });

  writes.length = 0;
  const doneOutcome = emitRunAgentStreamLine({
    lineRaw: 'data: {"result":{"status":"completed","type":"done"}}',
    textId: 'text-4',
    writer,
  });

  assert.equal(doneOutcome.emitted, false, 'done frame with no payload should not emit text');
  assert.equal(doneOutcome.terminalReached, true, 'done frame must terminate stream');
  assert.equal(writes.length, 0, 'done frame should not emit extra chunks');
}
