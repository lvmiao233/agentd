import assert from 'node:assert/strict';

import {
  approvalDecisionLabel,
  buildApprovalFeed,
} from '../lib/chat-approval-feed.js';

export async function run() {
  const pending = [
    {
      id: 'pending-new',
      tool: 'shell.exec',
      reason: 'needs approval',
      requested_at: '2026-03-08T01:10:00.000Z',
    },
    {
      id: 'pending-old',
      tool: 'fs.delete',
      reason: 'dangerous tool',
      requested_at: '2026-03-08T01:00:00.000Z',
    },
  ];

  const resolved = [
    {
      id: 'resolved-new',
      tool: 'git.push',
      reason: 'sensitive write',
      requested_at: '2026-03-08T00:55:00.000Z',
      decision: 'approve',
      resolvedAt: '2026-03-08T01:15:00.000Z',
    },
    {
      id: 'pending-new',
      tool: 'shell.exec',
      reason: 'still pending, should not duplicate',
      requested_at: '2026-03-08T01:10:00.000Z',
      decision: 'deny',
      resolvedAt: '2026-03-08T01:12:00.000Z',
    },
    {
      id: 'resolved-old',
      tool: 'mcp.search.query',
      reason: 'older resolved entry',
      requested_at: '2026-03-08T00:40:00.000Z',
      decision: 'deny',
      resolvedAt: '2026-03-08T00:45:00.000Z',
    },
  ];

  const feed = buildApprovalFeed({ pending, resolved, resolvedLimit: 2 });

  assert.deepEqual(feed.map((item) => [item.kind, item.approval.id]), [
    ['pending', 'pending-new'],
    ['pending', 'pending-old'],
    ['resolved', 'resolved-new'],
    ['resolved', 'resolved-old'],
  ]);

  assert.equal(approvalDecisionLabel('approve'), 'Approved');
  assert.equal(approvalDecisionLabel('deny'), 'Denied');
}
