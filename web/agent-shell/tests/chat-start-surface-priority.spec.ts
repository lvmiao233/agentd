import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function run() {
  const chatPage = await readFile(new URL('../app/chat/page.tsx', import.meta.url), 'utf8');

  assert.match(chatPage, /const cockpitResumeActions = composerFollowUpActions.length > 0 \? \[] : resumeActions;/, 'cockpit should already stand down when composer continuation exists');
  assert.match(chatPage, /nextActionTitle: cockpitResumeActions.find\(\(action\) => !action.disabled\)\?\.title,/, 'cockpit next-action highlight should follow the deduplicated action source');
  assert.match(chatPage, /Start with a coding action/, 'quick-start strip should remain the primary explicit empty-state action surface');
}
