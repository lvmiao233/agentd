'use client';

import { CommandIcon, RefreshCcw, SquareIcon, WandSparkles } from 'lucide-react';

import {
  PromptInputCommand,
  PromptInputCommandEmpty,
  PromptInputCommandGroup,
  PromptInputCommandInput,
  PromptInputCommandItem,
  PromptInputCommandList,
  PromptInputCommandSeparator,
} from '@/components/ai-elements/prompt-input';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import type { ChatCommandItem } from '@/lib/chat-command-menu.js';

type ChatCommandMenuProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  items: ChatCommandItem[];
  onSelect: (item: ChatCommandItem) => void;
};

function commandIcon(item: ChatCommandItem) {
  if (item.kind === 'action') {
    return item.action === 'stop' ? SquareIcon : RefreshCcw;
  }

  return WandSparkles;
}

export default function ChatCommandMenu({ open, onOpenChange, items, onSelect }: ChatCommandMenuProps) {
  const workflowItems = items.filter((item) => item.group === 'workflow');
  const conversationItems = items.filter((item) => item.group === 'conversation');

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="overflow-hidden p-0 sm:max-w-xl">
        <DialogHeader className="sr-only">
          <DialogTitle>Agent command palette</DialogTitle>
          <DialogDescription>Run a reusable coding command or control the current conversation.</DialogDescription>
        </DialogHeader>
        <PromptInputCommand className="[&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:py-2 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:uppercase [&_[cmdk-group-heading]]:tracking-[0.16em] [&_[cmdk-input-wrapper]]:border-b [&_[cmdk-input-wrapper]]:px-3 [&_[cmdk-input]]:h-12 [&_[cmdk-item]]:px-3 [&_[cmdk-item]]:py-3">
          <PromptInputCommandInput placeholder="Search workflow commands…" />
          <PromptInputCommandList>
            <PromptInputCommandEmpty>No matching commands.</PromptInputCommandEmpty>
            {workflowItems.length > 0 && (
              <PromptInputCommandGroup heading="Workflow">
                {workflowItems.map((item) => {
                  const Icon = commandIcon(item);
                  return (
                    <PromptInputCommandItem
                      key={item.id}
                      disabled={item.disabled}
                      keywords={item.keywords}
                      onSelect={() => onSelect(item)}
                      value={`${item.title} ${item.description} ${item.keywords.join(' ')}`}
                    >
                      <Icon className="mt-0.5 size-4" />
                      <div className="flex min-w-0 flex-1 flex-col gap-1">
                        <span className="text-sm font-medium leading-none">{item.title}</span>
                        <span className="text-xs text-muted-foreground">{item.description}</span>
                      </div>
                    </PromptInputCommandItem>
                  );
                })}
              </PromptInputCommandGroup>
            )}
            {workflowItems.length > 0 && conversationItems.length > 0 && <PromptInputCommandSeparator />}
            {conversationItems.length > 0 && (
              <PromptInputCommandGroup heading="Conversation">
                {conversationItems.map((item) => {
                  const Icon = commandIcon(item);
                  return (
                    <PromptInputCommandItem
                      key={item.id}
                      disabled={item.disabled}
                      keywords={item.keywords}
                      onSelect={() => onSelect(item)}
                      value={`${item.title} ${item.description} ${item.keywords.join(' ')}`}
                    >
                      <Icon className="mt-0.5 size-4" />
                      <div className="flex min-w-0 flex-1 flex-col gap-1">
                        <span className="text-sm font-medium leading-none">{item.title}</span>
                        <span className="text-xs text-muted-foreground">{item.description}</span>
                      </div>
                    </PromptInputCommandItem>
                  );
                })}
              </PromptInputCommandGroup>
            )}
          </PromptInputCommandList>
        </PromptInputCommand>
      </DialogContent>
    </Dialog>
  );
}
