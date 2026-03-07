import type { ApprovalItem } from '@/lib/daemon-rpc';

export type ResolvedApprovalItem = ApprovalItem & {
  decision: 'approve' | 'deny';
  resolvedAt: string;
};

export type ApprovalFeedItem =
  | { kind: 'pending'; approval: ApprovalItem }
  | { kind: 'resolved'; approval: ResolvedApprovalItem };

export function buildApprovalFeed(params: {
  pending: ApprovalItem[];
  resolved: ResolvedApprovalItem[];
  resolvedLimit?: number;
}): ApprovalFeedItem[];

export function approvalDecisionLabel(decision: 'approve' | 'deny'): 'Approved' | 'Denied';
