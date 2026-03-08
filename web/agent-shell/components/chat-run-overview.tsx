'use client';

import { type ComponentProps, useCallback } from 'react';
import { Activity, ChevronDownIcon, ListChecks, ShieldAlert } from 'lucide-react';

import {
  Task,
  TaskContent,
  TaskItem,
  TaskItemFile,
  TaskTrigger,
} from '@/components/ai-elements/task';
import { Suggestion, Suggestions } from '@/components/ai-elements/suggestion';
import type { ChatCommandItem } from '@/lib/chat-command-menu.js';
import type { ChatRunOverview } from '@/lib/chat-run-overview.js';
import { cn } from '@/lib/utils';

type ChatRunOverviewProps = ComponentProps<'section'> & {
  overview: ChatRunOverview;
  actions?: ChatCommandItem[];
  onActionSelect?: (action: ChatCommandItem) => void;
  onNavigateToTarget?: (targetId: string) => void;
};

function sectionIcon(sectionKey: string) {
  if (sectionKey === 'pending-approvals') {
    return ShieldAlert;
  }

  if (sectionKey === 'tool-activity') {
    return ListChecks;
  }

  return Activity;
}

function itemDotClass(item: ChatRunOverview['sections'][number]['items'][number]) {
  if (item.tone === 'error') {
    return 'bg-destructive';
  }

  if (item.completed) {
    return 'bg-emerald-500';
  }

  if (item.tone === 'warning') {
    return 'bg-amber-500';
  }

  return 'bg-muted-foreground/50';
}

export default function ChatRunOverviewPanel({
  overview,
  className,
  actions = [],
  onActionSelect,
  onNavigateToTarget,
  ...props
}: ChatRunOverviewProps) {
  const handleNavigate = useCallback(
    (targetId: string) => {
      if (onNavigateToTarget) {
        onNavigateToTarget(targetId);
        return;
      }

      document.getElementById(targetId)?.scrollIntoView({ behavior: 'smooth', block: 'center' });
    },
    [onNavigateToTarget],
  );

  return (
    <section
      className={cn('rounded-xl border border-border bg-card/70 p-4 shadow-sm', className)}
      {...props}
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="space-y-1">
          <div className="flex items-center gap-2 text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">
            <Activity className="size-4" />
            Run overview
          </div>
          <div className="text-sm font-medium text-foreground">{overview.statusLabel}</div>
          <p className="text-sm text-muted-foreground">{overview.statusSummary}</p>
        </div>
      </div>

      {actions.length > 0 && (
        <div className="mt-4 space-y-2">
          <div className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">
            Continue this run
          </div>
          <Suggestions>
            {actions.map((action) => (
              <Suggestion
                key={action.id}
                disabled={action.disabled}
                onClick={() => onActionSelect?.(action)}
                title={action.description}
              >
                {action.title}
              </Suggestion>
            ))}
          </Suggestions>
        </div>
      )}

      <div className="mt-4 space-y-3">
        {overview.sections.map((section) => {
          const Icon = sectionIcon(section.key);

          return (
            <Task key={section.key} defaultOpen={section.defaultOpen}>
              <TaskTrigger title={section.title}>
                <button
                  type="button"
                  className="flex w-full cursor-pointer items-center gap-2 text-left text-sm text-muted-foreground transition-colors hover:text-foreground"
                >
                  <Icon className="size-4" />
                  <span className="flex-1">{section.title}</span>
                  <TaskItemFile>{section.count}</TaskItemFile>
                  <ChevronDownIcon className="size-4 transition-transform group-data-[state=open]:rotate-180" />
                </button>
              </TaskTrigger>
              <TaskContent>
                {section.items.map((item) => (
                  <TaskItem key={item.key} className="rounded-md border border-transparent px-1 py-0.5">
                    <button
                      type="button"
                      disabled={!item.targetId}
                      onClick={() => item.targetId && handleNavigate(item.targetId)}
                      className={cn(
                        'flex w-full items-start gap-3 rounded-md text-left transition-colors',
                        item.targetId ? 'cursor-pointer hover:bg-muted/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50' : 'cursor-default',
                      )}
                    >
                      <span className={cn('mt-1.5 inline-flex size-2.5 shrink-0 rounded-full', itemDotClass(item))} />
                      <div className="min-w-0 flex-1 space-y-1">
                        <div className={cn('text-sm', item.completed ? 'text-foreground/80' : 'text-foreground')}>
                          {item.title}
                        </div>
                        <div className="text-xs leading-5 text-muted-foreground">{item.description}</div>
                        {item.targetId && <div className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground/80">Jump to context</div>}
                      </div>
                    </button>
                  </TaskItem>
                ))}
              </TaskContent>
            </Task>
          );
        })}
      </div>
    </section>
  );
}
