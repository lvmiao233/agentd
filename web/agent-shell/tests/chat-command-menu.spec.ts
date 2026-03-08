import assert from 'node:assert/strict';

import { buildChatCommandItems } from '../lib/chat-command-menu.js';

export async function run() {
  const starterItems = buildChatCommandItems({
    status: 'ready',
    lastAssistantText: '',
    hasToolParts: false,
    hasPendingApprovals: false,
    hasConversation: false,
    canRegenerate: false,
    selectedAgentRunnable: true,
  });

  assert.deepEqual(
    starterItems.map((item) => item.title),
    ['Plan and start coding', 'Inspect context first'],
    'empty chats should expose starter workflow commands',
  );

  const readyItems = buildChatCommandItems({
    status: 'ready',
    lastAssistantText: 'I updated the tool flow and can keep going.',
    hasToolParts: true,
    hasPendingApprovals: true,
    hasConversation: true,
    canRegenerate: true,
    selectedAgentRunnable: true,
  });

  assert.ok(readyItems.some((item) => item.title === 'Continue coding'), 'tool-backed chats should offer continue coding');
  assert.ok(readyItems.some((item) => item.title === 'Run verification'), 'tool-backed chats should offer verification');
  assert.ok(readyItems.some((item) => item.title === 'Explain approval'), 'pending approvals should produce an explanation command');
  assert.ok(readyItems.some((item) => item.title === 'Regenerate last answer'), 'completed chats should expose regenerate');
  assert.ok(!readyItems.some((item) => item.title === 'Stop current run'), 'ready chats should not expose stop');

  const streamingItems = buildChatCommandItems({
    status: 'streaming',
    lastAssistantText: 'Still working',
    hasToolParts: true,
    hasPendingApprovals: false,
    hasConversation: true,
    canRegenerate: true,
    selectedAgentRunnable: true,
  });

  assert.ok(streamingItems.some((item) => item.title === 'Stop current run'), 'active chats should expose a stop command');
  assert.ok(
    streamingItems.filter((item) => item.kind === 'prompt').every((item) => item.disabled),
    'prompt commands should be disabled while a run is active',
  );

  const blockedItems = buildChatCommandItems({
    status: 'ready',
    lastAssistantText: 'Need to continue the task.',
    hasToolParts: false,
    hasPendingApprovals: false,
    hasConversation: true,
    canRegenerate: false,
    selectedAgentRunnable: false,
  });

  assert.ok(
    blockedItems.every((item) => item.kind !== 'prompt' || item.disabled),
    'prompt commands should be disabled when no runnable agent is selected',
  );

  const unknownAgentItems = buildChatCommandItems({
    status: 'ready',
    lastAssistantText: '',
    hasToolParts: false,
    hasPendingApprovals: false,
    hasConversation: false,
    canRegenerate: false,
    selectedAgentRunnable: undefined,
  });

  assert.ok(
    unknownAgentItems.every((item) => item.kind !== 'prompt' || !item.disabled),
    'starter commands should stay available while agent readiness is still unknown',
  );
}
