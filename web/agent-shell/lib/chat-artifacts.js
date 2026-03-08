function normalizeLanguage(language) {
  return String(language ?? '')
    .trim()
    .toLowerCase();
}

function isPreviewLanguage(language) {
  return language === 'html' || language === 'svg' || language === 'jsx' || language === 'tsx';
}

function unwrapOuterExpression(expression) {
  let current = String(expression ?? '').trim();

  while (current.startsWith('(') && current.endsWith(')')) {
    current = current.slice(1, -1).trim();
  }

  return current;
}

function extractReturnedExpression(code) {
  const returnIndex = code.lastIndexOf('return');
  if (returnIndex === -1) {
    return null;
  }

  const afterReturn = code.slice(returnIndex + 'return'.length).trimStart();
  if (!afterReturn) {
    return null;
  }

  if (afterReturn.startsWith('(')) {
    let depth = 0;
    for (let index = 0; index < afterReturn.length; index += 1) {
      const character = afterReturn[index];
      if (character === '(') {
        depth += 1;
      } else if (character === ')') {
        depth -= 1;
        if (depth === 0) {
          return afterReturn.slice(1, index).trim();
        }
      }
    }
  }

  const line = afterReturn.split('\n')[0]?.replace(/;$/, '').trim();
  return line || null;
}

function extractArrowExpression(code) {
  const arrowIndex = code.lastIndexOf('=>');
  if (arrowIndex === -1) {
    return null;
  }

  const afterArrow = code.slice(arrowIndex + 2).trim();
  if (!afterArrow || afterArrow.startsWith('{')) {
    return null;
  }

  return afterArrow.replace(/;$/, '').trim();
}

function extractRenderableJsx(code, language) {
  if (language !== 'jsx' && language !== 'tsx') {
    return code;
  }

  const trimmed = String(code ?? '').trim();
  if (!trimmed) {
    return null;
  }

  const direct = unwrapOuterExpression(trimmed);
  if (direct.startsWith('<')) {
    return direct;
  }

  const returnedExpression = extractReturnedExpression(trimmed);
  if (returnedExpression) {
    const unwrappedReturn = unwrapOuterExpression(returnedExpression);
    if (unwrappedReturn.startsWith('<')) {
      return unwrappedReturn;
    }
  }

  const arrowExpression = extractArrowExpression(trimmed);
  if (arrowExpression) {
    const unwrappedArrow = unwrapOuterExpression(arrowExpression);
    if (unwrappedArrow.startsWith('<')) {
      return unwrappedArrow;
    }
  }

  return null;
}

function artifactTitle(language) {
  switch (language) {
    case 'html':
      return 'HTML preview';
    case 'svg':
      return 'SVG preview';
    case 'jsx':
      return 'JSX preview';
    case 'tsx':
      return 'TSX preview';
    default:
      return `${language.toUpperCase()} preview`;
  }
}

function buildPreviewArtifact(code, language) {
  if (!isPreviewLanguage(language)) {
    return null;
  }

  const previewCode = extractRenderableJsx(code, language);
  if ((language === 'jsx' || language === 'tsx') && !previewCode) {
    return null;
  }

  return {
    code,
    previewCode: previewCode ?? code,
    language,
    title: artifactTitle(language),
  };
}

export function extractPreviewArtifacts(markdown, options = {}) {
  const { includeIncomplete = false } = options;
  const text = String(markdown ?? '');
  const pattern = /```([\w-]+)?\n([\s\S]*?)```/g;
  const artifacts = [];
  let consumedUntil = 0;

  for (const match of text.matchAll(pattern)) {
    const language = normalizeLanguage(match[1]);
    const code = (match[2] ?? '').trim();
    consumedUntil = Math.max(consumedUntil, (match.index ?? 0) + match[0].length);

    if (!code) {
      continue;
    }

    const artifact = buildPreviewArtifact(code, language);
    if (artifact) {
      artifacts.push(artifact);
    }
  }

  if (includeIncomplete) {
    const trailing = text.slice(consumedUntil);
    const incompleteMatch = trailing.match(/```([\w-]+)?\n([\s\S]*)$/);
    if (incompleteMatch) {
      const language = normalizeLanguage(incompleteMatch[1]);
      const code = (incompleteMatch[2] ?? '').trim();
      if (code) {
        const artifact = buildPreviewArtifact(code, language);
        if (artifact) {
          artifacts.push(artifact);
        }
      }
    }
  }

  return artifacts;
}
