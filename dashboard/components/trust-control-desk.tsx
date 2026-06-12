"use client";

import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import {
  Activity,
  AlertCircle,
  AlertTriangle,
  BadgeCheck,
  Boxes,
  Check,
  CheckCircle2,
  ChevronRight,
  ClipboardCheck,
  Clock3,
  Copy,
  Database,
  FileCheck2,
  Fingerprint,
  Gauge,
  Globe2,
  Languages,
  LayoutDashboard,
  ListChecks,
  LoaderCircle,
  Network,
  RefreshCw,
  Search,
  Server,
  ShieldAlert,
  ShieldCheck,
  SlidersHorizontal,
  SquareTerminal,
  Zap,
} from "lucide-react";
import { useEffect, useMemo, useState, type ComponentType, type CSSProperties } from "react";

import { ExecutionFlow, ProvenanceFlow } from "@/components/flow-panels";
import { getHealth, getRunDetails, getRuns } from "@/lib/api";
import { translate, type TranslationKey } from "@/lib/i18n";
import { evidenceClaims, evidenceCoverage, previewPassport } from "@/lib/preview";
import type { DashboardTab, Locale, Run, RunEvent, RunStatus } from "@/lib/types";

const tabs: DashboardTab[] = ["overview", "findings", "evidence", "execution", "provenance"];
const statuses: Array<"all" | RunStatus> = ["all", "pending", "running", "waiting_approval", "success", "failed", "cancelled"];

const statusKeys: Record<RunStatus, TranslationKey> = {
  pending: "pending",
  running: "running",
  waiting_approval: "waitingApproval",
  success: "success",
  failed: "failed",
  cancelled: "cancelled",
};

const eventKeys: Record<RunEvent["event_type"], TranslationKey> = {
  run_created: "eventRunCreated",
  run_status_changed: "eventRunStatusChanged",
  node_started: "eventNodeStarted",
  node_finished: "eventNodeFinished",
  artifact_created: "eventArtifactCreated",
  evidence_created: "eventEvidenceCreated",
  approval_required: "eventApprovalRequired",
  approval_resolved: "eventApprovalResolved",
  attestation_submitted: "eventAttestationSubmitted",
  attestation_confirmed: "eventAttestationConfirmed",
  error: "eventError",
};

const navItems: Array<[TranslationKey, ComponentType<{ size?: number }>, boolean]> = [
  ["controlDesk", LayoutDashboard, true],
  ["runQueueNav", ListChecks, false],
  ["toolRegistryNav", Boxes, false],
  ["evidenceNav", Database, false],
  ["policyNav", ClipboardCheck, false],
  ["passportNav", FileCheck2, false],
];

