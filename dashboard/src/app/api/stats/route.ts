import { NextResponse } from "next/server";
import { auth } from "@/auth";
import { backendUrl, mgmtHeaders } from "@/lib/backend";

export async function GET() {
  const session = await auth();
  if (!session) return NextResponse.json({ error: "Unauthorized" }, { status: 401 });

  try {
    const res = await fetch(backendUrl("/api/v1/stats"), {
      cache: "no-store",
      headers: mgmtHeaders(),
    });
    const data = await res.json();
    return NextResponse.json(data, { status: res.status });
  } catch (err) {
    return NextResponse.json({ error: String(err) }, { status: 502 });
  }
}
