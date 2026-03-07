'use client';

import type { ButtonHTMLAttributes, HTMLAttributes } from 'react';

import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

export const Suggestions = ({ className, ...props }: HTMLAttributes<HTMLDivElement>) => (
  <div className={cn('flex flex-wrap gap-2', className)} {...props} />
);

export type SuggestionProps = ButtonHTMLAttributes<HTMLButtonElement>;

export const Suggestion = ({ className, type = 'button', ...props }: SuggestionProps) => (
  <Button
    className={cn('h-auto rounded-full px-3 py-1.5 text-xs', className)}
    size="sm"
    type={type}
    variant="outline"
    {...props}
  />
);
