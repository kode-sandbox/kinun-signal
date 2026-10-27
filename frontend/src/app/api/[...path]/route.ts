import { NextRequest, NextResponse } from "next/server";

export const dynamic = "force-dynamic";

let cachedIdToken: {
  token: string;
  expiresAt: number;
  audience: string;
} | null = null;

function getBackendBaseUrl(): string {
  const raw =
    process.env.BACKEND_INTERNAL_URL ||
    process.env.NEXT_PUBLIC_API_URL ||
    "http://localhost:8080";
  return raw.replace(/\/+$/, "");
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

function verifyRequestBasicAuth(req: NextRequest): boolean {
  const expectedUser = process.env.BASIC_AUTH_USER;
  const expectedPass = process.env.BASIC_AUTH_PASSWORD;
  if (!expectedUser || !expectedPass) {
    return true;
  }

  const authHeader = req.headers.get("authorization");
  if (!authHeader || !authHeader.startsWith("Basic ")) {
    return false;
  }

  try {
    const base64Credentials = authHeader.slice(6).trim();
    const decoded = Buffer.from(base64Credentials, "base64").toString("utf-8");
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

/**
 * Fetches an OIDC ID Token from the Google Cloud Run Metadata Server
 * so that the Frontend Service Account (`kinun-signal-frontend-sa`) can
 * invoke the private Backend Cloud Run service (`roles/run.invoker`).
 * Falls back to `null` when running locally outside of Google Cloud.
 */
async function getGcpIdToken(audience: string): Promise<string | null> {
  if (!audience.startsWith("https://")) {
    return null;
  }

  const now = Date.now();
  if (
    cachedIdToken &&
    cachedIdToken.audience === audience &&
    cachedIdToken.expiresAt > now
  ) {
    return cachedIdToken.token;
  }

  try {
    const metadataUrl = `http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/identity?audience=${encodeURIComponent(
      audience
    )}`;
    const res = await fetch(metadataUrl, {
      headers: { "Metadata-Flavor": "Google" },
      cache: "no-store",
    });
    if (!res.ok) {
      return null;
    }
    const token = (await res.text()).trim();
    if (token) {
      // Cache for 50 minutes (Google OIDC ID tokens expire after 60 minutes)
      cachedIdToken = {
        token,
        audience,
        expiresAt: now + 50 * 60 * 1000,
      };
      return token;
    }
    return null;
  } catch {
    return null;
  }
}

async function proxyRequest(
  req: NextRequest,
  context: { params: Promise<{ path: string[] }> }
): Promise<NextResponse> {
  if (!verifyRequestBasicAuth(req)) {
    return new NextResponse(
      JSON.stringify({ error: "Authentication Required" }),
      {
        status: 401,
        headers: {
          "content-type": "application/json",
          "WWW-Authenticate":
            'Basic realm="Personal Stock Analyst", charset="UTF-8"',
          "X-Robots-Tag": "noindex, nofollow, noarchive",
        },
      }
    );
  }

  const { path } = await context.params;
  const subPath = path.join("/");
  const search = req.nextUrl.search || "";
  const backendBaseUrl = getBackendBaseUrl();
  const targetUrl = `${backendBaseUrl}/api/${subPath}${search}`;

  const headers = new Headers();
  const contentType = req.headers.get("content-type");
  if (contentType) {
    headers.set("content-type", contentType);
  }

  // Attach Google Cloud Run Service Account OIDC token for service-to-service IAM auth
  const idToken = await getGcpIdToken(backendBaseUrl);
  if (idToken) {
    headers.set("Authorization", `Bearer ${idToken}`);
  }

  const init: RequestInit = {
    method: req.method,
    headers,
    cache: "no-store",
  };

  if (req.method !== "GET" && req.method !== "HEAD") {
    const bodyText = await req.text();
    if (bodyText) {
      init.body = bodyText;
    }
  }

  try {
    const upstream = await fetch(targetUrl, init);
    const responseBody = await upstream.text();
    return new NextResponse(responseBody, {
      status: upstream.status,
      headers: {
        "content-type":
          upstream.headers.get("content-type") || "application/json",
        "X-Robots-Tag": "noindex, nofollow, noarchive",
      },
    });
  } catch (error) {
    return NextResponse.json(
      {
        error: "Failed to reach backend service",
        details: error instanceof Error ? error.message : String(error),
      },
      { status: 502 }
    );
  }
}

export async function GET(
  req: NextRequest,
  context: { params: Promise<{ path: string[] }> }
) {
  return proxyRequest(req, context);
}

export async function POST(
  req: NextRequest,
  context: { params: Promise<{ path: string[] }> }
) {
  return proxyRequest(req, context);
}

export async function DELETE(
  req: NextRequest,
  context: { params: Promise<{ path: string[] }> }
) {
  return proxyRequest(req, context);
}
