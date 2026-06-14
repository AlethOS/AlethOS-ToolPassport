import { proxyPost } from "@/app/api/trust-core/proxy";

export async function POST(request: Request) {
  const body = await request.json();
  return proxyPost("/api/tools", body);
}
