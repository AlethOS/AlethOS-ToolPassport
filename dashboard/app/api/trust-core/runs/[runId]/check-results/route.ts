import { proxyTrustCore } from "@/app/api/trust-core/proxy";

export async function GET(
  _request: Request,
  context: { params: Promise<{ runId: string }> },
) {
  const { runId } = await context.params;
  return proxyTrustCore(`/api/runs/${encodeURIComponent(runId)}/check-results`);
}
