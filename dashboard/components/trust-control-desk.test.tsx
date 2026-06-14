import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { TrustControlDesk } from "@/components/trust-control-desk";
import type { Run, RunDetails, RunEvent } from "@/lib/types";

const waitingRun: Run = {
  run_id: "f1b3de22-bb10-4ce9-9f3d-66a85b2b42d1",
  goal: "Audit the framework permission boundaries",
  tool_id: "github:example/audit-framework",
  canonical_url: "https://github.com/example/audit-framework",
  tool: {
    name: "Audit Framework",
    tool_type: "agent_framework",
    urls: ["https://github.com/example/audit-framework"],
  },
  status: "waiting_approval",
  current_node: "human_review_gate",
  created_at: "2026-06-12T10:00:00Z",
  updated_at: "2026-06-12T10:05:00Z",
};

const runningRun: Run = {
  ...waitingRun,
  run_id: "8ed27889-264e-4571-a387-d585c90c1ef5",
  tool_id: "github:example/mcp-tool",
  canonical_url: "https://github.com/example/mcp-tool",
  tool: {
    name: "MCP Tool",
    tool_type: "mcp_server",
    urls: ["https://github.com/example/mcp-tool"],
  },
  status: "running",
  current_node: "gap_analysis",
};

const events: RunEvent[] = [
  {
    event_id: "845d361f-5748-4661-a3c6-0c7c594d6ed5",
    run_id: waitingRun.run_id,
    node_id: "human_review_gate",
    event_type: "approval_required",
    payload: {},
    created_at: "2026-06-12T10:05:00Z",
  },
];

function renderDesk() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });

  return render(
    <QueryClientProvider client={client}>
      <TrustControlDesk />
    </QueryClientProvider>,
  );
}

function response(body: unknown, status = 200) {
  return Promise.resolve(
    new Response(JSON.stringify(body), {
      status,
      headers: { "content-type": "application/json" },
    }),
  );
}

function mockTrustCore(runs: Run[], details: Record<string, RunDetails> = {}) {
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path.includes("/health")) return response({ status: "ok", service: "toolpassport-backend" });
      if (path === "/api/trust-core/runs") return response({ runs });
      if (path.includes("/check-results") || path.includes("/evidence-board/") || path.includes("/passport/")) {
        return response({ code: "not_found", message: "not found", details: {} }, 404);
      }
      const runId = path.split("/").at(-1) ?? "";
      return response(details[runId] ?? { run: runs[0], events: [] });
    }),
  );
}

