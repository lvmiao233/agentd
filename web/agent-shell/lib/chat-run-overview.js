const TOOL_STATE_LABELS = {
  'approval-requested': 'Awaiting approval',
  'approval-responded': 'Approval answered',
  'input-available': 'Running',
  'input-streaming': 'Preparing',
  'output-available': 'Completed',
  'output-denied': 'Denied',
  'output-error': 'Error',
};

function normalizeSnippet(value, maxLength = 140) {
  if (typeof value !== 'string') {
    return '';
  }

  const normalized = value.replace(/\s+/g, ' ').trim();
  if (!normalized) {
    return '';
  }

  if (normalized.length <= maxLength) {
    return normalized;
  }

  return `${normalized.slice(0, maxLength - 1).trimEnd()}…`;
}

function extractMessageText(message) {
  if (!message || !Array.isArray(message.parts)) {
    return '';
  }

  return normalizeSnippet(
    message.parts
      .filter((part) => part?.type === 'text')
      .map((part) => part.text)
      .join(' '),
  );
}

function isToolPart(part) {
  return part?.type === 'dynamic-tool' ||
    (typeof part?.type === 'string' && part.type.startsWith('tool-'));
}

function getToolDisplayName(part) {
  if (part?.type === 'dynamic-tool') {
    return normalizeSnippet(part.toolName || 'dynamic-tool', 80);
  }

  if (typeof part?.type === 'string') {
    const segments = part.type.split('-').slice(1);
    if (segments.length > 0) {
      return normalizeSnippet(segments.join('-'), 80);
    }
  }

  return 'tool';
}

function summarizeToolInput(input) {
  if (typeof input === 'string') {
    return normalizeSnippet(input, 90);
  }

  if (!input || typeof input !== 'object') {
    return '';
  }

  for (const field of ['path', 'command', 'query', 'url', 'task', 'prompt']) {
    if (typeof input[field] === 'string' && input[field].trim()) {
      return `${field}: ${normalizeSnippet(input[field], 72)}`;
    }
  }

  try {
    return normalizeSnippet(JSON.stringify(input), 90);
  } catch {
    return 'structured input';
  }
}

function countToolStates(toolItems) {
  return toolItems.reduce(
    (summary, item) => {
      if (item.tone === 'error') {
        summary.error += 1;
      } else if (item.completed) {
        summary.completed += 1;
      } else if (item.tone === 'warning') {
        summary.active += 1;
      } else {
        summary.pending += 1;
      }
      return summary;
    },
    { active: 0, completed: 0, error: 0, pending: 0 },
  );
}

function latestTurn(messages) {
  const lastUserIndex = [...messages]
    .map((message, index) => ({ message, index }))
    .reverse()
    .find(({ message }) => message?.role === 'user')?.index;

  if (typeof lastUserIndex !== 'number') {
    return {
      lastUserMessage: null,
      turnMessages: messages,
    };
  }

  return {
    lastUserMessage: messages[lastUserIndex],
    turnMessages: messages.slice(lastUserIndex),
  };
}

function buildToolItems(messages) {
  const toolItems = [];

  for (const message of messages) {
    if (!Array.isArray(message?.parts)) {
      continue;
    }

    for (const part of message.parts) {
      if (!isToolPart(part)) {
        continue;
      }

      const stateLabel = TOOL_STATE_LABELS[part.state] ?? normalizeSnippet(part.state || 'Unknown state', 48);
      const inputSummary = summarizeToolInput(part.input);
      toolItems.push({
        key: `${message.id}-${part.toolCallId ?? part.toolName ?? part.type}`,
        title: getToolDisplayName(part),
        description: inputSummary ? `${stateLabel} · ${inputSummary}` : stateLabel,
        completed: part.state === 'output-available',
        tone:
          part.state === 'output-error' || part.state === 'output-denied'
            ? 'error'
            : part.state === 'approval-requested' ||
                part.state === 'input-available' ||
                part.state === 'input-streaming'
              ? 'warning'
              : 'default',
      });
    }
  }

  return toolItems;
}

function buildApprovalItems(approvals) {
  return approvals.map((approval) => ({
    key: approval.id,
    title: normalizeSnippet(approval.tool || 'approval', 80),
    description: normalizeSnippet(
      approval.reason || approval.requested_at || 'Needs your confirmation before the run can continue.',
      120,
    ),
    completed: false,
    tone: 'warning',
  }));
}

