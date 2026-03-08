import type { ChatRunOverview } from '@/lib/chat-run-overview.js';

export type ChatCockpitPlan = {
  title: string;
  description: string;
  isStreaming: boolean;
  defaultOpen: boolean;
};

export declare function buildChatCockpitPlan(params: {
  status: string;
  runOverview: ChatRunOverview | null;
  approvalCount: number;
  checkpointCount: number;
  lastUserText: string;
  lastAssistantText: string;
  selectedAgentRunnable?: boolean;
}): ChatCockpitPlan;
