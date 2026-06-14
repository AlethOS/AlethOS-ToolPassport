import type {
  ApiErrorBody,
  ArtifactListResponse,
  CheckResults,
  EventListResponse,
  EvidenceFreezeResult,
  EvidenceListResponse,
  HealthResponse,
  PassportFreezeResult,
  Run,
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

// ── Write operations ────────────────────────────────────────────────

async function postJson<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch(path, {
    method: "POST",
    headers: { accept: "application/json", "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  const json = (await response.json().catch(() => null)) as ApiErrorBody | T | null;

  if (!response.ok) {
    const errorBody = json as ApiErrorBody | null;
    throw new TrustCoreApiError(
      errorBody?.message ?? "Trust Core request failed",
      response.status,
      errorBody,
    );
  }

  return json as T;
}

export interface ResolveToolRequest {
  intake_version: "0.1.0";
  name: string;
  tool_type: "generic" | "agent_framework" | "mcp_server" | "cli_api_tool";
  urls: string[];
}

export interface ResolveToolResponse {
  resolution_version: string;
  status: "resolved" | "create_candidate" | "needs_review";
  normalized_identifiers: Array<{
    namespace: string;
    value: string;
    canonical_url: string;
  }>;
  tool_id: string | null;
  candidate_tool_ids: string[];
  reason_codes: string[];
}

export interface CreateToolRequest {
  tool_id: string;
  name: string;
  tool_type: "generic" | "agent_framework" | "mcp_server" | "cli_api_tool";
  canonical_url: string;
  external_identifiers: ResolveToolResponse["normalized_identifiers"];
  aliases: string[];
}

export function resolveTool(request: ResolveToolRequest): Promise<ResolveToolResponse> {
  return postJson("/api/trust-core/tools/resolve", request);
}

export function createTool(request: CreateToolRequest): Promise<unknown> {
  return postJson("/api/trust-core/tools/create", request);
}

export function createRun(goal: string, toolId: string): Promise<Run> {
  return postJson("/api/trust-core/runs/create", { goal, tool_id: toolId });
}

export function launchInvestigation(runId: string): Promise<{ status: string; pid: number }> {
  return postJson(`/api/trust-core/runs/${encodeURIComponent(runId)}/investigate`, {});
}
