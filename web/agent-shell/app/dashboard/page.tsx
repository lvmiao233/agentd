'use client';

import { useCallback, useEffect, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import {
  Activity,
  Server,
  AlertTriangle,
  CheckCircle,
  Cpu,
  MemoryStick,
} from 'lucide-react';

type AgentSummary = {
  agent_id: string;
  name: string;
  model: string;
  status: string;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_usd: number;
  session_count: number;
  created_at: string;
};

type HealthStatus = {
  status: string;
  subsystems: Record<string, string>;
};

function statusVariant(status: string) {
  switch (status) {
    case 'running':
    case 'ok':
    case 'ready':
      return 'default' as const;
    case 'idle':
      return 'secondary' as const;
    case 'degraded':
      return 'outline' as const;
    default:
      return 'destructive' as const;
  }
}

export default function DashboardPage() {
  const [agents, setAgents] = useState<AgentSummary[]>([]);
  const [health, setHealth] = useState<HealthStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  const fetchData = useCallback(async () => {
    try {
      const [agentsRes, healthRes] = await Promise.all([
        fetch('/api/agents'),
        fetch('/api/health'),
      ]);
      if (agentsRes.ok) {
        const data = await agentsRes.json();
        setAgents(data.agents ?? []);
      }
      if (healthRes.ok) {
        setHealth(await healthRes.json());
      }
      setError(null);
    } catch {
      setError('无法连接到 agentd daemon');
    }
  }, []);

  useEffect(() => {
    fetchData();
    const timer = setInterval(fetchData, 5000);
    return () => clearInterval(timer);
  }, [fetchData]);

  const runningCount = agents.filter((a) => a.status === 'running').length;
  const totalTokens = agents.reduce(
    (s, a) => s + a.total_input_tokens + a.total_output_tokens,
    0,
  );

  return (
    <div className="space-y-6">
      <header>
        <h1 className="text-2xl font-bold">Agent Dashboard</h1>
        <p className="text-sm text-muted-foreground">
          来自 agentd daemon 的实时运行概览
        </p>
      </header>

      {error && (
        <div className="rounded-lg border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
          <AlertTriangle className="mr-2 inline size-4" />
          {error}
        </div>
      )}

      {/* Metrics */}
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <MetricCard
          icon={<Server className="size-4 text-blue-400" />}
          label="注册 Agents"
          value={agents.length}
        />
        <MetricCard
          icon={<Activity className="size-4 text-green-400" />}
          label="运行中"
          value={runningCount}
        />
        <MetricCard
          icon={<Cpu className="size-4 text-purple-400" />}
          label="总 Tokens"
          value={totalTokens.toLocaleString()}
        />
        <MetricCard
          icon={<CheckCircle className="size-4 text-cyan-400" />}
          label="Daemon"
          value={health?.status ?? '—'}
        />
      </div>

      {/* Health subsystems */}
      {health?.subsystems && (
        <section className="rounded-xl border border-border bg-card p-4">
          <h2 className="mb-3 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
            子系统
          </h2>
          <div className="flex flex-wrap gap-2">
            {Object.entries(health.subsystems).map(([k, v]) => (
              <Badge key={k} variant={statusVariant(v)}>
                {k}: {v}
              </Badge>
            ))}
          </div>
        </section>
      )}

      {/* Agent list */}
      <section className="rounded-xl border border-border bg-card p-4">
        <h2 className="mb-3 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
          Agent 列表
        </h2>
        {agents.length === 0 ? (
          <p className="py-8 text-center text-muted-foreground">
            暂无注册 Agent — 通过 CLI 或 API 创建
          </p>
        ) : (
          <ul className="space-y-2">
            {agents.map((agent) => (
              <li
                key={agent.agent_id}
                className="flex items-center justify-between rounded-lg border border-border bg-background p-3"
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate font-medium">{agent.name}</span>
                    <Badge variant={statusVariant(agent.status)}>
                      {agent.status}
                    </Badge>
                  </div>
                  <div className="mt-1 flex gap-3 text-xs text-muted-foreground">
                    <span>{agent.model}</span>
                    <span>
                      <MemoryStick className="mr-0.5 inline size-3" />
                      {(
                        agent.total_input_tokens + agent.total_output_tokens
                      ).toLocaleString()}{' '}
                      tokens
                    </span>
                    <span>Sessions: {agent.session_count}</span>
                  </div>
                </div>
                <span className="text-xs text-muted-foreground">
                  {agent.agent_id.slice(0, 8)}…
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function MetricCard({
  icon,
  label,
  value,
}: {
  icon: React.ReactNode;
  label: string;
  value: string | number;
}) {
  return (
    <div className="rounded-xl border border-border bg-card p-4">
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        {icon}
        {label}
      </div>
      <p className="mt-2 text-2xl font-bold">{value}</p>
    </div>
  );
}
