'use client';

import { useState, Fragment, useEffect, useRef } from 'react';
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
import { createDaemonWs, sendWsRpc } from '@/lib/daemon-rpc';
import {
  WebAgentChatModel,
  type WebAgentChatMessage,
} from '@/lib/web-agent-chat';

type ChatStatus = 'ready' | 'streaming' | 'error';

type ChatMessage = WebAgentChatMessage;

function nextMessageId() {
  return globalThis.crypto?.randomUUID?.() ?? `msg-${Date.now()}-${Math.random()}`;
}

function extractErrorMessage(payload: any): string | null {
  const error = payload?.error;
  if (typeof error === 'string' && error.trim()) return error;
  if (error && typeof error === 'object' && typeof error.message === 'string' && error.message.trim()) {
    return error.message;
  }
  const message = payload?.message;
  if (typeof message === 'string' && message.trim()) return message;
  if (payload?.status === 'failed' || payload?.status === 'blocked') {
    return 'RunAgent streaming failed.';
  }
  return null;
}

function extractTextDelta(payload: any): string {
  const normalized = payload?.result && typeof payload.result === 'object' ? payload.result : payload;
  const llmOutput = normalized?.llm?.output;
  if (typeof llmOutput === 'string' && llmOutput.length > 0) return llmOutput;
  for (const field of ['delta', 'token', 'text', 'content', 'output']) {
    const value = normalized?.[field];
    if (typeof value === 'string' && value.length > 0) return value;
  }
  return '';
}

function extractToolCalls(payload: any): Array<{ id: string; name: string; input: unknown }> {
  const normalized = payload?.result && typeof payload.result === 'object' ? payload.result : payload;
  const calls = normalized?.tool?.calls;
  if (!Array.isArray(calls)) return [];
  return calls
    .map((call: any, index: number) => {
      const id = typeof call?.id === 'string' && call.id.trim() ? call.id : `call-${index}`;
      const name = typeof call?.function?.name === 'string' && call.function.name.trim()
        ? call.function.name
        : 'unknown_tool';
      const argsRaw = typeof call?.function?.arguments === 'string' ? call.function.arguments : '';
      let input: unknown = {};
      if (argsRaw.trim()) {
        try {
          input = JSON.parse(argsRaw);
        } catch {
          input = argsRaw;
        }
      }
      return { id, name, input };
    });
}

