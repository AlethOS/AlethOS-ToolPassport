const BACKEND_URL = process.env.NEXT_PUBLIC_BACKEND_URL ?? "http://127.0.0.1:8080";

async function proxyFetch(path: string, init: RequestInit = {}): Promise<Response> {
  try {
    const response = await fetch(`${BACKEND_URL}${path}`, {
      ...init,
      cache: "no-store",
      signal: AbortSignal.timeout(30_000),
    });
    const body = await response.text();

    return new Response(body, {
      status: response.status,
      headers: { "content-type": response.headers.get("content-type") ?? "application/json" },
    });
  } catch {
    return Response.json(
      {
        code: "trust_core_unavailable",
        message: "unable to reach the Rust Trust Core",
        details: {},
      },
      { status: 503 },
    );
  }
}

export async function proxyTrustCore(path: string): Promise<Response> {
  return proxyFetch(path, { headers: { accept: "application/json" } });
}

export async function proxyPost(path: string, body: unknown): Promise<Response> {
  return proxyFetch(path, {
    method: "POST",
    headers: { accept: "application/json", "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}
