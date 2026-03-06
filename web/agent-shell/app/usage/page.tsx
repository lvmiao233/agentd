'use client';

import { useCallback, useEffect, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import {
  AlertTriangle,
  BarChart3,
  ArrowDown,
  ArrowUp,
  DollarSign,
} from 'lucide-react';

type AgentUsage = {
  agent_id: string;
  name: string;
  model: string;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  cost_usd: number;
};

type UsageTotals = {
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  cost_usd: number;
};

export default function UsagePage() {
  const [agents, setAgents] = useState<AgentUsage[]>([]);
  const [totals, setTotals] = useState<UsageTotals>({
    input_tokens: 0,
    output_tokens: 0,
    total_tokens: 0,
    cost_usd: 0,
  });
  const [error, setError] = useState<string | null>(null);

  const fetchUsage = useCallback(async () => {
    try {
      const res = await fetch('/api/usage');
      if (!res.ok) throw new Error('fetch failed');
      const data = await res.json();
      setAgents(data.agents ?? []);
      if (data.totals) setTotals(data.totals);
      setError(null);
    } catch {
      setError('无法从 daemon 获取用量数据');
    }
  }, []);

  useEffect(() => {
    fetchUsage();
    const timer = setInterval(fetchUsage, 10000);
    return () => clearInterval(timer);
  }, [fetchUsage]);

  const maxTokens = Math.max(1, ...agents.map((a) => a.total_tokens));

  return (
    <div className="space-y-6">
      <header>
        <h1 className="text-2xl font-bold">Usage &amp; Cost</h1>
        <p className="text-sm text-muted-foreground">
          来自 daemon 的 Agent token 用量与费用统计
        </p>
      </header>

      {error && (
        <div className="rounded-lg border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
          <AlertTriangle className="mr-2 inline size-4" />
          {error}
        </div>
      )}

      {/* Summary metrics */}
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <MetricCard
          icon={<BarChart3 className="size-4 text-blue-400" />}
          label="总 Tokens"
          value={totals.total_tokens.toLocaleString()}
        />
        <MetricCard
          icon={<ArrowDown className="size-4 text-green-400" />}
          label="输入 Tokens"
          value={totals.input_tokens.toLocaleString()}
        />
        <MetricCard
          icon={<ArrowUp className="size-4 text-orange-400" />}
          label="输出 Tokens"
          value={totals.output_tokens.toLocaleString()}
        />
        <MetricCard
          icon={<DollarSign className="size-4 text-yellow-400" />}
          label="预估费用 (USD)"
          value={`$${totals.cost_usd.toFixed(4)}`}
        />
      </div>

      {/* Per-agent breakdown */}
      <section className="token-chart rounded-xl border border-border bg-card p-4">
        <h2 className="mb-3 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
          Agent 用量明细
        </h2>
        {agents.length === 0 ? (
          <p className="py-8 text-center text-muted-foreground">
            暂无用量数据 — 通过 Chat 或 CLI 使用 Agent 后将显示
          </p>
        ) : (
          <ul className="space-y-3">
            {agents.map((agent) => {
              const pct = Math.round(
                (agent.total_tokens / maxTokens) * 100,
              );
              return (
                <li key={agent.agent_id} className="space-y-2">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <span className="font-medium">{agent.name}</span>
                      <Badge variant="secondary">{agent.model}</Badge>
                    </div>
                    <span className="text-sm font-mono">
                      {agent.total_tokens.toLocaleString()} tokens
                    </span>
                  </div>
                  <div className="h-2 w-full overflow-hidden rounded-full bg-muted">
                    <div
                      className="h-full rounded-full bg-blue-500 transition-all"
                      style={{ width: `${pct}%` }}
                    />
                  </div>
                  <div className="flex gap-4 text-xs text-muted-foreground">
                    <span>输入: {agent.input_tokens.toLocaleString()}</span>
                    <span>输出: {agent.output_tokens.toLocaleString()}</span>
                    <span>${agent.cost_usd.toFixed(4)}</span>
                    <span className="ml-auto">
                      {agent.agent_id.slice(0, 8)}…
                    </span>
                  </div>
                </li>
              );
            })}
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
