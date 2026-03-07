export function buildFollowUpSuggestions(params: {
  status: 'submitted' | 'streaming' | 'ready' | 'error';
  lastAssistantText: string;
  hasToolParts: boolean;
  hasPendingApprovals: boolean;
}): string[];
