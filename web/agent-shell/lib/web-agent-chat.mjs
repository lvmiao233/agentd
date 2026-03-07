export class WebAgentChatModel {
  constructor() {
    this.messages = [];
    this.connected = true;
    this.showReconnectBanner = false;
  }

  nextId() {
    if (globalThis.crypto?.randomUUID) {
      return globalThis.crypto.randomUUID();
    }
    return `msg-${Date.now()}-${Math.random()}`;
  }

  appendUserMessage(text, id = this.nextId()) {
    this.messages.push({ id, role: 'user', text, content: text });
    return id;
  }

  appendAssistantMessage(text, id = this.nextId()) {
    this.messages.push({
      id,
      role: 'assistant',
      text,
      content: text,
      streamTokens: [text],
    });
    return id;
  }

  appendAssistantToken(token) {
    const last = this.messages[this.messages.length - 1];
    if (!last || last.role !== 'assistant') {
      this.messages.push({
        id: this.nextId(),
        role: 'assistant',
        text: token,
        content: token,
        streamTokens: [token],
      });
      return;
    }

    last.text = `${last.text ?? last.content ?? ''}${token}`;
    last.content = last.text;
    if (!Array.isArray(last.streamTokens)) {
      last.streamTokens = [];
    }
    last.streamTokens.push(token);
  }

  appendToolCall(toolName, args, id = this.nextId(), output = undefined, errorText = undefined) {
    const existing = this.messages.find((message) => message.role === 'tool' && message.id === id);
    if (existing) {
      existing.toolName = toolName;
      existing.input = args;
      existing.tool = toolName;
      existing.args = args;
      existing.output = output;
      existing.errorText = errorText;
      return id;
    }

    this.messages.push({
      id,
      role: 'tool',
      toolName,
      input: args,
      tool: toolName,
      args,
      output,
      errorText,
    });
    return id;
  }

  appendToolResult(id, output = undefined, errorText = undefined) {
    const existing = this.messages.find((message) => message.role === 'tool' && message.id === id);
    if (existing) {
      existing.output = output;
      existing.errorText = errorText;
      return id;
    }

    this.messages.push({
      id,
      role: 'tool',
      toolName: 'unknown_tool',
      input: {},
      tool: 'unknown_tool',
      args: {},
      output,
      errorText,
    });
    return id;
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

  snapshot() {
    return {
      connected: this.connected,
      showReconnectBanner: this.showReconnectBanner,
      messages: this.messages.map((message) => ({
        ...message,
        ...(Array.isArray(message.streamTokens)
          ? { streamTokens: [...message.streamTokens] }
          : {}),
      })),
    };
  }
}
