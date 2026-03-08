import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function run() {
  const chatPage = await readFile(new URL('../app/chat/page.tsx', import.meta.url), 'utf8');

  assert.match(chatPage, /const cockpitResumeActions =\s+composerFollowUpActions.length > 0 \|\|\s+starterPromptActions.length > 0 \|\|\s+status === 'submitted' \|\|\s+status === 'streaming' \|\|\s+approvalQueue.length > 0\s+\? \[]\s+: resumeActions;/, 'cockpit should stand down whenever a higher-priority composer-side control surface is present');
  assert.match(chatPage, /nextActionTitle: cockpitResumeActions.find\(\(action\) => !action.disabled\)\?\.title,/, 'cockpit next-action highlight should follow the deduplicated action source');
  assert.match(chatPage, /Start with a coding action/, 'quick-start strip should remain the primary explicit empty-state action surface');
}
