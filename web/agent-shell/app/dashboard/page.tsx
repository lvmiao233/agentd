'use client';

import { useEffect, useMemo, useState } from 'react';

type AgentSummary = {
  id: string;
  status: 'running' | 'idle' | 'degraded';
  model: string;
  cpuPercent: number;
  memoryMiB: number;
};

const BASE_AGENTS: AgentSummary[] = [
  {
    id: 'agent-dev-01',
    status: 'running',
    model: 'claude-4-sonnet',
    cpuPercent: 14,
    memoryMiB: 512,
  },
  {
    id: 'agent-review-02',
    status: 'running',
    model: 'gpt-5.3-codex',
    cpuPercent: 11,
    memoryMiB: 640,
  },
  {
    id: 'agent-search-03',
    status: 'idle',
    model: 'claude-4-sonnet',
    cpuPercent: 4,
    memoryMiB: 296,
  },
];

export default function DashboardPage() {
  const [tick, setTick] = useState(0);

  useEffect(() => {
    const timer = window.setInterval(() => {
      setTick((current) => current + 1);
    }, 2500);
    return () => window.clearInterval(timer);
  }, []);

  const agents = useMemo(() => {
    return BASE_AGENTS.map((agent, index) => {
      const cpuJitter = (tick * (index + 2)) % 7;
      const memoryJitter = (tick * (index + 1) * 8) % 48;
      const status =
        index === 2 && tick % 4 === 3
          ? ('degraded' as const)
          : agent.status;
      return {
        ...agent,
        status,
        cpuPercent: Math.min(95, agent.cpuPercent + cpuJitter),
        memoryMiB: agent.memoryMiB + memoryJitter,
      };
    });
  }, [tick]);

  const runningCount = agents.filter((agent) => agent.status === 'running').length;
  const degradedCount = agents.filter((agent) => agent.status === 'degraded').length;
  const pendingApprovals = (tick % 3) + 1;

  return (
    <main className="shell-page">
      <header>
        <h1>Agent Dashboard</h1>
        <p className="page-hint">Live runtime overview for registered agents.</p>
      </header>

      <section className="metric-grid" aria-label="dashboard-metrics">
        <article className="metric-card">
          <h2>Registered Agents</h2>
          <p className="metric-value agent-count-card">{agents.length}</p>
        </article>
        <article className="metric-card">
          <h2>Running</h2>
          <p className="metric-value">{runningCount}</p>
        </article>
        <article className="metric-card">
          <h2>Pending Approvals</h2>
          <p className="metric-value">{pendingApprovals}</p>
        </article>
        <article className="metric-card">
          <h2>Degraded</h2>
          <p className="metric-value">{degradedCount}</p>
        </article>
      </section>

      <section>
        <h2>Agent Status</h2>
        <ul className="status-list">
          {agents.map((agent) => (
            <li key={agent.id}>
              <div>
                <strong>{agent.id}</strong>
                <small>{agent.model}</small>
              </div>
              <div className="status-meta">
                <span className={`status-pill ${agent.status}`}>{agent.status}</span>
                <span>CPU {agent.cpuPercent}%</span>
                <span>MEM {agent.memoryMiB} MiB</span>
              </div>
            </li>
          ))}
        </ul>
      </section>
    </main>
  );
}
