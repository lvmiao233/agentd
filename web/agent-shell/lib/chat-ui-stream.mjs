function parseDataPayload(line) {
  if (!line.startsWith('data:')) {
    return null;
  }

  const payload = line.slice(5).trim();
  if (!payload || payload === '[DONE]') {
    return null;
  }

  try {
    return JSON.parse(payload);
  } catch {
    return null;
  }
}

export async function consumeChatUiStream(response, handlers) {
  if (!response.body) {
    throw new Error('chat response body missing');
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';

  const onAssistantDelta = handlers?.onAssistantDelta ?? (() => {});
  const onToolInput = handlers?.onToolInput ?? (() => {});
  const onToolOutput = handlers?.onToolOutput ?? (() => {});
  const onFinish = handlers?.onFinish ?? (() => {});

  const processBuffer = () => {
    const lines = buffer.split('\n');
    buffer = lines.pop() ?? '';
    for (const rawLine of lines) {
      const line = rawLine.trim();
      if (!line) {
        continue;
      }

      const event = parseDataPayload(line);
      if (!event || typeof event !== 'object') {
        continue;
      }

      if (event.type === 'text-delta' && typeof event.delta === 'string') {
        onAssistantDelta(event.delta);
      } else if (event.type === 'tool-input-available') {
        onToolInput({
          toolCallId:
            typeof event.toolCallId === 'string' && event.toolCallId.trim()
              ? event.toolCallId
              : undefined,
          toolName:
            typeof event.toolName === 'string' && event.toolName.trim()
              ? event.toolName
              : 'unknown_tool',
          input: event.input,
        });
      } else if (event.type === 'tool-output-available') {
        onToolOutput({
          toolCallId:
            typeof event.toolCallId === 'string' && event.toolCallId.trim()
              ? event.toolCallId
              : undefined,
          output: event.output,
          errorText:
            typeof event.errorText === 'string' && event.errorText.trim()
              ? event.errorText
              : undefined,
        });
      } else if (event.type === 'finish') {
        onFinish(event.finishReason ?? 'stop');
      }
    }
  };

  for (;;) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    buffer += decoder.decode(value, { stream: true });
    processBuffer();
  }

  buffer += decoder.decode();
  processBuffer();
}
