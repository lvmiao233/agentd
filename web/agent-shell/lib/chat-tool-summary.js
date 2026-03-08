function normalizeSnippet(value, maxLength = 96) {
  if (typeof value !== 'string') {
    return '';
  }

  const normalized = value.replace(/\s+/g, ' ').trim();
  if (!normalized) {
    return '';
  }

  return normalized.length <= maxLength
    ? normalized
    : `${normalized.slice(0, maxLength - 1).trimEnd()}…`;
}

function stringValue(value) {
  return typeof value === 'string' && value.trim() ? value : '';
}

function numberValue(value) {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

export function summarizeToolInput(input) {
  if (typeof input === 'string') {
    return normalizeSnippet(input);
  }

  if (!input || typeof input !== 'object') {
    return '';
  }

  for (const field of ['path', 'command', 'query', 'url', 'task', 'prompt']) {
    if (typeof input[field] === 'string' && input[field].trim()) {
      return `${field}: ${normalizeSnippet(input[field], 72)}`;
    }
  }

  try {
    return normalizeSnippet(JSON.stringify(input));
  } catch {
    return 'structured input';
  }
}

export function summarizeToolOutput(output, errorText) {
  const error = stringValue(errorText);
  if (error) {
    return normalizeSnippet(error);
  }

  if (typeof output === 'string') {
    return normalizeSnippet(output);
  }

  if (!output || typeof output !== 'object') {
    return '';
  }

  for (const field of ['stdout', 'stderr', 'content', 'text', 'message', 'summary']) {
    const value = stringValue(output[field]);
    if (value) {
      return normalizeSnippet(value);
    }
  }

  for (const field of ['path', 'filePath', 'url', 'command']) {
    const value = stringValue(output[field]);
    if (value) {
      return `${field}: ${normalizeSnippet(value, 72)}`;
    }
  }

  const count = numberValue(output.count ?? output.total ?? output.items);
  if (count !== null) {
    return `count: ${count}`;
  }

  const exitCode = numberValue(output.exitCode);
  if (exitCode !== null) {
    return `exit code: ${exitCode}`;
  }

  if (output.ok === true) {
    return 'Completed successfully.';
  }

  try {
    return normalizeSnippet(JSON.stringify(output));
  } catch {
    return 'structured output';
  }
}

export function buildToolOutputFacts(output, errorText) {
  const facts = [];

  const error = stringValue(errorText);
  if (error) {
    facts.push({ label: 'Status', value: 'Error' });
  }

  if (output && typeof output === 'object') {
    if (output.ok === true) {
      facts.push({ label: 'Status', value: 'OK' });
    }

    const exitCode = numberValue(output.exitCode);
    if (exitCode !== null) {
      facts.push({ label: 'Exit code', value: String(exitCode) });
    }

    for (const field of ['path', 'filePath', 'command', 'url']) {
      const value = stringValue(output[field]);
      if (value) {
        facts.push({ label: field, value: normalizeSnippet(value, 48) });
      }
    }

    const count = numberValue(output.count ?? output.total ?? output.items);
    if (count !== null) {
      facts.push({ label: 'Count', value: String(count) });
    }
  }

  return facts.slice(0, 4);
}
