export function collectSourceParts(parts) {
  return parts
    .map((part, index) => ({ part, index }))
    .filter(({ part }) => part.type === 'source-url' || part.type === 'source-document');
}

export function countReasoningParts(parts) {
  return parts.filter((part) => part.type === 'reasoning').length;
}
