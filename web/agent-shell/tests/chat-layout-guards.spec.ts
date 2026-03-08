import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function run() {
  const chatPage = await readFile(new URL('../app/chat/page.tsx', import.meta.url), 'utf8');
  const conversation = await readFile(new URL('../components/ai-elements/conversation.tsx', import.meta.url), 'utf8');

  assert.match(chatPage, /<Conversation className="min-h-0">/, 'chat page should keep conversation in a dedicated min-h-0 flex zone');
  assert.match(chatPage, /<ConversationContent className="pt-2 pb-24">/, 'chat page should reserve top and bottom safe padding for the scrollable feed');
  assert.match(chatPage, /className="mt-3 shrink-0"/, 'prompt input should remain outside the scroll area as a shrink-0 footer');
  assert.match(chatPage, /<div className="shrink-0">\s*<ChatCockpitPlanPanel/s, 'cockpit plan should remain a shrink-0 sibling above the conversation');
  assert.match(conversation, /relative min-h-0 flex-1 overflow-y-hidden/, 'conversation root should explicitly opt into min-h-0 flex shrinking');
}
