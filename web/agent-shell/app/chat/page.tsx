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
  ConversationDownload,
  ConversationEmptyState,
  ConversationScrollButton,
} from '@/components/ai-elements/conversation';
import {
  Checkpoint,
  CheckpointIcon,
  CheckpointTrigger,
} from '@/components/ai-elements/checkpoint';
import {
  Artifact,
} from '@/components/ai-elements/artifact';
import {
  Attachment,
  AttachmentInfo,
  AttachmentPreview,
  AttachmentRemove,
  Attachments,
} from '@/components/ai-elements/attachments';
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
  PromptInputActionAddAttachments,
  PromptInputActionMenu,
  PromptInputActionMenuContent,
  PromptInputActionMenuTrigger,
  PromptInputButton,
  PromptInputBody,
  PromptInputHeader,
  PromptInputProvider,
  PromptInputTextarea,
  PromptInputFooter,
  PromptInputTools,
  PromptInputSubmit,
  type PromptInputMessage,
  usePromptInputController,
  usePromptInputAttachments,
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
  ToolProgress,
} from '@/components/ai-elements/tool';
import ChatCockpitPlanPanel from '@/components/chat-cockpit-plan';
import ChatCommandMenu from '@/components/chat-command-menu';
import { MessageSquare, RefreshCcw, Copy, CheckIcon, ShieldAlert, XIcon, CommandIcon } from 'lucide-react';
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
import {
  appendChatCheckpoint,
  createChatCheckpoint,
  pruneChatCheckpoints,
  type ChatCheckpoint,
} from '@/lib/chat-checkpoints.js';
import { collectMessageAttachments, getAttachmentLabel } from '@/lib/chat-attachments.js';
import { extractPreviewArtifacts } from '@/lib/chat-artifacts.js';
import {
  appendMessageBranch,
  getAssistantBranchKey,
  mergeMessageBranches,
} from '@/lib/chat-message-branches.js';
import { collectSourceParts } from '@/lib/chat-message-parts.js';
import { assignApprovalsToTools } from '@/lib/chat-tool-approvals.js';
import { buildChatCommandItems, type ChatCommandItem } from '@/lib/chat-command-menu.js';
import { buildChatCockpitPlan } from '@/lib/chat-cockpit-plan.js';
import { buildChatLiveActivity } from '@/lib/chat-live-activity.js';
import { buildChatLatestOutput } from '@/lib/chat-latest-output.js';
import { buildChatResumeActions } from '@/lib/chat-resume-actions.js';
import { buildChatSessionTimeline } from '@/lib/chat-session-timeline.js';
import { buildChatRunOverview } from '@/lib/chat-run-overview.js';
import { summarizeToolInput, summarizeToolOutput } from '@/lib/chat-tool-summary.js';

type ChatAgentOption = {
  agent_id: string;
  name: string;
  model: string;
  status: string;
  runnable?: boolean;
  runnable_reason?: string;
};

const CHAT_PROMPT_FORM_ID = 'chat-prompt-form';

function highlightConversationTarget(targetId: string) {
  const target = document.getElementById(targetId);
  if (!target) {
    return;
  }

  target.scrollIntoView({ behavior: 'smooth', block: 'center' });
  target.classList.add('ring-2', 'ring-sky-500/70', 'ring-offset-2', 'ring-offset-background');
  window.setTimeout(() => {
    target.classList.remove('ring-2', 'ring-sky-500/70', 'ring-offset-2', 'ring-offset-background');
  }, 1800);
}

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

function PromptInputAttachmentsDisplay() {
  const attachments = usePromptInputAttachments();

  if (attachments.files.length === 0) {
    return null;
  }

  return (
    <Attachments variant="inline">
      {attachments.files.map((attachment) => (
        <Attachment
          key={attachment.id}
          data={attachment}
          onRemove={() => attachments.remove(attachment.id)}
          variant="inline"
        >
          <AttachmentPreview />
          <AttachmentInfo />
          <AttachmentRemove />
        </Attachment>
      ))}
    </Attachments>
  );
}

