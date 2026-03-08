import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function run() {
  const chatPage = await readFile(new URL('../app/chat/page.tsx', import.meta.url), 'utf8');

  assert.doesNotMatch(chatPage, /<Suggestions id={`chat-message-actions-\$\{message\.id\}`}>/, 'message footer should no longer duplicate follow-up suggestion chips');
  assert.match(chatPage, /Continue this run/, 'continuation actions should still remain available in the composer/cockpit layers');
  assert.match(chatPage, /<MessageActions>[\s\S]*handleRegenerate\(\)[\s\S]*navigator\.clipboard\.writeText\(lastAssistantText\)/, 'message footer should keep regenerate and copy actions after deduplication');
}
