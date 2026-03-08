import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function run() {
  const chatPage = await readFile(new URL('../app/chat/page.tsx', import.meta.url), 'utf8');

  assert.match(chatPage, /const CHAT_AGENT_STORAGE_KEY = 'agent-shell:chat:selected-agent';/, 'chat page should persist the last selected agent');
  assert.match(chatPage, /window\.localStorage\.setItem\(CHAT_AGENT_STORAGE_KEY, agentId\)/, 'chat page should remember the selected agent in localStorage');
  assert.match(chatPage, /window\.localStorage\.getItem\(CHAT_AGENT_STORAGE_KEY\)/, 'chat page should restore the remembered agent on startup');
  assert.match(chatPage, /Detecting runnable agents…/, 'chat page should explain the loading state while agents are resolving');
  assert.match(chatPage, /Ready with \$\{selectedAgent\.name\} · \$\{selectedAgent\.model\}/, 'chat page should surface a ready-state hint once an agent is selected');
  assert.match(chatPage, /No agents found yet\. Create or start a ready agent to begin chatting\./, 'chat page should explain the empty-agent state');
}
