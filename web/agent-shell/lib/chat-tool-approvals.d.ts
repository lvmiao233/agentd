import type { ApprovalItem } from '@/lib/daemon-rpc';

export type ToolApprovalNode = {
  key: string;
  toolCallId: string;
  toolName: string;
};

export function assignApprovalsToTools(params: {
  toolNodes: ToolApprovalNode[];
  approvals: ApprovalItem[];
}): {
  assignments: Map<string, ApprovalItem>;
  unmatchedApprovals: ApprovalItem[];
};

export function getToolNameAliases(name: string): string[];