export function TrustControlDesk() {
  const [locale, setLocale] = useState<Locale>(() => {
    if (typeof window === "undefined") return "en";
    const stored = window.localStorage.getItem("toolpassport-locale");
    return stored === "zh-CN" || stored === "en" ? stored : navigator.language.startsWith("zh") ? "zh-CN" : "en";
  });
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<"all" | RunStatus>("all");
  const [tab, setTab] = useState<DashboardTab>("overview");
  const [copiedHash, setCopiedHash] = useState<string | null>(null);
  const t = (key: TranslationKey) => translate(locale, key);
  const refreshInterval = autoRefresh ? 5_000 : false;

  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);

  const healthQuery = useQuery({ queryKey: ["trust-core-health"], queryFn: getHealth, refetchInterval: refreshInterval });
  const runsQuery = useQuery({ queryKey: ["runs"], queryFn: getRuns, refetchInterval: refreshInterval });
  const runs = useMemo(() => runsQuery.data?.runs ?? [], [runsQuery.data?.runs]);
  const activeRunId = selectedRunId && runs.some((run) => run.run_id === selectedRunId) ? selectedRunId : runs[0]?.run_id ?? null;

  const detailsQuery = useQuery({
    queryKey: ["run", activeRunId],
    queryFn: () => getRunDetails(activeRunId!),
    enabled: Boolean(activeRunId),
    refetchInterval: refreshInterval,
  });
  const selectedRun = detailsQuery.data?.run ?? runs.find((run) => run.run_id === activeRunId) ?? null;
  const events = detailsQuery.data?.events ?? [];

  const filteredRuns = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return runs.filter((run) => {
      const matchesFilter = filter === "all" || run.status === filter;
      const matchesSearch =
        !needle ||
        run.run_id.toLowerCase().includes(needle) ||
        run.tool.name.toLowerCase().includes(needle) ||
        run.tool_id.toLowerCase().includes(needle);
      return matchesFilter && matchesSearch;
    });
  }, [filter, runs, search]);

  const counts = {
    running: runs.filter((run) => run.status === "running").length,
    waiting: runs.filter((run) => run.status === "waiting_approval").length,
    completed: runs.filter((run) => run.status === "success").length,
  };

  const setLanguage = (next: Locale) => {
    setLocale(next);
    window.localStorage.setItem("toolpassport-locale", next);
    document.documentElement.lang = next;
  };

  const copyHash = async (value: string) => {
    await navigator.clipboard?.writeText(value);
    setCopiedHash(value);
    window.setTimeout(() => setCopiedHash(null), 1_600);
  };

  return (
    <div className="app-shell">
      <Sidebar t={t} />
      <div className="workspace">
        <Topbar
          t={t}
          locale={locale}
          setLanguage={setLanguage}
          online={healthQuery.isSuccess}
          autoRefresh={autoRefresh}
          setAutoRefresh={setAutoRefresh}
        />
        <main className="desk-main">
          <MetricStrip t={t} runs={runs.length} counts={counts} online={healthQuery.isSuccess} />
          <div className="desk-grid">
            <RunQueue
              t={t}
              runs={filteredRuns}
              total={runs.length}
              selectedRunId={activeRunId}
              setSelectedRunId={setSelectedRunId}
              search={search}
              setSearch={setSearch}
              filter={filter}
              setFilter={setFilter}
              query={runsQuery}
              locale={locale}
            />
            <ResultWorkspace
              t={t}
              selectedRun={selectedRun}
              tab={tab}
              setTab={setTab}
              currentNode={selectedRun?.current_node ?? null}
              copiedHash={copiedHash}
              copyHash={copyHash}
            />
            <TrustInspector
              t={t}
              run={selectedRun}
              events={events}
              loading={detailsQuery.isLoading}
              error={detailsQuery.isError}
              locale={locale}
              openFindings={() => setTab("findings")}
            />
          </div>
        </main>
        <ActivityTicker t={t} events={events} autoRefresh={autoRefresh} locale={locale} />
      </div>
    </div>
  );
}

function Sidebar({ t }: { t: (key: TranslationKey) => string }) {
  return (
    <aside className="side-rail">
      <div className="brand">
        <Fingerprint size={30} aria-hidden="true" />
        <div>
          <strong>AlethOS</strong>
          <span>ToolPassport</span>
        </div>
      </div>
      <nav aria-label="Primary">
        {navItems.map(([key, Icon, active]) => (
          <button className={active ? "nav-item active" : "nav-item"} disabled={!active} key={key} title={!active ? t("futureView") : undefined}>
            <Icon size={17} aria-hidden="true" />
            <span>{t(key)}</span>
            {!active && <small>{t("preview")}</small>}
          </button>
        ))}
      </nav>
      <div className="operator">
        <div className="operator-mark">AO</div>
        <div>
          <strong>AlethOS Ops</strong>
          <span>{t("role")}</span>
        </div>
        <span className="live-dot" aria-label={t("live")} />
      </div>
    </aside>
  );
}

function Topbar({
  t,
  locale,
  setLanguage,
  online,
  autoRefresh,
  setAutoRefresh,
}: {
  t: (key: TranslationKey) => string;
  locale: Locale;
  setLanguage: (locale: Locale) => void;
  online: boolean;
  autoRefresh: boolean;
  setAutoRefresh: (value: boolean) => void;
}) {
  return (
    <header className="topbar">
      <div className="topbar-title">
        <h1>{t("controlDesk")}</h1>
        <span className={online ? "authority-chip online" : "authority-chip offline"}>
          {online ? <BadgeCheck size={14} /> : <AlertCircle size={14} />}
          {online ? t("backendOnline") : t("backendOffline")}
        </span>
      </div>
      <div className="topbar-actions">
        <button className={autoRefresh ? "icon-toggle active" : "icon-toggle"} onClick={() => setAutoRefresh(!autoRefresh)} title={t("autoRefresh")}>
          <RefreshCw size={15} />
          <span>{autoRefresh ? t("on") : t("off")}</span>
        </button>
        <div className="language-switch" aria-label={t("language")}>
          <Languages size={15} />
          <button className={locale === "en" ? "active" : ""} onClick={() => setLanguage("en")}>EN</button>
          <button className={locale === "zh-CN" ? "active" : ""} onClick={() => setLanguage("zh-CN")}>中文</button>
        </div>
      </div>
    </header>
  );
}

