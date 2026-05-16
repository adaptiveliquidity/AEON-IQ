const BACKEND  = process.env.BACKEND_URL     ?? "http://localhost:8080";
const MGMT_KEY = process.env.MANAGEMENT_API_KEY ?? "";

export function backendUrl(path: string): string {
  return `${BACKEND}${path}`;
}

export function mgmtHeaders(extra: Record<string, string> = {}): Record<string, string> {
  return {
    ...(MGMT_KEY ? { "x-management-key": MGMT_KEY } : {}),
    "Content-Type": "application/json",
    ...extra,
  };
}
