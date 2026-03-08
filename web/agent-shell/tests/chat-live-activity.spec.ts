import assert from 'node:assert/strict';

import { buildChatLiveActivity } from '../lib/chat-live-activity.js';

export async function run() {
  const active = buildChatLiveActivity([
    { id: 'user-1', role: 'user', parts: [{ type: 'text', text: 'Fix the bug' }] },
    {
      id: 'assistant-1',
      role: 'assistant',
      parts: [
        {
          type: 'dynamic-tool',
          toolName: 'mcp.fs.read_file',
          state: 'input-available',
          input: { path: 'src/app.tsx' },
        },
      ],
    },
  ]);

  assert.deepEqual(active, {
    title: 'mcp.fs.read_file',
    state: 'input-available',
    description: 'path: src/app.tsx',
    targetId: 'chat-tool-assistant-1-0',
  });

  const prefersRunningOverCompleted = buildChatLiveActivity([
    { id: 'user-2', role: 'user', parts: [{ type: 'text', text: 'Continue' }] },
    {
      id: 'assistant-2',
      role: 'assistant',
      parts: [
        { type: 'dynamic-tool', toolName: 'mcp.fs.write_file', state: 'output-available', output: { ok: true } },
        { type: 'dynamic-tool', toolName: 'mcp.shell.execute', state: 'approval-requested', input: { command: 'npm test' } },
      ],
    },
  ]);

  assert.equal(prefersRunningOverCompleted.title, 'mcp.shell.execute');
  assert.equal(prefersRunningOverCompleted.state, 'approval-requested');
  assert.equal(prefersRunningOverCompleted.targetId, 'chat-tool-assistant-2-1');

  assert.equal(buildChatLiveActivity([]), null);
}
