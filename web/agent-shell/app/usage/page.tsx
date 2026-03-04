'use client';

import { useEffect, useMemo, useState } from 'react';

const BASE_SERIES = [640, 720, 690, 810, 760, 880, 830];

export default function UsagePage() {
  const [tick, setTick] = useState(0);

  useEffect(() => {
    const timer = window.setInterval(() => {
      setTick((current) => current + 1);
    }, 2400);
    return () => window.clearInterval(timer);
  }, []);

  const usageSeries = useMemo(
    () =>
      BASE_SERIES.map((value, index) => {
        const jitter = ((tick + 1) * (index + 1) * 17) % 130;
        return value + jitter;
      }),
    [tick]
  );

  const maxValue = Math.max(...usageSeries);
  const totalTokens = usageSeries.reduce((acc, value) => acc + value, 0);
  const estimatedCost = (totalTokens * 0.000001).toFixed(4);

  return (
    <main className="shell-page">
      <header>
        <h1>Usage &amp; Cost</h1>
        <p className="page-hint">Token usage trend by recent sampling windows.</p>
      </header>

      <section className="metric-grid" aria-label="usage-metrics">
        <article className="metric-card">
          <h2>Total Tokens (window)</h2>
          <p className="metric-value">{totalTokens}</p>
        </article>
        <article className="metric-card">
          <h2>Estimated Cost (USD)</h2>
          <p className="metric-value">{estimatedCost}</p>
        </article>
      </section>

      <section>
        <h2>Token Chart</h2>
        <div className="token-chart" role="img" aria-label="token usage chart">
          {usageSeries.map((value, index) => {
            const heightPercent = Math.round((value / maxValue) * 100);
            return (
              <div key={`token-bar-${index}`} className="token-bar-item">
                <div className="token-bar" style={{ height: `${heightPercent}%` }} />
                <small>{value}</small>
              </div>
            );
          })}
        </div>
      </section>
    </main>
  );
}
