'use client';

import type { FileUIPart, SourceDocumentUIPart } from 'ai';
import type { ComponentProps, HTMLAttributes, ReactNode } from 'react';

import { Button } from '@/components/ui/button';
import {
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
} from '@/components/ui/hover-card';
import { cn } from '@/lib/utils';
import {
  FileCode2Icon,
  FileIcon,
  FileImageIcon,
  FileTextIcon,
  Link2Icon,
  Music4Icon,
  VideoIcon,
  XIcon,
} from 'lucide-react';
import {
  createContext,
  useContext,
} from 'react';

import {
  getAttachmentLabel,
  getAttachmentMediaCategory,
  type ChatAttachmentPart,
} from '@/lib/chat-attachments.js';

type AttachmentVariant = 'grid' | 'inline' | 'list';

type AttachmentContextValue = {
  data: ChatAttachmentPart;
  onRemove?: () => void;
  variant: AttachmentVariant;
};

const AttachmentContext = createContext<AttachmentContextValue | null>(null);

function useAttachmentContext() {
  const value = useContext(AttachmentContext);
  if (!value) {
    throw new Error('Attachment components must be used within <Attachment>.');
  }
  return value;
}

function iconForAttachment(data: ChatAttachmentPart) {
  switch (getAttachmentMediaCategory(data)) {
    case 'image':
      return <FileImageIcon className="size-4" />;
    case 'video':
      return <VideoIcon className="size-4" />;
    case 'audio':
      return <Music4Icon className="size-4" />;
    case 'source':
      return <Link2Icon className="size-4" />;
    case 'document':
      if (typeof data.mediaType === 'string' && data.mediaType.includes('json')) {
        return <FileCode2Icon className="size-4" />;
      }
      return <FileTextIcon className="size-4" />;
    default:
      return <FileIcon className="size-4" />;
  }
}

export type AttachmentsProps = HTMLAttributes<HTMLDivElement> & {
  variant?: AttachmentVariant;
};

export const Attachments = ({ className, variant = 'grid', ...props }: AttachmentsProps) => (
  <div
    data-variant={variant}
    className={cn(
      variant === 'grid' && 'grid gap-3 sm:grid-cols-2',
      variant === 'inline' && 'flex flex-wrap items-center gap-2',
      variant === 'list' && 'flex flex-col gap-2',
      className,
    )}
    {...props}
  />
);

export type AttachmentProps = HTMLAttributes<HTMLDivElement> & {
  data: ChatAttachmentPart;
  onRemove?: () => void;
  variant?: AttachmentVariant;
};

export const Attachment = ({
  children,
  className,
  data,
  onRemove,
  variant,
  ...props
}: AttachmentProps) => {
  const resolvedVariant = variant ?? 'grid';

  return (
    <AttachmentContext.Provider value={{ data, onRemove, variant: resolvedVariant }}>
      <div
        className={cn(
          'group relative overflow-hidden border bg-card text-card-foreground',
          resolvedVariant === 'grid' && 'rounded-xl p-3',
          resolvedVariant === 'inline' && 'inline-flex items-center gap-2 rounded-full px-2.5 py-1.5 text-xs',
          resolvedVariant === 'list' && 'flex items-center gap-3 rounded-lg p-3',
          className,
        )}
        {...props}
      >
        {children}
      </div>
    </AttachmentContext.Provider>
  );
};

export type AttachmentPreviewProps = HTMLAttributes<HTMLDivElement> & {
  fallbackIcon?: ReactNode;
};

export const AttachmentPreview = ({ className, fallbackIcon, ...props }: AttachmentPreviewProps) => {
  const { data, variant } = useAttachmentContext();
  const category = getAttachmentMediaCategory(data);
  const label = getAttachmentLabel(data);

  if (category === 'image' && data.type === 'file' && data.url) {
    return (
      <div
        className={cn(
          variant === 'grid' ? 'overflow-hidden rounded-lg border bg-muted' : 'shrink-0',
          className,
        )}
        {...props}
      >
        <img
          alt={label}
          className={cn(
            'object-cover',
            variant === 'grid' && 'h-24 w-full',
            variant === 'inline' && 'size-5 rounded',
            variant === 'list' && 'size-10 rounded-md',
          )}
          src={data.url}
        />
      </div>
    );
  }

  return (
    <div
      className={cn(
        'flex shrink-0 items-center justify-center rounded-md border bg-muted text-muted-foreground',
        variant === 'grid' && 'size-10',
        variant === 'inline' && 'size-5 rounded-sm border-none bg-transparent',
        variant === 'list' && 'size-10',
        className,
      )}
      {...props}
    >
      {fallbackIcon ?? iconForAttachment(data)}
    </div>
  );
};

export type AttachmentInfoProps = HTMLAttributes<HTMLDivElement> & {
  showMediaType?: boolean;
};

export const AttachmentInfo = ({ className, showMediaType = false, ...props }: AttachmentInfoProps) => {
  const { data, variant } = useAttachmentContext();
  const label = getAttachmentLabel(data);

  return (
    <div className={cn('min-w-0', className)} {...props}>
      <div className={cn('truncate font-medium', variant === 'inline' ? 'max-w-44 text-xs' : 'text-sm')}>
        {label}
      </div>
      {showMediaType && typeof data.mediaType === 'string' && data.mediaType.trim() && (
        <div className="truncate text-muted-foreground text-xs">{data.mediaType}</div>
      )}
    </div>
  );
};

export type AttachmentRemoveProps = ComponentProps<typeof Button> & {
  label?: string;
};

export const AttachmentRemove = ({ className, label = 'Remove attachment', ...props }: AttachmentRemoveProps) => {
  const { onRemove, variant } = useAttachmentContext();

  if (!onRemove) {
    return null;
  }

  return (
    <Button
      aria-label={label}
      className={cn(
        'shrink-0 text-muted-foreground hover:text-foreground',
        variant === 'grid' && 'absolute right-2 top-2 opacity-0 transition-opacity group-hover:opacity-100',
        variant !== 'grid' && 'h-6 w-6',
        className,
      )}
      onClick={onRemove}
      size="icon"
      type="button"
      variant="ghost"
      {...props}
    >
      <XIcon className="size-3.5" />
    </Button>
  );
};

export type AttachmentHoverCardProps = ComponentProps<typeof HoverCard>;

export const AttachmentHoverCard = ({ openDelay = 0, closeDelay = 0, ...props }: AttachmentHoverCardProps) => (
  <HoverCard openDelay={openDelay} closeDelay={closeDelay} {...props} />
);

export type AttachmentHoverCardTriggerProps = ComponentProps<typeof HoverCardTrigger>;

export const AttachmentHoverCardTrigger = (props: AttachmentHoverCardTriggerProps) => (
  <HoverCardTrigger {...props} />
);

export type AttachmentHoverCardContentProps = ComponentProps<typeof HoverCardContent>;

export const AttachmentHoverCardContent = ({ align = 'start', ...props }: AttachmentHoverCardContentProps) => (
  <HoverCardContent align={align} {...props} />
);

export const AttachmentEmpty = ({ className, ...props }: HTMLAttributes<HTMLDivElement>) => (
  <div className={cn('rounded-lg border border-dashed p-4 text-muted-foreground text-sm', className)} {...props} />
);