function MetricStrip({
  t,
  runs,
  counts,
  online,
}: {
  t: (key: TranslationKey) => string;
  runs: number;
  counts: { running: number; waiting: number; completed: number };
  online: boolean;
}) {
  const metrics = [
    { label: t("runs"), value: runs, icon: ListChecks, preview: false },
    { label: t("running"), value: counts.running, icon: Activity, preview: false },
    { label: t("waitingApproval"), value: counts.waiting, icon: Clock3, preview: false },
    { label: t("completed"), value: counts.completed, icon: CheckCircle2, preview: false },
    { label: t("previewCoverage"), value: `${previewPassport.coverage}%`, icon: Gauge, preview: true },
    { label: t("previewScore"), value: previewPassport.score, icon: ShieldCheck, preview: true },
  ];
  return (
    <section className="metric-strip" aria-label={t("localTrustCore")}>
      {metrics.map(({ label, value, icon: Icon, preview }) => (
        <div className="metric" key={label}>
          <div>
            <span>{label}</span>
            {preview && <small>{t("preview")}</small>}
          </div>
          <strong>{value}</strong>
          <Icon size={18} aria-hidden="true" />
        </div>
      ))}
      <div className="metric system-health">
        <div><span>{t("localTrustCore")}</span><small>{t("authoritative")}</small></div>
        <strong className={online ? "text-good" : "text-danger"}>{online ? t("live") : "—"}</strong>
        <Server size={18} />
      </div>
    </section>
  );
}

function RunQueue({
  t,
  runs,
  total,
  selectedRunId,
  setSelectedRunId,
  search,
  setSearch,
  filter,
  setFilter,
  query,
  locale,
}: {
  t: (key: TranslationKey) => string;
  runs: Run[];
  total: number;
  selectedRunId: string | null;
  setSelectedRunId: (runId: string) => void;
  search: string;
  setSearch: (value: string) => void;
  filter: "all" | RunStatus;
  setFilter: (value: "all" | RunStatus) => void;
  query: UseQueryResult<{ runs: Run[] }, Error>;
  locale: Locale;
}) {
  return (
    <aside className="panel run-queue">
      <PanelTitle title={t("runQueue")} badge={total.toString()} authority={t("authoritative")} />
      <label className="search-box">
        <Search size={15} />
        <input value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t("searchRuns")} />
      </label>
      <label className="filter-box">
        <SlidersHorizontal size={14} />
        <select value={filter} onChange={(event) => setFilter(event.target.value as "all" | RunStatus)}>
          {statuses.map((status) => (
            <option key={status} value={status}>{status === "all" ? t("allStatuses") : t(statusKeys[status])}</option>
          ))}
        </select>
      </label>
      <div className="run-list">
        {query.isLoading && <StateMessage icon={LoaderCircle} title={t("loadingRuns")} spin />}
        {query.isError && <StateMessage icon={AlertCircle} title={t("connectionError")} action={<button onClick={() => query.refetch()}>{t("retry")}</button>} />}
        {!query.isLoading && !query.isError && total === 0 && <StateMessage icon={ListChecks} title={t("noRuns")} detail={t("noRunsDetail")} />}
        {runs.map((run) => (
          <button className={selectedRunId === run.run_id ? "run-row selected" : "run-row"} key={run.run_id} onClick={() => setSelectedRunId(run.run_id)}>
            <ToolGlyph type={run.tool.tool_type} />
            <div className="run-row-main">
              <div><strong>{run.tool.name}</strong><StatusBadge status={run.status} t={t} /></div>
              <span>{shortId(run.run_id)}</span>
              <small>{run.current_node ?? run.tool_id}</small>
            </div>
            <time>{formatRelative(run.updated_at, locale)}</time>
          </button>
        ))}
      </div>
    </aside>
  );
}

