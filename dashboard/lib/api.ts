import type { ApiErrorBody, HealthResponse, RunDetails, RunListResponse } from "@/lib/types";

export class TrustCoreApiError extends Error {
  constructor(
    message: string,
    public readonly status: number,
    public readonly body: ApiErrorBody | null,
  ) {
    super(message);
  }
}

async function getJson<T>(path: string): Promise<T> {
  const response = await fetch(path, { headers: { accept: "application/json" } });
  const body = (await response.json().catch(() => null)) as ApiErrorBody | T | null;

  if (!response.ok) {
    const errorBody = body as ApiErrorBody | null;
    throw new TrustCoreApiError(errorBody?.message ?? "Trust Core request failed", response.status, errorBody);
  }

  return body as T;
}

export function getHealth(): Promise<HealthResponse> {
  return getJson("/api/trust-core/health");
}

export function getRuns(): Promise<RunListResponse> {
  return getJson("/api/trust-core/runs");
}

export function getRunDetails(runId: string): Promise<RunDetails> {
  return getJson(`/api/trust-core/runs/${encodeURIComponent(runId)}`);
}
