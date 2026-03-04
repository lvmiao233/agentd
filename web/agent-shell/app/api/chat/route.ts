import { streamText, UIMessage, convertToModelMessages } from 'ai';
import { createOpenAI } from '@ai-sdk/openai';

export const maxDuration = 60;

const oneApi = createOpenAI({
  baseURL: process.env.ONE_API_BASE_URL ?? 'http://127.0.0.1:3000/v1',
  apiKey: process.env.ONE_API_TOKEN ?? process.env.OPENAI_API_KEY ?? '',
});

const DAEMON_URL = process.env.AGENTD_DAEMON_URL ?? 'http://127.0.0.1:7000';

async function recordTokenUsage(
  agentId: string,
  modelName: string,
  inputTokens: number,
  outputTokens: number,
) {
  try {
    await fetch(`${DAEMON_URL}/rpc`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: Date.now(),
        method: 'RecordUsage',
        params: {
          agent_id: agentId,
          model_name: modelName,
          input_tokens: inputTokens,
          output_tokens: outputTokens,
          cost_usd: 0,
          usage_source: 'web-chat',
        },
      }),
    });
  } catch {
    // non-critical
  }
}

export async function POST(req: Request) {
  const {
    messages,
    model: modelId,
    agentId,
  }: {
    messages: UIMessage[];
    model?: string;
    agentId?: string;
  } = await req.json();

  const selectedModel = modelId ?? 'gpt-5.3-codex';

  const result = streamText({
    model: oneApi(selectedModel),
    messages: await convertToModelMessages(messages),
    onFinish: async ({ usage }) => {
      if (agentId && usage) {
        await recordTokenUsage(
          agentId,
          selectedModel,
          usage.inputTokens ?? 0,
          usage.outputTokens ?? 0,
        );
      }
    },
  });

  return result.toUIMessageStreamResponse();
}
