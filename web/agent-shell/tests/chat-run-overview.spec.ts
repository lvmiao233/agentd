import assert from 'node:assert/strict';

import { buildChatRunOverview } from '../lib/chat-run-overview.js';

export async function run() {
  const overview = buildChatRunOverview({
    messages: [
      {
        id: 'user-1',
        role: 'user',
        parts: [{ type: 'text', text: 'Please finish the refactor and verify it.' }],
      },
      {
        id: 'assistant-1',
        role: 'assistant',
        parts: [
          { type: 'text', text: 'I am checking the affected files now.' },
          {
            type: 'dynamic-tool',
            toolName: 'mcp.fs.read_file',
            toolCallId: 'call-1',
            state: 'input-available',
            input: { path: 'web/agent-shell/app/chat/page.tsx' },
          },
        ],
      },
    ],
    status: 'streaming',
    approvals: [
      {
        id: 'approval-1',
        tool: 'mcp.shell.execute',
        reason: 'Needs confirmation before running verification commands.',
        requested_at: '2026-03-08T02:00:00.000Z',
      },
    ],
  });

  assert.ok(overview, 'overview should be generated when the chat has messages');
  assert.equal(overview.statusLabel, 'Waiting for approval');
  assert.match(overview.statusSummary, /approval pending/i);
  assert.deepEqual(
    overview.sections.map((section) => section.title),
    ['Current turn', 'Tool activity', 'Pending approvals'],
    'streaming run with approvals should surface current turn, tools, and approvals sections',
  );
  assert.equal(overview.sections[1].items[0].title, 'mcp.fs.read_file');
  assert.match(overview.sections[1].items[0].description, /Running · path:/);
  assert.equal(overview.sections[0].items[0].targetId, 'chat-message-user-1');
  assert.equal(overview.sections[0].items[1].targetId, 'chat-message-assistant-1');
  assert.equal(overview.sections[1].items[0].targetId, 'chat-tool-assistant-1-1');
  assert.equal(overview.sections[2].items[0].targetId, 'chat-approval-approval-1');

  const completedOverview = buildChatRunOverview({
    messages: [
      {
        id: 'user-2',
        role: 'user',
        parts: [{ type: 'text', text: 'Summarize the latest changes.' }],
      },
      {
        id: 'assistant-2',
        role: 'assistant',
        parts: [
          {
            type: 'dynamic-tool',
            toolName: 'mcp.search.grep_code',
            toolCallId: 'call-2',
            state: 'output-available',
            input: { query: 'RunAgent' },
          },
          { type: 'text', text: 'I found the relevant stream bridge and summarized it.' },
        ],
      },
    ],
    status: 'ready',
    approvals: [],
  });

  assert.ok(completedOverview, 'completed run should still produce an overview');
  assert.equal(completedOverview.statusLabel, 'Latest run completed');
  assert.match(completedOverview.statusSummary, /1 completed/);
  assert.equal(completedOverview.sections.length, 2, 'no approvals section should be shown when queue is empty');

  const toolOnlyOverview = buildChatRunOverview({
    messages: [
      {
        id: 'user-4',
        role: 'user',
        parts: [{ type: 'text', text: 'Check the logs.' }],
      },
      {
        id: 'assistant-4',
        role: 'assistant',
        parts: [
          {
            type: 'dynamic-tool',
            toolName: 'mcp.logs.read',
            toolCallId: 'call-4',
            state: 'input-available',
            input: { path: 'logs/app.log' },
          },
        ],
      },
    ],
    status: 'streaming',
    approvals: [],
  });

  assert.ok(toolOnlyOverview, 'tool-only assistant turns should still produce an overview');
  assert.equal(
    toolOnlyOverview.sections[0].items[1].targetId,
    'chat-tool-assistant-4-0',
    'assistant state should fall back to the first tool anchor when there is no rendered message bubble',
  );

  const linkedApprovalOverview = buildChatRunOverview({
    messages: [
      {
        id: 'user-3',
        role: 'user',
        parts: [{ type: 'text', text: 'Run verification.' }],
      },
    ],
    status: 'ready',
    approvals: [],
    approvalCount: 1,
  });

  assert.ok(linkedApprovalOverview, 'overview should support separate approval counts');
  assert.equal(
    linkedApprovalOverview.statusLabel,
    'Waiting for approval',
    'status should still reflect linked approvals even when the overview section is driven by a filtered list',
  );
}
