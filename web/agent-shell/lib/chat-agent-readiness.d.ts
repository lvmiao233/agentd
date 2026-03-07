export function isAgentRunnable(agent: { status: string }): boolean;
export function choosePreferredAgent<T extends { status: string; model: string }>(
  agents: T[],
  preferredModel?: string,
): T | null;
export function buildChatAgentUnavailableMessage(
  agent:
    | {
        name: string;
        status: string;
      }
    | null
    | undefined,
): string;
