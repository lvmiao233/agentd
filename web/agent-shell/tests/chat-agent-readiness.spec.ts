import assert from 'node:assert/strict';
import {
  buildChatAgentUnavailableMessage,
  choosePreferredAgent,
  isAgentRunnable,
} from '../lib/chat-agent-readiness.js';

export async function run() {
  const agents = [
    {
      agent_id: 'agent-failed',
      name: 'failed-agent',
      model: 'gpt-5.3-codex',
      status: 'failed',
    },
    {
      agent_id: 'agent-ready-codex',
      name: 'ready-codex',
      model: 'gpt-5.3-codex',
      status: 'ready',
    },
    {
      agent_id: 'agent-ready-mini',
      name: 'ready-mini',
      model: 'gpt-4.1-mini',
      status: 'ready',
    },
  ];

  assert.equal(isAgentRunnable(agents[0]), false, 'failed agents are not runnable');
  assert.equal(isAgentRunnable(agents[1]), true, 'ready agents are runnable');

  assert.equal(
    choosePreferredAgent(agents)?.agent_id,
    'agent-ready-codex',
    'preferred model should win among runnable agents'
  );

  assert.equal(
    choosePreferredAgent([agents[0], agents[2]])?.agent_id,
    'agent-ready-mini',
    'first runnable fallback should be used when preferred model is unavailable'
  );

  assert.equal(
    choosePreferredAgent([agents[0]])?.agent_id,
    'agent-failed',
    'the only agent is still returned so the UI can explain why it is unavailable'
  );

  assert.equal(
    buildChatAgentUnavailableMessage(null),
    'No runnable agent is available. Create or start a ready agent first.',
    'missing selection should produce the generic fail-fast message'
  );

  assert.equal(
    buildChatAgentUnavailableMessage(agents[0]),
    'Agent failed-agent is failed and cannot run chat requests yet. Select a ready agent first.',
    'non-runnable selection should surface a specific agent status message'
  );
}
