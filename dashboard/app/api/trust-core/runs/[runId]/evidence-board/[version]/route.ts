import { proxyTrustCore } from "@/app/api/trust-core/proxy";

export async function GET(
  _request: Request,
  context: { params: Promise<{ runId: string; version: string }> },
) {
  const { runId, version } = await context.params;
  return proxyTrustCore(
    `/api/runs/${encodeURIComponent(runId)}/evidence-board/${encodeURIComponent(version)}`,
  );
}
