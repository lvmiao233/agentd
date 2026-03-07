import assert from 'node:assert/strict';

import {
  assignApprovalsToTools,
  getToolNameAliases,
} from '../lib/chat-tool-approvals.js';

export async function run() {
  assert.deepEqual(getToolNameAliases('mcp.fs.read_file'), [
    'mcp.fs.read_file',
    'fs.read_file',
    'read_file',
  ]);

  const toolNodes = [
    { key: 'tool-1', toolCallId: 'call-1', toolName: 'mcp.fs.read_file' },
    { key: 'tool-2', toolCallId: 'call-2', toolName: 'mcp.shell.execute' },
    { key: 'tool-3', toolCallId: 'call-3', toolName: 'builtin.lite.upper' },
  ];

  const approvals = [
    {
      id: 'approval-shell',
      tool: 'shell.execute',
      reason: 'dangerous shell command',
      requested_at: '2026-03-08T02:00:00.000Z',
    },
    {
      id: 'approval-fs',
      tool: 'mcp.fs.read_file',
      reason: 'read secret file',
      requested_at: '2026-03-08T01:00:00.000Z',
    },
    {
      id: 'approval-unmatched',
      tool: 'mcp.search.query',
      reason: 'not used in current message',
      requested_at: '2026-03-08T03:00:00.000Z',
    },
  ];

  const { assignments, unmatchedApprovals } = assignApprovalsToTools({ toolNodes, approvals });

  assert.equal(assignments.get('tool-1')?.id, 'approval-fs');
  assert.equal(assignments.get('tool-2')?.id, 'approval-shell');
  assert.equal(assignments.has('tool-3'), false);
  assert.deepEqual(unmatchedApprovals.map((approval) => approval.id), ['approval-unmatched']);
}