function ResultWorkspace({
  t,
  selectedRun,
  tab,
  setTab,
  currentNode,
  copiedHash,
  copyHash,
}: {
  t: (key: TranslationKey) => string;
  selectedRun: Run | null;
  tab: DashboardTab;
  setTab: (tab: DashboardTab) => void;
  currentNode: string | null;
  copiedHash: string | null;
  copyHash: (hash: string) => Promise<void>;
}) {
  return (
    <section className="panel result-workspace">
      <header className="result-header">
        <div>
          <span>{selectedRun ? t("selectedRun") : t("noRunSelected")}</span>
          <strong>{selectedRun ? `${selectedRun.tool.name} · ${shortId(selectedRun.run_id)}` : t("previewWorkspace")}</strong>
        </div>
        {selectedRun && <StatusBadge status={selectedRun.status} t={t} />}
      </header>
      <nav className="tabs" aria-label="Dashboard views">
        {tabs.map((item) => (
          <button className={tab === item ? "active" : ""} key={item} onClick={() => setTab(item)}>{t(item)}</button>
        ))}
      </nav>
      <div className="preview-banner"><Zap size={15} /><strong>{t("preview")}</strong><span>{t("previewDataNotice")}</span></div>
      <div className="result-content">
        {tab === "overview" && <Overview t={t} copiedHash={copiedHash} copyHash={copyHash} setTab={setTab} />}
        {tab === "findings" && <Findings t={t} />}
        {tab === "evidence" && <Evidence t={t} />}
        {tab === "execution" && <ExecutionFlow currentNode={currentNode} t={t} />}
        {tab === "provenance" && <ProvenanceFlow t={t} />}
      </div>
    </section>
  );
}

function Overview({
  t,
  copiedHash,
  copyHash,
  setTab,
}: {
  t: (key: TranslationKey) => string;
  copiedHash: string | null;
  copyHash: (hash: string) => Promise<void>;
  setTab: (tab: DashboardTab) => void;
}) {
  return (
    <div className="overview">
      <section className="assessment">
        <div className="assessment-copy">
          <span className="eyebrow">{t("overallAssessment")} · {t("evidenceBound")}</span>
          <div className="assessment-title"><ShieldCheck size={40} /><div><h2>{t("passWithConditions")}</h2><p>{t("assessmentDetail")}</p></div></div>
        </div>
        <ScoreBlock label={t("trustScore")} value={previewPassport.score} caption={t("projected")} />
        <ScoreBlock label={t("evidenceCoverage")} value={previewPassport.coverage} caption={`${t("confidence")}: ${t("mediumConfidence")}`} />
      </section>
      <section className="dimension-section">
        <div className="section-heading"><h2>{t("auditDimensions")}</h2><span className="preview-pill">{t("preview")}</span></div>
        <div className="dimensions">
          {previewPassport.dimensions.map((dimension) => (
            <div className="dimension" key={dimension.id}>
              <span>{t(dimension.labelKey as TranslationKey)}</span>
              <strong className={scoreTone(dimension.score)}>{dimension.score}</strong>
              <Progress value={dimension.score} />
            </div>
          ))}
        </div>
      </section>
      <section className="insight-grid">
        <InsightList icon={ShieldAlert} title={t("highRiskFindings")} tone="danger" items={previewPassport.findings.slice(0, 2).map((item) => t(item.titleKey as TranslationKey))} action={t("viewAllFindings")} onAction={() => setTab("findings")} />
        <InsightList icon={CheckCircle2} title={t("supportedCapabilities")} tone="good" items={previewPassport.capabilities.map((item) => t(item as TranslationKey))} />
        <InsightList icon={AlertTriangle} title={t("unresolvedGaps")} tone="warn" items={previewPassport.gaps.map((item) => t(item as TranslationKey))} />
        <InsightList icon={Globe2} title={t("scopeLimitations")} tone="info" items={previewPassport.limitations.map((item) => t(item as TranslationKey))} />
      </section>
      <section className="hash-section">
        <div className="section-heading"><h2>{t("previewCommitments")}</h2><span className="preview-pill">{t("notAttested")}</span></div>
        <div className="hash-grid">
          {([
            ["passportHash", previewPassport.hashes.passport, Fingerprint],
            ["auditLogHash", previewPassport.hashes.auditLog, FileCheck2],
            ["evidenceManifestHash", previewPassport.hashes.evidenceManifest, Database],
          ] as Array<[TranslationKey, string, ComponentType<{ size?: number }>]>) .map(([label, value, Icon]) => (
            <div className="hash-card" key={label}>
              <Icon size={20} />
              <div><span>{t(label)}</span><strong>{value}</strong><small>{t("notAttested")}</small></div>
              <button onClick={() => copyHash(value)} title={t("copy")}>{copiedHash === value ? <Check size={15} /> : <Copy size={15} />}</button>
            </div>
          ))}
        </div>
        <p className="trust-boundary"><AlertCircle size={14} />{t("trustBoundary")}</p>
      </section>
    </div>
  );
}

