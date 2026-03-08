function clone(value) {
  if (typeof structuredClone === 'function') {
    return structuredClone(value);
  }

  return JSON.parse(JSON.stringify(value));
}

function assistantText(message) {
  if (!message || !Array.isArray(message.parts)) {
    return '';
  }

  return message.parts
    .filter((part) => part?.type === 'text' && typeof part.text === 'string')
    .map((part) => part.text.trim())
    .filter(Boolean)
    .join(' ')
    .trim();
}

function checkpointLabel(messages, assistantMessage) {
  const assistantSummary = assistantText(assistantMessage);
  if (assistantSummary) {
    return assistantSummary.length > 72
      ? `${assistantSummary.slice(0, 71).trimEnd()}…`
      : assistantSummary;
  }

  const previousUserMessage = [...messages]
    .reverse()
    .find((message) => message.role === 'user' && message.id !== assistantMessage.id);

  if (!previousUserMessage) {
    return 'Restore this run';
  }

  const userSummary = assistantText(previousUserMessage);
  return userSummary ? `After: ${userSummary}` : 'Restore this run';
}

export function createChatCheckpoint({
  messages,
  assistantMessage,
  resolvedApprovals,
  messageBranchHistory,
}) {
  return {
    id: `checkpoint-${assistantMessage.id}`,
    messageCount: messages.length,
    messageId: assistantMessage.id,
    label: checkpointLabel(messages, assistantMessage),
    messages: clone(messages),
    resolvedApprovals: clone(resolvedApprovals),
    messageBranchHistory: clone(messageBranchHistory),
  };
}

export function appendChatCheckpoint(checkpoints, checkpoint) {
  if (checkpoints.some((item) => item.messageId === checkpoint.messageId)) {
    return checkpoints;
  }

  return [...checkpoints, checkpoint];
}

export function pruneChatCheckpoints(checkpoints, messageCount) {
  return checkpoints.filter((checkpoint) => checkpoint.messageCount <= messageCount);
}
