'use client';

import { useState, Fragment, useEffect, useRef, type ReactNode } from 'react';
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
  Artifact,
} from '@/components/ai-elements/artifact';
import {
  Message,
  MessageBranch,
  MessageBranchContent,
  MessageBranchNext,
  MessageBranchPage,
  MessageBranchPrevious,
  MessageBranchSelector,
  MessageContent,
  MessageResponse,
  MessageActions,
  MessageAction,
  MessageToolbar,
} from '@/components/ai-elements/message';
import { Suggestion, Suggestions } from '@/components/ai-elements/suggestion';
import {
  Reasoning,
  ReasoningContent,
  ReasoningTrigger,
} from '@/components/ai-elements/reasoning';
import {
  Source,
  Sources,
  SourcesContent,
  SourcesTrigger,
} from '@/components/ai-elements/sources';
import {
  Confirmation,
  ConfirmationAccepted,
  ConfirmationAction,
  ConfirmationActions,
  ConfirmationRejected,
  ConfirmationRequest,
} from '@/components/ai-elements/confirmation';
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
import {
  Tool,
  getToolDisplayName,
  ToolHeader,
  ToolContent,
  ToolInput,
  ToolOutput,
} from '@/components/ai-elements/tool';
import { MessageSquare, RefreshCcw, Copy, CheckIcon, ShieldAlert, XIcon } from 'lucide-react';
import { type ApprovalItem } from '@/lib/daemon-rpc';
import {
  buildChatAgentUnavailableMessage,
  choosePreferredAgent,
  isAgentRunnable,
} from '@/lib/chat-agent-readiness.js';
import {
  approvalDecisionLabel,
  buildApprovalFeed,
  type ResolvedApprovalItem,
} from '@/lib/chat-approval-feed.js';
import { buildFollowUpSuggestions } from '@/lib/chat-follow-up-suggestions.js';
import { extractPreviewArtifacts } from '@/lib/chat-artifacts.js';
import {
  appendMessageBranch,
  getAssistantBranchKey,
  mergeMessageBranches,
} from '@/lib/chat-message-branches.js';
import { collectSourceParts } from '@/lib/chat-message-parts.js';
import { assignApprovalsToTools } from '@/lib/chat-tool-approvals.js';

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

function artifactTitleForMessage(params: {
  baseTitle: string;
  branchIndex?: number;
  branchCount?: number;
}) {
  const { baseTitle, branchIndex, branchCount } = params;

  if (typeof branchIndex === 'number' && typeof branchCount === 'number' && branchCount > 1) {
    return `${baseTitle} · version ${branchIndex + 1}`;
  }

  return baseTitle;
}