function Findings({ t }: { t: (key: TranslationKey) => string }) {
  return (
    <section className="detail-view">
      <div className="section-heading"><div><span className="preview-pill">{t("preview")}</span><h2>{t("findings")}</h2><p>{t("previewFindingNotice")}</p></div></div>
      <div className="finding-list">
        {previewPassport.findings.map((finding) => (
          <article className="finding-row" key={finding.id}>
            <ShieldAlert size={20} />
            <div><div><strong>{t(finding.titleKey as TranslationKey)}</strong><Severity severity={finding.severity} t={t} /></div><p>{t(finding.detailKey as TranslationKey)}</p><small>{t("evidenceReference")}: {finding.evidence}</small></div>
          </article>
        ))}
      </div>
    </section>
  );
}

function Evidence({ t }: { t: (key: TranslationKey) => string }) {
  return (
    <section className="detail-view">
      <div className="section-heading"><div><span className="preview-pill">{t("preview")}</span><h2>{t("evidenceBoard")}</h2><p>{t("evidenceBoardDetail")}</p></div></div>
      <div className="coverage-list">
        {evidenceCoverage.map((item) => <div className="coverage-row" key={item.labelKey}><div><strong>{t(item.labelKey as TranslationKey)}</strong><span>{item.count}</span></div><Progress value={item.value} /><b>{item.value}%</b></div>)}
      </div>
      <div className="claim-table">
        <div className="claim-head"><span>{t("claim")}</span><span>{t("status")}</span><span>{t("sources")}</span></div>
        {evidenceClaims.map((item) => <div className="claim-row" key={item.claimKey}><strong>{t(item.claimKey as TranslationKey)}</strong><span className={`claim-status ${item.statusKey}`}>{t(item.statusKey as TranslationKey)}</span><b>{item.evidence}</b></div>)}
      </div>
    </section>
  );
}

function TrustInspector({
  t,
  run,
  events,
  loading,
  error,
  locale,
  openFindings,
}: {
  t: (key: TranslationKey) => string;
  run: Run | null;
  events: RunEvent[];
  loading: boolean;
  error: boolean;
  locale: Locale;
  openFindings: () => void;
}) {
  return (
    <aside className="panel inspector">
      <PanelTitle title={t("trustInspector")} authority={t("authoritative")} />
      {loading && <StateMessage icon={LoaderCircle} title={t("loadingDetails")} spin />}
      {error && <StateMessage icon={AlertCircle} title={t("connectionError")} />}
      {!loading && !error && (
        <>
          <section className="inspector-section">
            <h2>{t("runStatus")}</h2>
            {run ? (
              <dl className="run-facts">
                <Fact label={t("status")} value={<StatusBadge status={run.status} t={t} />} />
                <Fact label={t("currentNode")} value={run.current_node ?? "—"} />
                <Fact label={t("toolIdentity")} value={run.tool_id} />
                <Fact label={t("goal")} value={run.goal} />
                <Fact label={t("canonicalUrl")} value={run.canonical_url} />
                <Fact label={t("created")} value={formatDate(run.created_at, locale)} />
                <Fact label={t("updated")} value={formatDate(run.updated_at, locale)} />
              </dl>
            ) : <StateMessage icon={SquareTerminal} title={t("noRunSelected")} detail={t("noRunsDetail")} />}
          </section>
          <section className="inspector-section events-section">
            <h2>{t("latestEvents")}</h2>
            <p>{t("eventLogNotice")}</p>
            {events.length === 0 ? <div className="empty-events">{t("noEvents")}</div> : (
              <ol className="event-list">
                {[...events].reverse().slice(0, 6).map((event) => (
                  <li key={event.event_id}><span className="event-dot" /><time>{formatTime(event.created_at, locale)}</time><div><strong>{t(eventKeys[event.event_type])}</strong><small>{event.node_id}</small></div></li>
                ))}
              </ol>
            )}
          </section>
          <section className="inspector-section review-boundary">
            <h2><ShieldCheck size={16} />{t("humanReviewBoundary")}</h2>
            <p>{run?.status === "waiting_approval" ? t("humanReviewRequired") : t("noHumanReview")}</p>
            <button onClick={openFindings}>{t("openFindings")}<ChevronRight size={15} /></button>
          </section>
        </>
      )}
    </aside>
  );
}

