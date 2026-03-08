export function isAgentRunnable(agent: { status: string; runnable?: boolean }): boolean;
export function choosePreferredAgent<
  T extends { status: string; model: string; runnable?: boolean }
>(
  agents: T[],
  preferredModel?: string,
): T | null;
export function chooseInitialAgentSelection<
  T extends { agent_id: string; status: string; model: string; runnable?: boolean }
>(params: {
  agents: T[];
  currentAgentId?: string;
  rememberedAgentId?: string;
  preferredModel?: string;
}): T | null;
export function buildChatAgentUnavailableMessage(
  agent:
    | {
        name: string;
        status: string;
        runnable_reason?: string;
      }
    | null
    | undefined,
): string;
