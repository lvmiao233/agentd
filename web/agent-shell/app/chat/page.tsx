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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Button } from '@/components/ui/button';
import {
  Tool,
  ToolHeader,
  ToolContent,
  ToolInput,
  ToolOutput,
} from '@/components/ai-elements/tool';
import { MessageSquare, RefreshCcw, Copy } from 'lucide-react';
import {
  type ApprovalItem,
} from '@/lib/daemon-rpc';
import {
  WebAgentChatModel,
  type WebAgentChatMessage,
} from '@/lib/web-agent-chat';
import { consumeChatUiStream } from '@/lib/chat-ui-stream';
import {
  buildChatAgentUnavailableMessage,
  choosePreferredAgent,
  isAgentRunnable,
} from '@/lib/chat-agent-readiness.js';

type ChatAgentOption = {
  agent_id: string;
  name: string;
  model: string;
  status: string;
  runnable?: boolean;
  runnable_reason?: string;
};

type ChatStatus = 'ready' | 'streaming' | 'error';

type ChatMessage = WebAgentChatMessage;

function nextMessageId() {
  return globalThis.crypto?.randomUUID?.() ?? `msg-${Date.now()}-${Math.random()}`;
}

function agentLiteSessionId(agentId: string): string {
  return `web-${agentId}`;
}

