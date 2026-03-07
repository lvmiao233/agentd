export type ToolInputEvent = {
  toolCallId?: string;
  toolName: string;
  input: unknown;
};

export type ToolInputStartEvent = {
  toolCallId?: string;
  toolName: string;
};

export type ToolOutputEvent = {
  toolCallId?: string;
  output: unknown;
  errorText?: string;
};

export type ChatUiStreamHandlers = {
  onAssistantDelta?: (delta: string) => void;
  onToolInputStart?: (event: ToolInputStartEvent) => void;
  onToolInput?: (event: ToolInputEvent) => void;
  onToolOutput?: (event: ToolOutputEvent) => void;
  onFinish?: (finishReason: string) => void;
};

export function consumeChatUiStream(
  response: Response,
  handlers?: ChatUiStreamHandlers,
): Promise<void>;
