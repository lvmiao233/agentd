'use client';

import { FormEvent, useCallback, useEffect, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { evaluateThirdPartyOnboarding } from '@/lib/dashboard-events-model';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  AlertTriangle,
  CheckCircle,
  Loader2,
  Plus,
  Server,
  XCircle,
} from 'lucide-react';

type McpServer = {
  server: string;
  trust_level: string;
  health: string;
  capabilities: string[];
};

function statusIcon(health: string) {
  switch (health) {
    case 'healthy':
      return <CheckCircle className="size-4 text-green-400" />;
    case 'degraded':
      return <Loader2 className="size-4 animate-spin text-yellow-400" />;
    case 'unreachable':
      return <XCircle className="size-4 text-red-400" />;
    default:
      return <Server className="size-4 text-muted-foreground" />;
  }
}

function statusVariant(health: string) {
  switch (health) {
    case 'healthy':
      return 'default' as const;
    case 'degraded':
      return 'outline' as const;
    case 'unreachable':
      return 'destructive' as const;
    default:
      return 'secondary' as const;
  }
}

export default function SettingsPage() {
  const [servers, setServers] = useState<McpServer[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [agentSubmitting, setAgentSubmitting] = useState(false);

  const [name, setName] = useState('');
  const [command, setCommand] = useState('npx');
  const [argsText, setArgsText] = useState('["-y", "@modelcontextprotocol/server-figma"]');
  const [trustLevel, setTrustLevel] = useState('community');
  const [agentName, setAgentName] = useState('');
  const [agentModel, setAgentModel] = useState('gpt-5.3-codex');
  const [agentProvider, setAgentProvider] = useState('agent-lite');
  const [agentPolicy, setAgentPolicy] = useState('ask');
  const [allowedToolsText, setAllowedToolsText] = useState('');
  const [deniedToolsText, setDeniedToolsText] = useState('');

  function parseArgs(raw: string) {
    const trimmed = raw.trim();
    if (!trimmed) return [] as string[];
    if (trimmed.startsWith('[')) {
      const parsed = JSON.parse(trimmed);
      if (!Array.isArray(parsed) || parsed.some((item) => typeof item !== 'string')) {
        throw new Error('参数必须是字符串数组');
      }
      return parsed;
    }
    return trimmed
      .split(/\r?\n/)
      .map((entry) => entry.trim())
      .filter(Boolean);
  }

  function parseToolPatterns(raw: string) {
    return raw
      .split(/\r?\n/)
      .map((entry) => entry.trim())
      .filter(Boolean);
  }

  const fetchServers = useCallback(async () => {
    try {
      const res = await fetch('/api/mcp');
      if (!res.ok) throw new Error('fetch failed');
      const data = await res.json();
      setServers(data.servers ?? []);
      setError(null);
    } catch {
      setError('无法从 daemon 获取 MCP 服务器列表');
    }
  }, []);

  useEffect(() => {
    fetchServers();
    const timer = setInterval(fetchServers, 8000);
    return () => clearInterval(timer);
  }, [fetchServers]);

  async function handleOnboard(e: FormEvent) {
    e.preventDefault();
    const n = name.trim();
    const c = command.trim();
    if (!n || !c) {
      setFeedback('服务器名称和命令不能为空');
      return;
    }

    setSubmitting(true);
    setFeedback(null);
    try {
      const args = parseArgs(argsText);
      const res = await fetch('/api/mcp', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          name: n,
          command: c,
          args,
          trust_level: trustLevel,
        }),
      });
      const data = await res.json();
      if (res.ok) {
        setFeedback(`${n} 注册成功`);
        setName('');
        setCommand('');
        setArgsText('');
        await fetchServers();
      } else {
        setFeedback(data.error ?? '注册失败');
      }
    } catch (err) {
      setFeedback(err instanceof Error ? err.message : '请求失败');
    } finally {
      setSubmitting(false);
    }
  }

  async function handleCreateAgent(e: FormEvent) {
    e.preventDefault();
    const trimmedName = agentName.trim();
    const trimmedModel = agentModel.trim();
    if (!trimmedName || !trimmedModel) {
      setFeedback('Agent 名称和模型不能为空');
      return;
    }

    setAgentSubmitting(true);
    setFeedback(null);
    try {
      const response = await fetch('/api/agents', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            name: trimmedName,
            model: trimmedModel,
            provider: agentProvider,
            permission_policy: agentPolicy,
            allowed_tools: parseToolPatterns(allowedToolsText),
            denied_tools: parseToolPatterns(deniedToolsText),
        }),
      });
      const payload = await response.json();
      if (!response.ok) {
        setFeedback(payload.error ?? 'Agent 创建失败');
        return;
      }
      const latestAgentsResponse = await fetch('/api/agents');
      let successMessage = `${trimmedName} 创建成功`;
      if (latestAgentsResponse.ok) {
        const latestAgentsPayload = (await latestAgentsResponse.json()) as {
          agents?: Array<{
            name?: string;
            agent_id?: string;
            runnable?: boolean;
            runnable_reason?: string;
          }>;
        };
        const createdAgent = (latestAgentsPayload.agents ?? []).find(
          (agent) => agent.name === trimmedName,
        );
        if (createdAgent?.runnable === false && createdAgent.runnable_reason) {
          successMessage = `${trimmedName} 已创建，但当前不可运行：${createdAgent.runnable_reason}`;
        }
      }
      setFeedback(successMessage);
      setAgentName('');
      setAllowedToolsText('');
      setDeniedToolsText('');
    } catch (err) {
      setFeedback(err instanceof Error ? err.message : 'Agent 创建失败');
    } finally {
      setAgentSubmitting(false);
    }
  }

  const healthyCount = servers.filter((s) => s.health === 'healthy').length;
  const onboardingSummary = evaluateThirdPartyOnboarding({
    currentServers: servers,
    onboardingError:
      feedback && /失败|failed|error|invalid|无法/i.test(feedback) ? feedback : null,
  });

  return (
    <div className="space-y-6">
      <header>
        <h1 className="text-2xl font-bold">Settings</h1>
        <p className="text-sm text-muted-foreground">
          MCP 服务器管理、第三方注册与健康监控
        </p>
      </header>

      {error && (
        <div className="rounded-lg border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
          <AlertTriangle className="mr-2 inline size-4" />
          {error}
        </div>
      )}

      {/* Summary */}
      <div className="grid grid-cols-2 gap-3">
        <div className="rounded-xl border border-border bg-card p-4">
          <p className="text-xs text-muted-foreground">健康 MCP 服务器</p>
          <p className="mt-1 text-2xl font-bold">{healthyCount}</p>
        </div>
        <div className="rounded-xl border border-border bg-card p-4">
          <p className="text-xs text-muted-foreground">总数</p>
          <p className="mt-1 text-2xl font-bold">{servers.length}</p>
        </div>
      </div>

      <section className="rounded-xl border border-border bg-card p-4">
        <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
          <Badge variant={statusVariant(onboardingSummary.onboardingStatus === 'failed' ? 'unreachable' : onboardingSummary.builtinToolsIntact ? 'healthy' : 'degraded')}>
            onboarding: {onboardingSummary.onboardingStatus}
          </Badge>
          <Badge variant={onboardingSummary.builtinToolsIntact ? 'secondary' : 'destructive'}>
            builtin tools intact: {onboardingSummary.builtinToolsIntact ? 'yes' : 'no'}
          </Badge>
          <Badge variant="outline">
            healthy servers: {onboardingSummary.healthyServerCount}/{onboardingSummary.serverCount}
          </Badge>
        </div>
        {onboardingSummary.exposedTools.length > 0 && (
          <div className="mt-3 flex flex-wrap gap-1">
            {onboardingSummary.exposedTools.slice(0, 8).map((tool, index) => (
              <Badge key={`${tool ?? 'unknown'}-${index}`} variant="outline" className="text-xs">
                {tool ?? 'unknown'}
              </Badge>
            ))}
          </div>
        )}
      </section>

      <section className="rounded-xl border border-border bg-card p-4">
        <h2 className="mb-3 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
          创建 Agent Profile
        </h2>
        <form onSubmit={handleCreateAgent} className="space-y-3">
          <div className="grid gap-3 sm:grid-cols-3">
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">Agent 名称</label>
              <Input
                value={agentName}
                onChange={(e) => setAgentName(e.target.value)}
                placeholder="web-codex-agent"
              />
            </div>
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">模型</label>
              <Input
                value={agentModel}
                onChange={(e) => setAgentModel(e.target.value)}
                placeholder="gpt-5.3-codex"
              />
            </div>
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">Provider</label>
              <Select value={agentProvider} onValueChange={setAgentProvider}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="agent-lite">agent-lite</SelectItem>
                  <SelectItem value="one-api">one-api</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">默认策略</label>
              <Select value={agentPolicy} onValueChange={setAgentPolicy}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="allow">allow</SelectItem>
                  <SelectItem value="ask">ask</SelectItem>
                  <SelectItem value="deny">deny</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>
          <p className="text-xs text-muted-foreground">
            选择 <code>agent-lite</code> 可避免默认依赖 one-api token 映射；如需走受管 one-api 凭据，再切换为 <code>one-api</code>。
          </p>
          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">
                allow tool patterns（每行一个）
              </label>
              <Textarea
                value={allowedToolsText}
                onChange={(e) => setAllowedToolsText(e.target.value)}
                placeholder={'mcp.fs.read_file\nmcp.search.ripgrep'}
              />
            </div>
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">
                deny tool patterns（每行一个）
              </label>
              <Textarea
                value={deniedToolsText}
                onChange={(e) => setDeniedToolsText(e.target.value)}
                placeholder={'mcp.shell.execute'}
              />
            </div>
          </div>
          <Button type="submit" disabled={agentSubmitting || !agentName.trim() || !agentModel.trim()}>
            {agentSubmitting ? (
              <Loader2 className="mr-1 size-4 animate-spin" />
            ) : (
              <Plus className="mr-1 size-4" />
            )}
            创建 Agent
          </Button>
        </form>
      </section>

      {/* Onboard form */}
      <section className="rounded-xl border border-border bg-card p-4">
        <h2 className="mb-3 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
          注册第三方 MCP 服务器
        </h2>
        <form onSubmit={handleOnboard} className="mcp-onboarding-form space-y-3">
          <div className="grid gap-3 sm:grid-cols-3">
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">
                服务器名称
              </label>
              <Input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="mcp-figma"
              />
            </div>
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">
                启动命令
              </label>
              <Input
                value={command}
                onChange={(e) => setCommand(e.target.value)}
                placeholder="npx"
              />
            </div>
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">
                信任级别
              </label>
              <Select value={trustLevel} onValueChange={setTrustLevel}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="community">community</SelectItem>
                  <SelectItem value="verified">verified</SelectItem>
                  <SelectItem value="untrusted">untrusted</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">
              参数（JSON 数组或每行一个）
            </label>
            <Textarea
              value={argsText}
              onChange={(e) => setArgsText(e.target.value)}
              placeholder={'["-y", "@modelcontextprotocol/server-figma"]'}
            />
          </div>
          <p className="text-xs text-muted-foreground">
            已预填一个可直接修改的 MCP server 示例；只改名称即可试跑，接入其他 server 时再调整命令和参数。
          </p>
          <Button type="submit" disabled={submitting || !name.trim() || !command.trim()}>
            {submitting ? (
              <Loader2 className="mr-1 size-4 animate-spin" />
            ) : (
              <Plus className="mr-1 size-4" />
            )}
            注册
          </Button>
          {feedback && (
            <p className="text-sm text-muted-foreground">{feedback}</p>
          )}
        </form>
      </section>

      {/* Server list */}
      <section className="rounded-xl border border-border bg-card p-4">
        <h2 className="mb-3 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
          MCP 服务器注册表
        </h2>
        {servers.length === 0 ? (
          <p className="py-8 text-center text-muted-foreground">
            暂无 MCP 服务器 — daemon 启动后内置服务器将自动显示
          </p>
        ) : (
          <ul className="mcp-server-list space-y-2">
            {servers.map((server) => (
              <li
                key={server.server}
                className="rounded-lg border border-border bg-background p-3"
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    {statusIcon(server.health)}
                    <span className="font-medium">{server.server}</span>
                    <Badge variant={statusVariant(server.health)}>
                      {server.health}
                    </Badge>
                    <Badge variant="outline">{server.trust_level}</Badge>
                  </div>
                </div>
                {server.capabilities.length > 0 && (
                  <div className="mt-2 flex flex-wrap gap-1">
                    {server.capabilities.map((cap) => (
                      <Badge key={cap} variant="outline" className="text-xs">
                        {cap}
                      </Badge>
                    ))}
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