export default function ChatPage() {
  const [input, setInput] = useState('');
  const [showReconnectBanner, setShowReconnectBanner] = useState(false);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [status, setStatus] = useState<ChatStatus>('ready');
  const [agentId, setAgentId] = useState<string | null>(null);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const activeRequestIdRef = useRef<number | null>(null);
  const lastSubmittedInputRef = useRef('');
  const chatModelRef = useRef(new WebAgentChatModel());

  const syncChatModel = () => {
    const snapshot = chatModelRef.current.snapshot();
    setMessages(snapshot.messages);
    setShowReconnectBanner(snapshot.showReconnectBanner);
  };

  const resolvePreferredAgentId = async () => {
    const response = await fetch('/api/agents');
    if (!response.ok) {
      throw new Error('load agents failed');
    }
    const payload = (await response.json()) as { agents?: Array<{ agent_id: string; model: string; status: string }> };
    const agents = payload.agents ?? [];
    if (agents.length === 0) {
      return null;
    }
    const preferred =
      agents.find(
        (agent) => agent.status === 'ready' && agent.model === 'gpt-5.3-codex',
      ) ?? agents.find((agent) => agent.status === 'ready') ?? agents[0];
    setAgentId(preferred.agent_id);
    return preferred.agent_id;
  };

  useEffect(() => {
    let closed = false;

    const clearReconnectTimer = () => {
      if (reconnectTimerRef.current) {
        clearTimeout(reconnectTimerRef.current);
        reconnectTimerRef.current = null;
      }
    };

    const connect = () => {
      clearReconnectTimer();
      const ws = createDaemonWs();
      wsRef.current = ws;

      const appendAssistantError = (message: string) => {
        chatModelRef.current.appendAssistantMessage(`RunAgent failed: ${message}`);
        syncChatModel();
        setStatus('error');
        activeRequestIdRef.current = null;
      };

      const applyStreamPayload = (payload: any) => {
        const errorMessage = extractErrorMessage(payload);
        if (errorMessage) {
          appendAssistantError(errorMessage);
          return;
        }

        const delta = extractTextDelta(payload);
        if (delta) {
          chatModelRef.current.appendAssistantToken(delta);
          syncChatModel();
        }

        const toolCalls = extractToolCalls(payload);
        if (toolCalls.length > 0) {
          for (const toolCall of toolCalls) {
            chatModelRef.current.appendToolCall(
              toolCall.name,
              toolCall.input,
              toolCall.id,
            );
          }
          syncChatModel();
        }

        const normalized = payload?.result && typeof payload.result === 'object' ? payload.result : payload;
        if (normalized?.status === 'completed' || normalized?.type === 'done') {
          setStatus('ready');
          activeRequestIdRef.current = null;
        }
      };

      const handleStreamFrame = (frameText: string) => {
        const frames = frameText
          .split(/\n\n+/)
          .map((frame) => frame.trim())
          .filter(Boolean);
        for (const frame of frames) {
          if (!frame.startsWith('data:')) continue;
          const payloadText = frame.slice(5).trim();
          if (!payloadText || payloadText === '[DONE]') {
            setStatus('ready');
            activeRequestIdRef.current = null;
            continue;
          }
          try {
            applyStreamPayload(JSON.parse(payloadText));
          } catch {
            appendAssistantError('invalid websocket stream payload');
          }
        }
      };

      ws.onopen = () => {
        if (!closed) {
          chatModelRef.current.handleReconnect();
          syncChatModel();
        }
      };

      ws.onerror = () => {
        if (!closed) {
          chatModelRef.current.handleDisconnect();
          syncChatModel();
        }
      };

      ws.onclose = () => {
        if (closed) return;
        chatModelRef.current.handleDisconnect();
        syncChatModel();
        wsRef.current = null;
        reconnectTimerRef.current = setTimeout(connect, 1000);
      };

      ws.onmessage = (event) => {
        if (closed || typeof event.data !== 'string') return;
        const payloadText = event.data.trim();
        if (!payloadText) return;
        if (payloadText.startsWith('data:')) {
          handleStreamFrame(payloadText);
          return;
        }
        if (!payloadText.startsWith('{')) {
          return;
        }
        try {
          const payload = JSON.parse(payloadText);
          if (payload?.id === activeRequestIdRef.current && payload?.error) {
            appendAssistantError(
              typeof payload.error?.message === 'string'
                ? payload.error.message
                : 'websocket rpc request failed',
            );
          }
        } catch {
          appendAssistantError('websocket returned invalid rpc payload');
        }
      };
    };

    connect();

    return () => {
      closed = true;
      clearReconnectTimer();
      wsRef.current?.close();
      wsRef.current = null;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    resolvePreferredAgentId()
      .then((agents) => {
        if (cancelled) {
          return;
        }
      })
      .catch(() => {
        if (!cancelled) {
          setAgentId(null);
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  const submitPrompt = async (raw: string) => {
    const text = raw.trim();
    if (!text) return;
    chatModelRef.current.appendUserMessage(text, nextMessageId());
    syncChatModel();
    lastSubmittedInputRef.current = text;
    setStatus('streaming');

    const resolvedAgentId = agentId ?? (await resolvePreferredAgentId().catch(() => null));

    const ws = wsRef.current;
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      chatModelRef.current.appendAssistantMessage(
        'RunAgent websocket transport unavailable.',
      );
      syncChatModel();
      setStatus('error');
      return;
    }

    activeRequestIdRef.current = sendWsRpc(ws, 'RunAgent', {
      input: text,
      model: 'gpt-5.3-codex',
      ...(resolvedAgentId ? { agent_id: resolvedAgentId } : {}),
      stream: true,
    });
  };

  const handleSubmit = () => {
    void submitPrompt(input);
    setInput('');
  };

  const handleRegenerate = () => {
    if (!lastSubmittedInputRef.current) return;
    void submitPrompt(lastSubmittedInputRef.current);
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
                  {message.role === 'tool' && (
                    <Tool key={message.id} defaultOpen>
                      <ToolHeader type={`tool-${message.toolName}`} state="input-available" />
                      <ToolContent>
                        <ToolInput input={message.input} />
                        <ToolOutput output={undefined} errorText={undefined} />
                      </ToolContent>
                    </Tool>
                  )}
                  {(message.role === 'user' || message.role === 'assistant') && (
                    <Fragment>
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
                             <MessageResponse>{message.text}</MessageResponse>
                           </div>
                         </MessageContent>
                       </Message>
                      {message.role === 'assistant' &&
                        messageIndex === messages.length - 1 &&
                        status !== 'streaming' && (
                          <MessageActions>
                            <MessageAction onClick={handleRegenerate} label="重新生成">
                              <RefreshCcw className="size-3" />
                            </MessageAction>
                            <MessageAction
                              onClick={() => navigator.clipboard.writeText(message.text)}
                              label="复制"
                            >
                              <Copy className="size-3" />
                            </MessageAction>
                          </MessageActions>
                        )}
                    </Fragment>
                  )}
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
              className="send-button"
              status={status}
              disabled={!input.trim() && status !== 'streaming'}
            />
          </PromptInputFooter>
        </PromptInput>
      </div>
    </div>
  );
}
