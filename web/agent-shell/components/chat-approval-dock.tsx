'use client';

import { ShieldAlert } from 'lucide-react';

import {
  Queue,
  QueueItem,
  QueueItemAction,
  QueueItemActions,
  QueueItemContent,
  QueueItemDescription,
  QueueItemIndicator,
  QueueList,
  QueueSection,
  QueueSectionContent,
  QueueSectionLabel,
  QueueSectionTrigger,
} from '@/components/ai-elements/queue';
import type { ApprovalItem } from '@/lib/daemon-rpc';
import { cn } from '@/lib/utils';

type ChatApprovalDockProps = {
  approvals: ApprovalItem[];
  busyId: string | null;
  onJumpToApproval: (approvalId: string) => void;
  onDecision: (approvalId: string, decision: 'approve' | 'deny') => void;
};

export default function ChatApprovalDockPanel({
  approvals,
  busyId,
  onJumpToApproval,
  onDecision,
}: ChatApprovalDockProps) {
  if (approvals.length === 0) {
    return null;
  }

  return (
    <Queue className="mb-3 border-amber-500/30 bg-amber-500/5">
      <QueueSection>
        <QueueSectionTrigger className="bg-amber-500/10 hover:bg-amber-500/15">
          <QueueSectionLabel
            count={approvals.length}
            icon={<ShieldAlert className="size-4 text-amber-400" />}
            label="Pending approvals"
          />
        </QueueSectionTrigger>
        <QueueSectionContent>
          <QueueList>
            {approvals.map((approval) => (
              <QueueItem key={approval.id} className="border border-transparent hover:border-amber-500/20">
                <div className="flex items-start gap-3">
                  <QueueItemIndicator className="border-amber-400 bg-amber-400/20" />
                  <QueueItemContent>{approval.tool}</QueueItemContent>
                </div>
                <QueueItemDescription>
                  <div>{approval.reason}</div>
                  <div className="mt-1 text-[11px] uppercase tracking-[0.14em] text-muted-foreground/80">
                    Requested at {approval.requested_at}
                  </div>
                </QueueItemDescription>
                <QueueItemActions className="ml-5">
                  <QueueItemAction onClick={() => onJumpToApproval(approval.id)}>Jump</QueueItemAction>
                  <QueueItemAction
                    className={cn('text-emerald-300 hover:text-emerald-100')}
                    disabled={busyId === approval.id}
                    onClick={() => onDecision(approval.id, 'approve')}
                  >
                    Approve
                  </QueueItemAction>
                  <QueueItemAction
                    className={cn('text-red-300 hover:text-red-100')}
                    disabled={busyId === approval.id}
                    onClick={() => onDecision(approval.id, 'deny')}
                  >
                    Deny
                  </QueueItemAction>
                </QueueItemActions>
              </QueueItem>
            ))}
          </QueueList>
        </QueueSectionContent>
      </QueueSection>
    </Queue>
  );
}
