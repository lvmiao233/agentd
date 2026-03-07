function normalizeLanguage(language) {
  return String(language ?? '')
    .trim()
    .toLowerCase();
}

export function extractPreviewArtifacts(markdown) {
  const text = String(markdown ?? '');
  const pattern = /```([\w-]+)?\n([\s\S]*?)```/g;
  const artifacts = [];

  for (const match of text.matchAll(pattern)) {
    const language = normalizeLanguage(match[1]);
    const code = (match[2] ?? '').trim();

    if (!code) {
      continue;
    }

    if (language === 'html' || language === 'svg') {
      artifacts.push({
        code,
        language,
        title: language === 'html' ? 'HTML preview' : 'SVG preview',
      });
    }
  }

  return artifacts;
}
