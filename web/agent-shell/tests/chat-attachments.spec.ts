import assert from 'node:assert/strict';

import {
  buildAttachmentPromptSection,
  collectMessageAttachments,
  getAttachmentLabel,
  getAttachmentMediaCategory,
  isTextAttachment,
  serializeAttachmentForPrompt,
} from '../lib/chat-attachments.js';

export async function run() {
  const textAttachment = {
    type: 'file',
    filename: 'component.tsx',
    mediaType: 'text/plain',
    url: 'data:text/plain;base64,ZXhwb3J0IGNvbnN0IENvbXBvbmVudCA9ICgpID0+IDxkaXY+SGVsbG88L2Rpdj47',
  };

  const binaryAttachment = {
    type: 'file',
    filename: 'design.png',
    mediaType: 'image/png',
    url: 'data:image/png;base64,AAAA',
  };

  assert.equal(getAttachmentLabel(textAttachment), 'component.tsx');
  assert.equal(getAttachmentMediaCategory(textAttachment), 'document');
  assert.equal(getAttachmentMediaCategory(binaryAttachment), 'image');
  assert.equal(isTextAttachment(textAttachment), true);
  assert.equal(isTextAttachment(binaryAttachment), false);

  assert.equal(
    serializeAttachmentForPrompt(textAttachment),
    [
      '[attachment]',
      'filename: component.tsx',
      'media_type: text/plain',
      'content:',
      '```tsx',
      'export const Component = () => <div>Hello</div>;',
      '```',
    ].join('\n'),
  );

  assert.equal(
    serializeAttachmentForPrompt(binaryAttachment),
    [
      '[attachment]',
      'filename: design.png',
      'media_type: image/png',
      'note: Binary attachment included in chat UI but omitted from the text prompt.',
    ].join('\n'),
  );

  const truncated = serializeAttachmentForPrompt(
    {
      type: 'file',
      filename: 'long.txt',
      mediaType: 'text/plain',
      url: `data:text/plain,${encodeURIComponent('abcdefghijklmnopqrstuvwxyz')}`,
    },
    { maxCharacters: 10 },
  );

  assert.equal(
    truncated,
    [
      '[attachment]',
      'filename: long.txt',
      'media_type: text/plain',
      'note: truncated to 10 characters',
      'content:',
      '```txt',
      'abcdefghij',
      '…[truncated]',
      '```',
    ].join('\n'),
  );

  const fencedContent = serializeAttachmentForPrompt({
    type: 'file',
    filename: 'snippet.md',
    mediaType: 'text/markdown',
    url: `data:text/markdown,${encodeURIComponent('```tsx\nconst a = 1;\n```')}`,
  });

  assert.equal(
    fencedContent,
    [
      '[attachment]',
      'filename: snippet.md',
      'media_type: text/markdown',
      'content:',
      '````md',
      '```tsx',
      'const a = 1;',
      '```',
      '````',
    ].join('\n'),
  );

  const parts = [
    { type: 'text', text: 'hello' },
    textAttachment,
    binaryAttachment,
  ];

  assert.deepEqual(collectMessageAttachments(parts), [textAttachment, binaryAttachment]);
  assert.equal(
    buildAttachmentPromptSection(parts),
    [
      '[attachment]',
      'filename: component.tsx',
      'media_type: text/plain',
      'content:',
      '```tsx',
      'export const Component = () => <div>Hello</div>;',
      '```',
      '',
      '[attachment]',
      'filename: design.png',
      'media_type: image/png',
      'note: Binary attachment included in chat UI but omitted from the text prompt.',
    ].join('\n'),
  );
}
