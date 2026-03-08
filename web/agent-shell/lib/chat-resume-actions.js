export function buildChatResumeActions(commandItems, { maxPromptActions = 3 } = {}) {
  if (!Array.isArray(commandItems) || commandItems.length === 0) {
    return [];
  }

  const promptItems = commandItems
    .filter((item) => item.kind === 'prompt')
    .slice(0, maxPromptActions);

  const actionItems = commandItems.filter((item) => item.kind === 'action').slice(0, 1);

  return [...promptItems, ...actionItems];
}
