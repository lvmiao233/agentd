import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function run() {
  const chatPage = await readFile(new URL('../app/chat/page.tsx', import.meta.url), 'utf8');

  assert.match(chatPage, /function ActiveRunControls\(/, 'chat page should define an active run control strip');
  assert.match(chatPage, /status === 'submitted' \|\| status === 'streaming'/, 'active run controls should appear while a run is pending or streaming');
  assert.match(chatPage, /Stop current run/, 'active run controls should expose an explicit stop action');
  assert.match(chatPage, /Review live activity/, 'active run controls should let users jump to the current live activity');
  assert.match(chatPage, /<ActiveRunControls[\s\S]*onStop=\{\(\) => void stop\(\)\}/, 'active run controls should reuse the existing stop handler');
}
