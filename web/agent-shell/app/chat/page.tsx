'use client';

import { useState, Fragment, useEffect, useRef } from 'react';
import { useChat } from '@ai-sdk/react';
import {
  Conversation,
  ConversationContent,
  ConversationEmptyState,
  ConversationScrollButton,
} from '@/components/ai-elements/conversation';
import {
  Message,
  MessageContent,
  MessageResponse,
  MessageActions,
  MessageAction,
} from '@/components/ai-elements/message';
import {
  PromptInput,
  PromptInputBody,
  PromptInputTextarea,
  PromptInputFooter,
  PromptInputTools,
  PromptInputSubmit,
} from '@/components/ai-elements/prompt-input';
import {
  Tool,
  ToolHeader,
  ToolContent,
  ToolInput,
  ToolOutput,
} from '@/components/ai-elements/tool';
import { MessageSquare, RefreshCcw, Copy } from 'lucide-react';
import type { ToolUIPart } from 'ai';
import { createDaemonWs } from '@/lib/daemon-rpc';

export default function ChatPage() {
  const [input, setInput] = useState('');
  const [showReconnectBanner, setShowReconnectBanner] = useState(false);
  const { messages, sendMessage, status, regenerate } = useChat();
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let closed = false;
    let socket: WebSocket | null = null;

    const clearReconnectTimer = () => {
      if (reconnectTimerRef.current) {
        clearTimeout(reconnectTimerRef.current);
        reconnectTimerRef.current = null;
      }
    };

    const connect = () => {
      clearReconnectTimer();
      const ws = createDaemonWs();
      socket = ws;

      ws.onopen = () => {
        if (!closed) {
          setShowReconnectBanner(false);
        }
      };

      ws.onerror = () => {
        if (!closed) {
          setShowReconnectBanner(true);
        }
      };

      ws.onclose = () => {
        if (closed) return;
        setShowReconnectBanner(true);
        reconnectTimerRef.current = setTimeout(connect, 1000);
      };
    };

    connect();

    return () => {
      closed = true;
      clearReconnectTimer();
      socket?.close();
    };
  }, []);

  const handleSubmit = () => {
    const text = input.trim();
    if (!text) return;
    sendMessage({ text });
    setInput('');
  };

  return (
    <div className="flex h-[calc(100vh-120px)] flex-col rounded-xl border border-border bg-card shadow-lg">
      <div className="flex flex-1 flex-col overflow-hidden p-4">
        {showReconnectBanner && (
          <div className="reconnect-banner mb-3 rounded-lg border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-sm text-amber-200">
            Daemon WebSocket disconnected. Reconnecting…
          </div>
        )}
        <Conversation>
          <ConversationContent>
            {messages.length === 0 ? (
              <ConversationEmptyState
                icon={<MessageSquare className="size-12" />}
                title="Agent Chat"
                description="与 agentd 管理的 AI agent 对话，所有工具调用经 daemon 策略管控"
              />
            ) : (
              messages.map((message, messageIndex) => (
                <Fragment key={message.id}>
                  {message.parts.map((part, i) => {
                    switch (part.type) {
                      case 'text':
                        return (
                          <Fragment key={`${message.id}-${i}`}>
                            <Message from={message.role}>
                              <MessageContent>
                                <div
                                  className={
                                    message.role === 'assistant' &&
                                    messageIndex === messages.length - 1 &&
                                    status === 'streaming'
                                      ? 'stream-token'
                                      : undefined
                                  }
                                >
                                  <MessageResponse>{part.text}</MessageResponse>
                                </div>
                              </MessageContent>
                            </Message>
                            {message.role === 'assistant' &&
                              messageIndex === messages.length - 1 &&
                              status === 'ready' && (
                                <MessageActions>
                                  <MessageAction
                                    onClick={() => regenerate()}
                                    label="重新生成"
                                  >
                                    <RefreshCcw className="size-3" />
                                  </MessageAction>
                                  <MessageAction
                                    onClick={() =>
                                      navigator.clipboard.writeText(part.text)
                                    }
                                    label="复制"
                                  >
                                    <Copy className="size-3" />
                                  </MessageAction>
                                </MessageActions>
                              )}
                          </Fragment>
                        );
                      default: {
                        const toolPart = part as ToolUIPart;
                        if (
                          toolPart.type?.startsWith?.('tool-') ||
                          toolPart.state
                        ) {
                          return (
                            <Tool
                              key={`${message.id}-tool-${i}`}
                              defaultOpen={
                                toolPart.state === 'output-available' ||
                                toolPart.state === 'output-error'
                              }
                            >
                              <ToolHeader
                                type={toolPart.type}
                                state={toolPart.state}
                              />
                              <ToolContent>
                                <ToolInput input={toolPart.input} />
                                <ToolOutput
                                  output={
                                    toolPart.output ? (
                                      <MessageResponse>
                                        {typeof toolPart.output === 'string'
                                          ? toolPart.output
                                          : JSON.stringify(
                                              toolPart.output,
                                              null,
                                              2,
                                            )}
                                      </MessageResponse>
                                    ) : undefined
                                  }
                                  errorText={toolPart.errorText}
                                />
                              </ToolContent>
                            </Tool>
                          );
                        }
                        return null;
                      }
                    }
                  })}
                </Fragment>
              ))
            )}
          </ConversationContent>
          <ConversationScrollButton />
        </Conversation>

        <PromptInput onSubmit={handleSubmit} className="mt-3">
          <PromptInputBody>
            <PromptInputTextarea
              value={input}
              onChange={(e) => setInput(e.currentTarget.value)}
              placeholder="Ask the agent…"
            />
          </PromptInputBody>
          <PromptInputFooter>
            <PromptInputTools />
            <PromptInputSubmit
              status={status === 'streaming' ? 'streaming' : 'ready'}
              disabled={!input.trim() && status !== 'streaming'}
            />
          </PromptInputFooter>
        </PromptInput>
      </div>
    </div>
  );
}
