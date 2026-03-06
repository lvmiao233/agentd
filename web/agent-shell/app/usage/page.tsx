'use client';

import { useCallback, useEffect, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { buildUsageBars } from '@/lib/dashboard-events-model';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
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

const WINDOW_OPTIONS = ['1h', '24h', '7d', 'all'] as const;

type UsageWindow = (typeof WINDOW_OPTIONS)[number];

export default function UsagePage() {
  const [agents, setAgents] = useState<AgentUsage[]>([]);
  const [totals, setTotals] = useState<UsageTotals>({
    input_tokens: 0,
    output_tokens: 0,
    total_tokens: 0,
    cost_usd: 0,
  });
  const [error, setError] = useState<string | null>(null);
  const [window, setWindow] = useState<UsageWindow>('24h');
  const [selectedModel, setSelectedModel] = useState<string>('all');

  const fetchUsage = useCallback(async () => {
    try {
      const params = new URLSearchParams();
      if (window !== 'all') {
        params.set('window', window);
      }
      const res = await fetch(`/api/usage${params.size ? `?${params}` : ''}`);
      if (!res.ok) throw new Error('fetch failed');
      const data = await res.json();
      setAgents(data.agents ?? []);
      if (data.totals) setTotals(data.totals);
      setError(null);
    } catch {
      setError('无法从 daemon 获取用量数据');
    }
  }, [window]);

  useEffect(() => {
    fetchUsage();
    const timer = setInterval(fetchUsage, 10000);
    return () => clearInterval(timer);
  }, [fetchUsage]);

  const models = Array.from(new Set(agents.map((agent) => agent.model))).sort();
  const filteredAgents =
    selectedModel === 'all'
      ? agents
      : agents.filter((agent) => agent.model === selectedModel);
  const filteredTotals = filteredAgents.reduce(
    (acc, agent) => ({
      input_tokens: acc.input_tokens + agent.input_tokens,
      output_tokens: acc.output_tokens + agent.output_tokens,
      total_tokens: acc.total_tokens + agent.total_tokens,
      cost_usd: acc.cost_usd + agent.cost_usd,
    }),
    { input_tokens: 0, output_tokens: 0, total_tokens: 0, cost_usd: 0 },
  );
  const maxTokens = Math.max(1, ...filteredAgents.map((a) => a.total_tokens));
  const usageBars = buildUsageBars(filteredAgents.map((agent) => agent.total_tokens));

  return (
    <div className="space-y-6">
      <header>
        <h1 className="text-2xl font-bold">Usage &amp; Cost</h1>
        <p className="text-sm text-muted-foreground">
          来自 daemon 的 Agent token 用量与费用统计
        </p>
      </header>

      <div className="flex flex-wrap gap-3">
        <div className="min-w-40">
          <Select value={window} onValueChange={(value) => setWindow(value as UsageWindow)}>
            <SelectTrigger aria-label="Usage window selector">
              <SelectValue placeholder="时间窗口" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="1h">最近 1 小时</SelectItem>
              <SelectItem value="24h">最近 24 小时</SelectItem>
              <SelectItem value="7d">最近 7 天</SelectItem>
              <SelectItem value="all">全部时间</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div className="min-w-48">
          <Select value={selectedModel} onValueChange={setSelectedModel}>
            <SelectTrigger aria-label="Usage model selector">
              <SelectValue placeholder="按模型筛选" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">全部模型</SelectItem>
              {models.map((model) => (
                <SelectItem key={model} value={model}>
                  {model}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>

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
          value={filteredTotals.total_tokens.toLocaleString()}
        />
        <MetricCard
          icon={<ArrowDown className="size-4 text-green-400" />}
          label="输入 Tokens"
          value={filteredTotals.input_tokens.toLocaleString()}
        />
        <MetricCard
          icon={<ArrowUp className="size-4 text-orange-400" />}
          label="输出 Tokens"
          value={filteredTotals.output_tokens.toLocaleString()}
        />
        <MetricCard
          icon={<DollarSign className="size-4 text-yellow-400" />}
          label="预估费用 (USD)"
          value={`$${filteredTotals.cost_usd.toFixed(4)}`}
        />
      </div>

      {/* Per-agent breakdown */}
      <section className="token-chart rounded-xl border border-border bg-card p-4">
        <div className="mb-3 flex items-end justify-between gap-4">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
            Agent 用量明细
          </h2>
          {usageBars.length > 0 && (
            <div className="flex h-12 items-end gap-1" aria-label="token usage bars">
              {usageBars.map((bar, index) => (
                <div
                  key={`${bar.value}-${index}`}
                  className="w-2 rounded-t bg-blue-500/80"
                  style={{ height: `${Math.max(bar.heightPercent, 8)}%` }}
                  title={`${bar.value.toLocaleString()} tokens`}
                />
              ))}
            </div>
          )}
        </div>
        {filteredAgents.length === 0 ? (
          <p className="py-8 text-center text-muted-foreground">
            当前筛选条件下暂无用量数据
          </p>
        ) : (
          <ul className="space-y-3">
            {filteredAgents.map((agent) => {
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
