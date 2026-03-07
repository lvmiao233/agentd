export type ToolInputEvent = {
  toolCallId?: string;
  toolName: string;
  input: unknown;
};

export type ChatUiStreamHandlers = {
  onAssistantDelta?: (delta: string) => void;
  onToolInput?: (event: ToolInputEvent) => void;
  onFinish?: (finishReason: string) => void;
};

export function consumeChatUiStream(
  response: Response,
  handlers?: ChatUiStreamHandlers,
): Promise<void>;
