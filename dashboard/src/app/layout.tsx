import type { Metadata } from "next";
import "./globals.css";
import Nav from "@/components/nav";
import { auth } from "@/auth";
import { SessionProvider } from "next-auth/react";

export const metadata: Metadata = {
  title: "MemoryOS Kernel",
  description: "MMU Dashboard — memory explorer & cost analytics",
};

export default async function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const session = await auth();

  return (
    <html lang="en" className="dark">
      <body className="bg-zinc-950 text-zinc-100 min-h-screen antialiased">
        <SessionProvider session={session}>
          <Nav />
          <main className="max-w-6xl mx-auto px-4 py-8">{children}</main>
        </SessionProvider>
      </body>
    </html>
  );
}
