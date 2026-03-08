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
      previewCode: '<div class="card">Hello</div>',
      language: 'html',
      title: 'HTML preview',
    },
    {
      code: '<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="4" /></svg>',
      previewCode: '<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="4" /></svg>',
      language: 'svg',
      title: 'SVG preview',
    },
  ]);

  const jsxArtifacts = extractPreviewArtifacts([
    '```tsx',
    'export default function Demo() {',
    '  return (',
    '    <Card>',
    '      <CardHeader>',
    '        <CardTitle>Hello</CardTitle>',
    '      </CardHeader>',
    '    </Card>',
    '  );',
    '}',
    '```',
  ].join('\n'));

  assert.deepEqual(jsxArtifacts, [
    {
      code: [
        'export default function Demo() {',
        '  return (',
        '    <Card>',
        '      <CardHeader>',
        '        <CardTitle>Hello</CardTitle>',
        '      </CardHeader>',
        '    </Card>',
        '  );',
        '}',
      ].join('\n'),
      previewCode: [
        '<Card>',
        '      <CardHeader>',
        '        <CardTitle>Hello</CardTitle>',
        '      </CardHeader>',
        '    </Card>',
      ].join('\n'),
      language: 'tsx',
      title: 'TSX preview',
    },
  ]);

  const streamingArtifacts = extractPreviewArtifacts([
    'Working draft',
    '',
    '```jsx',
    '<Card>',
    '  <CardContent>Streaming</CardContent>',
  ].join('\n'), { includeIncomplete: true });

  assert.deepEqual(streamingArtifacts, [
    {
      code: ['<Card>', '  <CardContent>Streaming</CardContent>'].join('\n'),
      previewCode: ['<Card>', '  <CardContent>Streaming</CardContent>'].join('\n'),
      language: 'jsx',
      title: 'JSX preview',
    },
  ]);
}
