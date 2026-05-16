import NextAuth from "next-auth";
import Credentials from "next-auth/providers/credentials";

function emailToAgentId(email: string): string {
  return email.toLowerCase().replace(/[@.+]/g, "-").replace(/-{2,}/g, "-").replace(/^-|-$/g, "");
}

function isAdminEmail(email: string): boolean {
  const raw = process.env.DASHBOARD_ADMINS ?? process.env.DASHBOARD_ADMIN_EMAIL ?? "";
  return raw
    .split(",")
    .map((e) => e.trim().toLowerCase())
    .filter(Boolean)
    .includes(email.toLowerCase());
}

interface StoredUser {
  email: string;
  password: string;
  name?: string;
}

function findUser(email: string, password: string): StoredUser | null {
  // Try DASHBOARD_USERS JSON array first: '[{"email":"...","password":"..."}]'
  const usersJson = process.env.DASHBOARD_USERS;
  if (usersJson) {
    try {
      const users = JSON.parse(usersJson) as StoredUser[];
      return users.find((u) => u.email === email && u.password === password) ?? null;
    } catch {
      // fall through to single-admin check
    }
  }
  // Single admin via DASHBOARD_ADMIN_EMAIL / DASHBOARD_ADMIN_PASSWORD
  const adminEmail = process.env.DASHBOARD_ADMIN_EMAIL ?? "admin@memoryos.dev";
  const adminPass = process.env.DASHBOARD_ADMIN_PASSWORD ?? "changeme";
  if (email === adminEmail && password === adminPass) {
    return { email: adminEmail, password: adminPass, name: "Admin" };
  }
  return null;
}

export const { handlers, auth, signIn, signOut } = NextAuth({
  providers: [
    Credentials({
      credentials: {
        email:    { label: "Email",    type: "email"    },
        password: { label: "Password", type: "password" },
      },
      authorize: async (credentials) => {
        const email    = (credentials?.email    as string | undefined) ?? "";
        const password = (credentials?.password as string | undefined) ?? "";
        const user = findUser(email, password);
        if (!user) return null;
        return { id: user.email, email: user.email, name: user.name ?? user.email.split("@")[0] };
      },
    }),
  ],

  callbacks: {
    jwt: async ({ token, user }) => {
      if (user?.email) {
        token.agentId  = emailToAgentId(user.email);
        token.isAdmin  = isAdminEmail(user.email);
      }
      return token;
    },
    session: async ({ session, token }) => {
      if (session.user) {
        session.user.agentId = (token.agentId as string | undefined) ?? "";
        session.user.isAdmin = (token.isAdmin as boolean | undefined) ?? false;
      }
      return session;
    },
  },

  pages: { signIn: "/login" },

  session: { strategy: "jwt" },
});