function PromptInputAttachmentsHeader() {
  const attachments = usePromptInputAttachments();

  if (attachments.files.length === 0) {
    return null;
  }

  return (
    <PromptInputHeader>
      <PromptInputAttachmentsDisplay />
    </PromptInputHeader>
  );
}

function MessageAttachmentsDisplay({ parts }: { parts: UIMessage['parts'] }) {
  const attachments = collectMessageAttachments(parts);

  if (attachments.length === 0) {
    return null;
  }

  return (
    <Attachments className="mb-3" variant="grid">
      {attachments.map((attachment, index) => (
        <Attachment
          key={`${attachment.type}-${getAttachmentLabel(attachment)}-${index}`}
          data={attachment}
          variant="grid"
        >
          <div className="flex items-start gap-3">
            <AttachmentPreview />
            <AttachmentInfo showMediaType />
          </div>
        </Attachment>
      ))}
    </Attachments>
  );
}

function ChatPromptTools(props: {
  commandMenuOpen: boolean;
  commandMenuItems: ChatCommandItem[];
  onCommandMenuOpenChange: (open: boolean) => void;
  onRegenerate: () => Promise<void>;
  onStop: () => void;
}) {
  const { commandMenuOpen, commandMenuItems, onCommandMenuOpenChange, onRegenerate, onStop } = props;
  const controller = usePromptInputController();

  const handleCommandSelect = async (item: ChatCommandItem) => {
    if (item.disabled) {
      return;
    }

    onCommandMenuOpenChange(false);

    if (item.kind === 'prompt' && item.prompt) {
      const hasDraft =
        controller.textInput.value.trim().length > 0 || controller.attachments.files.length > 0;

      if (hasDraft) {
        const nextInput = controller.textInput.value.trim()
          ? `${controller.textInput.value.trim()}\n\n${item.prompt}`
          : item.prompt;
        controller.textInput.setInput(nextInput);
        return;
      }

      controller.textInput.setInput(item.prompt);
      requestAnimationFrame(() => {
        const form = document.getElementById(CHAT_PROMPT_FORM_ID);
        if (form instanceof HTMLFormElement) {
          form.requestSubmit();
        }
      });
      return;
    }

    if (item.action === 'regenerate') {
      await onRegenerate();
      return;
    }

    if (item.action === 'stop') {
      onStop();
    }
  };

  return (
    <PromptInputTools>
      <ChatCommandMenu
        open={commandMenuOpen}
        onOpenChange={onCommandMenuOpenChange}
        items={commandMenuItems}
        onSelect={(item) => void handleCommandSelect(item)}
      />
      <PromptInputButton
        onClick={() => onCommandMenuOpenChange(true)}
        tooltip={{ content: 'Open command palette', shortcut: '⌘K / Ctrl+K' }}
        variant="ghost"
      >
        <CommandIcon className="size-4" />
        <span className="hidden sm:inline">Commands</span>
      </PromptInputButton>
      <PromptInputActionMenu>
        <PromptInputActionMenuTrigger />
        <PromptInputActionMenuContent>
          <PromptInputActionAddAttachments />
        </PromptInputActionMenuContent>
      </PromptInputActionMenu>
    </PromptInputTools>
  );
}

