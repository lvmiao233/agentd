import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function run() {
  const chatPage = await readFile(new URL('../app/chat/page.tsx', import.meta.url), 'utf8');

  assert.match(chatPage, /const cockpitResumeActions = composerFollowUpActions.length > 0 \? \[] : resumeActions;/, 'cockpit continuation chips should stand down when composer follow-up actions are present');
  assert.match(chatPage, /resumeActions=\{cockpitResumeActions\}/, 'chat page should pass deduplicated continuation actions into the cockpit');
  assert.match(chatPage, /Continue this run/, 'composer continuation surface should remain available');
}
