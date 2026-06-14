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

  it("keeps preview modules visible when the Trust Core has no runs", async () => {
    mockTrustCore([]);
    renderDesk();
    expect(await screen.findByText("No authoritative runs yet")).toBeInTheDocument();
    expect(screen.getByText("Preview workspace")).toBeInTheDocument();
    expect(screen.getByText("Pass with conditions")).toBeInTheDocument();
    expect(screen.getAllByText("Preview").length).toBeGreaterThan(0);
  });

  it("shows a retryable failure without replacing it with mock run data", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => response({ code: "trust_core_unavailable", message: "unable to reach the Rust Trust Core", details: {} }, 503)),
    );
    renderDesk();
    expect(await screen.findByText("Unable to reach the Rust Trust Core.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
    expect(screen.getByText("Preview workspace")).toBeInTheDocument();
  });

  it("renders waiting approval read-only state and supports tabs and locale persistence", async () => {
    mockTrustCore([waitingRun], {
      [waitingRun.run_id]: { run: waitingRun, events },
    });
    const user = userEvent.setup();
    renderDesk();

    expect(await screen.findByText("This run is waiting for a human decision. No approval write action is available in this read-only slice.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /approve/i })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Findings" }));
    expect(screen.getByText("Unverified third-party endpoint")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "中文" }));
    expect(screen.getAllByText("可信审计控制台").length).toBeGreaterThan(0);
    expect(window.localStorage.getItem("toolpassport-locale")).toBe("zh-CN");
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
