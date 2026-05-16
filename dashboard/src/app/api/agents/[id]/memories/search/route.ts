import { NextRequest, NextResponse } from "next/server";
import { auth } from "@/auth";
import { backendUrl, mgmtHeaders } from "@/lib/backend";

type Ctx = { params: Promise<{ id: string }> };

export async function POST(req: NextRequest, { params }: Ctx) {
  const session = await auth();
  if (!session) return NextResponse.json({ error: "Unauthorized" }, { status: 401 });

  const { id } = await params;
  if (!session.user.isAdmin && id !== session.user.agentId) {
    return NextResponse.json({ error: "Forbidden" }, { status: 403 });
  }

  const body = await req.json();
  try {
    const res = await fetch(backendUrl("/api/v1/memories/search"), {
      method: "POST",
      headers: mgmtHeaders(),
      body: JSON.stringify({ agent_id: id, ...body }),
    });
    return NextResponse.json(await res.json(), { status: res.status });
  } catch (err) {
    return NextResponse.json({ error: String(err) }, { status: 502 });
  }
}
