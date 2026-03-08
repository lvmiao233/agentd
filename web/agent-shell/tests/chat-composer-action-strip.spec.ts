import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function run() {
  const chatPage = await readFile(new URL('../app/chat/page.tsx', import.meta.url), 'utf8');

  assert.match(chatPage, /function ComposerActionStrip\(/, 'chat page should define a shared composer action strip');
  assert.match(chatPage, /const \[primaryAction, \.\.\.secondaryActions\] = actions;/, 'composer action strip should promote one primary action and keep the rest as secondary options');
  assert.match(chatPage, /<Button type="button" size="sm" onClick=\{\(\) => onSelect\(primaryAction\)\}/, 'composer action strip should render the primary action as a regular button');
  assert.match(chatPage, /secondaryActions.length > 0/, 'composer action strip should only render suggestion chips for secondary actions');
}
