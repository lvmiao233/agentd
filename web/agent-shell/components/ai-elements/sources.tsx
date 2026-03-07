'use client';

import type {
  AnchorHTMLAttributes,
  DetailsHTMLAttributes,
  HTMLAttributes,
} from 'react';

import { cn } from '@/lib/utils';
import { ExternalLinkIcon, FileTextIcon, LinkIcon } from 'lucide-react';

export const Sources = ({ className, ...props }: DetailsHTMLAttributes<HTMLDetailsElement>) => (
  <details
    className={cn('group w-full rounded-xl border border-border/60 bg-muted/20', className)}
    {...props}
  />
);

export type SourcesTriggerProps = HTMLAttributes<HTMLElement> & {
  count: number;
};

export const SourcesTrigger = ({ count, className, ...props }: SourcesTriggerProps) => (
  <summary
    className={cn(
      'flex cursor-pointer list-none items-center gap-2 px-3 py-2 text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground marker:content-none',
      className,
    )}
    {...props}
  >
    <LinkIcon className="size-4" />
    {count} source{count === 1 ? '' : 's'}
  </summary>
);

export const SourcesContent = ({ className, ...props }: HTMLAttributes<HTMLDivElement>) => (
  <div className={cn('border-t border-border/50 p-2', className)} {...props} />
);

export type SourceProps = AnchorHTMLAttributes<HTMLAnchorElement> & {
  title: string;
  kind?: 'url' | 'document';
};

export const Source = ({ className, title, kind = 'url', href, ...props }: SourceProps) => {
  const Icon = kind === 'document' ? FileTextIcon : LinkIcon;

  return (
    <a
      className={cn(
        'flex items-center justify-between gap-3 rounded-lg px-3 py-2 text-sm text-foreground transition-colors hover:bg-accent hover:text-accent-foreground',
        className,
      )}
      href={href}
      rel="noreferrer"
      target={href ? '_blank' : undefined}
      {...props}
    >
      <span className="flex min-w-0 items-center gap-2">
        <Icon className="size-4 shrink-0 text-muted-foreground" />
        <span className="truncate">{title}</span>
      </span>
      {href ? <ExternalLinkIcon className="size-4 shrink-0 text-muted-foreground" /> : null}
    </a>
  );
};
