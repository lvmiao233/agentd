export type ChatCommandItem = {
  id: string;
  group: 'workflow' | 'conversation';
  kind: 'prompt' | 'action';
  title: string;
  description: string;
  prompt?: string;
  action?: 'regenerate' | 'stop';
  disabled: boolean;
  keywords: string[];
};

export declare function buildChatCommandItems(params: {
  status: string;
  lastAssistantText: string;
  hasToolParts: boolean;
  hasPendingApprovals: boolean;
  hasConversation: boolean;
  canRegenerate: boolean;
  selectedAgentRunnable?: boolean;
}): ChatCommandItem[];
