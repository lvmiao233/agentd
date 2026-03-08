import assert from 'node:assert/strict';

import { buildApprovalFeed } from '../lib/chat-approval-feed.js';

export async function run() {
  const feed = buildApprovalFeed({
    pending: [
      {
        id: 'approval-1',
        tool: 'mcp.shell.execute',
        reason: 'Need approval to run shell command',
        requested_at: '2026-03-08T03:00:00.000Z',
      },
      {
        id: 'approval-2',
        tool: 'mcp.fs.write_file',
        reason: 'Need approval to write file',
        requested_at: '2026-03-08T04:00:00.000Z',
      },
    ],
    resolved: [],
  });

  assert.deepEqual(
    feed.filter((item) => item.kind === 'pending').map((item) => item.approval.id),
    ['approval-2', 'approval-1'],
    'pending approvals should stay sorted newest-first so the top dock reflects current blockers first',
  );
}
