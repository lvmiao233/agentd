import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function run() {
  const chatPage = await readFile(new URL('../app/chat/page.tsx', import.meta.url), 'utf8');

  assert.doesNotMatch(chatPage, /function ComposerApprovalStrip\(/, 'approval-specific strip should be folded into the unified run-status strip');
  assert.match(chatPage, /Awaiting approval/, 'approval strip should clearly label pending approval state');
  assert.match(chatPage, /Review approval/, 'approval strip should include a review action');
  assert.match(chatPage, /approval=\{composerPendingApproval\}/, 'run status strip should still be driven by the current pending approval');
  assert.match(chatPage, /onApprove=\{\(approvalId\) => void handleApprovalDecision\(approvalId, 'approve'\)\}/, 'run status strip should reuse the existing approve handler');
  assert.match(chatPage, /onDeny=\{\(approvalId\) => void handleApprovalDecision\(approvalId, 'deny'\)\}/, 'run status strip should reuse the existing deny handler');
}
