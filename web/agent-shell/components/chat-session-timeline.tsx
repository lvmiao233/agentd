'use client';

import { HistoryIcon, RotateCcwIcon } from 'lucide-react';

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
import type { ChatCheckpoint } from '@/lib/chat-checkpoints.js';
import type { ChatSessionTimeline } from '@/lib/chat-session-timeline.js';
import { cn } from '@/lib/utils';

type ChatSessionTimelineProps = {
  timeline: ChatSessionTimeline;
  onJumpToMessage: (targetId: string) => void;
  onRestoreCheckpoint: (checkpoint: ChatCheckpoint) => void;
  checkpointsById: Record<string, ChatCheckpoint>;
};

export default function ChatSessionTimelinePanel({
  timeline,
  onJumpToMessage,
  onRestoreCheckpoint,
  checkpointsById,
}: ChatSessionTimelineProps) {
  return (
    <Queue className="mb-3">
      <QueueSection defaultOpen={false}>
        <QueueSectionTrigger>
          <QueueSectionLabel
            count={timeline.count}
            icon={<HistoryIcon className="size-4" />}
            label={timeline.title}
          />
        </QueueSectionTrigger>
        <QueueSectionContent>
          <QueueList>
            {timeline.items.map((item) => {
              const checkpoint = checkpointsById[item.id];

              return (
                <QueueItem key={item.id} className={cn(item.isActive && 'bg-muted/40')}>
                  <div className="flex items-start gap-3">
                    <QueueItemIndicator completed={item.completed} className={cn(item.isActive && 'border-sky-500 bg-sky-500/20')} />
                    <QueueItemContent completed={item.completed}>
                      {item.ordinal}. {item.label}
                    </QueueItemContent>
                  </div>
                  <QueueItemDescription completed={item.completed}>
                    {item.description}
                  </QueueItemDescription>
                  <QueueItemActions>
                    <QueueItemAction onClick={() => onJumpToMessage(item.targetId)}>
                      Jump
                    </QueueItemAction>
                    {checkpoint && (
                      <QueueItemAction onClick={() => onRestoreCheckpoint(checkpoint)}>
                        <RotateCcwIcon className="mr-1 size-3" />
                        Restore
                      </QueueItemAction>
                    )}
                  </QueueItemActions>
                </QueueItem>
              );
            })}
          </QueueList>
        </QueueSectionContent>
      </QueueSection>
    </Queue>
  );
}