export default function ChatPage() {
  const [chatNotice, setChatNotice] = useState<string | null>(null);
  const [agentId, setAgentId] = useState('');
  const [availableAgents, setAvailableAgents] = useState<ChatAgentOption[]>([]);
  const [approvalQueue, setApprovalQueue] = useState<ApprovalItem[]>([]);
  const [approvalBusyId, setApprovalBusyId] = useState<string | null>(null);
  const [resolvedApprovals, setResolvedApprovals] = useState<ResolvedApprovalItem[]>([]);
  const [messageBranchHistory, setMessageBranchHistory] = useState<Record<string, UIMessage[]>>({});
  const [commandMenuOpen, setCommandMenuOpen] = useState(false);
  const [checkpoints, setCheckpoints] = useState<ChatCheckpoint[]>([]);
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

  const lastAssistantMessage = [...messages]
    .reverse()
    .find((message) => message.role === 'assistant');
  const lastAssistantText = lastAssistantMessage
    ? extractMessageText(lastAssistantMessage)
    : '';
  const lastAssistantHasToolParts = lastAssistantMessage
    ? lastAssistantMessage.parts.some((part) => isToolUIPart(part))
    : false;

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
    setResolvedApprovals([]);
    setMessageBranchHistory({});
    setCheckpoints([]);
    setChatNotice('Agent changed. Started a fresh chat session for the new agent.');
  }, [agentId, clearError, messages.length, setMessages]);

  useEffect(() => {
    if (!(status === 'ready' || status === 'error') || !lastAssistantMessage) {
      return;
    }

    const checkpoint = createChatCheckpoint({
      messages,
      assistantMessage: lastAssistantMessage,
      resolvedApprovals,
      messageBranchHistory,
    });

    setCheckpoints((current) => appendChatCheckpoint(current, checkpoint));
  }, [lastAssistantMessage, messageBranchHistory, messages, resolvedApprovals, status]);

  const handleRestoreCheckpoint = (checkpoint: ChatCheckpoint) => {
    clearError();
    setMessages(checkpoint.messages);
    setResolvedApprovals(checkpoint.resolvedApprovals);
    setMessageBranchHistory(checkpoint.messageBranchHistory);
    setCheckpoints((current) => pruneChatCheckpoints(current, checkpoint.messageCount));
    setChatNotice(`Restored the conversation to checkpoint: ${checkpoint.label}`);
    if (agentId) {
      void refreshApprovalQueue(agentId);
    }
  };

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

  const submitPrompt = async ({ text, files }: PromptInputMessage): Promise<void> => {
    const trimmed = text.trim();
    if (!trimmed && files.length === 0) {
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

    await sendMessage(
      { text: trimmed, files },
      {
        body: buildChatRequestBody(selectedAgent),
      },
    );

    await refreshApprovalQueue(selectedAgent.agent_id);
  };

  const selectedAgent = availableAgents.find((candidate) => candidate.agent_id === agentId);
  const selectedAgentRunnable = selectedAgent ? isAgentRunnable(selectedAgent) : false;
  const commandPaletteRunnable = selectedAgent ? selectedAgentRunnable : undefined;
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
  const runOverview = buildChatRunOverview({
    messages,
    status,
    approvals: unmatchedApprovals,
    approvalCount: approvalQueue.length,
  });

  const followUpSuggestions = buildFollowUpSuggestions({
    status,
    lastAssistantText,
    hasToolParts: lastAssistantHasToolParts,
    hasPendingApprovals: approvalQueue.length > 0,
  });
  const commandMenuItems = buildChatCommandItems({
    status,
    lastAssistantText,
    hasToolParts: lastAssistantHasToolParts,
    hasPendingApprovals: approvalQueue.length > 0,
    hasConversation: messages.length > 0,
    canRegenerate: Boolean(lastAssistantMessage),
    selectedAgentRunnable: commandPaletteRunnable,
  });
  const resumeActions = buildChatResumeActions(commandMenuItems);
  const sessionTimeline = buildChatSessionTimeline({
    checkpoints,
    status,
    activeMessageId: lastAssistantMessage?.id,
  });
  const liveActivity = buildChatLiveActivity(messages);
  const latestOutput = buildChatLatestOutput(messages);
  const lastUserMessage = [...messages]
    .reverse()
    .find((message) => message.role === 'user');
  const cockpitPlan = buildChatCockpitPlan({
    status,
    runOverview,
    approvalCount: approvalQueue.length,
    checkpointCount: checkpoints.length,
    lastUserText: lastUserMessage ? extractMessageText(lastUserMessage) : '',
    lastAssistantText,
    selectedAgentRunnable: commandPaletteRunnable,
    nextActionTitle: resumeActions.find((action) => !action.disabled)?.title,
  });
  const checkpointsById = Object.fromEntries(checkpoints.map((checkpoint) => [checkpoint.id, checkpoint]));

  useEffect(() => {
    const handleCommandShortcut = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== 'k') {
        return;
      }

      event.preventDefault();
      setCommandMenuOpen((current) => !current);
    };

    window.addEventListener('keydown', handleCommandShortcut);
    return () => window.removeEventListener('keydown', handleCommandShortcut);
  }, []);

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

  const handleResumeAction = async (action: ChatCommandItem) => {
    if (action.disabled) {
      return;
    }

    if (action.kind === 'prompt' && action.prompt) {
      await submitPrompt({ text: action.prompt, files: [] });
      return;
    }

    if (action.action === 'regenerate') {
      await handleRegenerate();
      return;
    }

    if (action.action === 'stop') {
      stop();
    }
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

        <ChatCockpitPlanPanel
          plan={cockpitPlan}
          runOverview={runOverview}
          resumeActions={resumeActions}
          approvalQueue={approvalQueue}
          approvalBusyId={approvalBusyId}
          sessionTimeline={sessionTimeline}
          liveActivity={liveActivity}
          latestOutput={latestOutput}
          checkpointsById={checkpointsById}
          onActionSelect={(action) => void handleResumeAction(action)}
          onNavigateToTarget={highlightConversationTarget}
          onApprovalDecision={(approvalId, decision) => void handleApprovalDecision(approvalId, decision)}
          onRestoreCheckpoint={handleRestoreCheckpoint}
        />

        <Conversation>
          <ConversationDownload
            messages={messages.map((message) => ({ role: message.role, content: extractMessageText(message) || '[non-text message]' }))}
          />
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
                const previewArtifacts = extractPreviewArtifacts(extractMessageText(message), {
                  includeIncomplete: status === 'streaming' && messageIndex === messages.length - 1,
                });
                const assistantBranchKey =
                  message.role === 'assistant'
                    ? getAssistantBranchKey(messages, message.id)
                    : null;
                const messageCheckpoint =
                  message.role === 'assistant'
                    ? checkpoints.find((checkpoint) => checkpoint.messageId === message.id) ?? null
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
                    const previewArtifacts = extractPreviewArtifacts(extractMessageText(targetMessage), {
                      includeIncomplete: status === 'streaming' && messageIndex === messages.length - 1,
                    });
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
                      <Message key={contentKey} from={targetMessage.role} id={`chat-message-${targetMessage.id}`} className="scroll-mt-24">
                        <MessageContent>
                          {collectMessageAttachments(variantContentParts.map(({ part }) => part)).length > 0 && (
                            <MessageAttachmentsDisplay parts={variantContentParts.map(({ part }) => part)} />
                          )}
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
                        ? toolApprovalAssignments.get(`${targetMessage.id}-tool-${partIndex}`)
                        : undefined;
                      const toolDisplayName = getToolDisplayName({
                        type: part.type,
                        toolName: part.type === 'dynamic-tool' ? part.toolName : undefined,
                      });
                      const toolPreview =
                        part.state === 'output-available' || part.state === 'output-error'
                          ? summarizeToolOutput(part.output, part.errorText)
                          : summarizeToolInput(part.input);
                      variantSegments.push(
                        <Tool
                          id={`chat-tool-${targetMessage.id}-${partIndex}`}
                          className="scroll-mt-24"
                          key={toolKey}
                          defaultOpen={
                            part.state === 'input-streaming' ||
                            part.state === 'input-available' ||
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
                              preview={toolPreview}
                            />
                          ) : (
                            <ToolHeader type={part.type} state={part.state} preview={toolPreview} />
                          )}
                          <ToolContent>
                            <ToolProgress state={part.state} toolName={toolDisplayName} />
                            {linkedApproval && (
                              <Confirmation id={`chat-approval-${linkedApproval.id}`} state="approval-requested" className="scroll-mt-24">
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
                          id={`chat-artifact-${targetMessage.id}-${artifactIndex}`}
                          className="scroll-mt-24"
                          key={`${variantKey}-artifact-${artifactIndex}`}
                          code={artifact.code}
                          language={artifact.language}
                          previewCode={artifact.previewCode}
                          isStreaming={status === 'streaming' && messageIndex === messages.length - 1}
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
                    <Message key={contentKey} from={message.role} id={`chat-message-${message.id}`} className="scroll-mt-24">
                      <MessageContent>
                        {collectMessageAttachments(contentParts.map(({ part }) => part)).length > 0 && (
                          <MessageAttachmentsDisplay parts={contentParts.map(({ part }) => part)} />
                        )}
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
                      const toolDisplayName = getToolDisplayName({
                        type: part.type,
                        toolName: part.type === 'dynamic-tool' ? part.toolName : undefined,
                      });
                      const toolPreview =
                        part.state === 'output-available' || part.state === 'output-error'
                          ? summarizeToolOutput(part.output, part.errorText)
                          : summarizeToolInput(part.input);
                      renderedSegments.push(
                        <Tool
                          id={`chat-tool-${message.id}-${partIndex}`}
                          className="scroll-mt-24"
                          key={toolKey}
                          defaultOpen={
                            part.state === 'input-streaming' ||
                            part.state === 'input-available' ||
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
                            preview={toolPreview}
                          />
                        ) : (
                          <ToolHeader type={part.type} state={part.state} preview={toolPreview} />
                        )}
                        <ToolContent>
                          <ToolProgress state={part.state} toolName={toolDisplayName} />
                          {linkedApproval && (
                            <Confirmation id={`chat-approval-${linkedApproval.id}`} state="approval-requested" className="scroll-mt-24">
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
                              id={`chat-artifact-${message.id}-${artifactIndex}`}
                              className="scroll-mt-24"
                              key={`${message.id}-artifact-${artifactIndex}`}
                              code={artifact.code}
                              language={artifact.language}
                              previewCode={artifact.previewCode}
                              isStreaming={status === 'streaming' && messageIndex === messages.length - 1}
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
                          <Suggestions id={`chat-message-actions-${message.id}`}>
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

                    {messageCheckpoint && (
                      <Checkpoint className="mt-3">
                        <CheckpointIcon />
                        <CheckpointTrigger
                          tooltip="Restore the conversation to this point"
                          onClick={() => handleRestoreCheckpoint(messageCheckpoint)}
                        >
                          {messageCheckpoint.label}
                        </CheckpointTrigger>
                      </Checkpoint>
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
                          <Confirmation id={`chat-approval-${approval.id}`} key={approval.id} state="approval-requested" className="scroll-mt-24">
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
                         <Confirmation id={`chat-approval-${approval.id}`} key={approval.id} state={resolvedState} className="scroll-mt-24">
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

        <PromptInputProvider>
          <PromptInput
            id={CHAT_PROMPT_FORM_ID}
            onSubmit={(message) => void submitPrompt(message)}
            className="mt-3"
            globalDrop
            multiple
          >
            <PromptInputAttachmentsHeader />
            <PromptInputBody>
              <PromptInputTextarea placeholder="Ask the agent…" disabled={availableAgents.length > 0 && !selectedAgentRunnable} />
            </PromptInputBody>
            <PromptInputFooter>
              <ChatPromptTools
                commandMenuOpen={commandMenuOpen}
                commandMenuItems={commandMenuItems}
                onCommandMenuOpenChange={setCommandMenuOpen}
                onRegenerate={handleRegenerate}
                onStop={stop}
              />
              <PromptInputSubmit
                className="send-button"
                status={status}
                onStop={status === 'submitted' || status === 'streaming' ? () => void stop() : undefined}
                disabled={availableAgents.length > 0 && !selectedAgentRunnable}
              />
            </PromptInputFooter>
          </PromptInput>
        </PromptInputProvider>
      </div>
    </div>
  );
}
