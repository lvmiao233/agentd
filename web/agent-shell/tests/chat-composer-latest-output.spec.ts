import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function run() {
  const chatPage = await readFile(new URL('../app/chat/page.tsx', import.meta.url), 'utf8');

  assert.match(chatPage, /function ComposerLatestOutputStrip\(/, 'chat page should define a composer-adjacent latest output strip');
  assert.match(chatPage, /Latest output/, 'composer latest output strip should label the latest result clearly');
  assert.match(chatPage, /Review output/, 'composer latest output strip should expose a review action');
  assert.match(chatPage, /latestOutput=\{latestOutput\}/, 'composer latest output strip should be driven by the shared latestOutput helper');
  assert.match(chatPage, /onReview=\{highlightConversationTarget\}/, 'composer latest output strip should reuse the existing target navigation handler');
}
