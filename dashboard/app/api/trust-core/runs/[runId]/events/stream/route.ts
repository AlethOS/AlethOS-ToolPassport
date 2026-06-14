const BACKEND_URL = process.env.NEXT_PUBLIC_BACKEND_URL ?? "http://127.0.0.1:8080";

export async function GET(
  _request: Request,
  context: { params: Promise<{ runId: string }> },
): Promise<Response> {
  const { runId } = await context.params;
  const backendPath = `/api/runs/${encodeURIComponent(runId)}/events/stream`;

  try {
    const backendResponse = await fetch(`${BACKEND_URL}${backendPath}`, {
      cache: "no-store",
      headers: { accept: "text/event-stream" },
    });

    if (!backendResponse.ok) {
      const body = await backendResponse.text().catch(() => "");
      return new Response(body, {
        status: backendResponse.status,
        headers: { "content-type": "application/json" },
      });
    }

    // Passthrough the SSE stream with correct headers.
    return new Response(backendResponse.body, {
      status: 200,
      headers: {
        "content-type": "text/event-stream",
        "cache-control": "no-cache",
        connection: "keep-alive",
      },
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
