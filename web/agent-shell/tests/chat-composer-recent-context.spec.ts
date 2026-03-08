import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function run() {
  const chatPage = await readFile(new URL('../app/chat/page.tsx', import.meta.url), 'utf8');

  assert.match(chatPage, /function ComposerRecentContextStrip\(/, 'chat page should define a compact recent-context strip');
  assert.match(chatPage, /Recent context/, 'recent-context strip should label the grouped carry-forward context');
  assert.match(chatPage, /<ComposerLatestOutputStrip latestOutput=\{latestOutput\} onReview=\{onReviewOutput\} \/>/, 'recent-context strip should embed the existing latest-output strip');
  assert.match(chatPage, /<ComposerLastReplyStrip[\s\S]*lastAssistantText=\{lastAssistantText\}/, 'recent-context strip should embed the existing last-reply strip');
  assert.match(chatPage, /<ComposerRecentContextStrip[\s\S]*latestOutput=\{latestOutput\}[\s\S]*lastAssistantText=\{lastAssistantText\}/, 'chat page should render the grouped recent-context strip near the composer');
}
