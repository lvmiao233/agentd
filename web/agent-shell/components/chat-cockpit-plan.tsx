'use client';

import {
  Plan,
  PlanAction,
  PlanContent,
  PlanDescription,
  PlanFooter,
  PlanHeader,
  PlanTitle,
  PlanTrigger,
} from '@/components/ai-elements/plan';
import { Suggestion, Suggestions } from '@/components/ai-elements/suggestion';
import ChatApprovalDockPanel from '@/components/chat-approval-dock';
import ChatRunOverviewPanel from '@/components/chat-run-overview';
import ChatSessionTimelinePanel from '@/components/chat-session-timeline';
import type { ChatCheckpoint } from '@/lib/chat-checkpoints.js';
import type { ChatCommandItem } from '@/lib/chat-command-menu.js';
import type { ChatCockpitPlan } from '@/lib/chat-cockpit-plan.js';
import type { ApprovalItem } from '@/lib/daemon-rpc';
import type { ChatRunOverview } from '@/lib/chat-run-overview.js';
import type { ChatSessionTimeline } from '@/lib/chat-session-timeline.js';

type ChatCockpitPlanPanelProps = {
  plan: ChatCockpitPlan;
  runOverview: ChatRunOverview | null;
  resumeActions: ChatCommandItem[];
  approvalQueue: ApprovalItem[];
  approvalBusyId: string | null;
  sessionTimeline: ChatSessionTimeline | null;
  checkpointsById: Record<string, ChatCheckpoint>;
  onActionSelect: (action: ChatCommandItem) => void;
  onNavigateToTarget: (targetId: string) => void;
  onApprovalDecision: (approvalId: string, decision: 'approve' | 'deny') => void;
  onRestoreCheckpoint: (checkpoint: ChatCheckpoint) => void;
};

export default function ChatCockpitPlanPanel({
  plan,
  runOverview,
  resumeActions,
  approvalQueue,
  approvalBusyId,
  sessionTimeline,
  checkpointsById,
  onActionSelect,
  onNavigateToTarget,
  onApprovalDecision,
  onRestoreCheckpoint,
}: ChatCockpitPlanPanelProps) {
  return (
    <Plan className="mb-3 border-border/70 bg-card/80" defaultOpen={plan.defaultOpen} isStreaming={plan.isStreaming}>
      <PlanHeader className="gap-3">
        <div className="space-y-1">
          <div className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">Agent cockpit</div>
          <PlanTitle>{plan.title}</PlanTitle>
          <PlanDescription>{plan.description}</PlanDescription>
        </div>
        <PlanAction>
          <PlanTrigger />
        </PlanAction>
      </PlanHeader>

      <PlanContent className="space-y-3">
        {approvalQueue.length > 0 && (
          <ChatApprovalDockPanel
            approvals={approvalQueue}
            busyId={approvalBusyId}
            className="mb-0"
            onDecision={onApprovalDecision}
            onJumpToApproval={(approvalId) => onNavigateToTarget(`chat-approval-${approvalId}`)}
          />
        )}

        {runOverview && (
          <ChatRunOverviewPanel
            overview={runOverview}
            className="mb-0 border-border/60 bg-background/60 shadow-none"
            onNavigateToTarget={onNavigateToTarget}
          />
        )}

        {sessionTimeline && (
          <ChatSessionTimelinePanel
            timeline={sessionTimeline}
            checkpointsById={checkpointsById}
            className="mb-0"
            onJumpToMessage={onNavigateToTarget}
            onRestoreCheckpoint={onRestoreCheckpoint}
          />
        )}
      </PlanContent>

      {resumeActions.length > 0 && (
        <PlanFooter className="pt-0">
          <div className="space-y-2">
            <div className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">Continue this run</div>
            <Suggestions>
              {resumeActions.map((action) => (
                <Suggestion
                  key={action.id}
                  disabled={action.disabled}
                  onClick={() => onActionSelect(action)}
                  title={action.description}
                >
                  {action.title}
                </Suggestion>
              ))}
            </Suggestions>
          </div>
        </PlanFooter>
      )}
    </Plan>
  );
}
