import type {
  ApiErrorBody,
  ArtifactListResponse,
  CheckResults,
  EventListResponse,
  EvidenceFreezeResult,
  EvidenceListResponse,
  HealthResponse,
  PassportFreezeResult,
  RunDetails,
  RunListResponse,
} from "@/lib/types";

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

export function getRunEvents(runId: string): Promise<EventListResponse> {
  return getJson(`/api/trust-core/runs/${encodeURIComponent(runId)}/events`);
}

export function getRunEvidence(runId: string): Promise<EvidenceListResponse> {
  return getJson(`/api/trust-core/runs/${encodeURIComponent(runId)}/evidence`);
}

export function getRunArtifacts(runId: string): Promise<ArtifactListResponse> {
  return getJson(`/api/trust-core/runs/${encodeURIComponent(runId)}/artifacts`);
}

export function getRunCheckResults(runId: string): Promise<CheckResults> {
  return getJson(`/api/trust-core/runs/${encodeURIComponent(runId)}/check-results`);
}

export function getEvidenceBoard(
  runId: string,
  version: number,
): Promise<EvidenceFreezeResult> {
  return getJson(
    `/api/trust-core/runs/${encodeURIComponent(runId)}/evidence-board/${version}`,
  );
}

export function getPassport(runId: string, sequence: number): Promise<PassportFreezeResult> {
  return getJson(
    `/api/trust-core/runs/${encodeURIComponent(runId)}/passport/${sequence}`,
  );
}
