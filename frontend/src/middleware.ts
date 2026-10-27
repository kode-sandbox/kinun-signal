import { NextRequest, NextResponse } from "next/server";

function getRuntimeEnv(keyParts: string[]): string | undefined {
  const key = keyParts.join("_");
  return process.env[key];
}

function timingSafeEqualStr(a: string, b: string): boolean {
  const encoder = new TextEncoder();
  const aBytes = encoder.encode(a);
  const bBytes = encoder.encode(b);
  if (aBytes.length !== bBytes.length) {
    return false;
  }
  let diff = 0;
  for (let i = 0; i < aBytes.length; i++) {
    diff |= aBytes[i] ^ bBytes[i];
  }
  return diff === 0;
}

function verifyBasicAuth(
  authHeader: string | null,
  expectedUser: string,
  expectedPass: string
): boolean {
  if (!authHeader || !authHeader.startsWith("Basic ")) {
    return false;
  }
  const base64Credentials = authHeader.slice(6).trim();
  try {
    const decoded = atob(base64Credentials);
    const colonIndex = decoded.indexOf(":");
    if (colonIndex === -1) {
      return false;
    }
    const inputUser = decoded.slice(0, colonIndex);
    const inputPass = decoded.slice(colonIndex + 1);
    const userMatch = timingSafeEqualStr(inputUser, expectedUser);
    const passMatch = timingSafeEqualStr(inputPass, expectedPass);
    return userMatch && passMatch;
  } catch {
    return false;
  }
}

export function middleware(req: NextRequest) {
  const expectedUser = getRuntimeEnv(["BASIC", "AUTH", "USER"]);
  const expectedPass = getRuntimeEnv(["BASIC", "AUTH", "PASSWORD"]);

  // If BASIC_AUTH_USER and BASIC_AUTH_PASSWORD are set, enforce Basic Authentication
  if (expectedUser && expectedPass) {
    const authHeader = req.headers.get("authorization");
    if (!verifyBasicAuth(authHeader, expectedUser, expectedPass)) {
      return new NextResponse("Authentication Required", {
        status: 401,
        headers: {
          "WWW-Authenticate":
            'Basic realm="Personal Stock Analyst", charset="UTF-8"',
          "X-Robots-Tag": "noindex, nofollow, noarchive",
          "Cache-Control": "no-store",
        },
      });
    }
  }

  const res = NextResponse.next();
  res.headers.set("X-Robots-Tag", "noindex, nofollow, noarchive");
  return res;
}

export const config = {
  matcher: ["/((?!robots\\.txt$).*)"],
};
