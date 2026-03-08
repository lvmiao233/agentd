'use client';

import type { ComponentProps } from 'react';

import { cn } from '@/lib/utils';

type ShimmerProps = ComponentProps<'span'>;

export function Shimmer({ className, ...props }: ShimmerProps) {
  return (
    <span
      className={cn(
        'inline-block animate-pulse bg-gradient-to-r from-muted-foreground/60 via-foreground to-muted-foreground/60 bg-clip-text text-transparent',
        className,
      )}
      {...props}
    />
  );
}
