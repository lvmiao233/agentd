export class WebAgentChatModel {
  constructor() {
    this.messages = [];
    this.connected = true;
    this.showReconnectBanner = false;
  }

  appendUserMessage(text) {
    this.messages.push({ role: 'user', content: text });
  }

  appendAssistantToken(token) {
    const last = this.messages[this.messages.length - 1];
    if (!last || last.role !== 'assistant') {
      this.messages.push({ role: 'assistant', content: token, streamTokens: [token] });
      return;
    }

    last.content += token;
    if (!Array.isArray(last.streamTokens)) {
      last.streamTokens = [];
    }
    last.streamTokens.push(token);
  }

  appendToolCall(toolName, args) {
    this.messages.push({
      role: 'tool',
      tool: toolName,
      args,
    });
  }

  handleDisconnect() {
    this.connected = false;
    this.showReconnectBanner = true;
  }

  handleReconnect() {
    this.connected = true;
    this.showReconnectBanner = false;
  }

  applyBridgeEvent(event) {
    const method = event?.method;
    const params = event?.params ?? {};

    if (method === 'Chat.StreamToken' && typeof params.token === 'string') {
      this.appendAssistantToken(params.token);
      return;
    }

    if (method === 'Tool.Call') {
      this.appendToolCall(params.tool ?? 'unknown', params.args ?? {});
    }
  }
}