export default function ChatPage() {
  const [input, setInput] = useState('');
  const [showReconnectBanner, setShowReconnectBanner] = useState(false);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [status, setStatus] = useState<ChatStatus>('ready');
  const [agentId, setAgentId] = useState('');
  const [availableAgents, setAvailableAgents] = useState<ChatAgentOption[]>([]);
  const [approvalQueue, setApprovalQueue] = useState<ApprovalItem[]>([]);
  const [approvalBusyId, setApprovalBusyId] = useState<string | null>(null);
  const activeRequestAbortRef = useRef<AbortController | null>(null);
  const lastSubmittedInputRef = useRef('');
  const chatModelRef = useRef(new WebAgentChatModel());

  const syncChatModel = () => {
    const snapshot = chatModelRef.current.snapshot();
    setMessages(snapshot.messages);
    setShowReconnectBanner(snapshot.showReconnectBanner);
  };

  const loadAgents = async () => {
    const response = await fetch('/api/agents');
    if (!response.ok) {
      throw new Error('load agents failed');
    }
      const payload = (await response.json()) as {
      agents?: ChatAgentOption[];
    };
    const agents = payload.agents ?? [];
    setAvailableAgents(agents);
    if (agents.length === 0) {
      setAgentId('');
      return null;
    }
    const preferred = choosePreferredAgent(agents);
    if (!preferred) {
      setAgentId('');
      return null;
    }
    setAgentId((current) => {
      const currentAgent = current
        ? agents.find((agent) => agent.agent_id === current)
        : undefined;
      if (currentAgent && isAgentRunnable(currentAgent)) {
        return current;
      }
      return preferred.agent_id;
    });
    return preferred.agent_id;
  };

  const refreshApprovalQueue = async (targetAgentId: string) => {
    if (!targetAgentId) {
      setApprovalQueue([]);
      return;
    }
    try {
      const response = await fetch(
        `/api/approvals?agent_id=${encodeURIComponent(targetAgentId)}`,
      );
      if (!response.ok) {
        throw new Error('load approvals failed');
      }
      const payload = (await response.json()) as { approvals?: ApprovalItem[] };
      setApprovalQueue(payload.approvals ?? []);
    } catch {
      setApprovalQueue([]);
    }
  };

  useEffect(() => {
    chatModelRef.current.handleReconnect();
    syncChatModel();
    return () => {
      activeRequestAbortRef.current?.abort();
      activeRequestAbortRef.current = null;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    loadAgents()
      .then((agents) => {
        if (cancelled) {
          return;
        }
      })
      .catch(() => {
        if (!cancelled) {
          setAgentId('');
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    void refreshApprovalQueue(agentId);
    if (!agentId) {
      return;
    }
    const timer = setInterval(() => {
      void refreshApprovalQueue(agentId);
    }, 3000);
    return () => clearInterval(timer);
  }, [agentId]);

  const handleApprovalDecision = async (
    approvalId: string,
    decision: 'approve' | 'deny',
  ) => {
    if (!agentId) {
      return;
    }
    setApprovalBusyId(approvalId);
    try {
      const response = await fetch('/api/approvals', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          agent_id: agentId,
          approval_id: approvalId,
          decision,
        }),
      });
      if (!response.ok) {
        const payload = (await response.json()) as { error?: string };
        throw new Error(payload.error ?? 'approval resolution failed');
      }
      chatModelRef.current.appendAssistantMessage(
        `Approval resolved: ${decision} (${approvalId})`,
      );
      syncChatModel();
      await refreshApprovalQueue(agentId);
    } catch (error) {
      const message =
        error instanceof Error ? error.message : 'approval resolution failed';
      chatModelRef.current.appendAssistantMessage(`Approval failed: ${message}`);
      syncChatModel();
    } finally {
      setApprovalBusyId(null);
    }
  };

  const submitPrompt = async (raw: string): Promise<boolean> => {
    const text = raw.trim();
    if (!text) return false;

    const resolvedAgentId = agentId || (await loadAgents().catch(() => null));
    const selectedAgent = availableAgents.find(
      (candidate) => candidate.agent_id === resolvedAgentId,
    );

    if (!selectedAgent || !isAgentRunnable(selectedAgent)) {
      chatModelRef.current.appendAssistantMessage(
        buildChatAgentUnavailableMessage(selectedAgent),
      );
      syncChatModel();
      setStatus('error');
      return false;
    }

    chatModelRef.current.appendUserMessage(text, nextMessageId());
    syncChatModel();
    lastSubmittedInputRef.current = text;
    setStatus('streaming');

    activeRequestAbortRef.current?.abort();
    const abortController = new AbortController();
    activeRequestAbortRef.current = abortController;

    try {
      const response = await fetch('/api/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          messages: [
            {
              id: nextMessageId(),
              role: 'user',
              parts: [{ type: 'text', text }],
            },
          ],
          model: selectedAgent.model,
          agentId: selectedAgent.agent_id,
          sessionId: agentLiteSessionId(selectedAgent.agent_id),
          runtime: 'agent-lite',
        }),
        signal: abortController.signal,
      });

      if (!response.ok) {
        throw new Error(`chat request failed (${response.status})`);
      }

      await consumeChatUiStream(response, {
        onAssistantDelta: (delta) => {
          chatModelRef.current.appendAssistantToken(delta);
          syncChatModel();
        },
        onToolInput: ({ toolCallId, toolName, input }) => {
          chatModelRef.current.appendToolCall(toolName, input, toolCallId);
          syncChatModel();
        },
        onToolOutput: ({ toolCallId, output, errorText }) => {
          chatModelRef.current.appendToolResult(toolCallId ?? nextMessageId(), output, errorText);
          syncChatModel();
        },
      });

      setStatus('ready');
      await refreshApprovalQueue(selectedAgent.agent_id);
      return true;
    } catch (error) {
      const message = error instanceof Error ? error.message : 'chat transport failed';
      chatModelRef.current.appendAssistantMessage(`RunAgent failed: ${message}`);
      syncChatModel();
      setStatus('error');
      await refreshApprovalQueue(selectedAgent.agent_id);
      return false;
    } finally {
      if (activeRequestAbortRef.current === abortController) {
        activeRequestAbortRef.current = null;
      }
    }
  };

  const handleSubmit = () => {
    void submitPrompt(input).then((submitted) => {
      if (submitted) {
        setInput('');
      }
    });
  };

  const handleRegenerate = () => {
    if (!lastSubmittedInputRef.current) return;
    void submitPrompt(lastSubmittedInputRef.current);
  };

  const selectedAgent = availableAgents.find((candidate) => candidate.agent_id === agentId);
  const selectedAgentRunnable = selectedAgent ? isAgentRunnable(selectedAgent) : false;

  return (
    <div className="flex h-[calc(100vh-120px)] flex-col rounded-xl border border-border bg-card shadow-lg">
      <div className="flex flex-1 flex-col overflow-hidden p-4">
        <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
          <div>
            <h1 className="text-lg font-semibold">Agent Chat</h1>
            <p className="text-sm text-muted-foreground">
              选择一个 agent 后再发起实时对话
            </p>
          </div>
          <div className="min-w-72">
            <Select value={agentId} onValueChange={setAgentId}>
              <SelectTrigger aria-label="Active agent selector">
                <SelectValue placeholder="Select an agent" />
              </SelectTrigger>
              <SelectContent>
                {availableAgents.map((agent) => (
                  <SelectItem
                    key={agent.agent_id}
                    value={agent.agent_id}
                    disabled={!isAgentRunnable(agent)}
                  >
                    {agent.name} · {agent.model} · {agent.status}
                    {isAgentRunnable(agent) ? '' : ' · unrunnable'}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>
        {selectedAgent && !selectedAgentRunnable && (
          <div className="mb-3 rounded-lg border border-amber-500/40 bg-amber-500/10 p-3 text-sm text-amber-100">
            {buildChatAgentUnavailableMessage(selectedAgent)}
          </div>
        )}
        {approvalQueue.length > 0 && (
          <div className="mb-3 rounded-lg border border-amber-500/40 bg-amber-500/10 p-3 text-sm text-amber-100">
            <div className="mb-2 font-medium">Pending approvals</div>
            <div className="space-y-2">
              {approvalQueue.map((approval) => (
                <div
                  key={approval.id}
                  className="rounded-md border border-amber-500/30 bg-background/30 p-3"
                >
                  <div className="font-medium text-foreground">{approval.tool}</div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    reason: {approval.reason}
                  </div>
                  <div className="text-xs text-muted-foreground">
                    requested: {approval.requested_at}
                  </div>
                  <div className="mt-2 flex gap-2">
                    <Button
                      size="sm"
                      onClick={() => void handleApprovalDecision(approval.id, 'approve')}
                      disabled={approvalBusyId === approval.id}
                    >
                      Approve
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => void handleApprovalDecision(approval.id, 'deny')}
                      disabled={approvalBusyId === approval.id}
                    >
                      Deny
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
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
                      <ToolHeader
                        type={`tool-${message.toolName}`}
                        state={message.errorText ? 'output-error' : message.output !== undefined ? 'output-available' : 'input-available'}
                      />
                      <ToolContent>
                        <ToolInput input={message.input} />
                        <ToolOutput output={message.output} errorText={message.errorText} />
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
              disabled={availableAgents.length > 0 && !selectedAgentRunnable}
            />
          </PromptInputBody>
          <PromptInputFooter>
            <PromptInputTools />
            <PromptInputSubmit
              className="send-button"
              status={status}
              disabled={(!input.trim() && status !== 'streaming') || (availableAgents.length > 0 && !selectedAgentRunnable)}
            />
          </PromptInputFooter>
        </PromptInput>
      </div>
    </div>
  );
}