describe("TrustControlDesk", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.restoreAllMocks();
  });

  it("shows an authoritative loading state while runs are pending", () => {
    vi.stubGlobal("fetch", vi.fn(() => new Promise<Response>(() => {})));
    renderDesk();
    expect(screen.getByText("Loading authoritative runs…")).toBeInTheDocument();
  });

  it("does not invent result data when the Trust Core has no runs", async () => {
    mockTrustCore([]);
    renderDesk();
    expect(await screen.findByText("No authoritative runs yet")).toBeInTheDocument();
    expect(screen.getAllByText("No authoritative run selected").length).toBeGreaterThan(0);
    expect(screen.queryByText("Pass with conditions")).not.toBeInTheDocument();
  });

  it("shows a retryable failure without replacing it with mock run data", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => response({ code: "trust_core_unavailable", message: "unable to reach the Rust Trust Core", details: {} }, 503)),
    );
    renderDesk();
    expect(await screen.findByText("Unable to reach the Rust Trust Core.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
    expect(screen.getAllByText("No authoritative run selected").length).toBeGreaterThan(0);
  });

  it("renders waiting approval read-only state and supports tabs and locale persistence", async () => {
    mockTrustCore([waitingRun], {
      [waitingRun.run_id]: { run: waitingRun, events },
    });
    const user = userEvent.setup();
    renderDesk();

    expect(await screen.findByText("Review the frozen commitments before recording an immutable decision.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /approve/i })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Findings" }));
    expect(screen.getByText("Authoritative data pending")).toBeInTheDocument();
    expect(screen.queryByText("Unverified third-party endpoint")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "中文" }));
    expect(screen.getAllByText("可信审计控制台").length).toBeGreaterThan(0);
    expect(window.localStorage.getItem("toolpassport-locale")).toBe("zh-CN");
  });

  it("renders deterministic score and dimensions from Rust Check Results", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path.includes("/health")) return response({ status: "ok", service: "toolpassport-backend" });
        if (path === "/api/trust-core/runs") return response({ runs: [runningRun] });
        if (path.includes("/check-results")) {
          const dimension = (dimension_id: string, score: number) => ({
            dimension_id,
            score,
            applicable_check_count: 1,
            weighted_points: score,
            max_weighted_points: 100,
          });
          return response({
            total_score: 41,
            rating: "trial",
            results: [{ check_id: "generic.claim_traceability", finding: "partial", rationale: "Bound to evidence", evidence_ids: ["ev-1"], not_applicable_reason: null }],
            dimension_scores: {
              capability_clarity: dimension("capability_clarity", 40),
              interface_openness: dimension("interface_openness", 41),
              automation_readiness: dimension("automation_readiness", 42),
              data_portability: dimension("data_portability", 43),
              permission_risk: dimension("permission_risk", 44),
              evidence_quality: dimension("evidence_quality", 45),
              ecosystem_fit: dimension("ecosystem_fit", 46),
            },
          });
        }
        if (path.includes("/evidence-board/") || path.includes("/passport/")) {
          return response({ code: "not_found", message: "not found", details: {} }, 404);
        }
        return response({ run: runningRun, events: [] });
      }),
    );

    renderDesk();
    expect(await screen.findByText("trial")).toBeInTheDocument();
    expect(screen.getByText("Capability clarity")).toBeInTheDocument();
    expect(screen.getByText("41", { selector: ".score-block > strong" })).toBeInTheDocument();
    expect(screen.queryByText("Pass with conditions")).not.toBeInTheDocument();
  });

  it("submits an offchain approval bound to frozen provenance", async () => {
    let approvalBody: Record<string, unknown> | null = null;
    const provenanceEvent: RunEvent = {
      ...events[0],
      event_type: "provenance_frozen",
      payload: { passport_sequence: 1 },
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const path = String(input);
        if (path.includes("/health")) return response({ status: "ok", service: "toolpassport-backend" });
        if (path === "/api/trust-core/runs") return response({ runs: [waitingRun] });
        if (path.includes("/passport/1")) {
          return response({
            passport: { passport_sequence: 1 },
            provenance: {
              passport_hash: `0x${"1".repeat(64)}`,
              audit_log_hash: `0x${"2".repeat(64)}`,
              evidence_manifest_hash: `0x${"3".repeat(64)}`,
            },
          });
        }
        if (path.endsWith("/approval")) {
          approvalBody = JSON.parse(String(init?.body));
          return response({ approval_id: "approval-1" }, 201);
        }
        if (path.includes("/check-results")) {
          return response({ code: "not_found", message: "not found", details: {} }, 404);
        }
        return response({ run: waitingRun, events: [provenanceEvent] });
      }),
    );

    const user = userEvent.setup();
    renderDesk();
    await user.click(await screen.findByRole("button", { name: "Approve offchain" }));
    await waitFor(() => expect(approvalBody).not.toBeNull());
    expect(approvalBody).toMatchObject({
      decision: "approve_offchain",
      passport_sequence: 1,
      chain_id: null,
      registry_contract: null,
    });
  });

  it("submits one approved Sepolia attestation and displays its receipt", async () => {
    let submitted = false;
    const receipt = {
      attestation_receipt_schema_version: "0.1.0",
      attestation_id: "879eaf72-bf3e-4268-bb99-23b840b4e9ed",
      run_id: runningRun.run_id,
      tool_id: runningRun.tool_id,
      passport_hash: `0x${"1".repeat(64)}`,
      audit_log_hash: `0x${"2".repeat(64)}`,
      evidence_manifest_hash: `0x${"3".repeat(64)}`,
      onchain_run_id: `0x${"4".repeat(64)}`,
      chain_id: 11_155_111,
      registry_contract: `0x${"b".repeat(40)}`,
      status: "confirmed",
      transaction_hash: `0x${"a".repeat(64)}`,
      submitted_at: "2026-06-14T12:00:00Z",
      confirmed_at: "2026-06-14T12:01:00Z",
    };
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
        const path = String(input);
        if (path.includes("/health")) return response({ status: "ok", service: "toolpassport-backend" });
        if (path === "/api/trust-core/runs") return response({ runs: [runningRun] });
        if (path.endsWith("/approval")) {
          return response({
            approval_schema_version: "0.1.0",
            decision: "approve_testnet_attestation",
          });
        }
        if (path.endsWith("/attestation")) {
          if (init?.method === "POST") {
            submitted = true;
            return response(receipt, 201);
          }
          return submitted
            ? response(receipt)
            : response({ code: "attestation_not_found", message: "not found", details: {} }, 404);
        }
        if (path.includes("/check-results") || path.includes("/evidence-board/") || path.includes("/passport/")) {
          return response({ code: "not_found", message: "not found", details: {} }, 404);
        }
        return response({ run: runningRun, events: [] });
      }),
    );

    const user = userEvent.setup();
    renderDesk();
    await user.click(await screen.findByRole("button", { name: "Submit Sepolia attestation" }));
    expect(await screen.findByText(receipt.transaction_hash)).toBeInTheDocument();
    expect(submitted).toBe(true);
  });

  it("shows public Sepolia preflight readiness without exposing secrets", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path.includes("/health")) return response({ status: "ok", service: "toolpassport-backend" });
        if (path.endsWith("/attestation/preflight")) {
          return response({
            attestation_preflight_schema_version: "0.1.0",
            ready: true,
            expected_chain_id: 11_155_111,
            connected_chain_id: 11_155_111,
            signer_address: `0x${"c".repeat(40)}`,
            signer_balance_wei: "1000000000000000",
            registry_contract: `0x${"b".repeat(40)}`,
            registry_code_present: true,
            issues: [],
          });
        }
        if (path === "/api/trust-core/runs") return response({ runs: [waitingRun] });
        if (path.includes("/check-results") || path.includes("/evidence-board/") || path.includes("/passport/") || path.endsWith("/approval") || path.endsWith("/attestation")) {
          return response({ code: "not_found", message: "not found", details: {} }, 404);
        }
        return response({ run: waitingRun, events });
      }),
    );

    renderDesk();

    expect(await screen.findByText("Attestation readiness")).toBeInTheDocument();
    expect(screen.getByText(`0x${"c".repeat(40)}`)).toBeInTheDocument();
    expect(screen.queryByText(/private_key|rpc_url/i)).not.toBeInTheDocument();
  });

  it("filters authoritative run rows and selects a different run", async () => {
    mockTrustCore([waitingRun, runningRun], {
      [waitingRun.run_id]: { run: waitingRun, events },
      [runningRun.run_id]: { run: runningRun, events: [] },
    });
    const user = userEvent.setup();
    renderDesk();

    expect(await screen.findByText("Audit Framework")).toBeInTheDocument();
    await user.type(screen.getByPlaceholderText("Search runs"), "MCP Tool");
    expect(screen.queryByText("Audit Framework")).not.toBeInTheDocument();
    await user.click(screen.getByText("MCP Tool"));
    await waitFor(() => expect(screen.getAllByText(/MCP Tool/).length).toBeGreaterThan(0));
  });

  it("warns when collected evidence has no validated claim mappings", async () => {
    const freezeEvent: RunEvent = {
      ...events[0],
      run_id: runningRun.run_id,
      event_type: "evidence_board_frozen",
      payload: { evidence_board_version: 1 },
    };
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path.includes("/health")) {
          return response({ status: "ok", service: "toolpassport-backend" });
        }
        if (path === "/api/trust-core/runs") return response({ runs: [runningRun] });
        if (path.includes("/evidence-board/1")) {
          return response({
            evidence_board: {
              version: 1,
              evidence_ids: ["evidence-1"],
              claims: [],
              gaps: [],
            },
            evidence_manifest: { entries: [] },
          });
        }
        if (path.includes("/check-results")) {
          return response({ code: "not_found", message: "not found", details: {} }, 404);
        }
        return response({ run: runningRun, events: [freezeEvent] });
      }),
    );

    const user = userEvent.setup();
    renderDesk();
    await user.click(await screen.findByRole("button", { name: "Evidence" }));
    expect(await screen.findByText(/Collected evidence is not linked/)).toBeInTheDocument();
  });

  it("creates a resolved tool candidate before launching a live investigation", async () => {
    const requested: string[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        requested.push(path);
        if (path.includes("/health")) {
          return response({ status: "ok", service: "toolpassport-backend" });
        }
        if (path === "/api/trust-core/runs") return response({ runs: [] });
        if (path === "/api/trust-core/tools/resolve") {
          return response({
            resolution_version: "0.1.0",
            status: "create_candidate",
            normalized_identifiers: [
              {
                namespace: "github",
                value: "example/new-tool",
                canonical_url: "https://github.com/example/new-tool",
              },
            ],
            tool_id: null,
            candidate_tool_ids: [],
            reason_codes: ["new_strong_identifier"],
          });
        }
        if (path === "/api/trust-core/tools/create") {
          return response({ tool_id: "github:example/new-tool" }, 201);
        }
        if (path === "/api/trust-core/runs/create") {
          return response({ ...runningRun, run_id: "new-run" }, 201);
        }
        if (path === "/api/trust-core/runs/new-run/investigate") {
          return response({ status: "launched", pid: 1234 });
        }
        return response({ code: "not_found", message: "not found", details: {} }, 404);
      }),
    );

    const user = userEvent.setup();
    renderDesk();
    await user.type(screen.getByPlaceholderText("https://github.com/owner/repo"), "https://github.com/example/new-tool");
    await user.click(screen.getByRole("button", { name: "Audit" }));

    expect(await screen.findByRole("button", { name: "Created" })).toBeInTheDocument();
    expect(requested).toContain("/api/trust-core/tools/create");
    expect(requested).toContain("/api/trust-core/runs/new-run/investigate");
  });
});
