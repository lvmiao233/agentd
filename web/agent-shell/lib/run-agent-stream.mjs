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
      let name = 'unknown_tool';
      let argumentsText = '';

      if (functionValue && typeof functionValue === 'object') {
        const nameValue = functionValue.name;
        if (typeof nameValue === 'string' && nameValue.trim()) {
          name = nameValue;
        }

        const argumentsValue = functionValue.arguments;
        if (typeof argumentsValue === 'string') {
          argumentsText = argumentsValue;
        }
      }

      return {
        id,
        name,
        argumentsText,
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

function isTerminalStreamFrame(frame) {
  const payload = normalizeStreamPayload(frame);
  const status = payload.status;
  if (status === 'completed' || status === 'done' || status === 'failed' || status === 'blocked') {
    return true;
  }
  const kind = payload.type ?? payload.event ?? payload.kind;
  return kind === 'done' || kind === 'completed' || kind === 'finish' || kind === 'finished';
}

export function emitRunAgentStreamLine({ lineRaw, textId, writer }) {
  let line = (lineRaw ?? '').trim();
  if (!line) {
    return { emitted: false, terminalReached: false };
  }

  if (line.startsWith('data:')) {
    line = line.slice(5).trim();
  }
  if (!line) {
    return { emitted: false, terminalReached: false };
  }

  let parsed;
  try {
    parsed = JSON.parse(line);
  } catch {
    writer.write({ type: 'text-delta', id: textId, delta: line });
    return { emitted: true, terminalReached: false };
  }

  const errorMessage = extractStreamError(parsed);
  if (errorMessage) {
    writer.write({
      type: 'text-delta',
      id: textId,
      delta: `RunAgent failed: ${errorMessage}`,
    });
    return { emitted: true, terminalReached: true };
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
      writer.write({
        type: 'tool-input-available',
        toolCallId: toolCall.id,
        toolName: toolCall.name,
        input: parseToolCallInput(toolCall.argumentsText),
      });
    }
    emitted = true;
  }

  return {
    emitted,
    terminalReached: isTerminalStreamFrame(parsed),
  };
}
