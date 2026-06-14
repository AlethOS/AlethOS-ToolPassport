import { proxyPost } from "@/app/api/trust-core/proxy";

export async function POST(
  _request: Request,
  context: { params: Promise<{ runId: string }> },
) {
  const { runId } = await context.params;
  return proxyPost(`/api/runs/${encodeURIComponent(runId)}/investigate`, {});
}
