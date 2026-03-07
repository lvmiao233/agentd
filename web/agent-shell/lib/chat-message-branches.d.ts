import type { UIMessage } from 'ai';

export function getAssistantBranchKey(
  messages: UIMessage[],
  assistantMessageId: string,
): string | null;

export function appendMessageBranch(
  history: Record<string, UIMessage[]>,
  branchKey: string,
  message: UIMessage,
): Record<string, UIMessage[]>;

export function mergeMessageBranches(
  archivedBranches: UIMessage[],
  currentMessage: UIMessage,
): UIMessage[];
