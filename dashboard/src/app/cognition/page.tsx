import { auth } from "@/auth";
import CognitionClient from "./client";

export default async function CognitionPage() {
  const session = await auth();
  return (
    <CognitionClient
      userAgentId={session?.user?.agentId ?? ""}
      isAdmin={session?.user?.isAdmin ?? false}
    />
  );
}
