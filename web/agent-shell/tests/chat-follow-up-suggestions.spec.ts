import assert from 'node:assert/strict';

import { buildFollowUpSuggestions } from '../lib/chat-follow-up-suggestions.js';

export async function run() {
  assert.deepEqual(
    buildFollowUpSuggestions({
      status: 'streaming',
      lastAssistantText: 'working',
      hasToolParts: true,
      hasPendingApprovals: false,
    }),
    [],
  );

  assert.deepEqual(
    buildFollowUpSuggestions({
      status: 'ready',
      lastAssistantText: 'Done so far.',
      hasToolParts: true,
      hasPendingApprovals: true,
    }),
    [
      'Explain the pending approval and what will happen after I approve it.',
      'Continue coding until this task is fully finished.',
      'Run verification now and fix any issues you find.',
      'Summarize what changed and what still remains.',
    ],
  );

  assert.deepEqual(
    buildFollowUpSuggestions({
      status: 'error',
      lastAssistantText: 'Something failed.',
      hasToolParts: false,
      hasPendingApprovals: false,
    }),
    [
      'Retry the last step with a different approach.',
      'Summarize what changed and what still remains.',
      'Show the next highest-impact step and execute it.',
    ],
  );
}
