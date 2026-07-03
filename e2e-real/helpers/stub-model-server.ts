import http from 'node:http';
import type { AddressInfo } from 'node:net';

/**
 * A running stub model endpoint, as handed back by
 * {@link startStubModelServer}.
 */
export interface StubModelServer {
  /** Base URL to configure as the connection's `baseUrl` (`http://127.0.0.1:<port>`). */
  url: string;
  /** The model id/name this server reports via `/api/tags` and echoes back
   * for every `/api/chat` call. */
  modelName: string;
  /** The fixed text every `/api/chat` call resolves with, regardless of the
   * prompt -- so the real-surface inject assertion is deterministic. */
  refinedText: string;
  /** Number of `/api/chat` calls received so far -- lets a spec assert
   * refine actually round-tripped through the real backend/network stack,
   * not just that the UI looks right. */
  chatCallCount(): number;
  close(): Promise<void>;
}

/**
 * Starts a tiny local HTTP server implementing just enough of Ollama's
 * native API (`GET /api/tags`, `POST /api/chat`) for `crates/llm-provider`'s
 * `OllamaProvider` (see `ollama.rs`) to complete a real `connection_add` /
 * `connection_refresh_models` / `refine` round trip against it.
 *
 * This exists so the real-surface acceptance (`refine-loop.e2e.ts`) can
 * drive the actual packaged app through a real network call without either
 * a live Ollama install or real cloud provider API keys -- see D4's task
 * note: "Use a stub/local model endpoint (or the Ollama default) so it
 * doesn't need real cloud keys." Always returns the same `refinedText`
 * regardless of the prompt sent, so the inject-loop assertion doesn't
 * depend on a real model's (non-deterministic) output.
 */
export function startStubModelServer(modelName: string, refinedText: string): Promise<StubModelServer> {
  let chatCalls = 0;

  return new Promise((resolve, reject) => {
    const server = http.createServer((req, res) => {
      const chunks: Buffer[] = [];
      req.on('data', (chunk) => chunks.push(chunk));
      req.on('end', () => {
        if (req.method === 'GET' && req.url === '/api/tags') {
          res.writeHead(200, { 'content-type': 'application/json' });
          res.end(JSON.stringify({ models: [{ name: modelName }] }));
          return;
        }

        if (req.method === 'POST' && req.url === '/api/chat') {
          chatCalls += 1;
          res.writeHead(200, { 'content-type': 'application/json' });
          res.end(
            JSON.stringify({
              model: modelName,
              message: { role: 'assistant', content: refinedText },
              eval_count: 1,
            }),
          );
          return;
        }

        res.writeHead(404, { 'content-type': 'application/json' });
        res.end(JSON.stringify({ error: `stub model server: no handler for ${req.method} ${req.url}` }));
      });
    });

    server.on('error', reject);

    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address() as AddressInfo;
      resolve({
        url: `http://127.0.0.1:${port}`,
        modelName,
        refinedText,
        chatCallCount: () => chatCalls,
        close: () =>
          new Promise((resolveClose) => {
            server.close(() => resolveClose());
          }),
      });
    });
  });
}
