import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function run() {
  const chatPage = await readFile(new URL('../app/chat/page.tsx', import.meta.url), 'utf8');

  assert.match(chatPage, /const starterPromptActions = messages.length === 0/, 'chat page should derive starter prompt actions for empty conversations');
  assert.match(chatPage, /Start with a coding action/, 'empty state should label starter workflow prompts explicitly');
  assert.match(chatPage, /Or open Commands to pick another workflow prompt\./, 'empty state should still hint that the command palette remains available');
  assert.match(chatPage, /<ComposerActionStrip[\s\S]*label="Start with a coding action"[\s\S]*actions=\{starterPromptActions\}/, 'empty state should render starter prompt actions through the shared composer action strip');
}
