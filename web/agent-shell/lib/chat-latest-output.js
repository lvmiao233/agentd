import { extractPreviewArtifacts } from './chat-artifacts.js';

function snippet(value, maxLength = 96) {
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

function toolDisplayName(part) {
  if (part?.type === 'dynamic-tool') {
    return part.toolName || 'dynamic-tool';
  }

  if (typeof part?.type === 'string') {
    const segments = part.type.split('-').slice(1);
    return segments.length > 0 ? segments.join('-') : 'tool';
  }

  return 'tool';
}

export function buildChatLatestOutput(messages) {
  if (!Array.isArray(messages)) {
    return null;
  }

  for (let messageIndex = messages.length - 1; messageIndex >= 0; messageIndex -= 1) {
    const message = messages[messageIndex];
    const text = Array.isArray(message?.parts)
      ? message.parts
          .filter((part) => part?.type === 'text' && typeof part.text === 'string')
          .map((part) => part.text)
          .join(' ')
      : '';

    const artifacts = extractPreviewArtifacts(text);
    if (artifacts.length > 0) {
      const artifactIndex = artifacts.length - 1;
      const artifact = artifacts[artifactIndex];
      return {
        kind: 'artifact',
        title: artifact.title || 'Preview artifact',
        description: snippet(artifact.code),
        targetId: `chat-artifact-${message.id}-${artifactIndex}`,
      };
    }

    if (!Array.isArray(message?.parts)) {
      continue;
    }

    for (let partIndex = message.parts.length - 1; partIndex >= 0; partIndex -= 1) {
      const part = message.parts[partIndex];
      if (!isToolPart(part)) {
        continue;
      }

      if (part.state !== 'output-available' && part.state !== 'output-error') {
        continue;
      }

      return {
        kind: 'tool',
        title: toolDisplayName(part),
        description:
          part.state === 'output-error'
            ? snippet(part.errorText || 'Tool returned an error')
            : snippet(JSON.stringify(part.output)),
        targetId: `chat-tool-${message.id}-${partIndex}`,
      };
    }
  }

  return null;
}
