'use client';

import type { ComponentProps, HTMLAttributes } from 'react';

import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

export type ConfirmationState = 'approval-requested' | 'approved' | 'rejected';

export type ConfirmationProps = HTMLAttributes<HTMLDivElement> & {
  state: ConfirmationState;
};

const confirmationStateClasses: Record<ConfirmationState, string> = {
  'approval-requested': 'border-amber-500/30 bg-amber-500/10 text-foreground',
  approved: 'border-emerald-500/30 bg-emerald-500/10 text-foreground',
  rejected: 'border-rose-500/30 bg-rose-500/10 text-foreground',
};

export const Confirmation = ({ state, className, ...props }: ConfirmationProps) => (
  <div
    className={cn(
      'w-full rounded-xl border p-4 shadow-sm',
      confirmationStateClasses[state],
      className,
    )}
    data-state={state}
    {...props}
  />
);

export const ConfirmationRequest = ({ className, ...props }: HTMLAttributes<HTMLDivElement>) => (
  <div className={cn('space-y-1 text-sm', className)} {...props} />
);

export const ConfirmationAccepted = ({ className, ...props }: HTMLAttributes<HTMLDivElement>) => (
  <div className={cn('flex items-start gap-2 text-sm', className)} {...props} />
);

export const ConfirmationRejected = ({ className, ...props }: HTMLAttributes<HTMLDivElement>) => (
  <div className={cn('flex items-start gap-2 text-sm', className)} {...props} />
);

export const ConfirmationActions = ({ className, ...props }: HTMLAttributes<HTMLDivElement>) => (
  <div className={cn('mt-3 flex flex-wrap gap-2', className)} {...props} />
);

export const ConfirmationAction = ({
  className,
  size = 'sm',
  type = 'button',
  ...props
}: ComponentProps<typeof Button>) => (
  <Button className={className} size={size} type={type} {...props} />
);
