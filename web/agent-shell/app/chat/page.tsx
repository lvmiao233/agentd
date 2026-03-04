export default function ChatPage() {
  return (
    <main className="chat-page">
      <header>
        <h1>Agent Chat</h1>
      </header>
      <section className="reconnect-banner" hidden>
        WebSocket disconnected, reconnecting…
      </section>
      <section className="chat-stream">
        <div className="stream-token" />
      </section>
      <footer>
        <textarea className="chat-input" placeholder="Ask the agent…" />
        <button className="send-button" type="button">
          Send
        </button>
      </footer>
    </main>
  );
}
