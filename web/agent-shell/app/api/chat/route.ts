import { handleChatPost } from '@/lib/chat-route-handler';

export const maxDuration = 60;

export async function POST(req: Request) {
  return handleChatPost(req);
}
