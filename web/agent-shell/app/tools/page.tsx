'use client';

import { useCallback, useEffect, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { AlertTriangle, ShieldCheck, Wrench } from 'lucide-react';

type AgentOption = {
  agent_id: string;
  name: string;
  model: string;
};

type AvailableTool = {
  server: string;
  tool: string;
  policy_tool: string;
  trust_level: string;
  health: string;
  decision: string;
  reason?: string;
  trace_id?: string;
};

function healthVariant(health: string) {
  switch (health) {
    case 'healthy':
      return 'default' as const;
    case 'degraded':
      return 'outline' as const;
    default:
      return 'destructive' as const;
  }
}

export default function ToolsPage() {
  const [agents, setAgents] = useState<AgentOption[]>([]);
  const [agentId, setAgentId] = useState('');
  const [tools, setTools] = useState<AvailableTool[]>([]);
  const [error, setError] = useState<string | null>(null);

  const fetchTools = useCallback(async (selectedAgentId?: string) => {
    try {
      const params = new URLSearchParams();
      if (selectedAgentId) {
        params.set('agent_id', selectedAgentId);
      }

      const suffix = params.toString() ? `?${params.toString()}` : '';
      const res = await fetch(`/api/tools${suffix}`);
      if (!res.ok) {
        throw new Error('fetch failed');
      }

      const data = await res.json();
      setAgents(data.agents ?? []);
      setTools(data.tools ?? []);

      if (!selectedAgentId && data.agent_id) {
        setAgentId(data.agent_id);
      }
      setError(null);
    } catch {
      setError('无法从 daemon 获取可用工具列表');
    }
  }, []);

  useEffect(() => {
    void fetchTools(agentId || undefined);
    const timer = setInterval(() => {
      void fetchTools(agentId || undefined);
    }, 8000);
    return () => clearInterval(timer);
  }, [agentId, fetchTools]);

  return (
    <div className="space-y-6">
      <header>
        <h1 className="text-2xl font-bold">Tools</h1>
        <p className="text-sm text-muted-foreground">
          按 Agent 查看实时可用工具（含策略过滤后结果）
        </p>
      </header>

      {error && (
        <div className="rounded-lg border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
          <AlertTriangle className="mr-2 inline size-4" />
          {error}
        </div>
      )}

      <section className="rounded-xl border border-border bg-card p-4">
        <h2 className="mb-3 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
          Agent 选择
        </h2>
        {agents.length === 0 ? (
          <p className="text-sm text-muted-foreground">暂无 Agent，请先创建 Agent。</p>
        ) : (
          <Select
            value={agentId || agents[0].agent_id}
            onValueChange={(value) => setAgentId(value)}
          >
            <SelectTrigger className="max-w-xl">
              <SelectValue placeholder="选择 Agent" />
            </SelectTrigger>
            <SelectContent>
              {agents.map((agent) => (
                <SelectItem key={agent.agent_id} value={agent.agent_id}>
                  {agent.name} · {agent.model} · {agent.agent_id.slice(0, 8)}…
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
      </section>

      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <div className="rounded-xl border border-border bg-card p-4">
          <p className="text-xs text-muted-foreground">可用工具数</p>
          <p className="mt-1 text-2xl font-bold">{tools.length}</p>
        </div>
        <div className="rounded-xl border border-border bg-card p-4">
          <p className="text-xs text-muted-foreground">健康工具</p>
          <p className="mt-1 text-2xl font-bold">
            {tools.filter((tool) => tool.health === 'healthy').length}
          </p>
        </div>
        <div className="rounded-xl border border-border bg-card p-4">
          <p className="text-xs text-muted-foreground">策略允许</p>
          <p className="mt-1 text-2xl font-bold text-green-400">
            {tools.filter((tool) => tool.decision === 'allow').length}
          </p>
        </div>
        <div className="rounded-xl border border-border bg-card p-4">
          <p className="text-xs text-muted-foreground">策略询问</p>
          <p className="mt-1 text-2xl font-bold text-yellow-400">
            {tools.filter((tool) => tool.decision === 'ask').length}
          </p>
        </div>
      </div>

      <section className="rounded-xl border border-border bg-card p-4">
        <h2 className="mb-3 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
          工具列表
        </h2>
        {tools.length === 0 ? (
          <p className="py-8 text-center text-muted-foreground">
            当前 Agent 暂无可用工具（可能被策略过滤或 MCP 未就绪）
          </p>
        ) : (
          <ul className="space-y-2">
            {tools.map((tool) => (
              <li
                key={`${tool.server}:${tool.tool}:${tool.trace_id ?? ''}`}
                className="rounded-lg border border-border bg-background p-3"
              >
                <div className="flex flex-wrap items-center gap-2">
                  <Wrench className="size-4 text-muted-foreground" />
                  <span className="font-medium">{tool.tool}</span>
                  <Badge variant="outline">{tool.server}</Badge>
                  <Badge variant={healthVariant(tool.health)}>{tool.health}</Badge>
                  <Badge variant="secondary">{tool.trust_level}</Badge>
                  <Badge variant={tool.decision === 'allow' ? 'default' : 'outline'}>
                    <ShieldCheck className="mr-1 size-3" />
                    {tool.decision}
                  </Badge>
                </div>
                <p className="mt-1 text-xs text-muted-foreground">
                  policy: <code>{tool.policy_tool}</code>
                </p>
                {tool.reason && (
                  <p className="mt-1 text-xs text-muted-foreground">reason: {tool.reason}</p>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
