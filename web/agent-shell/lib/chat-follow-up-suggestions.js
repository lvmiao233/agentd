function uniqueSuggestions(items) {
  const seen = new Set();
  return items.filter((item) => {
    const normalized = item.trim().toLowerCase();
    if (!normalized || seen.has(normalized)) {
      return false;
    }
    seen.add(normalized);
    return true;
  });
}

export function buildFollowUpSuggestions({
  status,
  lastAssistantText,
  hasToolParts,
  hasPendingApprovals,
}) {
  if (status === 'streaming' || !lastAssistantText.trim()) {
    return [];
  }

  const suggestions = [];

  if (status === 'error') {
    suggestions.push('Retry the last step with a different approach.');
  }

  if (hasPendingApprovals) {
    suggestions.push('Explain the pending approval and what will happen after I approve it.');
  }

  if (hasToolParts) {
    suggestions.push('Continue coding until this task is fully finished.');
    suggestions.push('Run verification now and fix any issues you find.');
  }

  suggestions.push('Summarize what changed and what still remains.');
  suggestions.push('Show the next highest-impact step and execute it.');

  return uniqueSuggestions(suggestions).slice(0, 4);
}
