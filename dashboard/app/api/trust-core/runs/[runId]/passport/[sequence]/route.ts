import { proxyTrustCore } from "@/app/api/trust-core/proxy";

export async function GET(
  _request: Request,
  context: { params: Promise<{ runId: string; sequence: string }> },
) {
  const { runId, sequence } = await context.params;
  return proxyTrustCore(
    `/api/runs/${encodeURIComponent(runId)}/passport/${encodeURIComponent(sequence)}`,
  );
}
