export function buildChatCockpitActions({ runOverview, approvalQueue, resumeActions }) {
  const objectiveTargetId = runOverview?.sections
    ?.find((section) => section.key === 'current-turn')
    ?.items?.find((item) => item.key === 'goal')
    ?.targetId;

  const blockerTargetId = approvalQueue.length > 0
    ? `chat-approval-${approvalQueue[0].id}`
    : runOverview?.sections
        ?.find((section) => section.key === 'current-turn')
        ?.items?.find((item) => item.key === 'assistant-state')
        ?.targetId;

  const nextAction = resumeActions.find((action) => !action.disabled) ?? null;

  return {
    objective: objectiveTargetId
      ? {
          kind: 'navigate',
          label: 'Jump to instruction',
          targetId: objectiveTargetId,
        }
      : null,
    blocker: blockerTargetId
      ? {
          kind: 'navigate',
          label: approvalQueue.length > 0 ? 'Review blocker' : 'Inspect status',
          targetId: blockerTargetId,
        }
      : null,
    next: nextAction
      ? {
          kind: 'command',
          label: nextAction.title,
          action: nextAction,
        }
      : null,
  };
}
