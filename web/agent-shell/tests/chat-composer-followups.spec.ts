import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function run() {
  const chatPage = await readFile(new URL('../app/chat/page.tsx', import.meta.url), 'utf8');

  assert.match(chatPage, /const composerFollowUpActions =/, 'chat page should derive composer-adjacent follow-up actions');
  assert.match(chatPage, /Continue this run/, 'chat page should render a composer-adjacent continue strip');
  assert.match(chatPage, /Keep moving without scrolling back to the previous reply\./, 'composer follow-up strip should explain why it exists');
  assert.match(chatPage, /composerFollowUpActions\.map\(\(action\) => \(/, 'composer follow-up strip should render follow-up suggestions from existing actions');
  assert.match(chatPage, /onClick=\{\(\) => void handleResumeAction\(action\)\}/, 'composer follow-up strip should reuse the existing resume action handler');
}
