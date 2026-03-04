'use client';

import { FormEvent, useCallback, useEffect, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
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
  name: string;
  command: string;
  trust_level: string;
  source: string;
  status: string;
  capabilities: string[];
  message: string;
};

function statusIcon(status: string) {
  switch (status) {
    case 'healthy':
      return <CheckCircle className="size-4 text-green-400" />;
    case 'onboarding':
      return <Loader2 className="size-4 animate-spin text-yellow-400" />;
    case 'failed':
      return <XCircle className="size-4 text-red-400" />;
    default:
      return <Server className="size-4 text-muted-foreground" />;
  }
}

function statusVariant(status: string) {
  switch (status) {
    case 'healthy':
      return 'default' as const;
    case 'onboarding':
      return 'outline' as const;
    case 'failed':
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

  const [name, setName] = useState('');
  const [command, setCommand] = useState('');
  const [trustLevel, setTrustLevel] = useState('community');

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
      const res = await fetch('/api/mcp', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: n, command: c, trust_level: trustLevel }),
      });
      const data = await res.json();
      if (res.ok) {
        setFeedback(`${n} 注册成功`);
        setName('');
        setCommand('');
        await fetchServers();
      } else {
        setFeedback(data.error ?? '注册失败');
      }
    } catch {
      setFeedback('请求失败');
    } finally {
      setSubmitting(false);
    }
  }

  const healthyCount = servers.filter((s) => s.status === 'healthy').length;

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

      {/* Onboard form */}
      <section className="rounded-xl border border-border bg-card p-4">
        <h2 className="mb-3 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
          注册第三方 MCP 服务器
        </h2>
        <form onSubmit={handleOnboard} className="space-y-3">
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
                placeholder="npx -y @modelcontextprotocol/server-figma"
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
          <Button type="submit" disabled={submitting}>
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
          <ul className="space-y-2">
            {servers.map((server) => (
              <li
                key={server.name}
                className="rounded-lg border border-border bg-background p-3"
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    {statusIcon(server.status)}
                    <span className="font-medium">{server.name}</span>
                    <Badge variant={statusVariant(server.status)}>
                      {server.status}
                    </Badge>
                    <Badge variant="outline">{server.trust_level}</Badge>
                    <Badge variant="secondary">{server.source}</Badge>
                  </div>
                </div>
                <p className="mt-1 text-sm text-muted-foreground">
                  <code className="rounded bg-muted px-1 text-xs">
                    {server.command}
                  </code>
                </p>
                {server.message && (
                  <p className="mt-1 text-xs text-muted-foreground">
                    {server.message}
                  </p>
                )}
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
