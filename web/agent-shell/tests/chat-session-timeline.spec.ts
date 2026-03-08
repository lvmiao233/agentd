import assert from 'node:assert/strict';

import { buildChatSessionTimeline } from '../lib/chat-session-timeline.js';

export async function run() {
  const timeline = buildChatSessionTimeline({
    checkpoints: [
      { id: 'checkpoint-a-1', label: 'Investigated the bug', messageCount: 2, messageId: 'a-1', messages: [], resolvedApprovals: [], messageBranchHistory: {} },
      { id: 'checkpoint-a-2', label: 'Applied the fix', messageCount: 4, messageId: 'a-2', messages: [], resolvedApprovals: [], messageBranchHistory: {} },
    ],
    status: 'ready',
    activeMessageId: 'a-2',
  });

  assert.ok(timeline, 'timeline should be created when checkpoints exist');
  assert.equal(timeline.title, 'Session timeline');
  assert.equal(timeline.count, 2);
  assert.deepEqual(timeline.items.map((item) => item.id), ['checkpoint-a-2', 'checkpoint-a-1']);
  assert.equal(timeline.items[0].isLatest, true);
  assert.equal(timeline.items[0].isActive, true);
  assert.equal(timeline.items[0].targetId, 'chat-message-a-2');

  const activeRunTimeline = buildChatSessionTimeline({
    checkpoints: [{ id: 'checkpoint-a-2', label: 'Applied the fix', messageCount: 4, messageId: 'a-2', messages: [], resolvedApprovals: [], messageBranchHistory: {} }],
    status: 'streaming',
    activeMessageId: 'a-2',
  });

  assert.match(activeRunTimeline.items[0].description, /latest stable point/i);
  assert.equal(buildChatSessionTimeline({ checkpoints: [], status: 'ready' }), null);
}
