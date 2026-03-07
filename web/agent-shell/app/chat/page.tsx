'use client';

import { useState, Fragment, useEffect, useRef } from 'react';
import { useChat } from '@ai-sdk/react';
import {
  DefaultChatTransport,
  isToolUIPart,
  type UIMessage,
} from 'ai';
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
  type PromptInputMessage,
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
import { type ApprovalItem } from '@/lib/daemon-rpc';
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

function nextMessageId() {
  return globalThis.crypto?.randomUUID?.() ?? `msg-${Date.now()}-${Math.random()}`;
}

function agentLiteSessionId(agentId: string): string {
  return `web-${agentId}`;
}

function buildChatRequestBody(agent: ChatAgentOption) {
  return {
    model: agent.model,
    agentId: agent.agent_id,
    sessionId: `${agentLiteSessionId(agent.agent_id)}-${nextMessageId()}`,
    runtime: 'agent-lite',
  };
}

function extractMessageText(message: UIMessage): string {
  return message.parts
    .filter((part) => part.type === 'text')
    .map((part) => part.text)
    .join('')
    .trim();
}

export default function ChatPage() {
  const [input, setInput] = useState('');
  const [chatNotice, setChatNotice] = useState<string | null>(null);
  const [agentId, setAgentId] = useState('');
  const [availableAgents, setAvailableAgents] = useState<ChatAgentOption[]>([]);
  const [approvalQueue, setApprovalQueue] = useState<ApprovalItem[]>([]);
  const [approvalBusyId, setApprovalBusyId] = useState<string | null>(null);
  const previousAgentIdRef = useRef<string | null>(null);

  const {
    messages,
    status,
    error,
    sendMessage,
    regenerate,
    stop,
    clearError,
    setMessages,
  } = useChat({
    transport: new DefaultChatTransport({ api: '/api/chat' }),
    experimental_throttle: 40,
  });

  const loadAgents = async (): Promise<ChatAgentOption | null> => {
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

    const currentAgent = agentId
      ? agents.find((candidate) => candidate.agent_id === agentId)
      : undefined;

    const resolved =
      currentAgent && isAgentRunnable(currentAgent) ? currentAgent : preferred;

    setAgentId(resolved.agent_id);
    return resolved;
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
    let cancelled = false;

    loadAgents()
      .then((selected) => {
        if (!cancelled) {
          void refreshApprovalQueue(selected?.agent_id ?? '');
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

  useEffect(() => {
    const previousAgentId = previousAgentIdRef.current;
    previousAgentIdRef.current = agentId || null;

    if (!previousAgentId || !agentId || previousAgentId === agentId) {
      return;
    }

    if (messages.length === 0) {
      return;
    }

    clearError();
    setMessages([]);
    setInput('');
    setChatNotice('Agent changed. Started a fresh chat session for the new agent.');
  }, [agentId, clearError, messages.length, setMessages]);

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

      setChatNotice(`Approval resolved: ${decision} (${approvalId})`);
      await refreshApprovalQueue(agentId);
    } catch (approvalError) {
      const message =
        approvalError instanceof Error
          ? approvalError.message
          : 'approval resolution failed';
      setChatNotice(`Approval failed: ${message}`);
    } finally {
      setApprovalBusyId(null);
    }
  };

  const submitPrompt = async ({ text }: PromptInputMessage): Promise<void> => {
    const trimmed = text.trim();
    if (!trimmed) {
      return;
    }

    let selectedAgent: ChatAgentOption | null | undefined = availableAgents.find(
      (candidate) => candidate.agent_id === agentId,
    );

    if (!selectedAgent || !isAgentRunnable(selectedAgent)) {
      selectedAgent = await loadAgents().catch(() => null);
    }

    if (!selectedAgent || !isAgentRunnable(selectedAgent)) {
      setChatNotice(buildChatAgentUnavailableMessage(selectedAgent));
      return;
    }

    setChatNotice(null);
    if (error) {
      clearError();
    }

    setInput('');
    await sendMessage(
      { text: trimmed },
      {
        body: buildChatRequestBody(selectedAgent),
      },
    );

    await refreshApprovalQueue(selectedAgent.agent_id);
  };

  const selectedAgent = availableAgents.find((candidate) => candidate.agent_id === agentId);
  const selectedAgentRunnable = selectedAgent ? isAgentRunnable(selectedAgent) : false;

  const lastAssistantMessage = [...messages]
    .reverse()
    .find((message) => message.role === 'assistant');
  const lastAssistantText = lastAssistantMessage
    ? extractMessageText(lastAssistantMessage)
    : '';

  const handleRegenerate = async () => {
    if (!selectedAgent || !isAgentRunnable(selectedAgent)) {
      setChatNotice(buildChatAgentUnavailableMessage(selectedAgent));
      return;
    }

    setChatNotice(null);
    if (error) {
      clearError();
    }

    await regenerate({ body: buildChatRequestBody(selectedAgent) });
    await refreshApprovalQueue(selectedAgent.agent_id);
  };

  return (
    <div className="flex h-[calc(100vh-120px)] flex-col rounded-xl border border-border bg-card shadow-lg">
      <div className="flex flex-1 flex-col overflow-hidden p-4">
        <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
          <div>
            <h1 className="text-lg font-semibold">Agent Chat</h1>
            <p className="text-sm text-muted-foreground">
              选择一个 agent 后发起实时对话，回复会逐步流式显示
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

        {chatNotice && (
          <div className="mb-3 rounded-lg border border-sky-500/40 bg-sky-500/10 p-3 text-sm text-sky-100">
            {chatNotice}
          </div>
        )}

        {error && (
          <div className="mb-3 rounded-lg border border-destructive/50 bg-destructive/10 p-3 text-sm text-destructive">
            Something went wrong while streaming this response. You can retry.
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

        <Conversation>
          <ConversationContent>
            {messages.length === 0 ? (
              <ConversationEmptyState
                icon={<MessageSquare className="size-12" />}
                title="Agent Chat"
                description="与 agentd 管理的 AI agent 对话，所有工具调用经 daemon 策略管控"
              />
            ) : (
              messages.map((message, messageIndex) => {
                const textParts = message.parts.filter(
                  (part) =>
                    part.type === 'text' ||
                    part.type === 'reasoning' ||
                    part.type === 'source-url' ||
                    part.type === 'source-document',
                );
                const toolParts = message.parts.filter((part) => isToolUIPart(part));

                return (
                  <Fragment key={message.id}>
                    {textParts.length > 0 && (
                      <Message from={message.role}>
                        <MessageContent>
                          {textParts.map((part, partIndex) => {
                            if (part.type === 'text') {
                              return (
                                <MessageResponse key={`${message.id}-text-${partIndex}`}>
                                  {part.text}
                                </MessageResponse>
                              );
                            }

                            if (part.type === 'reasoning') {
                              return (
                                <pre
                                  key={`${message.id}-reasoning-${partIndex}`}
                                  className="overflow-x-auto rounded-md border border-border/50 bg-muted/30 p-3 text-xs text-muted-foreground"
                                >
                                  {part.text}
                                </pre>
                              );
                            }

                            if (part.type === 'source-url') {
                              return (
                                <a
                                  key={`${message.id}-source-url-${partIndex}`}
                                  href={part.url}
                                  target="_blank"
                                  rel="noreferrer"
                                  className="text-xs text-sky-400 underline-offset-2 hover:underline"
                                >
                                  [{part.title ?? part.url}]
                                </a>
                              );
                            }

                            if (part.type === 'source-document') {
                              return (
                                <div
                                  key={`${message.id}-source-doc-${partIndex}`}
                                  className="text-xs text-muted-foreground"
                                >
                                  [Document: {part.title}]
                                </div>
                              );
                            }

                            return null;
                          })}
                        </MessageContent>
                      </Message>
                    )}

                    {toolParts.map((part, toolIndex) => (
                      <Tool
                        key={`${message.id}-tool-${toolIndex}`}
                        defaultOpen={
                          part.state === 'output-available' || part.state === 'output-error'
                        }
                      >
                        {part.type === 'dynamic-tool' ? (
                          <ToolHeader
                            type="dynamic-tool"
                            state={part.state}
                            toolName={part.toolName}
                          />
                        ) : (
                          <ToolHeader type={part.type} state={part.state} />
                        )}
                        <ToolContent>
                          <ToolInput input={part.input} />
                          <ToolOutput output={part.output} errorText={part.errorText} />
                        </ToolContent>
                      </Tool>
                    ))}

                    {message.role === 'assistant' &&
                      messageIndex === messages.length - 1 &&
                      (status === 'ready' || status === 'error') &&
                      lastAssistantText && (
                        <MessageActions>
                          <MessageAction onClick={() => void handleRegenerate()} label="重新生成">
                            <RefreshCcw className="size-3" />
                          </MessageAction>
                          <MessageAction
                            onClick={() => navigator.clipboard.writeText(lastAssistantText)}
                            label="复制"
                          >
                            <Copy className="size-3" />
                          </MessageAction>
                        </MessageActions>
                      )}
                  </Fragment>
                );
              })
            )}
          </ConversationContent>
          <ConversationScrollButton />
        </Conversation>

        <PromptInput onSubmit={(message) => void submitPrompt(message)} className="mt-3">
          <PromptInputBody>
            <PromptInputTextarea
              value={input}
              onChange={(event) => setInput(event.currentTarget.value)}
              placeholder="Ask the agent…"
              disabled={availableAgents.length > 0 && !selectedAgentRunnable}
            />
          </PromptInputBody>
          <PromptInputFooter>
            <PromptInputTools />
            <PromptInputSubmit
              className="send-button"
              status={status}
              onStop={status === 'submitted' || status === 'streaming' ? () => void stop() : undefined}
              disabled={
                (!input.trim() && status !== 'submitted' && status !== 'streaming') ||
                (availableAgents.length > 0 && !selectedAgentRunnable)
              }
            />
          </PromptInputFooter>
        </PromptInput>
      </div>
    </div>
  );
}