function ActivityTicker({ t, events, autoRefresh, locale }: { t: (key: TranslationKey) => string; events: RunEvent[]; autoRefresh: boolean; locale: Locale }) {
  return (
    <footer className="activity-ticker">
      <div className="ticker-label"><span className="live-dot" />{t("liveActivity")}</div>
      <div className="ticker-events">
        {[...events].reverse().slice(0, 3).map((event) => <span key={event.event_id}><time>{formatTime(event.created_at, locale)}</time>{t(eventKeys[event.event_type])}: {event.node_id}</span>)}
        {events.length === 0 && <span>{t("noEvents")}</span>}
      </div>
      <div className="ticker-refresh"><Activity size={14} />{t("autoRefresh")} {autoRefresh ? t("on") : t("off")}</div>
    </footer>
  );
}

function PanelTitle({ title, badge, authority }: { title: string; badge?: string; authority?: string }) {
  return <div className="panel-title"><div><h2>{title}</h2>{badge && <b>{badge}</b>}</div>{authority && <span>{authority}</span>}</div>;
}

function StateMessage({ icon: Icon, title, detail, action, spin }: { icon: ComponentType<{ size?: number; className?: string }>; title: string; detail?: string; action?: React.ReactNode; spin?: boolean }) {
  return <div className="state-message"><Icon size={22} className={spin ? "spin" : ""} /><strong>{title}</strong>{detail && <p>{detail}</p>}{action}</div>;
}

function ToolGlyph({ type }: { type: string }) {
  const Icon = type === "agent_framework" ? Network : type === "mcp_server" ? Server : type === "cli_api_tool" ? SquareTerminal : Boxes;
  return <span className="tool-glyph"><Icon size={18} /></span>;
}

function StatusBadge({ status, t }: { status: RunStatus; t: (key: TranslationKey) => string }) {
  return <span className={`status-badge ${status}`}><span />{t(statusKeys[status])}</span>;
}

function ScoreBlock({ label, value, caption }: { label: string; value: number; caption: string }) {
  return <div className="score-block"><span>{label}</span><strong>{value}<small>/100</small></strong><Progress value={value} /><p>{caption}</p></div>;
}

function Progress({ value }: { value: number }) {
  return <div className="progress-track" aria-label={`${value}%`}><span style={{ "--progress": `${value}%` } as CSSProperties} /></div>;
}

function InsightList({ icon: Icon, title, items, tone, action, onAction }: { icon: ComponentType<{ size?: number }>; title: string; items: string[]; tone: string; action?: string; onAction?: () => void }) {
  return <article className={`insight ${tone}`}><h3><Icon size={16} />{title}</h3><ul>{items.slice(0, 5).map((item) => <li key={item}><CheckCircle2 size={13} />{item}</li>)}</ul>{action && <button onClick={onAction}>{action}<ChevronRight size={14} /></button>}</article>;
}

function Severity({ severity, t }: { severity: "critical" | "high" | "medium" | "low"; t: (key: TranslationKey) => string }) {
  return <span className={`severity ${severity}`}>{t(severity)}</span>;
}

function Fact({ label, value }: { label: string; value: React.ReactNode }) {
  return <div><dt>{label}</dt><dd>{value}</dd></div>;
}

function shortId(value: string): string {
  return value.length > 16 ? `${value.slice(0, 8)}…${value.slice(-5)}` : value;
}

function formatDate(value: string, locale: Locale): string {
  return new Intl.DateTimeFormat(locale, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }).format(new Date(value));
}

function formatTime(value: string, locale: Locale): string {
  return new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit", second: "2-digit" }).format(new Date(value));
}

function formatRelative(value: string, locale: Locale): string {
  const minutes = Math.max(0, Math.round((Date.now() - new Date(value).getTime()) / 60_000));
  if (minutes < 1) return locale === "zh-CN" ? "刚刚" : "now";
  if (minutes < 60) return locale === "zh-CN" ? `${minutes} 分钟` : `${minutes}m`;
  const hours = Math.round(minutes / 60);
  return locale === "zh-CN" ? `${hours} 小时` : `${hours}h`;
}

function scoreTone(score: number): string {
  if (score >= 75) return "text-good";
  if (score >= 60) return "text-warn";
  return "text-danger";
}
