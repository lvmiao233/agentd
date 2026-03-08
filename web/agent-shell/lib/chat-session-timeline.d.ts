import type { ChatCheckpoint } from '@/lib/chat-checkpoints.js';

export type ChatSessionTimelineItem = {
  id: string;
  label: string;
  description: string;
  completed: boolean;
  messageId: string;
  targetId: string;
  isActive: boolean;
  isLatest: boolean;
  ordinal: number;
};

export type ChatSessionTimeline = {
  title: string;
  count: number;
  items: ChatSessionTimelineItem[];
};

export declare function buildChatSessionTimeline(params: {
  checkpoints: ChatCheckpoint[];
  status: string;
  activeMessageId?: string;
}): ChatSessionTimeline | null;