export default function ChatPage() {
  const [input, setInput] = useState('');
  const [chatNotice, setChatNotice] = useState<string | null>(null);
  const [agentId, setAgentId] = useState('');
  const [availableAgents, setAvailableAgents] = useState<ChatAgentOption[]>([]);
  const [approvalQueue, setApprovalQueue] = useState<ApprovalItem[]>([]);
  const [approvalBusyId, setApprovalBusyId] = useState<string | null>(null);
  const [resolvedApprovals, setResolvedApprovals] = useState<ResolvedApprovalItem[]>([]);
  const [messageBranchHistory, setMessageBranchHistory] = useState<Record<string, UIMessage[]>>({});
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
    setResolvedApprovals([]);
    setMessageBranchHistory({});
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
      const resolvedApproval = approvalQueue.find((approval) => approval.id === approvalId);
      if (resolvedApproval) {
        setResolvedApprovals((current) => [
          {
            ...resolvedApproval,
            decision,
            resolvedAt: new Date().toISOString(),
          },
          ...current.filter((approval) => approval.id !== approvalId),
        ].slice(0, 6));
      }
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
  const toolApprovalNodes = messages.flatMap((message) =>
    message.parts.flatMap((part, partIndex) => {
      if (!isToolUIPart(part)) {
        return [];
      }

      return [
        {
          key: `${message.id}-tool-${partIndex}`,
          toolCallId: part.toolCallId,
          toolName: getToolDisplayName({
            type: part.type,
            toolName: part.type === 'dynamic-tool' ? part.toolName : undefined,
          }),
        },
      ];
    }),
  );
  const { assignments: toolApprovalAssignments, unmatchedApprovals } = assignApprovalsToTools({
    toolNodes: toolApprovalNodes,
    approvals: approvalQueue,
  });
  const approvalFeed = buildApprovalFeed({ pending: unmatchedApprovals, resolved: resolvedApprovals });

  const lastAssistantMessage = [...messages]
    .reverse()
    .find((message) => message.role === 'assistant');
  const lastAssistantText = lastAssistantMessage
    ? extractMessageText(lastAssistantMessage)
    : '';
  const lastAssistantHasToolParts = lastAssistantMessage
    ? lastAssistantMessage.parts.some((part) => isToolUIPart(part))
    : false;
  const followUpSuggestions = buildFollowUpSuggestions({
    status,
    lastAssistantText,
    hasToolParts: lastAssistantHasToolParts,
    hasPendingApprovals: approvalQueue.length > 0,
  });

  const handleRegenerate = async () => {
    if (!selectedAgent || !isAgentRunnable(selectedAgent)) {
      setChatNotice(buildChatAgentUnavailableMessage(selectedAgent));
      return;
    }

    if (lastAssistantMessage) {
      const branchKey = getAssistantBranchKey(messages, lastAssistantMessage.id);
      if (branchKey) {
        setMessageBranchHistory((current) =>
          appendMessageBranch(current, branchKey, lastAssistantMessage),
        );
      }
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

        <Conversation>
          <ConversationContent>
            {messages.length === 0 && approvalFeed.length === 0 ? (
              <ConversationEmptyState
                icon={<MessageSquare className="size-12" />}
                title="Agent Chat"
                description="与 agentd 管理的 AI agent 对话，所有工具调用经 daemon 策略管控"
              />
            ) : (
              messages.map((message, messageIndex) => {
                const renderedSegments: ReactNode[] = [];
                const sourceParts = collectSourceParts(message.parts);
                const previewArtifacts = extractPreviewArtifacts(extractMessageText(message));
                const assistantBranchKey =
                  message.role === 'assistant'
                    ? getAssistantBranchKey(messages, message.id)
                    : null;
                const branchMessages =
                  message.role === 'assistant' && assistantBranchKey
                    ? mergeMessageBranches(messageBranchHistory[assistantBranchKey] ?? [], message)
                    : [message];
                const contentParts: Array<{
                  part: UIMessage['parts'][number];
                  partIndex: number;
                }> = [];

                const renderMessageVariant = (
                  targetMessage: UIMessage,
                  variantKey: string,
                  allowLinkedApprovals: boolean,
                  branchIndex?: number,
                  branchCount?: number,
                ) => {
                  const variantSegments: ReactNode[] = [];
                  const variantSourceParts = collectSourceParts(targetMessage.parts);
                  const previewArtifacts = extractPreviewArtifacts(extractMessageText(targetMessage));
                  const variantContentParts: Array<{
                    part: UIMessage['parts'][number];
                    partIndex: number;
                  }> = [];

                  const flushVariantContentParts = () => {
                    if (variantContentParts.length === 0) {
                      return;
                    }

                    const contentKey = `${variantKey}-content-${variantContentParts[0]?.partIndex ?? 0}`;
                    variantSegments.push(
                      <Message key={contentKey} from={targetMessage.role}>
                        <MessageContent>
                          {variantContentParts.map(({ part, partIndex }) => {
                            if (part.type === 'text') {
                              return (
                                <MessageResponse key={`${variantKey}-text-${partIndex}`}>
                                  {part.text}
                                </MessageResponse>
                              );
                            }

                            if (part.type === 'reasoning') {
                              return (
                                <Reasoning
                                  key={`${variantKey}-reasoning-${partIndex}`}
                                  className="w-full"
                                  isStreaming={
                                    status === 'streaming' &&
                                    messageIndex === messages.length - 1 &&
                                    partIndex === targetMessage.parts.length - 1
                                  }
                                >
                                  <ReasoningTrigger />
                                  <ReasoningContent>{part.text}</ReasoningContent>
                                </Reasoning>
                              );
                            }

                            if (part.type === 'source-url' || part.type === 'source-document') {
                              return null;
                            }

                            return null;
                          })}
                        </MessageContent>
                      </Message>,
                    );
                    variantContentParts.length = 0;
                  };

                  for (const [partIndex, part] of targetMessage.parts.entries()) {
                    if (isToolUIPart(part)) {
                      flushVariantContentParts();
                      const toolKey = `${variantKey}-tool-${partIndex}`;
                      const linkedApproval = allowLinkedApprovals
                        ? toolApprovalAssignments.get(`${message.id}-tool-${partIndex}`)
                        : undefined;
                      variantSegments.push(
                        <Tool
                          key={toolKey}
                          defaultOpen={
                            part.state === 'output-available' ||
                            part.state === 'output-error' ||
                            linkedApproval !== undefined
                          }
                        >
                          {part.type === 'dynamic-tool' ? (
                            <ToolHeader type="dynamic-tool" state={part.state} toolName={part.toolName} />
                          ) : (
                            <ToolHeader type={part.type} state={part.state} />
                          )}
                          <ToolContent>
                            {linkedApproval && (
                              <Confirmation state="approval-requested">
                                <ConfirmationRequest>
                                  <div className="font-medium">Approval required for {linkedApproval.tool}</div>
                                  <div className="text-muted-foreground text-sm">
                                    {linkedApproval.reason}
                                  </div>
                                  <div className="text-muted-foreground text-xs">
                                    Requested at {linkedApproval.requested_at}
                                  </div>
                                </ConfirmationRequest>
                                <ConfirmationActions>
                                  <ConfirmationAction
                                    onClick={() => void handleApprovalDecision(linkedApproval.id, 'approve')}
                                    disabled={approvalBusyId === linkedApproval.id}
                                  >
                                    Approve
                                  </ConfirmationAction>
                                  <ConfirmationAction
                                    variant="outline"
                                    onClick={() => void handleApprovalDecision(linkedApproval.id, 'deny')}
                                    disabled={approvalBusyId === linkedApproval.id}
                                  >
                                    Deny
                                  </ConfirmationAction>
                                </ConfirmationActions>
                              </Confirmation>
                            )}
                            <ToolInput input={part.input} />
                            <ToolOutput output={part.output} errorText={part.errorText} />
                          </ToolContent>
                        </Tool>,
                      );
                      continue;
                    }

                    variantContentParts.push({ part, partIndex });
                  }

                  flushVariantContentParts();

                  return (
                    <div key={variantKey} className="space-y-4">
                          {previewArtifacts.map((artifact, artifactIndex) => (
                        <Artifact
                          key={`${variantKey}-artifact-${artifactIndex}`}
                          code={artifact.code}
                          language={artifact.language}
                          title={artifactTitleForMessage({
                            baseTitle: artifact.title,
                            branchIndex,
                            branchCount,
                          })}
                        />
                      ))}

                      {targetMessage.role === 'assistant' && variantSourceParts.length > 0 && (
                        <Sources open>
                          <SourcesTrigger count={variantSourceParts.length} />
                          <SourcesContent className="space-y-1">
                            {variantSourceParts.map(({ part, index }) => {
                              if (part.type === 'source-url') {
                                return (
                                  <Source
                                    key={`${variantKey}-source-${index}`}
                                    href={part.url}
                                    title={part.title ?? part.url}
                                    kind="url"
                                  />
                                );
                              }

                              if (part.type === 'source-document') {
                                return (
                                  <Source
                                    key={`${variantKey}-source-${index}`}
                                    title={part.title ?? 'Document source'}
                                    kind="document"
                                  />
                                );
                              }

                              return null;
                            })}
                          </SourcesContent>
                        </Sources>
                      )}

                      {variantSegments}
                    </div>
                  );
                };

                const flushContentParts = () => {
                  if (contentParts.length === 0) {
                    return;
                  }

                  const contentKey = `${message.id}-content-${contentParts[0]?.partIndex ?? 0}`;
                  renderedSegments.push(
                    <Message key={contentKey} from={message.role}>
                      <MessageContent>
                        {contentParts.map(({ part, partIndex }) => {
                          if (part.type === 'text') {
                            return (
                              <MessageResponse key={`${message.id}-text-${partIndex}`}>
                                {part.text}
                              </MessageResponse>
                            );
                          }

                          if (part.type === 'reasoning') {
                            return (
                              <Reasoning
                                key={`${message.id}-reasoning-${partIndex}`}
                                className="w-full"
                                isStreaming={
                                  status === 'streaming' &&
                                  messageIndex === messages.length - 1 &&
                                  partIndex === message.parts.length - 1
                                }
                              >
                                <ReasoningTrigger />
                                <ReasoningContent>{part.text}</ReasoningContent>
                              </Reasoning>
                            );
                          }

                          if (part.type === 'source-url' || part.type === 'source-document') {
                            return null;
                          }

                          return null;
                        })}
                      </MessageContent>
                    </Message>,
                  );
                  contentParts.length = 0;
                };

                for (const [partIndex, part] of message.parts.entries()) {
                  if (isToolUIPart(part)) {
                    flushContentParts();
                    const toolKey = `${message.id}-tool-${partIndex}`;
                    const linkedApproval = toolApprovalAssignments.get(toolKey);
                    renderedSegments.push(
                      <Tool
                        key={toolKey}
                        defaultOpen={
                          part.state === 'output-available' ||
                          part.state === 'output-error' ||
                          linkedApproval !== undefined
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
                          {linkedApproval && (
                            <Confirmation state="approval-requested">
                              <ConfirmationRequest>
                                <div className="font-medium">Approval required for {linkedApproval.tool}</div>
                                <div className="text-muted-foreground text-sm">
                                  {linkedApproval.reason}
                                </div>
                                <div className="text-muted-foreground text-xs">
                                  Requested at {linkedApproval.requested_at}
                                </div>
                              </ConfirmationRequest>
                              <ConfirmationActions>
                                <ConfirmationAction
                                  onClick={() => void handleApprovalDecision(linkedApproval.id, 'approve')}
                                  disabled={approvalBusyId === linkedApproval.id}
                                >
                                  Approve
                                </ConfirmationAction>
                                <ConfirmationAction
                                  variant="outline"
                                  onClick={() => void handleApprovalDecision(linkedApproval.id, 'deny')}
                                  disabled={approvalBusyId === linkedApproval.id}
                                >
                                  Deny
                                </ConfirmationAction>
                              </ConfirmationActions>
                            </Confirmation>
                          )}
                          <ToolInput input={part.input} />
                          <ToolOutput output={part.output} errorText={part.errorText} />
                        </ToolContent>
                      </Tool>,
                    );
                    continue;
                  }

                  contentParts.push({ part, partIndex });
                }

                flushContentParts();

                return (
                  <Fragment key={message.id}>
                    {message.role === 'assistant' && branchMessages.length > 1 ? (
                      <MessageBranch defaultBranch={branchMessages.length - 1}>
                        <MessageBranchContent>
                          {branchMessages.map((branchMessage, branchIndex) =>
                            renderMessageVariant(
                              branchMessage,
                              `${message.id}-branch-${branchIndex}`,
                              branchIndex === branchMessages.length - 1,
                              branchIndex,
                              branchMessages.length,
                            ),
                          )}
                        </MessageBranchContent>
                        <MessageToolbar>
                          <MessageBranchSelector>
                            <MessageBranchPrevious />
                            <MessageBranchPage />
                            <MessageBranchNext />
                          </MessageBranchSelector>
                        </MessageToolbar>
                      </MessageBranch>
                    ) : (
                      <>
                        {message.role === 'assistant' &&
                          previewArtifacts.map((artifact, artifactIndex) => (
                            <Artifact
                              key={`${message.id}-artifact-${artifactIndex}`}
                              code={artifact.code}
                              language={artifact.language}
                              title={artifact.title}
                            />
                          ))}

                        {message.role === 'assistant' && sourceParts.length > 0 && (
                          <Sources open>
                            <SourcesTrigger count={sourceParts.length} />
                            <SourcesContent className="space-y-1">
                              {sourceParts.map(({ part, index }) => {
                                if (part.type === 'source-url') {
                                  return (
                                    <Source
                                      key={`${message.id}-source-${index}`}
                                      href={part.url}
                                      title={part.title ?? part.url}
                                      kind="url"
                                    />
                                  );
                                }

                                if (part.type === 'source-document') {
                                  return (
                                    <Source
                                      key={`${message.id}-source-${index}`}
                                      title={part.title ?? 'Document source'}
                                      kind="document"
                                    />
                                  );
                                }

                                return null;
                              })}
                            </SourcesContent>
                          </Sources>
                        )}

                        {renderedSegments}
                      </>
                    )}

                    {message.role === 'assistant' &&
                      messageIndex === messages.length - 1 &&
                      (status === 'ready' || status === 'error') &&
                      lastAssistantText && (
                        <>
                          {followUpSuggestions.length > 0 && (
                            <Suggestions>
                              {followUpSuggestions.map((suggestion) => (
                                <Suggestion
                                  key={suggestion}
                                  onClick={() => void submitPrompt({ text: suggestion, files: [] })}
                                >
                                  {suggestion}
                                </Suggestion>
                              ))}
                            </Suggestions>
                          )}
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
                        </>
                      )}
                  </Fragment>
                );
              })
            )}

            {approvalFeed.length > 0 && (
              <Message from="assistant" className="max-w-full">
                <MessageContent className="w-full max-w-full gap-3">
                  <div className="flex items-center gap-2 text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">
                    <ShieldAlert className="size-4" />
                    Approval inbox
                  </div>

                  <div className="space-y-3">
                    {approvalFeed.map((item) => {
                      if (item.kind === 'pending') {
                        const approval = item.approval;
                        return (
                          <Confirmation key={approval.id} state="approval-requested">
                            <ConfirmationRequest>
                              <div className="font-medium">{approval.tool}</div>
                              <div className="text-muted-foreground text-sm">{approval.reason}</div>
                              <div className="text-muted-foreground text-xs">
                                Requested at {approval.requested_at}
                              </div>
                            </ConfirmationRequest>
                            <ConfirmationActions>
                              <ConfirmationAction
                                onClick={() => void handleApprovalDecision(approval.id, 'approve')}
                                disabled={approvalBusyId === approval.id}
                              >
                                Approve
                              </ConfirmationAction>
                              <ConfirmationAction
                                variant="outline"
                                onClick={() => void handleApprovalDecision(approval.id, 'deny')}
                                disabled={approvalBusyId === approval.id}
                              >
                                Deny
                              </ConfirmationAction>
                            </ConfirmationActions>
                          </Confirmation>
                        );
                      }

                      const approval = item.approval;
                      const resolvedState = approval.decision === 'approve' ? 'approved' : 'rejected';

                      return (
                        <Confirmation key={approval.id} state={resolvedState}>
                          {approval.decision === 'approve' ? (
                            <ConfirmationAccepted>
                              <CheckIcon className="mt-0.5 size-4" />
                              <div>
                                <div className="font-medium">
                                  {approvalDecisionLabel(approval.decision)} {approval.tool}
                                </div>
                                <div className="text-muted-foreground text-xs">
                                  Resolved at {approval.resolvedAt}
                                </div>
                              </div>
                            </ConfirmationAccepted>
                          ) : (
                            <ConfirmationRejected>
                              <XIcon className="mt-0.5 size-4" />
                              <div>
                                <div className="font-medium">
                                  {approvalDecisionLabel(approval.decision)} {approval.tool}
                                </div>
                                <div className="text-muted-foreground text-xs">
                                  Resolved at {approval.resolvedAt}
                                </div>
                              </div>
                            </ConfirmationRejected>
                          )}
                        </Confirmation>
                      );
                    })}
                  </div>
                </MessageContent>
              </Message>
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
