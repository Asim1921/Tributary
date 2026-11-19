/** Facts about Tributary that every page shares. Keep them true (SPEC section 4). */
export const site = {
  name: "Tributary",
  tagline: "A download manager for Windows",
  description:
    "Tributary splits each download across several connections, resumes after a crash, takes over downloads from Chrome and Edge, and saves video from the pages you watch. Free, for Windows.",
  url: process.env.NEXT_PUBLIC_SITE_URL || "http://localhost:8080",
  /** The API counts the download and nginx hands over the installer. */
  downloadPath: "/api/download/latest",
} as const;

export const facts = {
  fallbackVersion: "0.1.0",
  fallbackFile: "Tributary_0.1.0_x64-setup.exe",
  requirements: [
    "Windows 10 or 11, 64-bit",
    "Installs for your user only, so it needs no administrator rights",
    "Microsoft Edge WebView2, which is part of Windows 11; the installer fetches it if it's missing",
  ],
  builtWith: ["Rust", "Tauri", "Svelte"],
  /** Accurate on 2026-11-19. */
  proof: {
    rustTests: 609,
    interfaceTests: 189,
    endToEndSteps: 66,
    browsers: ["Chrome", "Edge"],
    checkedOn: "2026-11-19",
  },
  history: [
    {
      date: "2026-11-06",
      title: "The download engine",
      detail:
        "Downloads split across several connections, with resume, and the test server that drops connections and changes files mid-download to prove it. The desktop app and its installer.",
    },
    {
      date: "2026-11-07",
      title: "Everything around the downloads",
      detail:
        "Queues, a scheduler, speed limits and categories. FTP, server logins and proxies. Taking over downloads from the browser. Saving HLS and DASH video. The site grabber. Passwords kept off the disk. Tested on real Chrome.",
    },
    {
      date: "2026-11-12",
      title: "Video from the page you're watching",
      detail:
        "A download button beside any video, YouTube included. The right video when it was opened from a playlist, and qualities named the way video sites name them.",
    },
    {
      date: "2026-11-18",
      title: "More sites, and one file per video",
      detail:
        "Players embedded from other sites, and sites that offer only HLS. Picture and sound arrive as one item and one file.",
    },
    {
      date: "2026-11-19",
      title: "Every picture and video on a page",
      detail:
        "Tributary scrolls a page as you would, gathers what it shows at the largest size offered, and saves it all with one click.",
    },
  ],
} as const;
