import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function run() {
  const chatPage = await readFile(new URL('../app/chat/page.tsx', import.meta.url), 'utf8');

  assert.match(chatPage, /function ComposerLastReplyStrip\(/, 'chat page should define a composer-adjacent last reply strip');
  assert.match(chatPage, /Last reply/, 'last reply strip should label the recent assistant text clearly');
  assert.match(chatPage, /Jump to reply/, 'last reply strip should expose a jump action');
  assert.match(chatPage, /Copy reply/, 'last reply strip should expose a copy action');
  assert.match(chatPage, /<ComposerLastReplyStrip[\s\S]*lastAssistantText=\{lastAssistantText\}[\s\S]*targetId=\{lastAssistantTargetId\}/, 'last reply strip should be embedded inside the recent-context wrapper with the latest assistant text and target');
}
