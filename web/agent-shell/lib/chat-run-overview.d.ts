import type { ApprovalItem } from '@/lib/daemon-rpc';

export type ChatRunOverviewItem = {
  key: string;
  title: string;
  description: string;
  completed: boolean;
  tone: 'default' | 'warning' | 'error';
};

export type ChatRunOverviewSection = {
  key: string;
  title: string;
  count: number;
  defaultOpen: boolean;
  items: ChatRunOverviewItem[];
};

export type ChatRunOverview = {
  statusLabel: string;
  statusSummary: string;
  sections: ChatRunOverviewSection[];
};

export declare function buildChatRunOverview(params: {
  messages: Array<{ id: string; role: string; parts: unknown[] }>;
  status: string;
  approvals?: ApprovalItem[];
  approvalCount?: number;
}): ChatRunOverview | null;
