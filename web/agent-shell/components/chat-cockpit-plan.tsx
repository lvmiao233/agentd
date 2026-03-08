'use client';

import { useEffect, useState } from 'react';

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
import { Button } from '@/components/ui/button';
import ChatApprovalDockPanel from '@/components/chat-approval-dock';
import ChatRunOverviewPanel from '@/components/chat-run-overview';
import ChatSessionTimelinePanel from '@/components/chat-session-timeline';
import { buildChatCockpitActions } from '@/lib/chat-cockpit-actions.js';
import type { ChatCheckpoint } from '@/lib/chat-checkpoints.js';
import type { ChatCommandItem } from '@/lib/chat-command-menu.js';
import type { ChatCockpitPlan } from '@/lib/chat-cockpit-plan.js';
import type { ApprovalItem } from '@/lib/daemon-rpc';
import type { ChatLatestOutput } from '@/lib/chat-latest-output.js';
import type { ChatRunOverview } from '@/lib/chat-run-overview.js';
import type { ChatSessionTimeline } from '@/lib/chat-session-timeline.js';

type ChatCockpitPlanPanelProps = {
  plan: ChatCockpitPlan;
  runOverview: ChatRunOverview | null;
  resumeActions: ChatCommandItem[];
  approvalQueue: ApprovalItem[];
  approvalBusyId: string | null;
  sessionTimeline: ChatSessionTimeline | null;
  latestOutput: ChatLatestOutput | null;
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
  latestOutput,
  checkpointsById,
  onActionSelect,
  onNavigateToTarget,
  onApprovalDecision,
  onRestoreCheckpoint,
}: ChatCockpitPlanPanelProps) {
  const [open, setOpen] = useState(plan.defaultOpen);
  const cockpitActions = buildChatCockpitActions({
    runOverview,
    approvalQueue,
    resumeActions,
  });
  const toneClasses: Record<'default' | 'warning' | 'success', string> = {
    default: 'border-border/60 bg-background/60',
    warning: 'border-amber-500/30 bg-amber-500/10',
    success: 'border-emerald-500/30 bg-emerald-500/10',
  };

  const handleHighlightAction = (key: 'objective' | 'blocker' | 'next') => {
    const action = cockpitActions[key];
    if (!action) {
      return;
    }

    if (action.kind === 'navigate') {
      onNavigateToTarget(action.targetId);
      return;
    }

    onActionSelect(action.action);
  };

  useEffect(() => {
    if (plan.mode === 'blocked' || plan.mode === 'error' || plan.mode === 'unrunnable') {
      setOpen(true);
      return;
    }

    setOpen(plan.defaultOpen);
  }, [plan.defaultOpen, plan.mode]);

  return (
    <Plan className="mb-3 border-border/70 bg-card/80" isStreaming={plan.isStreaming} onOpenChange={setOpen} open={open}>
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

      <div className="grid gap-2 px-6 md:grid-cols-3">
        {plan.highlights.map((item) => {
          const itemAction = cockpitActions[item.key];

          return (
            <div key={item.key} className={`rounded-lg border px-3 py-2 ${toneClasses[item.tone]}`}>
              <div className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
                {item.label}
              </div>
              <div className="mt-1 text-sm text-foreground">{item.value}</div>
              {itemAction && (
                <Button
                  className="mt-2 h-7 px-2 text-xs"
                  onClick={() => handleHighlightAction(item.key)}
                  size="sm"
                  type="button"
                  variant="ghost"
                >
                  {itemAction.label}
                </Button>
              )}
            </div>
          );
        })}
        {latestOutput && (
          <div className="rounded-lg border border-sky-500/30 bg-sky-500/10 px-3 py-2">
            <div className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
              Latest output
            </div>
            <div className="mt-1 text-sm text-foreground">{latestOutput.title}</div>
            <div className="mt-1 text-xs text-muted-foreground">{latestOutput.description}</div>
            <Button
              className="mt-2 h-7 px-2 text-xs"
              onClick={() => onNavigateToTarget(latestOutput.targetId)}
              size="sm"
              type="button"
              variant="ghost"
            >
              Review output
            </Button>
          </div>
        )}
      </div>

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
