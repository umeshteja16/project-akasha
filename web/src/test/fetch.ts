// A fake `fetch` for tests: route table keyed by "METHOD /path".

export type Handler = (request: Request) => Response | Promise<Response>;

export function json(body: unknown, status = 200, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", ...headers },
  });
}

export function apiError(status: number, code: string, message: string): Response {
  return json({ error: { code, message } }, status);
}

export function fakeFetch(routes: Record<string, Handler>) {
  const calls: string[] = [];
  const fetch = async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const request = input instanceof Request ? input : new Request(input, init);
    const key = `${request.method} ${new URL(request.url).pathname}`;
    calls.push(key);
    const handler = routes[key];
    if (!handler) return apiError(404, "not_found", `no fake for ${key}`);
    return handler(request);
  };
  return { fetch: fetch as typeof globalThis.fetch, calls };
}

export const USER = {
  id: "7f0c7c2e-0000-4000-8000-000000000001",
  email: "ada@example.test",
  display_name: "Ada Lovelace",
  created_at: "2026-10-01T10:00:00Z",
};

export const META = { version: "0.1.0", allow_registration: true, chat_model: false };
