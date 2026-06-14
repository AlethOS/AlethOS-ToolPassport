import { proxyPost, proxyTrustCore } from "@/app/api/trust-core/proxy";

export async function GET(
  _request: Request,
  context: { params: Promise<{ runId: string }> },
) {
  const { runId } = await context.params;
  return proxyTrustCore(`/api/runs/${encodeURIComponent(runId)}/attestation`);
}

export async function POST(
  _request: Request,
  context: { params: Promise<{ runId: string }> },
) {
  const { runId } = await context.params;
  return proxyPost(`/api/runs/${encodeURIComponent(runId)}/attestation`, {});
}
