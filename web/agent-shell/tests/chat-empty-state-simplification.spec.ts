import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function run() {
  const chatPage = await readFile(new URL('../app/chat/page.tsx', import.meta.url), 'utf8');

  assert.match(chatPage, /<ConversationEmptyState\s+className="min-h-\[24rem\] justify-start pt-10"\s*>/, 'empty state should use a single custom child layout instead of duplicating title/description props');
  assert.match(chatPage, /<h3 className="font-medium text-sm">Agent Chat<\/h3>/, 'empty state should still render the chat title once inside the custom layout');
  assert.doesNotMatch(chatPage, /title="Agent Chat"/, 'empty state should not duplicate the title prop when children already render it');
  assert.doesNotMatch(chatPage, /description="与 agentd 管理的 AI agent 对话，所有工具调用经 daemon 策略管控"/, 'empty state should not duplicate the description prop when children already render it');
}
