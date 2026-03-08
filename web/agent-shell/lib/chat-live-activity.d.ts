export type ChatLiveActivity = {
  title: string;
  state: string;
  description: string;
  targetId: string;
};

export declare function buildChatLiveActivity(messages: unknown[]): ChatLiveActivity | null;
