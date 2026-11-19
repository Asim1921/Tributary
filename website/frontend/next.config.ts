import path from "node:path";

import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // The container runs the standalone server behind nginx.
  output: "standalone",
  // Builders previewing at the same time each use their own output folder (see CONTRACT.md).
  distDir: process.env.NEXT_DIST_DIR || ".next",
  poweredByHeader: false,
  reactStrictMode: true,
  // The media is already sized and encoded by scripts/make-media.mjs.
  images: { unoptimized: true },
  // This app is its own workspace, though the repository above has a package-lock.json too.
  turbopack: { root: path.join(__dirname) },
  outputFileTracingRoot: path.join(__dirname),
};

export default nextConfig;
