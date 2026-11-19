import { readFile } from "node:fs/promises";
import { join } from "node:path";

import { ImageResponse } from "next/og";

import { site } from "@/lib/site";

export const alt = "Tributary: a download manager for Windows";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";
export const dynamic = "force-static";

// Brand colours from SPEC section 2 (the dark theme). Satori can't read CSS variables.
const riverbed = "#05161B";
const current = "#2FC4B2";
const estuary = "#4C9BD1";
const foam = "#E4F1F0";
const silt = "#8FA9AC";

// OFL fonts kept in the repository (licences beside them), so building never asks Google.
const fonts = join(process.cwd(), "components", "pages", "og");

export default async function OpenGraphImage() {
  const [display, text] = await Promise.all([
    readFile(join(fonts, "BricolageGrotesque-Bold.ttf")),
    readFile(join(fonts, "PublicSans-Medium.ttf")),
  ]);

  return new ImageResponse(
    (
      <div style={{ width: "100%", height: "100%", display: "flex", position: "relative", background: riverbed }}>
        {/* The confluence at full size: two streams join one channel that ends in the download. */}
        <svg
          width="700"
          height="700"
          viewBox="0 0 1024 1024"
          style={{ position: "absolute", right: -70, top: -30 }}
        >
          <g fill="none" strokeLinecap="round" strokeLinejoin="round" strokeWidth="46">
            <path d="M250 212 C 250 420, 512 420, 512 600" stroke={estuary} strokeOpacity="0.7" />
            <path d="M774 212 C 774 420, 512 420, 512 600" stroke={current} strokeOpacity="0.55" />
            <path d="M512 200 L 512 640" stroke={current} />
            <path d="M336 560 L 512 760 L 688 560" stroke={current} />
          </g>
          <rect x="300" y="818" width="424" height="40" rx="20" fill={current} />
        </svg>

        <div
          style={{
            display: "flex",
            flexDirection: "column",
            justifyContent: "space-between",
            width: "100%",
            padding: "76px 84px",
          }}
        >
          <svg width="104" height="104" viewBox="0 0 1024 1024">
            <defs>
              <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
                <stop offset="0" stopColor="#0f766e" />
                <stop offset="1" stopColor="#134e6f" />
              </linearGradient>
            </defs>
            <rect x="48" y="48" width="928" height="928" rx="208" fill="url(#bg)" />
            <g fill="none" stroke="#ffffff" strokeLinecap="round" strokeWidth="72">
              <path d="M250 212 C 250 420, 512 420, 512 600" strokeOpacity="0.72" />
              <path d="M774 212 C 774 420, 512 420, 512 600" strokeOpacity="0.72" />
              <path d="M512 200 L 512 640" />
              <path d="M336 560 L 512 760 L 688 560" strokeLinejoin="round" />
            </g>
            <rect x="300" y="818" width="424" height="56" rx="28" fill="#ffffff" />
          </svg>

          <div style={{ display: "flex", flexDirection: "column" }}>
            <div
              style={{
                fontFamily: "Bricolage Grotesque",
                fontSize: 136,
                lineHeight: 1,
                letterSpacing: "-0.04em",
                color: foam,
              }}
            >
              {site.name}
            </div>
            <div style={{ fontFamily: "Public Sans", fontSize: 44, marginTop: 22, color: silt }}>{site.tagline}</div>
          </div>
        </div>
      </div>
    ),
    {
      ...size,
      fonts: [
        { name: "Bricolage Grotesque", data: display, weight: 700, style: "normal" },
        { name: "Public Sans", data: text, weight: 500, style: "normal" },
      ],
    },
  );
}
