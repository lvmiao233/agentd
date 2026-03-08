import assert from 'node:assert/strict';

import {
  appendChatCheckpoint,
  createChatCheckpoint,
  pruneChatCheckpoints,
} from '../lib/chat-checkpoints.js';

export async function run() {
  const messages = [
    { id: 'u-1', role: 'user', parts: [{ type: 'text', text: 'Investigate the bug' }] },
    { id: 'a-1', role: 'assistant', parts: [{ type: 'text', text: 'I inspected the failing handler and found the issue.' }] },
  ];

  const checkpoint = createChatCheckpoint({
    messages,
    assistantMessage: messages[1],
    resolvedApprovals: [{ id: 'approval-1', tool: 'mcp.shell.execute', decision: 'approve', resolvedAt: '2026-03-08T02:00:00.000Z', requested_at: '2026-03-08T01:59:00.000Z' }],
    messageBranchHistory: { 'u-1': [messages[1]] },
  });

  assert.equal(checkpoint.id, 'checkpoint-a-1');
  assert.equal(checkpoint.messageId, 'a-1');
  assert.match(checkpoint.label, /inspected the failing handler/i);
  assert.notEqual(checkpoint.messages, messages, 'checkpoint should clone messages instead of keeping live references');

  const appended = appendChatCheckpoint([], checkpoint);
  assert.equal(appended.length, 1);
  const deduped = appendChatCheckpoint(appended, checkpoint);
  assert.equal(deduped.length, 1, 'same assistant checkpoint should not duplicate');

  const pruned = pruneChatCheckpoints(
    [
      checkpoint,
      { ...checkpoint, id: 'checkpoint-a-2', messageId: 'a-2', messageCount: 4, label: 'Later state' },
    ],
    2,
  );
  assert.deepEqual(pruned.map((item) => item.messageId), ['a-1']);
}
