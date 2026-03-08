function snippet(value, maxLength = 120) {
  if (typeof value !== 'string') {
    return '';
  }

  const normalized = value.replace(/\s+/g, ' ').trim();
  if (!normalized) {
    return '';
  }

  return normalized.length <= maxLength
    ? normalized
    : `${normalized.slice(0, maxLength - 1).trimEnd()}…`;
}

export function buildChatCockpitPlan({
  status,
  runOverview,
  approvalCount,
  checkpointCount,
  lastUserText,
  lastAssistantText,
  selectedAgentRunnable,
}) {
  if (selectedAgentRunnable === false) {
    return {
      title: 'Select a runnable agent',
      description: 'The current agent cannot continue until you choose one that is ready.',
      isStreaming: false,
      defaultOpen: true,
    };
  }

  if (approvalCount > 0) {
    return {
      title: 'Resolve the blocker and continue',
      description: `${approvalCount} approval${approvalCount === 1 ? '' : 's'} pending. Review the blocker, then keep the run moving.`,
      isStreaming: false,
      defaultOpen: true,
    };
  }

  if (status === 'streaming' || status === 'submitted') {
    return {
      title: runOverview?.statusLabel ?? 'The agent is actively working',
      description:
        runOverview?.statusSummary ??
        'Monitor the active run, inspect the timeline, or stop it if you need to steer the agent.',
      isStreaming: true,
      defaultOpen: true,
    };
  }

  if (status === 'error') {
    return {
      title: 'Review the last run and try again',
      description:
        snippet(lastAssistantText, 140) ||
        'The last run needs attention. Restore a checkpoint or try a different next step.',
      isStreaming: false,
      defaultOpen: true,
    };
  }

  if (checkpointCount > 0) {
    return {
      title: 'Continue the current coding session',
      description:
        snippet(lastUserText, 140) ||
        runOverview?.statusSummary ||
        'Resume the task from the latest stable point, inspect blockers, or restore an earlier checkpoint.',
      isStreaming: false,
      defaultOpen: false,
    };
  }

  return {
    title: 'Start a new coding session',
    description: 'Use the cockpit to inspect progress, blockers, and checkpoints once the first run begins.',
    isStreaming: false,
    defaultOpen: true,
  };
}
