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
  assert.equal(toolOutcome.finishReason, null, 'working frame should not provide finish reason');
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
  assert.equal(
    fallbackArgsOutcome.finishReason,
    null,
    'non-terminal tool frame should not provide finish reason'
  );
  assert.deepEqual(writes[0], {
    type: 'tool-input-available',
    toolCallId: 'call_2',
    toolName: 'lookup',
    input: 'not-json',
  });

  writes.length = 0;
  const toolOutputOutcome = emitRunAgentStreamLine({
    lineRaw:
      'data: {"result":{"status":"working","tool":{"calls":[{"id":"call_3","type":"function","function":{"name":"lookup","arguments":"{\\"path\\":\\"/tmp/out.txt\\"}"},"output":{"text":"done"}}]}}}',
    textId: 'text-2b',
    writer,
  });

  assert.equal(toolOutputOutcome.emitted, true, 'tool output frame should emit chunks');
  assert.deepEqual(writes[0], {
    type: 'tool-input-available',
    toolCallId: 'call_3',
    toolName: 'lookup',
    input: { path: '/tmp/out.txt' },
  });
  assert.deepEqual(writes[1], {
    type: 'tool-output-available',
    toolCallId: 'call_3',
    output: { text: 'done' },
    errorText: undefined,
  });

  writes.length = 0;
  const flatToolOutcome = emitRunAgentStreamLine({
    lineRaw:
      'data: {"result":{"status":"working","tool":{"calls":[{"id":"call_flat","name":"mcp.fs.read_file","arguments":"{\\"path\\":\\"README.md\\"}","output":{"content":"ok"}}]}}}',
    textId: 'text-flat',
    writer,
  });

  assert.equal(flatToolOutcome.emitted, true, 'flat agent-lite tool frame should emit chunks');
  assert.deepEqual(writes[0], {
    type: 'tool-input-available',
    toolCallId: 'call_flat',
    toolName: 'mcp.fs.read_file',
    input: { path: 'README.md' },
  });
  assert.deepEqual(writes[1], {
    type: 'tool-output-available',
    toolCallId: 'call_flat',
    output: { content: 'ok' },
    errorText: undefined,
  });

  writes.length = 0;
  const failedOutcome = emitRunAgentStreamLine({
    lineRaw: 'data: {"error":{"message":"upstream overloaded"},"status":"failed"}',
    textId: 'text-3',
    writer,
  });

  assert.equal(failedOutcome.emitted, true, 'failed frame should emit visible error text');
  assert.equal(failedOutcome.terminalReached, true, 'failed frame must terminate stream');
  assert.equal(failedOutcome.finishReason, 'error', 'failed frame should map to error finish reason');
  assert.deepEqual(writes[0], {
    type: 'text-delta',
    id: 'text-3',
    delta: 'RunAgent failed: upstream overloaded',
  });

  writes.length = 0;
  const mixedFailedOutcome = emitRunAgentStreamLine({
    lineRaw:
      'data: {"result":{"status":"failed","llm":{"output":"partial tail"},"error":{"message":"budget exceeded"}}}',
    textId: 'text-3b',
    writer,
  });

  assert.equal(mixedFailedOutcome.emitted, true, 'mixed output+error frame should emit visible chunks');
  assert.equal(
    mixedFailedOutcome.terminalReached,
    true,
    'mixed output+error frame should still terminate the stream'
  );
  assert.equal(
    mixedFailedOutcome.finishReason,
    'error',
    'mixed output+error frame should map to error finish reason'
  );
  assert.deepEqual(writes[0], {
    type: 'text-delta',
    id: 'text-3b',
    delta: 'partial tail',
  });
  assert.deepEqual(writes[1], {
    type: 'text-delta',
    id: 'text-3b',
    delta: 'RunAgent failed: budget exceeded',
  });

  writes.length = 0;
  const doneOutcome = emitRunAgentStreamLine({
    lineRaw: 'data: {"result":{"status":"completed","type":"done"}}',
    textId: 'text-4',
    writer,
  });

  assert.equal(doneOutcome.emitted, false, 'done frame with no payload should not emit text');
  assert.equal(doneOutcome.terminalReached, true, 'done frame must terminate stream');
  assert.equal(doneOutcome.finishReason, 'stop', 'done frame should map to stop finish reason');
  assert.equal(writes.length, 0, 'done frame should not emit extra chunks');

  const providerDoneOutcome = emitRunAgentStreamLine({
    lineRaw: 'data: [DONE]',
    textId: 'text-5',
    writer,
  });
  assert.equal(providerDoneOutcome.emitted, false, '[DONE] marker should not emit text');
  assert.equal(providerDoneOutcome.terminalReached, true, '[DONE] marker should terminate stream');
  assert.equal(providerDoneOutcome.finishReason, 'stop', '[DONE] should map to stop finish reason');
}
