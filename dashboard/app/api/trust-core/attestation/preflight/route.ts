import { proxyTrustCore } from "@/app/api/trust-core/proxy";

export async function GET() {
  return proxyTrustCore("/api/attestation/preflight");
}
