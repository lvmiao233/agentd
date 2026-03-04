'use client';

import { FormEvent, useMemo, useState } from 'react';

type McpServerRow = {
  name: string;
  command: string;
  trustLevel: string;
  source: 'builtin' | 'third-party';
  status: 'healthy' | 'onboarding' | 'failed';
  capabilities: string[];
  message: string;
};

const BUILTIN_SERVERS: McpServerRow[] = [
  {
    name: 'mcp-fs',
    command: 'python -m agentd_mcp_fs',
    trustLevel: 'builtin',
    source: 'builtin',
    status: 'healthy',
    capabilities: ['fs.read_file', 'fs.list_directory'],
    message: 'initialized',
  },
  {
    name: 'mcp-search',
    command: 'python -m agentd_mcp_search',
    trustLevel: 'builtin',
    source: 'builtin',
    status: 'healthy',
    capabilities: ['search.ripgrep'],
    message: 'initialized',
  },
];

export default function SettingsPage() {
  const [servers, setServers] = useState<McpServerRow[]>(BUILTIN_SERVERS);
  const [name, setName] = useState('mcp-figma');
  const [command, setCommand] = useState('npx -y @modelcontextprotocol/server-figma');
  const [trustLevel, setTrustLevel] = useState('community');
  const [feedback, setFeedback] = useState('Ready to onboard third-party MCP server.');

  const healthyCount = useMemo(
    () => servers.filter((server) => server.status === 'healthy').length,
    [servers]
  );

  function handleOnboard(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const normalizedName = name.trim();
    const normalizedCommand = command.trim();
    if (!normalizedName || !normalizedCommand) {
      setFeedback('Server name and command are required.');
      return;
    }

    const duplicate = servers.some((server) => server.name === normalizedName);
    if (duplicate) {
      setFeedback(`Server ${normalizedName} already exists.`);
      return;
    }

    const onboardingEntry: McpServerRow = {
      name: normalizedName,
      command: normalizedCommand,
      trustLevel,
      source: 'third-party',
      status: 'onboarding',
      capabilities: [],
      message: 'running initialize handshake',
    };
    setServers((prev) => [onboardingEntry, ...prev]);
    setFeedback(`Onboarding ${normalizedName}...`);

    window.setTimeout(() => {
      const shouldFail =
        normalizedCommand.includes('--fail') || normalizedCommand.includes('broken');
      setServers((prev) =>
        prev.map((server) => {
          if (server.name !== normalizedName) {
            return server;
          }
          if (shouldFail) {
            return {
              ...server,
              status: 'failed',
              message: 'initialize handshake failed (isolated)',
            };
          }
          return {
            ...server,
            status: 'healthy',
            message: 'onboarded',
            capabilities: ['figma.get_file', 'figma.export_frame'],
          };
        })
      );

      if (shouldFail) {
        setFeedback(
          `${normalizedName} failed handshake but builtin MCP servers remain healthy.`
        );
      } else {
        setFeedback(`${normalizedName} onboarded successfully.`);
      }
    }, 280);
  }

  return (
    <main className="shell-page">
      <header>
        <h1>Settings</h1>
        <p className="page-hint">
          Profile defaults, MCP server onboarding, and compatibility health.
        </p>
      </header>

      <section className="metric-grid" aria-label="settings-metrics">
        <article className="metric-card">
          <h2>Healthy MCP Servers</h2>
          <p className="metric-value">{healthyCount}</p>
        </article>
        <article className="metric-card">
          <h2>Compatibility</h2>
          <p className="metric-value">daemon ↔ agent-lite ↔ web</p>
        </article>
      </section>

      <section className="settings-section">
        <h2>Third-party MCP Onboarding</h2>
        <form className="mcp-onboarding-form" onSubmit={handleOnboard}>
          <label>
            Server Name
            <input value={name} onChange={(event) => setName(event.target.value)} />
          </label>
          <label>
            Command
            <input value={command} onChange={(event) => setCommand(event.target.value)} />
          </label>
          <label>
            Trust Level
            <select
              value={trustLevel}
              onChange={(event) => setTrustLevel(event.target.value)}
            >
              <option value="community">community</option>
              <option value="verified">verified</option>
              <option value="untrusted">untrusted</option>
            </select>
          </label>
          <button type="submit">Onboard server</button>
        </form>
        <p className="settings-feedback">{feedback}</p>
      </section>

      <section className="settings-section">
        <h2>MCP Server Registry</h2>
        <ul className="mcp-server-list">
          {servers.map((server) => (
            <li key={server.name}>
              <div>
                <strong>{server.name}</strong>
                <small>{server.command}</small>
              </div>
              <div className="status-meta">
                <span className={`status-pill ${server.status}`}>{server.status}</span>
                <span>{server.trustLevel}</span>
                <span>{server.source}</span>
              </div>
              <p>{server.message}</p>
              {server.capabilities.length > 0 ? (
                <small>capabilities: {server.capabilities.join(', ')}</small>
              ) : null}
            </li>
          ))}
        </ul>
      </section>
    </main>
  );
}
