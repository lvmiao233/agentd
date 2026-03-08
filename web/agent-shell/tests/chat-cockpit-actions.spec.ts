import assert from 'node:assert/strict';

import { buildChatCockpitActions } from '../lib/chat-cockpit-actions.js';

export async function run() {
  const actions = buildChatCockpitActions({
    runOverview: {
      statusLabel: 'Waiting for approval',
      statusSummary: '1 approval pending',
      sections: [
        {
          key: 'current-turn',
          title: 'Current turn',
          count: 2,
          defaultOpen: true,
          items: [
            { key: 'goal', title: 'Start the coding task', description: 'Latest user instruction', completed: false, tone: 'default', targetId: 'chat-message-user-1' },
            { key: 'assistant-state', title: 'Waiting for approval', description: 'blocked', completed: false, tone: 'warning', targetId: 'chat-message-assistant-1' },
          ],
        },
      ],
    },
    approvalQueue: [
      { id: 'approval-1', tool: 'mcp.shell.execute', reason: 'Need approval', requested_at: '2026-03-08T05:00:00.000Z' },
    ],
    resumeActions: [
      { id: 'continue', group: 'workflow', kind: 'prompt', title: 'Continue coding', description: 'Continue', prompt: 'Continue coding until this task is fully finished.', disabled: false, keywords: [] },
    ],
  });

  assert.deepEqual(actions.objective, {
    kind: 'navigate',
    label: 'Jump to instruction',
    targetId: 'chat-message-user-1',
  });
  assert.deepEqual(actions.blocker, {
    kind: 'navigate',
    label: 'Review blocker',
    targetId: 'chat-approval-approval-1',
  });
  assert.equal(actions.next?.kind, 'command');
  assert.equal(actions.next?.label, 'Continue coding');

  const fallback = buildChatCockpitActions({
    runOverview: {
      statusLabel: 'Latest run completed',
      statusSummary: '1 completed',
      sections: [
        {
          key: 'current-turn',
          title: 'Current turn',
          count: 1,
          defaultOpen: true,
          items: [
            { key: 'assistant-state', title: 'Latest run completed', description: 'done', completed: true, tone: 'default', targetId: 'chat-message-assistant-2' },
          ],
        },
      ],
    },
    approvalQueue: [],
    resumeActions: [],
  });

  assert.equal(fallback.objective, null);
  assert.deepEqual(fallback.blocker, {
    kind: 'navigate',
    label: 'Inspect status',
    targetId: 'chat-message-assistant-2',
  });
  assert.equal(fallback.next, null);
}
