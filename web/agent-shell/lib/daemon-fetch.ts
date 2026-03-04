const DAEMON_URL = process.env.AGENTD_DAEMON_URL ?? 'http://127.0.0.1:7000';

let idCounter = 0;

export async function daemonRpc<T = unknown>(method: string, params: unknown = {}): Promise<T> {
  const res = await fetch(`${DAEMON_URL}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: ++idCounter, method, params }),
    cache: 'no-store',
  });

  const json = await res.json();
  if (json.error) {
    throw new Error(`${method}: ${json.error.message}`);
  }
  return json.result as T;
}
