import assert from 'node:assert/strict';
import { WebAgentChatModel } from '../lib/web-agent-chat.mjs';

export async function run() {
  const model = new WebAgentChatModel();

  model.appendToolCall('mcp.fs.read_file', { path: 'README.md' }, 'call-1');
  model.appendToolCall(
    'mcp.fs.read_file',
    { path: 'README.md' },
    'call-1',
    { downstream: { content: 'hello' } },
  );

  const snapshot = model.snapshot();
  assert.equal(snapshot.messages.length, 1, 'same tool call id should update existing tool row');
  assert.deepEqual(snapshot.messages[0], {
    id: 'call-1',
    role: 'tool',
    toolName: 'mcp.fs.read_file',
    input: { path: 'README.md' },
    tool: 'mcp.fs.read_file',
    args: { path: 'README.md' },
    output: { downstream: { content: 'hello' } },
    errorText: undefined,
  });
}
