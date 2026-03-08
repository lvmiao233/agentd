import assert from 'node:assert/strict';

import { buildChatCockpitPlan } from '../lib/chat-cockpit-plan.js';

export async function run() {
  const blocked = buildChatCockpitPlan({
    status: 'ready',
    runOverview: null,
    approvalCount: 2,
    checkpointCount: 3,
    lastUserText: 'Finish the refactor',
    lastAssistantText: 'Waiting',
    selectedAgentRunnable: true,
  });

  assert.equal(blocked.title, 'Resolve the blocker and continue');
  assert.equal(blocked.mode, 'blocked');
  assert.match(blocked.description, /2 approvals pending/i);
  assert.equal(blocked.highlights[0].label, 'Current objective');
  assert.match(blocked.highlights[1].value, /2 approvals pending/i);

  const streaming = buildChatCockpitPlan({
    status: 'streaming',
    runOverview: { statusLabel: 'Running tools and drafting the reply', statusSummary: '1 tool active', sections: [] },
    approvalCount: 0,
    checkpointCount: 1,
    lastUserText: 'Continue coding',
    lastAssistantText: '',
    selectedAgentRunnable: true,
    nextActionTitle: 'Run verification',
  });

  assert.equal(streaming.title, 'Running tools and drafting the reply');
  assert.equal(streaming.mode, 'streaming');
  assert.equal(streaming.isStreaming, true);
  assert.equal(streaming.highlights[2].value, 'Run verification');

  const resumed = buildChatCockpitPlan({
    status: 'ready',
    runOverview: { statusLabel: 'Latest run completed', statusSummary: '1 completed', sections: [] },
    approvalCount: 0,
    checkpointCount: 2,
    lastUserText: 'Continue the implementation after the fix',
    lastAssistantText: 'Done',
    selectedAgentRunnable: true,
  });

  assert.equal(resumed.title, 'Continue the current coding session');
  assert.equal(resumed.mode, 'resumable');
  assert.equal(resumed.defaultOpen, false);
  assert.match(resumed.highlights[1].value, /latest stable checkpoint/i);

  const unrunnable = buildChatCockpitPlan({
    status: 'ready',
    runOverview: null,
    approvalCount: 0,
    checkpointCount: 0,
    lastUserText: '',
    lastAssistantText: '',
    selectedAgentRunnable: false,
  });

  assert.equal(unrunnable.title, 'Select a runnable agent');
  assert.equal(unrunnable.mode, 'unrunnable');
  assert.equal(unrunnable.highlights[2].value, 'Choose a runnable agent.');
}
