import type { UIMessage } from 'ai';

export declare function buildConversationInput(messages: UIMessage[]): string;

export declare function normalizeChatMessages(
  messages: UIMessage[],
  trigger?: string,
  messageId?: string,
): UIMessage[];

export declare function buildSingleTextStreamResponse(text: string): Response;

export declare function describeTransportFailure(error: unknown): string;

export declare function handleChatPost(
  req: Request,
  options?: {
    fetchImpl?: typeof fetch;
    daemonUrl?: string;
  }
): Promise<Response>;
