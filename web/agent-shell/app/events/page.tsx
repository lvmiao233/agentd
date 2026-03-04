'use client';

import { useEffect, useState } from 'react';

type RuntimeEvent = {
  id: string;
  type: string;
  detail: string;
  at: string;
};

const INITIAL_EVENTS: RuntimeEvent[] = [
  {
    id: 'evt-1',
    type: 'AgentCreated',
    detail: 'agent-dev-01 profile loaded',
    at: 'just now',
  },
  {
    id: 'evt-2',
    type: 'ToolInvoked',
    detail: 'mcp.fs.read_file requested by agent-review-02',
    at: 'just now',
  },
  {
    id: 'evt-3',
    type: 'PolicyDecision',
    detail: 'policy.ask queued approval item #appr-14',
    at: 'just now',
  },
];

const EVENT_ROTATION: Array<Omit<RuntimeEvent, 'id' | 'at'>> = [
  { type: 'ToolDenied', detail: 'mcp.shell.execute denied by policy guardrail' },
  { type: 'UsageRecorded', detail: 'token usage aggregated for 1m window' },
  { type: 'ToolApproved', detail: 'approval queue resolved for mcp.search.ripgrep' },
];

export default function EventsPage() {
  const [events, setEvents] = useState<RuntimeEvent[]>(INITIAL_EVENTS);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    const timer = window.setInterval(() => {
      setTick((current) => {
        const next = current + 1;
        const blueprint = EVENT_ROTATION[next % EVENT_ROTATION.length];
        const event: RuntimeEvent = {
          id: `evt-live-${next}`,
          type: blueprint.type,
          detail: blueprint.detail,
          at: `${next * 3}s ago`,
        };
        setEvents((prev) => [event, ...prev].slice(0, 12));
        return next;
      });
    }, 3000);
    return () => window.clearInterval(timer);
  }, []);

  return (
    <main className="shell-page">
      <header>
        <h1>Events &amp; Audit</h1>
        <p className="page-hint">Realtime event stream merged with policy audit highlights.</p>
      </header>

      <section className="metric-grid events-summary" aria-label="events-summary">
        <article className="metric-card">
          <h2>Tracked Events</h2>
          <p className="metric-value">{events.length}</p>
        </article>
        <article className="metric-card">
          <h2>Live Ticks</h2>
          <p className="metric-value">{tick}</p>
        </article>
      </section>

      <section>
        <h2>Stream</h2>
        <ul className="events-stream">
          {events.map((event) => (
            <li key={event.id}>
              <div>
                <strong>{event.type}</strong>
                <p>{event.detail}</p>
              </div>
              <small>{event.at}</small>
            </li>
          ))}
        </ul>
      </section>
    </main>
  );
}
