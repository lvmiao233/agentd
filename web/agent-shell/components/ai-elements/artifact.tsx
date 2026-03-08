'use client';

import type { HTMLAttributes } from 'react';
import type { BundledLanguage } from 'shiki';

import {
  CodeBlock,
  CodeBlockActions,
  CodeBlockCopyButton,
  CodeBlockFilename,
  CodeBlockHeader,
  CodeBlockTitle,
} from '@/components/ai-elements/code-block';
import {
  JSXPreview,
  JSXPreviewContent,
  JSXPreviewError,
} from '@/components/ai-elements/jsx-preview';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Separator } from '@/components/ui/separator';
import { Textarea } from '@/components/ui/textarea';
import { cn } from '@/lib/utils';
import { CodeIcon, EyeIcon, FileCodeIcon } from 'lucide-react';
import { useMemo, useState } from 'react';

export type ArtifactLanguage = 'html' | 'svg' | 'jsx' | 'tsx';

const JSX_PREVIEW_COMPONENTS = {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Input,
  Separator,
  Textarea,
};

export type ArtifactProps = HTMLAttributes<HTMLDivElement> & {
  code: string;
  language: ArtifactLanguage;
  title?: string;
  previewCode?: string;
  isStreaming?: boolean;
};

function buildPreviewDocument(code: string, language: ArtifactLanguage) {
  if (language === 'svg') {
    return `<!doctype html><html><body style="margin:0;display:flex;min-height:100vh;align-items:center;justify-content:center;background:#0b1020;">${code}</body></html>`;
  }

  return code;
}

function codeLanguageForArtifact(language: ArtifactLanguage): BundledLanguage {
  if (language === 'svg') {
    return 'xml';
  }

  return language === 'jsx' ? 'jsx' : language;
}

export const Artifact = ({
  className,
  code,
  language,
  title,
  previewCode,
  isStreaming = false,
  ...props
}: ArtifactProps) => {
  const [view, setView] = useState<'preview' | 'code'>('preview');
  const previewDocument = useMemo(() => buildPreviewDocument(code, language), [code, language]);
  const resolvedPreviewCode = previewCode ?? code;
  const isJsxArtifact = language === 'jsx' || language === 'tsx';

  return (
    <div className={cn('overflow-hidden rounded-xl border border-border/60 bg-muted/20', className)} {...props}>
      <div className="flex items-center justify-between border-b border-border/50 px-3 py-2">
        <div className="flex items-center gap-2 text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">
          <FileCodeIcon className="size-4" />
          {title ?? `${language} artifact`}
        </div>
        <div className="flex items-center gap-1">
          <Button
            size="sm"
            type="button"
            variant={view === 'preview' ? 'secondary' : 'ghost'}
            onClick={() => setView('preview')}
          >
            <EyeIcon className="size-4" />
            Preview
          </Button>
          <Button
            size="sm"
            type="button"
            variant={view === 'code' ? 'secondary' : 'ghost'}
            onClick={() => setView('code')}
          >
            <CodeIcon className="size-4" />
            Code
          </Button>
        </div>
      </div>

      {view === 'preview' ? (
        isJsxArtifact ? (
          <div className="space-y-3 bg-background p-3">
            <JSXPreview
              className="rounded-lg border border-border bg-card p-4 shadow-sm"
              components={JSX_PREVIEW_COMPONENTS}
              isStreaming={isStreaming}
              jsx={resolvedPreviewCode}
            >
              <JSXPreviewContent className="min-h-24" />
              <JSXPreviewError className="mt-3" />
            </JSXPreview>
            <p className="text-xs text-muted-foreground">
              Rendered with ai-elements JSX Preview using a safe subset of local UI components.
            </p>
          </div>
        ) : (
          <div className="bg-background p-3">
            <iframe
              className="h-72 w-full rounded-lg border border-border bg-white"
              sandbox="allow-scripts"
              srcDoc={previewDocument}
              title={title ?? `${language} preview`}
            />
          </div>
        )
      ) : (
        <CodeBlock code={code} language={codeLanguageForArtifact(language)}>
          <CodeBlockHeader>
            <CodeBlockTitle>
              <CodeBlockFilename>{title ?? `${language} artifact`}</CodeBlockFilename>
            </CodeBlockTitle>
            <CodeBlockActions>
              <CodeBlockCopyButton />
            </CodeBlockActions>
          </CodeBlockHeader>
        </CodeBlock>
      )}
    </div>
  );
};
