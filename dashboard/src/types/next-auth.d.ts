import type { DefaultSession } from "next-auth";

declare module "next-auth" {
  interface Session {
    user: {
      agentId: string;
      isAdmin: boolean;
    } & DefaultSession["user"];
  }
}

declare module "next-auth/jwt" {
  interface JWT {
    agentId?: string;
    isAdmin?: boolean;
  }
}
