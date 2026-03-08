function normalizeStreamPayload(frame) {
  const maybeResult = frame?.result;
  if (maybeResult && typeof maybeResult === 'object') {
    return maybeResult;
  }
  return frame ?? {};
}

function extractStreamError(frame) {
  const payload = normalizeStreamPayload(frame);
  const error = payload.error;
  if (typeof error === 'string' && error.trim()) return error;
  if (error && typeof error === 'object') {
    const message = error.message;
    if (typeof message === 'string' && message.trim()) return message;
  }
  const message = payload.message;
  if (typeof message === 'string' && message.trim()) return message;
  const status = payload.status;
  if (status === 'failed' || status === 'blocked') {
    return 'RunAgent streaming failed.';
  }
  return null;
}

function extractStreamText(frame) {
  const payload = normalizeStreamPayload(frame);
  const llm = payload.llm;
  if (llm && typeof llm === 'object') {
    const output = llm.output;
    if (typeof output === 'string' && output.length > 0) {
      return output;
    }
  }
  for (const field of ['delta', 'token', 'text', 'content', 'output']) {
    const value = payload[field];
    if (typeof value === 'string' && value.length > 0) {
      return value;
    }
  }
  return '';
}

function extractStreamToolCalls(frame) {
  const payload = normalizeStreamPayload(frame);

  const toolContainer = payload.tool;
  if (!toolContainer || typeof toolContainer !== 'object') {
    return [];
  }

  const calls = toolContainer.calls;
  if (!Array.isArray(calls)) {
    return [];
  }

  return calls
    .map((call, index) => {
      if (!call || typeof call !== 'object') return null;

      const idValue = call.id;
      const id = typeof idValue === 'string' && idValue.trim() ? idValue : `call-${index}`;

      const functionValue = call.function;
      let name;
      let argumentsText = '';
      let hasInput = false;
      let inputStarted = false;

      if (functionValue && typeof functionValue === 'object') {
        const nameValue = functionValue.name;
        if (typeof nameValue === 'string' && nameValue.trim()) {
          name = nameValue;
        }

        const argumentsValue = functionValue.arguments;
        if (typeof argumentsValue === 'string') {
          argumentsText = argumentsValue;
          hasInput = true;
        } else if (argumentsValue !== undefined) {
          argumentsText = JSON.stringify(argumentsValue);
          hasInput = true;
        }
      }

      if (!name) {
        for (const key of ['name', 'toolName', 'tool']) {
          const value = call[key];
          if (typeof value === 'string' && value.trim()) {
            name = value;
            break;
          }
        }
      }

      for (const key of ['phase', 'state', 'status']) {
        const value = call[key];
        if (value === 'input-start') {
          inputStarted = true;
          break;
        }
      }

      if (!argumentsText) {
        for (const key of ['arguments', 'input', 'args']) {
          const value = call[key];
          if (typeof value === 'string') {
            argumentsText = value;
            hasInput = true;
            break;
          }
          if (value !== undefined) {
            argumentsText = JSON.stringify(value);
            hasInput = true;
            break;
          }
        }
      }

      return {
        id,
        name: typeof name === 'string' && name.trim() ? name : 'unknown_tool',
        argumentsText,
        hasInput,
        inputStarted,
        output: call.output,
        errorText:
          typeof call.error === 'string'
            ? call.error
            : typeof call.error?.message === 'string'
              ? call.error.message
              : undefined,
      };
    })
    .filter((entry) => entry !== null);
}

function parseToolCallInput(argumentsText) {
  const trimmed = argumentsText.trim();
  if (!trimmed) {
    return {};
  }

  try {
    return JSON.parse(trimmed);
  } catch {
    return trimmed;
  }
}

function createRunAgentStreamStateRecord() {
  return {
    inputStarted: false,
    lastArgumentsText: '',
    lastAvailableArgumentsText: null,
  };
}

function getToolCallState(streamState, toolCallId) {
  if (!streamState.toolCalls.has(toolCallId)) {
    streamState.toolCalls.set(toolCallId, createRunAgentStreamStateRecord());
  }

  return streamState.toolCalls.get(toolCallId);
}

function detectStructuredArguments(argumentsText) {
  const trimmed = argumentsText.trim();
  return trimmed.startsWith('{') || trimmed.startsWith('[');
}

function getInputAvailability(argumentsText) {
  const trimmed = argumentsText.trim();
  if (!trimmed) {
    return {
      input: {},
      isComplete: true,
      isStructured: false,
    };
  }

  const isStructured = detectStructuredArguments(argumentsText);

  if (!isStructured) {
    return {
      input: trimmed,
      isComplete: true,
      isStructured: false,
    };
  }

  try {
    return {
      input: JSON.parse(trimmed),
      isComplete: true,
      isStructured: true,
    };
  } catch {
    return {
      input: trimmed,
      isComplete: false,
      isStructured: true,
    };
  }
}

