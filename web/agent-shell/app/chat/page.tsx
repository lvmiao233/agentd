'use client';

import { useEffect, useMemo, useState } from 'react';

type ChatMessage = {
  role: 'user' | 'assistant';
  content: string;
};

type ToolCall = {
  tool: string;
  args: Record<string, string>;
};

type PolicyEvent = {
  type: string;
  detail: string;
};

declare global {
  interface Window {
    __qaDisconnect?: () => void;
    __qaReconnect?: () => void;
  }
}

function chunkByGrapheme(text: string): string[] {
  const chars = Array.from(text);
  const chunks: string[] = [];
  for (let idx = 0; idx < chars.length; idx += 2) {
    chunks.push(chars.slice(idx, idx + 2).join(''));
  }
  return chunks.length > 0 ? chunks : [text];
}

function wait(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export default function ChatPage() {
  const [input, setInput] = useState('');
  const [connected, setConnected] = useState(true);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [toolCalls, setToolCalls] = useState<ToolCall[]>([]);
  const [policyEvents, setPolicyEvents] = useState<PolicyEvent[]>([]);
  const [streamToken, setStreamToken] = useState('');
  const [isStreaming, setIsStreaming] = useState(false);

  useEffect(() => {
    window.__qaDisconnect = () => setConnected(false);
    window.__qaReconnect = () => setConnected(true);

    return () => {
      delete window.__qaDisconnect;
      delete window.__qaReconnect;
    };
  }, []);

  const reconnectVisible = useMemo(() => !connected, [connected]);

  async function streamAssistantReply(prompt: string) {
    const reply = `已分析：${prompt}`;
    const chunks = chunkByGrapheme(reply);
    setIsStreaming(true);
    setStreamToken('');
    setMessages((prev) => [...prev, { role: 'assistant', content: '' }]);

    for (const chunk of chunks) {
      await wait(90);
      setStreamToken((prev) => `${prev}${chunk}`);
      setMessages((prev) => {
        const next = [...prev];
        const last = next[next.length - 1];
        if (last && last.role === 'assistant') {
          next[next.length - 1] = {
            ...last,
            content: `${last.content}${chunk}`,
          };
        }
        return next;
      });
    }

    setIsStreaming(false);
  }

  async function handleSend() {
    const prompt = input.trim();
    if (!prompt) {
      return;
    }

    setMessages((prev) => [...prev, { role: 'user', content: prompt }]);
    setToolCalls((prev) => [
      ...prev,
      {
        tool: 'analysis.parse',
        args: { target: 'main.rs' },
      },
    ]);
    setPolicyEvents((prev) => [
      ...prev,
      {
        type: 'approval.policy.ask',
        detail: 'analysis.parse requires explicit approval context',
      },
    ]);

    setInput('');
    await streamAssistantReply(prompt);
  }

  return (
    <main className="chat-page">
      <header>
        <h1>Agent Chat</h1>
      </header>

      <section className="reconnect-banner" hidden={!reconnectVisible}>
        WebSocket disconnected, reconnecting…
      </section>

      <section className="chat-history">
        <ul className="chat-messages">
          {messages.map((message, index) => (
            <li key={`${message.role}-${index}`}>
              <strong>{message.role}:</strong> {message.content}
            </li>
          ))}
        </ul>
      </section>

      <section className="chat-stream">
        <div className="stream-token">{streamToken}</div>
        {isStreaming ? <small>streaming...</small> : null}
      </section>

      <section className="tool-panel">
        <h2>Tool Calls</h2>
        <ul className="tool-calls">
          {toolCalls.map((item, index) => (
            <li key={`${item.tool}-${index}`}>
              {item.tool} {JSON.stringify(item.args)}
            </li>
          ))}
        </ul>
      </section>

      <section className="events-panel">
        <h2>Policy Events</h2>
        <ul className="policy-events">
          {policyEvents.map((event, index) => (
            <li key={`${event.type}-${index}`}>
              {event.type}: {event.detail}
            </li>
          ))}
        </ul>
      </section>

      <footer>
        <textarea
          className="chat-input"
          placeholder="Ask the agent…"
          value={input}
          onChange={(event) => setInput(event.target.value)}
        />
        <button className="send-button" type="button" onClick={handleSend}>
          Send
        </button>
      </footer>
    </main>
  );
}
