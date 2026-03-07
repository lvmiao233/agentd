import assert from 'node:assert/strict';

import {
  appendMessageBranch,
  getAssistantBranchKey,
  mergeMessageBranches,
} from '../lib/chat-message-branches.js';

export async function run() {
  const messages = [
    { id: 'u-1', role: 'user', parts: [{ type: 'text', text: 'first prompt' }] },
    { id: 'a-1', role: 'assistant', parts: [{ type: 'text', text: 'first answer' }] },
    { id: 'u-2', role: 'user', parts: [{ type: 'text', text: 'follow up' }] },
    { id: 'a-2', role: 'assistant', parts: [{ type: 'text', text: 'second answer' }] },
  ];

  assert.equal(getAssistantBranchKey(messages, 'a-1'), 'u-1');
  assert.equal(getAssistantBranchKey(messages, 'a-2'), 'u-2');
  assert.equal(getAssistantBranchKey(messages, 'missing'), null);

  const history = appendMessageBranch({}, 'u-2', messages[3]);
  assert.equal(history['u-2'].length, 1);
  const deduped = appendMessageBranch(history, 'u-2', messages[3]);
  assert.equal(deduped['u-2'].length, 1, 'same assistant snapshot should not duplicate');

  const merged = mergeMessageBranches(history['u-2'], {
    id: 'a-2b',
    role: 'assistant',
    parts: [{ type: 'text', text: 'alternative answer' }],
  });
  assert.equal(merged.length, 2);
  assert.deepEqual(
    merged.map((message) => message.parts[0].text),
    ['second answer', 'alternative answer'],
  );
}