function buildStatusLabel({ status, approvalCount, toolSummary }) {
  if (approvalCount > 0) {
    return 'Waiting for approval';
  }

  if (status === 'submitted') {
    return 'Submitting the next turn';
  }

  if (status === 'streaming' && toolSummary.active > 0) {
    return 'Running tools and drafting the reply';
  }

  if (status === 'streaming') {
    return 'Streaming the latest reply';
  }

  if (status === 'error' || toolSummary.error > 0) {
    return 'Latest run needs attention';
  }

  if (toolSummary.completed > 0) {
    return 'Latest run completed';
  }

  return 'Ready for the next instruction';
}

function buildStatusSummary({ status, approvalCount, toolSummary }) {
  const fragments = [];

  if (approvalCount > 0) {
    fragments.push(`${approvalCount} approval${approvalCount === 1 ? '' : 's'} pending`);
  }

  if (toolSummary.active > 0) {
    fragments.push(`${toolSummary.active} tool${toolSummary.active === 1 ? '' : 's'} active`);
  }

  if (toolSummary.completed > 0) {
    fragments.push(`${toolSummary.completed} completed`);
  }

  if (toolSummary.error > 0) {
    fragments.push(`${toolSummary.error} error${toolSummary.error === 1 ? '' : 's'}`);
  }

  if (fragments.length > 0) {
    return fragments.join(' · ');
  }

  if (status === 'submitted') {
    return 'The request is on its way to the agent runtime.';
  }

  if (status === 'streaming') {
    return 'The assistant is still composing its reply.';
  }

  if (status === 'error') {
    return 'Retry this turn or steer the agent toward a different approach.';
  }

  return 'No active tool work in the latest turn.';
}

export function buildChatRunOverview({
  messages,
  status,
  approvals = [],
  approvalCount,
}) {
  if (!Array.isArray(messages) || messages.length === 0) {
    return null;
  }

  const { lastUserMessage, turnMessages } = latestTurn(messages);
  const toolItems = buildToolItems(turnMessages);
  const approvalItems = buildApprovalItems(approvals);
  const effectiveApprovalCount =
    typeof approvalCount === 'number' ? approvalCount : approvalItems.length;
  const toolSummary = countToolStates(toolItems);
  const latestAssistantMessage = [...turnMessages]
    .reverse()
    .find((message) => message?.role === 'assistant');

  const sections = [
    {
      key: 'current-turn',
      title: 'Current turn',
      count: 2,
      defaultOpen: true,
      items: [
        {
          key: 'goal',
          title: normalizeSnippet(extractMessageText(lastUserMessage) || 'Continue the current task.', 110),
          description: 'Latest user instruction',
          completed: status !== 'submitted',
          tone: 'default',
        },
        {
          key: 'assistant-state',
          title: buildStatusLabel({ status, approvalCount: effectiveApprovalCount, toolSummary }),
          description: normalizeSnippet(
            extractMessageText(latestAssistantMessage) ||
              buildStatusSummary({ status, approvalCount: effectiveApprovalCount, toolSummary }),
            140,
          ),
          completed: status === 'ready' && effectiveApprovalCount === 0 && toolSummary.active === 0,
          tone: status === 'error' ? 'error' : effectiveApprovalCount > 0 || status === 'streaming' ? 'warning' : 'default',
        },
      ],
    },
  ];

  if (toolItems.length > 0) {
    sections.push({
      key: 'tool-activity',
      title: 'Tool activity',
      count: toolItems.length,
      defaultOpen: status === 'streaming' || effectiveApprovalCount > 0,
      items: toolItems,
    });
  }

  if (approvalItems.length > 0) {
    sections.push({
      key: 'pending-approvals',
      title: 'Pending approvals',
      count: approvalItems.length,
      defaultOpen: true,
      items: approvalItems,
    });
  }

  return {
    statusLabel: buildStatusLabel({ status, approvalCount: effectiveApprovalCount, toolSummary }),
    statusSummary: buildStatusSummary({ status, approvalCount: effectiveApprovalCount, toolSummary }),
    sections,
  };
}
