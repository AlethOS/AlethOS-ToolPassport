import { proxyPost, proxyTrustCore } from "@/app/api/trust-core/proxy";

export async function GET(
  _request: Request,
  context: { params: Promise<{ runId: string }> },
) {
  const { runId } = await context.params;
  return proxyTrustCore(`/api/runs/${encodeURIComponent(runId)}/approval`);
}

export async function POST(
  request: Request,
  context: { params: Promise<{ runId: string }> },
) {
  const { runId } = await context.params;
  return proxyPost(`/api/runs/${encodeURIComponent(runId)}/approval`, await request.json());
}
