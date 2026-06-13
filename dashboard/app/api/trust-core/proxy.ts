const BACKEND_URL = process.env.NEXT_PUBLIC_BACKEND_URL ?? "http://127.0.0.1:8080";

export async function proxyTrustCore(path: string): Promise<Response> {
  try {
    const response = await fetch(`${BACKEND_URL}${path}`, {
      cache: "no-store",
      headers: { accept: "application/json" },
      signal: AbortSignal.timeout(4_000),
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
