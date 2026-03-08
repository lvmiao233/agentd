import { buildFollowUpSuggestions } from './chat-follow-up-suggestions.js';

function createPromptCommand({ id, title, description, prompt, disabled, keywords = [] }) {
  return {
    id,
    group: 'workflow',
    kind: 'prompt',
    title,
    description,
    prompt,
    disabled,
    keywords,
  };
}

function createActionCommand({ id, title, description, action, disabled, keywords = [] }) {
  return {
    id,
    group: 'conversation',
    kind: 'action',
    title,
    description,
    action,
    disabled,
    keywords,
  };
}

function promptTitle(prompt) {
  if (prompt === 'Continue coding until this task is fully finished.') {
    return 'Continue coding';
  }

  if (prompt === 'Run verification now and fix any issues you find.') {
    return 'Run verification';
  }

  if (prompt === 'Summarize what changed and what still remains.') {
    return 'Summarize progress';
  }

  if (prompt === 'Show the next highest-impact step and execute it.') {
    return 'Execute next step';
  }

  if (prompt === 'Explain the pending approval and what will happen after I approve it.') {
    return 'Explain approval';
  }

  if (prompt === 'Retry the last step with a different approach.') {
    return 'Retry with a new approach';
  }

  if (prompt === 'Analyze the task, propose the highest-impact next step, and start implementing it.') {
    return 'Plan and start coding';
  }

  if (prompt === 'Inspect the current codebase, explain the relevant context, and then continue the implementation.') {
    return 'Inspect context first';
  }

  return prompt;
}

export function buildChatCommandItems({
  status,
  lastAssistantText,
  hasToolParts,
  hasPendingApprovals,
  hasConversation,
  canRegenerate,
  selectedAgentRunnable,
}) {
  const suggestions = hasConversation
    ? buildFollowUpSuggestions({
        status,
        lastAssistantText,
        hasToolParts,
        hasPendingApprovals,
      })
    : [
        'Analyze the task, propose the highest-impact next step, and start implementing it.',
        'Inspect the current codebase, explain the relevant context, and then continue the implementation.',
      ];

  const promptDisabled =
    status === 'submitted' ||
    status === 'streaming' ||
    selectedAgentRunnable === false;

  const promptDisabledReason = status === 'submitted' || status === 'streaming'
    ? 'Stop the active run before sending another command.'
    : selectedAgentRunnable === false && hasConversation
      ? 'Pick a runnable agent before sending a command.'
      : 'Run this command immediately.';

  const items = suggestions.map((prompt) =>
    createPromptCommand({
      id: `prompt-${prompt}`,
      title: promptTitle(prompt),
      description: promptDisabled ? promptDisabledReason : prompt,
      prompt,
      disabled: promptDisabled,
      keywords: prompt.toLowerCase().split(/\s+/),
    }),
  );

  if (canRegenerate) {
    items.push(
      createActionCommand({
        id: 'regenerate-last-answer',
        title: 'Regenerate last answer',
        description: 'Ask the current agent to try the latest turn again.',
        action: 'regenerate',
        disabled: status === 'submitted' || status === 'streaming',
        keywords: ['retry', 'rerun', 'regenerate'],
      }),
    );
  }

  if (status === 'submitted' || status === 'streaming') {
    items.push(
      createActionCommand({
        id: 'stop-current-run',
        title: 'Stop current run',
        description: 'Interrupt the active response so you can steer the agent.',
        action: 'stop',
        disabled: false,
        keywords: ['stop', 'interrupt', 'cancel'],
      }),
    );
  }

  return items;
}
