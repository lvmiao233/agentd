import assert from 'node:assert/strict';

import { buildChatLatestOutput } from '../lib/chat-latest-output.js';

export async function run() {
  const artifactOutput = buildChatLatestOutput([
    {
      id: 'assistant-1',
      role: 'assistant',
      parts: [{ type: 'text', text: '```html\n<div>Hello</div>\n```' }],
    },
  ]);

  assert.deepEqual(artifactOutput, {
    kind: 'artifact',
    title: 'HTML preview',
    description: '<div>Hello</div>',
    targetId: 'chat-artifact-assistant-1-0',
  });

  const jsxArtifactOutput = buildChatLatestOutput([
    {
      id: 'assistant-jsx',
      role: 'assistant',
      parts: [{ type: 'text', text: '```jsx\n<Button>Ship it</Button>\n```' }],
    },
  ]);

  assert.deepEqual(jsxArtifactOutput, {
    kind: 'artifact',
    title: 'JSX preview',
    description: '<Button>Ship it</Button>',
    targetId: 'chat-artifact-assistant-jsx-0',
  });

  const toolOutput = buildChatLatestOutput([
    {
      id: 'assistant-2',
      role: 'assistant',
      parts: [
        {
          type: 'dynamic-tool',
          toolName: 'mcp.fs.read_file',
          state: 'output-available',
          output: { path: 'src/app.tsx', ok: true },
        },
      ],
    },
  ]);

  assert.equal(toolOutput.kind, 'tool');
  assert.equal(toolOutput.title, 'mcp.fs.read_file');
  assert.equal(toolOutput.description, 'path: src/app.tsx');
  assert.equal(toolOutput.targetId, 'chat-tool-assistant-2-0');

  assert.equal(buildChatLatestOutput([]), null);
}
