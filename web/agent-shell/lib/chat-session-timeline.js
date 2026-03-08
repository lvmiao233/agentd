export function buildChatSessionTimeline({ checkpoints, status, activeMessageId }) {
  if (!Array.isArray(checkpoints) || checkpoints.length === 0) {
    return null;
  }

  const items = [...checkpoints].reverse().map((checkpoint, index, reversed) => {
    const isLatest = index === 0;
    const isActive = checkpoint.messageId === activeMessageId;
    return {
      id: checkpoint.id,
      label: checkpoint.label,
      description: isLatest
        ? status === 'streaming' || status === 'submitted'
          ? 'Latest stable point before the active run.'
          : 'Most recent stable checkpoint.'
        : `Restore to message ${checkpoint.messageCount}.`,
      completed: !isLatest,
      messageId: checkpoint.messageId,
      targetId: `chat-message-${checkpoint.messageId}`,
      isActive,
      isLatest,
      ordinal: reversed.length - index,
    };
  });

  return {
    title: 'Session timeline',
    count: items.length,
    items,
  };
}