function getInputTextDelta(previousText, nextText) {
  if (!nextText || nextText === previousText) {
    return '';
  }

  if (!previousText) {
    return nextText;
  }

  if (nextText.startsWith(previousText)) {
    return nextText.slice(previousText.length);
  }

  return nextText;
}

function isTerminalStreamFrame(frame) {
  const payload = normalizeStreamPayload(frame);
  const status = payload.status;
  if (status === 'completed' || status === 'done' || status === 'failed' || status === 'blocked') {
    return true;
  }
  const kind = payload.type ?? payload.event ?? payload.kind;
  return kind === 'done' || kind === 'completed' || kind === 'finish' || kind === 'finished';
}

function terminalFinishReason(frame) {
  const payload = normalizeStreamPayload(frame);
  const status = payload.status;
  if (status === 'failed' || status === 'blocked') {
    return 'error';
  }
  return 'stop';
}

export function emitRunAgentStreamLine({
  lineRaw,
  textId,
  writer,
  streamState = { toolCalls: new Map() },
}) {
  let line = (lineRaw ?? '').trim();
  if (!line) {
    return { emitted: false, terminalReached: false, finishReason: null };
  }

  if (line.startsWith('data:')) {
    line = line.slice(5).trim();
  }
  if (!line) {
    return { emitted: false, terminalReached: false, finishReason: null };
  }
  if (line === '[DONE]') {
    return { emitted: false, terminalReached: true, finishReason: 'stop' };
  }

  let parsed;
  try {
    parsed = JSON.parse(line);
  } catch {
    writer.write({ type: 'text-delta', id: textId, delta: line });
    return { emitted: true, terminalReached: false, finishReason: null };
  }

  let emitted = false;

  const chunk = extractStreamText(parsed);
  if (chunk) {
    writer.write({ type: 'text-delta', id: textId, delta: chunk });
    emitted = true;
  }

  const toolCalls = extractStreamToolCalls(parsed);
  if (toolCalls.length > 0) {
    for (const toolCall of toolCalls) {
      const toolCallState = getToolCallState(streamState, toolCall.id);

      if (toolCall.inputStarted || (toolCall.hasInput && !toolCallState.inputStarted)) {
        writer.write({
          type: 'tool-input-start',
          toolCallId: toolCall.id,
          toolName: toolCall.name,
          dynamic: true,
        });
        toolCallState.inputStarted = true;
      }

      if (toolCall.hasInput) {
        const deltaText = getInputTextDelta(
          toolCallState.lastArgumentsText,
          toolCall.argumentsText,
        );
        if (deltaText) {
          writer.write({
            type: 'tool-input-delta',
            toolCallId: toolCall.id,
            inputTextDelta: deltaText,
          });
        }

        const inputAvailability = getInputAvailability(toolCall.argumentsText);
        const shouldEmitAvailable =
          inputAvailability.isComplete ||
          toolCall.output !== undefined ||
          toolCall.errorText !== undefined;

        if (
          shouldEmitAvailable &&
          toolCallState.lastAvailableArgumentsText !== toolCall.argumentsText
        ) {
          writer.write({
            type: 'tool-input-available',
            toolCallId: toolCall.id,
            toolName: toolCall.name,
            input: inputAvailability.isComplete
              ? inputAvailability.input
              : parseToolCallInput(toolCall.argumentsText),
            dynamic: true,
          });
          toolCallState.lastAvailableArgumentsText = toolCall.argumentsText;
        }

        toolCallState.lastArgumentsText = toolCall.argumentsText;
      }

      if (toolCall.errorText !== undefined) {
        writer.write({
          type: 'tool-output-error',
          toolCallId: toolCall.id,
          errorText: toolCall.errorText,
        });
      } else if (toolCall.output !== undefined) {
        writer.write({
          type: 'tool-output-available',
          toolCallId: toolCall.id,
          output: toolCall.output,
        });
      }
    }
    emitted = true;
  }

  const errorMessage = extractStreamError(parsed);
  if (errorMessage) {
    writer.write({
      type: 'text-delta',
      id: textId,
      delta: `RunAgent failed: ${errorMessage}`,
    });
    return { emitted: true, terminalReached: true, finishReason: 'error' };
  }

  const terminalReached = isTerminalStreamFrame(parsed);
  return {
    emitted,
    terminalReached,
    finishReason: terminalReached ? terminalFinishReason(parsed) : null,
  };
}
