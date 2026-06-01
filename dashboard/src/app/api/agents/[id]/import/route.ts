import { NextRequest, NextResponse } from "next/server";
import { auth } from "@/auth";
import { backendUrl, mgmtHeaders } from "@/lib/backend";

type Ctx = { params: Promise<{ id: string }> };

function forbidden() {
  return NextResponse.json({ error: "Forbidden" }, { status: 403 });
}

export async function POST(req: NextRequest, { params }: Ctx) {
  const session = await auth();
  if (!session) return NextResponse.json({ error: "Unauthorized" }, { status: 401 });

  const { id } = await params;
  if (!session.user.isAdmin && id !== session.user.agentId) return forbidden();

  const body = await req.text();
  try {
    const res = await fetch(
      backendUrl(`/api/v1/agents/${encodeURIComponent(id)}/import`),
      {
        method: "POST",
        headers: mgmtHeaders({ "Content-Type": "application/x-ndjson" }),
        body,
      }
    );
    return NextResponse.json(await res.json(), { status: res.status });
  } catch (err) {
    return NextResponse.json({ error: String(err) }, { status: 502 });
  }
}
