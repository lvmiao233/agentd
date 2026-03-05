import { emitRunAgentStreamLine } from './run-agent-stream.mjs';

function parseSseDataLine(line) {
  if (!line.startsWith('data:')) {
    return null;
  }

  let payload = line.slice(5);
  if (payload.startsWith(' ')) {
    payload = payload.slice(1);
  }
  return payload;
}

export async function consumeRunAgentStream({ responseBody, textId, writer }) {
  const reader = responseBody.getReader();
  const decoder = new TextDecoder();

  let pendingText = '';
  let pendingDataLines = [];
  let emitted = false;
  let terminalReached = false;
  let finishReason = null;

  const commitEvent = () => {
    if (pendingDataLines.length === 0) {
      return;
    }

    const eventData = pendingDataLines.join('\n');
    pendingDataLines = [];

    const outcome = emitRunAgentStreamLine({
      lineRaw: eventData,
      textId,
      writer,
    });
    emitted = emitted || outcome.emitted;
    terminalReached = terminalReached || outcome.terminalReached;
    if (outcome.finishReason !== null) {
      finishReason = outcome.finishReason;
    }
  };

  const handleLine = (lineRaw) => {
    const line = lineRaw.endsWith('\r') ? lineRaw.slice(0, -1) : lineRaw;

    if (line.length === 0) {
      commitEvent();
      return;
    }

    if (line.startsWith(':')) {
      return;
    }

    const data = parseSseDataLine(line);
    if (data !== null) {
      if (pendingDataLines.length > 0) {
        commitEvent();
        if (terminalReached) {
          return;
        }
      }
      pendingDataLines.push(data);
      return;
    }

    const fieldSeparator = line.indexOf(':');
    if (fieldSeparator >= 0) {
      const fieldName = line.slice(0, fieldSeparator);
      if (/^[a-zA-Z]+$/.test(fieldName)) {
        return;
      }
    }

    if (line.startsWith('{') || line.startsWith('[')) {
      commitEvent();
    }

    const outcome = emitRunAgentStreamLine({
      lineRaw: line,
      textId,
      writer,
    });
    emitted = emitted || outcome.emitted;
    terminalReached = terminalReached || outcome.terminalReached;
    if (outcome.finishReason !== null) {
      finishReason = outcome.finishReason;
    }
  };

  const drainPendingLines = (flushRemainder) => {
    let newlineIndex = pendingText.indexOf('\n');
    while (newlineIndex >= 0) {
      const line = pendingText.slice(0, newlineIndex);
      pendingText = pendingText.slice(newlineIndex + 1);
      handleLine(line);
      if (terminalReached) {
        return;
      }
      newlineIndex = pendingText.indexOf('\n');
    }

    if (!flushRemainder) {
      return;
    }

    if (pendingText.length > 0) {
      handleLine(pendingText);
      pendingText = '';
    }
    commitEvent();
  };

  while (!terminalReached) {
    const { done, value } = await reader.read();
    if (done) {
      pendingText += decoder.decode();
      break;
    }

    pendingText += decoder.decode(value, { stream: true });
    drainPendingLines(false);
  }

  if (!terminalReached) {
    drainPendingLines(true);
  }

  return {
    emitted,
    terminalReached,
    finishReason,
  };
}
