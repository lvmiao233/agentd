export type PreviewArtifact = {
  code: string;
  language: 'html' | 'svg';
  title: string;
};

export function extractPreviewArtifacts(markdown: string): PreviewArtifact[];
