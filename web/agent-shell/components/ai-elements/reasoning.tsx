'use client';

import type { DetailsHTMLAttributes, HTMLAttributes } from 'react';

import { cn } from '@/lib/utils';
import { ChevronDownIcon, SparklesIcon } from 'lucide-react';

export type ReasoningProps = DetailsHTMLAttributes<HTMLDetailsElement> & {
  isStreaming?: boolean;
};

export const Reasoning = ({
  className,
  isStreaming = false,
  open,
  ...props
}: ReasoningProps) => (
  <details
    className={cn(
      'group w-full overflow-hidden rounded-xl border border-border/60 bg-muted/20',
      className,
    )}
    open={open ?? isStreaming}
    {...props}
  />
);

export const ReasoningTrigger = ({ className, ...props }: HTMLAttributes<HTMLElement>) => (
  <summary
    className={cn(
      'flex cursor-pointer list-none items-center justify-between gap-3 px-3 py-2 text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground marker:content-none',
      className,
    )}
    {...props}
  >
    <span className="flex items-center gap-2">
      <SparklesIcon className="size-4" />
      Reasoning
    </span>
    <ChevronDownIcon className="size-4 transition-transform group-open:rotate-180" />
  </summary>
);

export const ReasoningContent = ({ className, ...props }: HTMLAttributes<HTMLDivElement>) => (
  <div
    className={cn(
      'border-t border-border/50 px-3 py-3 text-xs leading-6 text-muted-foreground whitespace-pre-wrap',
      className,
    )}
    {...props}
  />
);
