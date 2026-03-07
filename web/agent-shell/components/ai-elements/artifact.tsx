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
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { CodeIcon, EyeIcon, FileCodeIcon } from 'lucide-react';
import { useMemo, useState } from 'react';

export type ArtifactLanguage = 'html' | 'svg';

export type ArtifactProps = HTMLAttributes<HTMLDivElement> & {
  code: string;
  language: ArtifactLanguage;
  title?: string;
};

function buildPreviewDocument(code: string, language: ArtifactLanguage) {
  if (language === 'svg') {
    return `<!doctype html><html><body style="margin:0;display:flex;min-height:100vh;align-items:center;justify-content:center;background:#0b1020;">${code}</body></html>`;
  }

  return code;
}

function codeLanguageForArtifact(language: ArtifactLanguage): BundledLanguage {
  return language === 'svg' ? 'xml' : 'html';
}

export const Artifact = ({
  className,
  code,
  language,
  title,
  ...props
}: ArtifactProps) => {
  const [view, setView] = useState<'preview' | 'code'>('preview');
  const previewDocument = useMemo(() => buildPreviewDocument(code, language), [code, language]);

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
        <div className="bg-background p-3">
          <iframe
            className="h-72 w-full rounded-lg border border-border bg-white"
            sandbox="allow-scripts"
            srcDoc={previewDocument}
            title={title ?? `${language} preview`}
          />
        </div>
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
