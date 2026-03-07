'use client';

import { useEffect, useRef, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Activity,
  AlertTriangle,
  ShieldCheck,
  ShieldX,
  Wrench,
  RefreshCw,
  Pause,
  Play,
} from 'lucide-react';

type RuntimeEvent = {
  id: string;
  event_type: string;
  agent_id?: string;
  severity: string;
  result: string;
  tool_name?: string;
  message?: string;
  metadata?: Record<string, unknown>;
  created_at: string;
};

function normalizeRuntimeEvent(event: Record<string, unknown>): RuntimeEvent {
  const payload =
    event.payload && typeof event.payload === 'object'
      ? (event.payload as Record<string, unknown>)
      : {};
  return {
    id: typeof event.id === 'string' ? event.id : String(event.event_id ?? 'unknown'),
    event_type:
      typeof event.event_type === 'string' ? event.event_type : String(event.type ?? 'unknown'),
    agent_id: typeof event.agent_id === 'string' ? event.agent_id : undefined,
    severity: typeof event.severity === 'string' ? event.severity : 'info',
    result: typeof event.result === 'string' ? event.result : 'success',
    tool_name: typeof payload.tool_name === 'string' ? payload.tool_name : undefined,
    message: typeof payload.message === 'string' ? payload.message : undefined,
    metadata:
      payload.metadata && typeof payload.metadata === 'object'
        ? (payload.metadata as Record<string, unknown>)
        : payload,
    created_at:
      typeof event.created_at === 'string'
        ? event.created_at
        : String(event.timestamp ?? new Date().toISOString()),
  };
}

function eventIcon(eventType: string) {
  if (eventType.includes('Denied')) return <ShieldX className="size-4 text-red-400" />;
  if (eventType.includes('Approved')) return <ShieldCheck className="size-4 text-green-400" />;
  if (eventType.includes('Tool')) return <Wrench className="size-4 text-blue-400" />;
  return <Activity className="size-4 text-muted-foreground" />;
}

function severityVariant(severity: string) {
  switch (severity) {
    case 'error':
      return 'destructive' as const;
    case 'warn':
      return 'outline' as const;
    default:
      return 'secondary' as const;
  }
}

export default function EventsPage() {
  const [events, setEvents] = useState<RuntimeEvent[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [paused, setPaused] = useState(false);
  const [connected, setConnected] = useState(false);
  const cursorRef = useRef<string | undefined>(undefined);

  const mergeEvents = (incoming: RuntimeEvent[]) => {
    if (incoming.length === 0) {
      return;
    }
    setEvents((prev) => {
      const seen = new Set(prev.map((event) => event.id));
      const merged = [...incoming.filter((event) => !seen.has(event.id)), ...prev];
      return merged.slice(0, 200);
    });
  };

  const refreshOnce = async () => {
    try {
      const params = new URLSearchParams({ limit: '50', wait_timeout_secs: '0' });
      if (cursorRef.current) {
        params.set('cursor', cursorRef.current);
      }
      const response = await fetch(`/api/events?${params}`);
      if (!response.ok) {
        throw new Error('fetch failed');
      }
      const payload = (await response.json()) as {
        events?: RuntimeEvent[];
        next_cursor?: string;
      };
      if (payload.next_cursor) {
        cursorRef.current = payload.next_cursor;
      }
      mergeEvents((payload.events ?? []).map((event) => normalizeRuntimeEvent(event as Record<string, unknown>)));
      setError(null);
      setConnected(true);
    } catch {
      setError('无法从 daemon 获取事件');
      setConnected(false);
    }
  };

  useEffect(() => {
    void refreshOnce();
    const pollTimer = setInterval(() => {
      if (!paused) {
        void refreshOnce();
      }
    }, 2000);

    return () => {
      clearInterval(pollTimer);
    };
  }, [paused]);

  return (
    <div className="space-y-6">
      <header className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">Events &amp; Audit</h1>
          <p className="text-sm text-muted-foreground">
            来自 daemon SubscribeEvents 的实时运行时事件流
          </p>
        </div>
        <div className="flex gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setPaused((current) => !current)}
          >
            {paused ? (
              <Play className="mr-1 size-3" />
            ) : (
              <Pause className="mr-1 size-3" />
            )}
            {paused ? '恢复' : '暂停'}
          </Button>
          <Button variant="outline" size="sm" onClick={() => void refreshOnce()}>
            <RefreshCw className="mr-1 size-3" />
            刷新
          </Button>
        </div>
      </header>

      {error && (
        <div className="rounded-lg border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
          <AlertTriangle className="mr-2 inline size-4" />
          {error}
        </div>
      )}

      <div className="grid grid-cols-2 gap-3 md:grid-cols-3">
        <div className="rounded-xl border border-border bg-card p-4">
          <p className="text-xs text-muted-foreground">事件总数</p>
          <p className="mt-1 text-2xl font-bold">{events.length}</p>
        </div>
        <div className="rounded-xl border border-border bg-card p-4">
          <p className="text-xs text-muted-foreground">策略拒绝</p>
          <p className="mt-1 text-2xl font-bold text-red-400">
            {events.filter((event) => event.event_type.includes('Denied')).length}
          </p>
        </div>
        <div className="rounded-xl border border-border bg-card p-4">
          <p className="text-xs text-muted-foreground">连接状态</p>
          <p className="mt-1 text-2xl font-bold">
            {paused ? '已暂停' : connected ? '实时连接' : '重连中'}
          </p>
        </div>
      </div>

      <section className="rounded-xl border border-border bg-card p-4">
        <h2 className="mb-3 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
          事件流
        </h2>
        {events.length === 0 ? (
          <p className="py-8 text-center text-muted-foreground">
            暂无事件 — daemon 运行后将自动显示
          </p>
        ) : (
          <ul className="space-y-2">
            {events.map((event) => (
              <li
                key={event.id}
                className="flex items-start gap-3 rounded-lg border border-border bg-background p-3"
              >
                {eventIcon(event.event_type)}
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-medium">{event.event_type}</span>
                    <Badge variant={severityVariant(event.severity)}>
                      {event.severity}
                    </Badge>
                    {event.tool_name && (
                      <Badge variant="outline">{event.tool_name}</Badge>
                    )}
                  </div>
                  {event.message && (
                    <p className="mt-1 text-sm text-muted-foreground">
                      {event.message}
                    </p>
                  )}
                  <div className="mt-1 flex gap-3 text-xs text-muted-foreground">
                    {event.agent_id && (
                      <span>Agent: {event.agent_id.slice(0, 8)}…</span>
                    )}
                    <span>{event.created_at}</span>
                  </div>
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
