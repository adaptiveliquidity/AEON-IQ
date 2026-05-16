import { auth } from "@/auth";
import { NextResponse } from "next/server";

export default auth((req) => {
  const isLoggedIn  = !!req.auth;
  const isLoginPage = req.nextUrl.pathname.startsWith("/login");
  const isAuthApi   = req.nextUrl.pathname.startsWith("/api/auth");

  if (isAuthApi) return NextResponse.next();
  if (!isLoggedIn && !isLoginPage) {
    const loginUrl = new URL("/login", req.url);
    loginUrl.searchParams.set("callbackUrl", req.nextUrl.pathname);
    return NextResponse.redirect(loginUrl);
  }
  if (isLoggedIn && isLoginPage) {
    return NextResponse.redirect(new URL("/", req.url));
  }
  return NextResponse.next();
});

export const config = {
  matcher: ["/((?!_next/static|_next/image|favicon.ico).*)"],
};
