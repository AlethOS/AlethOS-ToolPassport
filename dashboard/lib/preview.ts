import type { PreviewPassportResult } from "@/lib/types";

export const previewPassport: PreviewPassportResult = {
  kind: "preview",
  score: 72,
  coverage: 68,
  confidenceKey: "mediumConfidence",
  assessmentKey: "passWithConditions",
  dimensions: [
    { id: "capability", score: 80, labelKey: "capabilityClarity" },
    { id: "interface", score: 68, labelKey: "interfaceOpenness" },
    { id: "automation", score: 75, labelKey: "automationReadiness" },
    { id: "portability", score: 63, labelKey: "dataPortability" },
    { id: "permission", score: 58, labelKey: "permissionRisk" },
    { id: "evidence", score: 70, labelKey: "evidenceQuality" },
    { id: "ecosystem", score: 74, labelKey: "ecosystemFit" },
  ],
  findings: [
    {
      id: "preview-finding-1",
      titleKey: "unverifiedEndpoint",
      detailKey: "unverifiedEndpointDetail",
      severity: "high",
      evidence: "docs/integrations.md",
    },
    {
      id: "preview-finding-2",
      titleKey: "retentionPolicyGap",
      detailKey: "retentionPolicyGapDetail",
      severity: "high",
      evidence: "policy/retention.md",
    },
    {
      id: "preview-finding-3",
      titleKey: "modelCardGap",
      detailKey: "modelCardGapDetail",
      severity: "medium",
      evidence: "docs/model-card.md",
    },
  ],
  capabilities: ["roleBasedAccess", "inputValidation", "secretsManagement", "structuredOutput", "auditLogging"],
  gaps: ["incompleteModelCard", "missingSbom", "unclearTrainingData"],
  limitations: ["versionScoped", "publicSourcesOnly", "runtimeNotExecuted"],
  hashes: {
    passport: "preview:passport:7d1a…c914",
    auditLog: "preview:audit-log:6b42…0fd8",
    evidenceManifest: "preview:evidence:19ce…a7b3",
  },
};

export const evidenceCoverage = [
  { labelKey: "officialDocs", value: 82, count: "23 / 28" },
  { labelKey: "repositoryEvidence", value: 71, count: "17 / 24" },
  { labelKey: "publicExamples", value: 54, count: "7 / 13" },
  { labelKey: "counterEvidence", value: 46, count: "6 / 13" },
];

export const evidenceClaims = [
  { claimKey: "structuredOutputClaim", statusKey: "supported", evidence: 4 },
  { claimKey: "stateRecoveryClaim", statusKey: "partiallySupported", evidence: 3 },
  { claimKey: "permissionIsolationClaim", statusKey: "unsupported", evidence: 1 },
  { claimKey: "dataExportClaim", statusKey: "notChecked", evidence: 0 },
];
