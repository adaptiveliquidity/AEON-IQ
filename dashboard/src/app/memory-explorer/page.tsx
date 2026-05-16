import { auth } from "@/auth";
import MemoryExplorerClient from "./client";

export default async function MemoryExplorerPage() {
  const session = await auth();
  return (
    <MemoryExplorerClient
      userAgentId={session?.user?.agentId ?? ""}
      isAdmin={session?.user?.isAdmin ?? false}
    />
  );
}
