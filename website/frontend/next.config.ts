import path from "node:path";

import type { NextConfig } from "next";

// On Vercel there is no nginx: the platform serves the build, so the standalone server output is
// skipped there and the security headers nginx normally adds are added here instead.
const onVercel = Boolean(process.env.VERCEL);

// The API project's https URL (Vercel). Unset under docker compose, where nginx routes /api.
const apiOrigin = (process.env.API_ORIGIN || "").trim().replace(/\/+$/, "");

// Same policy as nginx/snippets/security-headers.conf.
const securityHeaders = [
  { key: "X-Content-Type-Options", value: "nosniff" },
  { key: "Referrer-Policy", value: "strict-origin-when-cross-origin" },
  { key: "Permissions-Policy", value: "camera=(), microphone=(), geolocation=()" },
  { key: "X-Frame-Options", value: "DENY" },
  {
    key: "Content-Security-Policy",
    value:
      "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self'; font-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'",
  },
];

const nextConfig: NextConfig = {
  // The container runs the standalone server behind nginx.
  ...(onVercel ? {} : { output: "standalone" as const }),
  // Builders previewing at the same time each use their own output folder (see CONTRACT.md).
  distDir: process.env.NEXT_DIST_DIR || ".next",
  poweredByHeader: false,
  reactStrictMode: true,
  // The media is already sized and encoded by scripts/make-media.mjs.
  images: { unoptimized: true },
  // This app is its own workspace, though the repository above has a package-lock.json too.
  turbopack: { root: path.join(__dirname) },
  outputFileTracingRoot: path.join(__dirname),
  async rewrites() {
    if (!apiOrigin) return [];
    return [{ source: "/api/:path*", destination: `${apiOrigin}/api/:path*` }];
  },
  async headers() {
    if (!onVercel) return [];
    return [{ source: "/:path*", headers: securityHeaders }];
  },
};

export default nextConfig;
