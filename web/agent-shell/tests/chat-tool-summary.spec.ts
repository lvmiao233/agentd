import assert from 'node:assert/strict';

import {
  buildToolOutputFacts,
  summarizeToolInput,
  summarizeToolOutput,
} from '../lib/chat-tool-summary.js';

export async function run() {
  assert.equal(summarizeToolInput({ path: 'src/app.tsx' }), 'path: src/app.tsx');
  assert.equal(
    summarizeToolOutput({ stdout: 'tests passed\nwith details' }, undefined),
    'tests passed with details',
  );
  assert.equal(
    summarizeToolOutput({ path: 'README.md', ok: true }, undefined),
    'path: README.md',
  );
  assert.equal(summarizeToolOutput(undefined, 'permission denied'), 'permission denied');

  assert.deepEqual(buildToolOutputFacts({ path: 'README.md', ok: true, exitCode: 0 }, undefined), [
    { label: 'Status', value: 'OK' },
    { label: 'Exit code', value: '0' },
    { label: 'path', value: 'README.md' },
  ]);
}
