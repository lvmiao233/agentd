import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function run() {
  const chatPage = await readFile(new URL('../app/chat/page.tsx', import.meta.url), 'utf8');

  assert.match(chatPage, /function ComposerApprovalStrip\(/, 'chat page should define a composer-adjacent approval strip');
  assert.match(chatPage, /Awaiting approval/, 'approval strip should clearly label pending approval state');
  assert.match(chatPage, /Review approval/, 'approval strip should include a review action');
  assert.match(chatPage, /onApprove=\{\(approvalId\) => void handleApprovalDecision\(approvalId, 'approve'\)\}/, 'approval strip should reuse the existing approve handler');
  assert.match(chatPage, /onDeny=\{\(approvalId\) => void handleApprovalDecision\(approvalId, 'deny'\)\}/, 'approval strip should reuse the existing deny handler');
  assert.match(chatPage, /approval=\{composerPendingApproval\}/, 'approval strip should be driven by the current pending approval');
}
