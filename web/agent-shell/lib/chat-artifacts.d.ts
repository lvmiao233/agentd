export type PreviewArtifact = {
  code: string;
  previewCode: string;
  language: 'html' | 'svg' | 'jsx' | 'tsx';
  title: string;
};

export function extractPreviewArtifacts(
  markdown: string,
  options?: { includeIncomplete?: boolean },
): PreviewArtifact[];
