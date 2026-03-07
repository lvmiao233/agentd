import assert from 'node:assert/strict';

import { collectSourceParts, countReasoningParts } from '../lib/chat-message-parts.js';

export async function run() {
  const parts = [
    { type: 'text', text: 'hello' },
    { type: 'reasoning', text: 'thinking' },
    { type: 'source-url', url: 'https://example.com', title: 'Example' },
    { type: 'source-document', title: 'spec.pdf', mediaType: 'application/pdf' },
    { type: 'reasoning', text: 'more thinking' },
  ];

  assert.equal(countReasoningParts(parts), 2);
  assert.deepEqual(
    collectSourceParts(parts).map(({ part, index }) => [part.type, index]),
    [
      ['source-url', 2],
      ['source-document', 3],
    ],
  );
}
