import type { ChatCommandItem } from '@/lib/chat-command-menu.js';

export declare function buildChatResumeActions(
  commandItems: ChatCommandItem[],
  options?: { maxPromptActions?: number },
): ChatCommandItem[];
