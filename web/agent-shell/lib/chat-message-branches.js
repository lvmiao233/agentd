function messageSignature(message) {
  return JSON.stringify({ role: message.role, parts: message.parts });
}

export function getAssistantBranchKey(messages, assistantMessageId) {
  let previousUserId = null;

  for (const message of messages) {
    if (message.role === 'user') {
      previousUserId = message.id;
    }

    if (message.id === assistantMessageId) {
      return previousUserId ?? `assistant:${assistantMessageId}`;
    }
  }

  return null;
}

export function appendMessageBranch(history, branchKey, message) {
  const current = history[branchKey] ?? [];
  const nextSignature = messageSignature(message);

  if (current.some((entry) => messageSignature(entry) === nextSignature)) {
    return history;
  }

  return {
    ...history,
    [branchKey]: [...current, message],
  };
}

export function mergeMessageBranches(archivedBranches, currentMessage) {
  const result = [];
  const seen = new Set();

  for (const message of [...archivedBranches, currentMessage]) {
    const signature = messageSignature(message);
    if (seen.has(signature)) {
      continue;
    }
    seen.add(signature);
    result.push(message);
  }

  return result;
}
