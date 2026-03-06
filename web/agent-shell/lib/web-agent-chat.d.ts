export type WebAgentChatMessage =
  | {
      id: string;
      role: 'user' | 'assistant';
      text: string;
      content: string;
      streamTokens?: string[];
    }
  | {
      id: string;
      role: 'tool';
      toolName: string;
      input: unknown;
      tool: string;
      args: unknown;
    };

export declare class WebAgentChatModel {
  messages: WebAgentChatMessage[];
  connected: boolean;
  showReconnectBanner: boolean;
  nextId(): string;
  appendUserMessage(text: string, id?: string): string;
  appendAssistantMessage(text: string, id?: string): string;
  appendAssistantToken(token: string): void;
  appendToolCall(toolName: string, args: unknown, id?: string): string;
  handleDisconnect(): void;
  handleReconnect(): void;
  applyBridgeEvent(event: unknown): void;
  snapshot(): {
    connected: boolean;
    showReconnectBanner: boolean;
    messages: WebAgentChatMessage[];
  };
}
