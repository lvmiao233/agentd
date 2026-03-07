'use client';

import { useCallback, useEffect, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { summarizeDashboardState } from '@/lib/dashboard-events-model';
import {
  Activity,
  BellRing,
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
  runnable?: boolean;
  runnable_reason?: string;
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

type RuntimeEventSummary = {
  event_type: string;
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
  const [events, setEvents] = useState<RuntimeEventSummary[]>([]);
  const [error, setError] = useState<string | null>(null);

  const fetchData = useCallback(async () => {
    try {
      const [agentsRes, healthRes, eventsRes] = await Promise.all([
        fetch('/api/agents'),
        fetch('/api/health'),
        fetch('/api/events?limit=1&wait_timeout_secs=0'),
      ]);
      if (agentsRes.ok) {
        const data = await agentsRes.json();
        setAgents(data.agents ?? []);
      }
      if (healthRes.ok) {
        setHealth(await healthRes.json());
      }
      if (eventsRes.ok) {
        const data = await eventsRes.json();
        setEvents(data.events ?? []);
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

  const summary = summarizeDashboardState({
    agents,
    events: events.map((event) => ({ type: event.event_type })),
  });
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
          className="agent-count-card"
          icon={<Server className="size-4 text-blue-400" />}
          label="注册 Agents"
          value={summary.agentCount}
        />
        <MetricCard
          icon={<Activity className="size-4 text-green-400" />}
          label="运行中"
          value={summary.runningCount}
        />
        <MetricCard
          icon={<Cpu className="size-4 text-purple-400" />}
          label="总 Tokens"
          value={totalTokens.toLocaleString()}
        />
        <MetricCard
          icon={<BellRing className="size-4 text-cyan-400" />}
          label="最近事件"
          value={summary.latestEventType}
        />
      </div>

      <section className="rounded-xl border border-border bg-card p-4">
        <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
          <Badge variant={statusVariant(health?.status ?? 'unknown')}>
            Daemon: {health?.status ?? '—'}
          </Badge>
          <Badge variant={summary.degradedCount > 0 ? 'outline' : 'secondary'}>
            Degraded agents: {summary.degradedCount}
          </Badge>
          <Badge variant="secondary">Latest event: {summary.latestEventType}</Badge>
        </div>
      </section>

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
                    {agent.runnable === false && (
                      <Badge variant="destructive">unrunnable</Badge>
                    )}
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
                  {agent.runnable === false && agent.runnable_reason && (
                    <div className="mt-2 text-xs text-amber-300">
                      {agent.runnable_reason}
                    </div>
                  )}
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
  className,
  icon,
  label,
  value,
}: {
  className?: string;
  icon: React.ReactNode;
  label: string;
  value: string | number;
}) {
  return (
    <div className={className ? `${className} rounded-xl border border-border bg-card p-4` : 'rounded-xl border border-border bg-card p-4'}>
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        {icon}
        {label}
      </div>
      <p className="mt-2 text-2xl font-bold">{value}</p>
    </div>
  );
}
