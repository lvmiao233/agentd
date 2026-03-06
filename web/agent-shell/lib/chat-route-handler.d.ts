import type { UIMessage } from 'ai';

export declare function buildConversationInput(messages: UIMessage[]): string;

export declare function buildSingleTextStreamResponse(text: string): Response;

export declare function handleChatPost(
  req: Request,
  options?: {
    fetchImpl?: typeof fetch;
    daemonUrl?: string;
  }
): Promise<Response>;
