import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function run() {
  const chatPage = await readFile(new URL('../app/chat/page.tsx', import.meta.url), 'utf8');

  assert.match(chatPage, /const composerFollowUpActions =/, 'chat page should derive composer-adjacent follow-up actions');
  assert.match(chatPage, /Continue this run/, 'chat page should render a composer-adjacent continue strip');
  assert.match(chatPage, /Keep moving without scrolling back to the previous reply\./, 'composer follow-up strip should explain why it exists');
  assert.match(chatPage, /<ComposerActionStrip[\s\S]*label="Continue this run"[\s\S]*actions=\{composerFollowUpActions\}/, 'composer follow-up strip should reuse the shared action-strip component');
}
