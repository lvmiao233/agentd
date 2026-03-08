import assert from 'node:assert/strict';

import { buildChatResumeActions } from '../lib/chat-resume-actions.js';

export async function run() {
  const actions = buildChatResumeActions([
    { id: 'p-1', kind: 'prompt', group: 'workflow', title: 'Continue coding', description: 'Continue', prompt: 'Continue coding until this task is fully finished.', disabled: false, keywords: [] },
    { id: 'p-2', kind: 'prompt', group: 'workflow', title: 'Run verification', description: 'Verify', prompt: 'Run verification now and fix any issues you find.', disabled: false, keywords: [] },
    { id: 'p-3', kind: 'prompt', group: 'workflow', title: 'Summarize progress', description: 'Summarize', prompt: 'Summarize what changed and what still remains.', disabled: false, keywords: [] },
    { id: 'p-4', kind: 'prompt', group: 'workflow', title: 'Execute next step', description: 'Next', prompt: 'Show the next highest-impact step and execute it.', disabled: false, keywords: [] },
    { id: 'a-1', kind: 'action', group: 'conversation', title: 'Regenerate last answer', description: 'Regenerate', action: 'regenerate', disabled: false, keywords: [] },
    { id: 'a-2', kind: 'action', group: 'conversation', title: 'Stop current run', description: 'Stop', action: 'stop', disabled: false, keywords: [] },
  ]);

  assert.deepEqual(
    actions.map((item) => item.id),
    ['p-1', 'p-2', 'p-3', 'a-1'],
    'resume bar should surface the first three prompt actions plus the highest-priority conversation action',
  );

  const customLimit = buildChatResumeActions(actions, { maxPromptActions: 1 });
  assert.deepEqual(customLimit.map((item) => item.id), ['p-1', 'a-1']);

  assert.deepEqual(buildChatResumeActions([]), [], 'empty command lists should produce no resume actions');
}
