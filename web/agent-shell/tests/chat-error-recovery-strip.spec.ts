import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function run() {
  const chatPage = await readFile(new URL('../app/chat/page.tsx', import.meta.url), 'utf8');

  assert.match(chatPage, /Something went wrong while streaming this response\. You can retry\./, 'chat page should keep the composer-adjacent error notice');
  assert.match(chatPage, /Retry last answer/, 'error notice should expose a direct retry action');
  assert.match(chatPage, /const canRetryErroredTurn = Boolean\(lastUserMessage && selectedAgent && isAgentRunnable\(selectedAgent\)\);/, 'retry availability should survive after the last assistant message has been trimmed during regenerate');
  assert.match(chatPage, /canRetryErroredTurn && \(/, 'retry action should render when the current errored turn can be retried');
  assert.match(chatPage, /onClick=\{\(\) => void handleRegenerate\(\)\}/, 'retry action should reuse the existing regenerate handler');
}
