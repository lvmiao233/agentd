import type { ChatRunOverview } from '@/lib/chat-run-overview.js';

export type ChatCockpitPlan = {
  mode: 'idle' | 'resumable' | 'streaming' | 'error' | 'blocked' | 'unrunnable';
  title: string;
  description: string;
  isStreaming: boolean;
  defaultOpen: boolean;
  highlights: Array<{
    key: 'objective' | 'blocker' | 'next';
    label: string;
    value: string;
    tone: 'default' | 'warning' | 'success';
  }>;
};

export declare function buildChatCockpitPlan(params: {
  status: string;
  runOverview: ChatRunOverview | null;
  approvalCount: number;
  checkpointCount: number;
  lastUserText: string;
  lastAssistantText: string;
  selectedAgentRunnable?: boolean;
  nextActionTitle?: string;
}): ChatCockpitPlan;
