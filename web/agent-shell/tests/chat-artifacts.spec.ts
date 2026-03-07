import assert from 'node:assert/strict';

import { extractPreviewArtifacts } from '../lib/chat-artifacts.js';

export async function run() {
  const artifacts = extractPreviewArtifacts([
    'before',
    '',
    '```html',
    '<div class="card">Hello</div>',
    '```',
    '',
    'middle',
    '',
    '```svg',
    '<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="4" /></svg>',
    '```',
    '',
    '```ts',
    "console.log('ignore me')",
    '```',
  ].join('\n'));

  assert.deepEqual(artifacts, [
    {
      code: '<div class="card">Hello</div>',
      language: 'html',
      title: 'HTML preview',
    },
    {
      code: '<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="4" /></svg>',
      language: 'svg',
      title: 'SVG preview',
    },
  ]);
}
