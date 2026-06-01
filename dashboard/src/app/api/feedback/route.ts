import { NextRequest, NextResponse } from "next/server";
import { auth } from "@/auth";
import { backendUrl, mgmtHeaders } from "@/lib/backend";

export async function POST(req: NextRequest) {
  const session = await auth();
  if (!session) return NextResponse.json({ error: "Unauthorized" }, { status: 401 });

  const body = await req.json();
  try {
    const res = await fetch(backendUrl("/api/v1/feedback"), {
      method: "POST",
      headers: mgmtHeaders(),
      body: JSON.stringify(body),
    });
    return NextResponse.json(await res.json(), { status: res.status });
  } catch (err) {
    return NextResponse.json({ error: String(err) }, { status: 502 });
  }
}
