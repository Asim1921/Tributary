/**
 * Full-page screenshots of every page, for reviewing the design: 1440x900 and 390x844, in the
 * dark and the light theme. Uses the installed Google Chrome (no browser download).
 *
 *   node scripts/screenshots.mjs                          # the stack at http://localhost:8080
 *   $env:BASE_URL='http://localhost:3103'; node scripts/screenshots.mjs
 *
 * Options (environment):
 *   BASE_URL        where the site runs (default http://localhost:8080)
 *   OUT_DIR         where the PNGs go (default ../artifacts/screens, beside frontend/)
 *   ROUTES          comma-separated paths (default /,/download,/changelog,/privacy)
 *   REDUCED_MOTION  1 to emulate prefers-reduced-motion
 *   FULL_PAGE       0 for the first screen only
 *
 * Exits with 1 if a page fails to load or logs a console error.
 */
import { mkdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { chromium } from "@playwright/test";

const here = path.dirname(fileURLToPath(import.meta.url));
const baseUrl = (process.env.BASE_URL || "http://localhost:8080").replace(/\/+$/, "");
const outDir = path.resolve(process.env.OUT_DIR || path.join(here, "..", "..", "artifacts", "screens"));
const routes = (process.env.ROUTES || "/,/download,/changelog,/privacy")
  .split(",")
  .map((route) => route.trim())
  .filter(Boolean);
const viewports = [
  { name: "1440", width: 1440, height: 900, mobile: false },
  { name: "390", width: 390, height: 844, mobile: true },
];
const themes = ["dark", "light"];
const fullPage = process.env.FULL_PAGE !== "0";
const reducedMotion = process.env.REDUCED_MOTION === "1" ? "reduce" : "no-preference";

const slug = (route) => (route === "/" ? "home" : route.replace(/^\/+/, "").replace(/[^a-z0-9]+/gi, "-"));

/** Scrolls to the bottom a screen at a time (so on-scroll effects and lazy media run), then back. */
async function scrollThrough(page) {
  await page.evaluate(async () => {
    const step = Math.max(200, Math.floor(window.innerHeight * 0.8));
    for (let y = 0; y < document.documentElement.scrollHeight; y += step) {
      window.scrollTo(0, y);
      await new Promise((resolve) => setTimeout(resolve, 60));
    }
    window.scrollTo(0, 0);
  });
}

await mkdir(outDir, { recursive: true });
const browser = await chromium.launch({ channel: "chrome" });
let problems = 0;

try {
  for (const theme of themes) {
    for (const viewport of viewports) {
      const context = await browser.newContext({
        viewport: { width: viewport.width, height: viewport.height },
        deviceScaleFactor: 1,
        isMobile: viewport.mobile,
        hasTouch: viewport.mobile,
        colorScheme: theme,
        reducedMotion,
      });
      // The site's own toggle reads this before the first paint.
      await context.addInitScript((value) => {
        try {
          localStorage.setItem("tributary-theme", value);
        } catch {
          // Storage refused: the color scheme emulation still applies.
        }
      }, theme);
      const page = await context.newPage();
      const errors = [];
      page.on("console", (message) => {
        if (message.type() === "error") errors.push(message.text());
      });
      page.on("pageerror", (error) => errors.push(error.message));

      for (const route of routes) {
        errors.length = 0;
        const file = path.join(outDir, `${slug(route)}-${viewport.name}-${theme}.png`);
        const response = await page.goto(baseUrl + route, { waitUntil: "networkidle" });
        await page.evaluate(() => document.fonts.ready);
        await scrollThrough(page);
        await page.waitForTimeout(700);
        await page.screenshot({ path: file, fullPage });
        const status = response?.status() ?? 0;
        console.log(`${status} ${route} ${viewport.name} ${theme} -> ${file}`);
        if (status >= 400) problems += 1;
        for (const error of errors) {
          problems += 1;
          console.error(`  console error on ${route}: ${error}`);
        }
      }
      await context.close();
    }
  }
} finally {
  await browser.close();
}

if (problems > 0) {
  console.error(`${problems} problem(s); see above.`);
  process.exitCode = 1;
}
