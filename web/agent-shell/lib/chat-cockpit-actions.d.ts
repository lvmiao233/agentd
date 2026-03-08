import type { ChatCommandItem } from '@/lib/chat-command-menu.js';
import type { ApprovalItem } from '@/lib/daemon-rpc';
import type { ChatRunOverview } from '@/lib/chat-run-overview.js';

export type ChatCockpitCardAction =
  | { kind: 'navigate'; label: string; targetId: string }
  | { kind: 'command'; label: string; action: ChatCommandItem };

export declare function buildChatCockpitActions(params: {
  runOverview: ChatRunOverview | null;
  approvalQueue: ApprovalItem[];
  resumeActions: ChatCommandItem[];
}): {
  objective: ChatCockpitCardAction | null;
  blocker: ChatCockpitCardAction | null;
  next: ChatCockpitCardAction | null;
};
