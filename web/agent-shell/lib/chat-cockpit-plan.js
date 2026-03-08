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
  nextActionTitle,
}) {
  const objective = snippet(lastUserText, 96) || 'Start the next coding task.';
  const blocker =
    selectedAgentRunnable === false
      ? 'Current agent is not runnable.'
      : approvalCount > 0
        ? `${approvalCount} approval${approvalCount === 1 ? '' : 's'} pending.`
        : status === 'error'
          ? 'Last run needs attention.'
          : status === 'streaming' || status === 'submitted'
            ? 'No blocker — active run in progress.'
            : checkpointCount > 0
              ? 'No blocker — latest stable checkpoint is ready.'
              : 'No blocker yet.';
  const nextStep =
    selectedAgentRunnable === false
      ? 'Choose a runnable agent.'
      : approvalCount > 0
        ? 'Review and resolve the pending approval.'
        : nextActionTitle ||
          (status === 'streaming' || status === 'submitted'
            ? 'Monitor or stop the active run.'
            : 'Start the next implementation step.');

  if (selectedAgentRunnable === false) {
    return {
      mode: 'unrunnable',
      title: 'Select a runnable agent',
      description: 'The current agent cannot continue until you choose one that is ready.',
      isStreaming: false,
      defaultOpen: true,
      highlights: [
        { key: 'objective', label: 'Current objective', value: objective, tone: 'default' },
        { key: 'blocker', label: 'Blocker', value: blocker, tone: 'warning' },
        { key: 'next', label: 'Next action', value: nextStep, tone: 'warning' },
      ],
    };
  }

  if (approvalCount > 0) {
    return {
      mode: 'blocked',
      title: 'Resolve the blocker and continue',
      description: `${approvalCount} approval${approvalCount === 1 ? '' : 's'} pending. Review the blocker, then keep the run moving.`,
      isStreaming: false,
      defaultOpen: true,
      highlights: [
        { key: 'objective', label: 'Current objective', value: objective, tone: 'default' },
        { key: 'blocker', label: 'Blocker', value: blocker, tone: 'warning' },
        { key: 'next', label: 'Next action', value: nextStep, tone: 'warning' },
      ],
    };
  }

  if (status === 'streaming' || status === 'submitted') {
    return {
      mode: 'streaming',
      title: runOverview?.statusLabel ?? 'The agent is actively working',
      description:
        runOverview?.statusSummary ??
        'Monitor the active run, inspect the timeline, or stop it if you need to steer the agent.',
      isStreaming: true,
      defaultOpen: true,
      highlights: [
        { key: 'objective', label: 'Current objective', value: objective, tone: 'default' },
        { key: 'blocker', label: 'Blocker', value: blocker, tone: 'success' },
        { key: 'next', label: 'Next action', value: nextStep, tone: 'default' },
      ],
    };
  }

  if (status === 'error') {
    return {
      mode: 'error',
      title: 'Review the last run and try again',
      description:
        snippet(lastAssistantText, 140) ||
        'The last run needs attention. Restore a checkpoint or try a different next step.',
      isStreaming: false,
      defaultOpen: true,
      highlights: [
        { key: 'objective', label: 'Current objective', value: objective, tone: 'default' },
        { key: 'blocker', label: 'Blocker', value: blocker, tone: 'warning' },
        { key: 'next', label: 'Next action', value: nextStep, tone: 'warning' },
      ],
    };
  }

  if (checkpointCount > 0) {
    return {
      mode: 'resumable',
      title: 'Continue the current coding session',
      description:
        snippet(lastUserText, 140) ||
        runOverview?.statusSummary ||
        'Resume the task from the latest stable point, inspect blockers, or restore an earlier checkpoint.',
      isStreaming: false,
      defaultOpen: false,
      highlights: [
        { key: 'objective', label: 'Current objective', value: objective, tone: 'default' },
        { key: 'blocker', label: 'Blocker', value: blocker, tone: 'success' },
        { key: 'next', label: 'Next action', value: nextStep, tone: 'default' },
      ],
    };
  }

  return {
    mode: 'idle',
    title: 'Start a new coding session',
    description: 'Use the cockpit to inspect progress, blockers, and checkpoints once the first run begins.',
    isStreaming: false,
    defaultOpen: true,
    highlights: [
      { key: 'objective', label: 'Current objective', value: objective, tone: 'default' },
      { key: 'blocker', label: 'Blocker', value: blocker, tone: 'success' },
      { key: 'next', label: 'Next action', value: nextStep, tone: 'default' },
    ],
  };
}
