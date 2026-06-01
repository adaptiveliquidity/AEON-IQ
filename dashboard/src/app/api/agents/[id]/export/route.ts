import { NextRequest, NextResponse } from "next/server";
import { auth } from "@/auth";
import { backendUrl, mgmtHeaders } from "@/lib/backend";

type Ctx = { params: Promise<{ id: string }> };

function forbidden() {
  return NextResponse.json({ error: "Forbidden" }, { status: 403 });
}

export async function GET(_req: NextRequest, { params }: Ctx) {
  const session = await auth();
  if (!session) return NextResponse.json({ error: "Unauthorized" }, { status: 401 });

  const { id } = await params;
  if (!session.user.isAdmin && id !== session.user.agentId) return forbidden();

  try {
    const res = await fetch(
      backendUrl(`/api/v1/agents/${encodeURIComponent(id)}/export`),
      { cache: "no-store", headers: mgmtHeaders() }
    );
    const text = await res.text();
    return new NextResponse(text, {
      status: res.status,
      headers: {
        "Content-Type": res.headers.get("Content-Type") ?? "application/x-ndjson",
      },
    });
  } catch (err) {
    return NextResponse.json({ error: String(err) }, { status: 502 });
  }
}
