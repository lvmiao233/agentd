function normalizeSnippet(value, maxLength = 96) {
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

function isToolPart(part) {
  return part?.type === 'dynamic-tool' ||
    (typeof part?.type === 'string' && part.type.startsWith('tool-'));
}

function latestTurn(messages) {
  const lastUserIndex = [...messages]
    .map((message, index) => ({ message, index }))
    .reverse()
    .find(({ message }) => message?.role === 'user')?.index;

  return typeof lastUserIndex === 'number' ? messages.slice(lastUserIndex) : messages;
}

function toolName(part) {
  if (part?.type === 'dynamic-tool') {
    return part.toolName || 'dynamic-tool';
  }

  if (typeof part?.type === 'string') {
    const segments = part.type.split('-').slice(1);
    return segments.length > 0 ? segments.join('-') : 'tool';
  }

  return 'tool';
}

function summarizeInput(input) {
  if (typeof input === 'string') {
    return normalizeSnippet(input);
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
    return normalizeSnippet(JSON.stringify(input));
  } catch {
    return 'structured input';
  }
}

function summarizeOutput(output, errorText) {
  if (typeof errorText === 'string' && errorText.trim()) {
    return normalizeSnippet(errorText);
  }

  if (typeof output === 'string') {
    return normalizeSnippet(output);
  }

  if (!output || typeof output !== 'object') {
    return '';
  }

  try {
    return normalizeSnippet(JSON.stringify(output));
  } catch {
    return 'structured output';
  }
}

const STATE_PRIORITY = {
  'approval-requested': 5,
  'input-streaming': 4,
  'input-available': 3,
  'output-error': 2,
  'output-available': 1,
  'approval-responded': 0,
  'output-denied': 0,
};

export function buildChatLiveActivity(messages) {
  if (!Array.isArray(messages) || messages.length === 0) {
    return null;
  }

  const toolParts = [];
  for (const message of latestTurn(messages)) {
    if (!Array.isArray(message?.parts)) {
      continue;
    }

    for (const [partIndex, part] of message.parts.entries()) {
      if (!isToolPart(part)) {
        continue;
      }

      toolParts.push({ message, part, partIndex });
    }
  }

  if (toolParts.length === 0) {
    return null;
  }

  const picked = [...toolParts]
    .reverse()
    .sort((a, b) => (STATE_PRIORITY[b.part.state] ?? -1) - (STATE_PRIORITY[a.part.state] ?? -1))[0];

  if (!picked) {
    return null;
  }

  const description =
    picked.part.state === 'output-available' || picked.part.state === 'output-error'
      ? summarizeOutput(picked.part.output, picked.part.errorText)
      : summarizeInput(picked.part.input);

  return {
    title: toolName(picked.part),
    state: picked.part.state,
    description:
      description ||
      (picked.part.state === 'approval-requested'
        ? 'Waiting for approval before the tool can continue.'
        : picked.part.state === 'input-available' || picked.part.state === 'input-streaming'
          ? 'Tool is currently running.'
          : 'Latest tool result is available.'),
    targetId: `chat-tool-${picked.message.id}-${picked.partIndex}`,
  };
}
