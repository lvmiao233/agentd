import type { UIMessage } from 'ai';
import type { ResolvedApprovalItem } from '@/lib/chat-approval-feed.js';

export type ChatCheckpoint = {
  id: string;
  messageCount: number;
  messageId: string;
  label: string;
  messages: UIMessage[];
  resolvedApprovals: ResolvedApprovalItem[];
  messageBranchHistory: Record<string, UIMessage[]>;
};

export declare function createChatCheckpoint(params: {
  messages: UIMessage[];
  assistantMessage: UIMessage;
  resolvedApprovals: ResolvedApprovalItem[];
  messageBranchHistory: Record<string, UIMessage[]>;
}): ChatCheckpoint;

export declare function appendChatCheckpoint(
  checkpoints: ChatCheckpoint[],
  checkpoint: ChatCheckpoint,
): ChatCheckpoint[];

export declare function pruneChatCheckpoints(
  checkpoints: ChatCheckpoint[],
  messageCount: number,
): ChatCheckpoint[];
