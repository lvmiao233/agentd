export type ChatLatestOutput = {
  kind: 'artifact' | 'tool';
  title: string;
  description: string;
  targetId: string;
};

export declare function buildChatLatestOutput(messages: unknown[]): ChatLatestOutput | null;
