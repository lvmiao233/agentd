import assert from 'node:assert/strict';
import { WebAgentChatModel } from '../lib/web-agent-chat.mjs';

export async function run() {
  const model = new WebAgentChatModel();

  model.appendUserMessage('分析 main.rs');
  model.applyBridgeEvent({
    method: 'Chat.StreamToken',
    params: { token: '分析' },
  });
  model.applyBridgeEvent({
    method: 'Chat.StreamToken',
    params: { token: ' main.rs' },
  });

  const assistant = model.messages.find((message) => message.role === 'assistant');
  assert.ok(assistant, 'assistant message should exist');
  assert.equal(assistant.content, '分析 main.rs');
  assert.ok(assistant.streamTokens.length >= 2, 'stream-token should appear');

  model.handleDisconnect();
  assert.equal(model.showReconnectBanner, true, 'reconnect-banner should be visible after disconnect');

  model.handleReconnect();
  assert.equal(model.showReconnectBanner, false, 'reconnect-banner should hide after reconnect');
}
