import type { UIMessage } from 'ai';

export type MessagePartWithIndex = {
  part: UIMessage['parts'][number];
  index: number;
};

export function collectSourceParts(parts: UIMessage['parts']): MessagePartWithIndex[];
export function countReasoningParts(parts: UIMessage['parts']): number;
