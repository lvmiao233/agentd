const TEXT_ATTACHMENT_EXTENSIONS = new Set([
  '.c',
  '.cc',
  '.cpp',
  '.css',
  '.csv',
  '.go',
  '.h',
  '.hpp',
  '.html',
  '.java',
  '.js',
  '.json',
  '.jsx',
  '.kt',
  '.md',
  '.mdx',
  '.mjs',
  '.py',
  '.rb',
  '.rs',
  '.sh',
  '.sql',
  '.svg',
  '.swift',
  '.toml',
  '.ts',
  '.tsx',
  '.txt',
  '.xml',
  '.yaml',
  '.yml',
]);

const MEDIA_TYPE_LANGUAGE_HINTS = new Map([
  ['application/json', 'json'],
  ['application/ld+json', 'json'],
  ['application/sql', 'sql'],
  ['application/toml', 'toml'],
  ['application/typescript', 'ts'],
  ['application/x-sh', 'sh'],
  ['application/xml', 'xml'],
  ['application/yaml', 'yaml'],
  ['image/svg+xml', 'svg'],
  ['text/css', 'css'],
  ['text/csv', 'csv'],
  ['text/html', 'html'],
  ['text/javascript', 'js'],
  ['text/jsx', 'jsx'],
  ['text/markdown', 'md'],
  ['text/plain', 'txt'],
  ['text/x-python', 'py'],
  ['text/xml', 'xml'],
]);

const MAX_ATTACHMENT_CHARACTERS = 12000;

function extensionFromFilename(filename) {
  if (typeof filename !== 'string') {
    return '';
  }

  const lastDot = filename.lastIndexOf('.');
  if (lastDot < 0) {
    return '';
  }

  return filename.slice(lastDot).toLowerCase();
}

export function getAttachmentLabel(part) {
  if (!part || typeof part !== 'object') {
    return 'Attachment';
  }

  if (part.type === 'source-document') {
    return typeof part.title === 'string' && part.title.trim() ? part.title : 'Source document';
  }

  if (typeof part.filename === 'string' && part.filename.trim()) {
    return part.filename;
  }

  return 'Attachment';
}

export function getAttachmentMediaCategory(part) {
  if (!part || typeof part !== 'object') {
    return 'unknown';
  }

  if (part.type === 'source-document') {
    return 'source';
  }

  const mediaType = typeof part.mediaType === 'string' ? part.mediaType.toLowerCase() : '';

  if (mediaType.startsWith('image/')) {
    return 'image';
  }

  if (mediaType.startsWith('video/')) {
    return 'video';
  }

  if (mediaType.startsWith('audio/')) {
    return 'audio';
  }

  if (mediaType.startsWith('text/') || mediaType.includes('json') || mediaType.includes('xml')) {
    return 'document';
  }

  return mediaType ? 'document' : 'unknown';
}

export function collectMessageAttachments(parts) {
  if (!Array.isArray(parts)) {
    return [];
  }

  return parts.filter((part) => part?.type === 'file' || part?.type === 'source-document');
}

export function isTextAttachment(part) {
  if (!part || part.type !== 'file') {
    return false;
  }

  const mediaType = typeof part.mediaType === 'string' ? part.mediaType.toLowerCase() : '';
  if (mediaType.startsWith('text/')) {
    return true;
  }

  if (
    mediaType.includes('json') ||
    mediaType.includes('xml') ||
    mediaType.includes('yaml') ||
    mediaType.includes('toml') ||
    mediaType.includes('javascript') ||
    mediaType.includes('typescript') ||
    mediaType === 'image/svg+xml'
  ) {
    return true;
  }

  return TEXT_ATTACHMENT_EXTENSIONS.has(extensionFromFilename(part.filename));
}

function languageHintFromFilename(filename) {
  const extension = extensionFromFilename(filename);
  if (!extension) {
    return 'text';
  }

  return extension.slice(1);
}

function languageHintFromMediaType(mediaType) {
  if (typeof mediaType !== 'string') {
    return '';
  }

  return MEDIA_TYPE_LANGUAGE_HINTS.get(mediaType.toLowerCase()) ?? '';
}

function getAttachmentLanguageHint(part) {
  const mediaTypeHint = languageHintFromMediaType(part.mediaType);
  const filenameHint = languageHintFromFilename(part.filename);

  if (filenameHint && filenameHint !== 'text' && (!mediaTypeHint || mediaTypeHint === 'txt')) {
    return filenameHint;
  }

  return mediaTypeHint || filenameHint;
}

function fenceForContent(content) {
  const matches = content.match(/`+/g) ?? [];
  const longestRun = matches.reduce((max, candidate) => Math.max(max, candidate.length), 0);
  return '`'.repeat(Math.max(3, longestRun + 1));
}

function decodeDataUrlText(dataUrl) {
  if (typeof dataUrl !== 'string' || !dataUrl.startsWith('data:')) {
    return null;
  }

  const commaIndex = dataUrl.indexOf(',');
  if (commaIndex < 0) {
    return null;
  }

  const metadata = dataUrl.slice(5, commaIndex);
  const payload = dataUrl.slice(commaIndex + 1);

  try {
    if (metadata.includes(';base64')) {
      return Buffer.from(payload, 'base64').toString('utf8');
    }

    return decodeURIComponent(payload);
  } catch {
    return null;
  }
}

export function serializeAttachmentForPrompt(part, options = {}) {
  if (!part || part.type !== 'file') {
    return null;
  }

  const maxCharacters =
    typeof options.maxCharacters === 'number' && options.maxCharacters > 0
      ? options.maxCharacters
      : MAX_ATTACHMENT_CHARACTERS;

  const label = getAttachmentLabel(part);
  const mediaType = typeof part.mediaType === 'string' && part.mediaType.trim() ? part.mediaType : 'unknown';

  if (!isTextAttachment(part)) {
    return `[attachment]\nfilename: ${label}\nmedia_type: ${mediaType}\nnote: Binary attachment included in chat UI but omitted from the text prompt.`;
  }

  const content = decodeDataUrlText(part.url);
  if (typeof content !== 'string') {
    return `[attachment]\nfilename: ${label}\nmedia_type: ${mediaType}\nnote: Text attachment could not be decoded from the uploaded payload.`;
  }

  const normalized = content.replace(/\r\n/g, '\n');
  const truncated = normalized.length > maxCharacters;
  const displayedContent = truncated ? `${normalized.slice(0, maxCharacters)}\n…[truncated]` : normalized;
  const language = getAttachmentLanguageHint(part);
  const fence = fenceForContent(displayedContent);

  const lines = [
    '[attachment]',
    `filename: ${label}`,
    `media_type: ${mediaType}`,
  ];

  if (truncated) {
    lines.push(`note: truncated to ${maxCharacters} characters`);
  }

  lines.push('content:');
  lines.push(`${fence}${language}`);
  lines.push(displayedContent);
  lines.push(fence);

  return lines.join('\n');
}

export function buildAttachmentPromptSection(parts, options = {}) {
  const serialized = collectMessageAttachments(parts)
    .map((part) => serializeAttachmentForPrompt(part, options))
    .filter((part) => typeof part === 'string' && part.trim().length > 0);

  if (serialized.length === 0) {
    return '';
  }

  return serialized.join('\n\n');
}
