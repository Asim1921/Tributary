import { createHash } from "node:crypto";

import { expect, test, type Page } from "@playwright/test";

const pages = ["/", "/download", "/changelog", "/privacy", "/no-such-page"];

/** Collects console errors and uncaught page errors while a page loads. */
function watchErrors(page: Page) {
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  page.on("pageerror", (error) => errors.push(error.message));
  return errors;
}

for (const path of pages) {
  test(`loads ${path} without console errors`, async ({ page }) => {
    const errors = watchErrors(page);
    const response = await page.goto(path);
    expect(response?.status()).toBe(path === "/no-such-page" ? 404 : 200);
    await expect(page.locator("main h1").first()).toBeVisible();
    await page.waitForLoadState("networkidle");
    // A missing favicon or API in a partial environment is not a page fault.
    expect(errors.filter((text) => !/favicon|Failed to load resource/i.test(text))).toEqual([]);
  });
}

const languages = [
  ["ur", "rtl"],
  ["ar", "rtl"],
  ["hi", "ltr"],
  ["es", "ltr"],
] as const;

for (const [lang, dir] of languages) {
  for (const path of pages) {
    const address = path === "/" ? `/${lang}` : `/${lang}${path}`;
    test(`loads ${address} in its own language and direction`, async ({ page }) => {
      const errors = watchErrors(page);
      const response = await page.goto(address);
      expect(response?.status()).toBe(path === "/no-such-page" ? 404 : 200);
      await expect(page.locator("html")).toHaveAttribute("lang", lang);
      await expect(page.locator("html")).toHaveAttribute("dir", dir);
      await expect(page.locator("main h1").first()).toBeVisible();
      await page.waitForLoadState("networkidle");
      expect(errors.filter((text) => !/favicon|Failed to load resource/i.test(text))).toEqual([]);
    });
  }
}

test("the language menu leads to the same page in another language", async ({ page }) => {
  await page.goto("/download");
  await page.locator("header details summary").click();
  await page.locator('header details a[hreflang="ur"]').click();
  await expect(page).toHaveURL(/\/ur\/download$/);
  await expect(page.locator("html")).toHaveAttribute("dir", "rtl");
  // And back to English, at its plain address.
  await page.locator("header details summary").click();
  await page.locator('header details a[hreflang="en"]').click();
  await expect(page).toHaveURL(/\/download$/);
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
});

test("an /en address sends the browser to the plain English one", async ({ page }) => {
  await page.goto("/en/privacy");
  await expect(page).toHaveURL(/\/privacy$/);
  expect(new URL(page.url()).pathname).toBe("/privacy");
});

for (const width of [360, 390]) {
  for (const path of pages.slice(0, 4)) {
    test(`no horizontal scroll at ${width}px on ${path}`, async ({ page }) => {
      await page.setViewportSize({ width, height: 800 });
      await page.goto(path);
      await page.waitForLoadState("networkidle");
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      expect(overflow).toBeLessThanOrEqual(0);
    });
  }
}

test("keyboard focus is visible", async ({ page }) => {
  await page.goto("/download");
  for (let i = 0; i < 4; i += 1) {
    await page.keyboard.press("Tab");
    const outline = await page.evaluate(() => {
      const el = document.activeElement;
      if (!el || el === document.body) return null;
      const style = getComputedStyle(el);
      return { width: parseFloat(style.outlineWidth), style: style.outlineStyle, shadow: style.boxShadow };
    });
    expect(outline).not.toBeNull();
    expect(outline!.style !== "none" && outline!.width > 0 || outline!.shadow !== "none").toBe(true);
  }
});

test("the landmarks are in place", async ({ page }) => {
  await page.goto("/download");
  await expect(page.locator("header").first()).toBeVisible();
  await expect(page.locator("main")).toHaveCount(1);
  await expect(page.locator("footer").first()).toBeVisible();
  await expect(page.locator("h1")).toHaveCount(1);
});

test("reduced motion shows the static hero", async ({ browser, baseURL }) => {
  const context = await browser.newContext({ reducedMotion: "reduce", baseURL });
  const page = await context.newPage();
  await page.goto("/");
  await page.waitForTimeout(1200);
  // Nothing may loop: no infinite CSS animations.
  const running = await page.evaluate(
    () => document.getAnimations().filter((a) => a.playState === "running" && a.effect?.getTiming().iterations === Infinity).length,
  );
  expect(running).toBe(0);
  // The hero is a still frame: two captures a moment apart are identical.
  const hero = page.locator("main section").first();
  const first = await hero.screenshot();
  await page.waitForTimeout(700);
  const second = await hero.screenshot();
  expect(second.equals(first)).toBe(true);
  await context.close();
});

test.describe("the installer and the API", () => {
  test.beforeEach(async ({ request }) => {
    const res = await request.get("/api/releases/latest").catch(() => null);
    test.skip(!res || !res.ok(), "the API is not running at BASE_URL");
  });

  test("the download returns the installer whose SHA-256 the API reports", async ({ request }) => {
    const latest = await (await request.get("/api/releases/latest")).json();
    const expected = String(latest.sha256 ?? latest.release?.sha256 ?? "").toLowerCase();
    expect(expected).toMatch(/^[0-9a-f]{64}$/);
    const res = await request.get("/api/download/latest");
    expect(res.ok()).toBe(true);
    const body = await res.body();
    expect(createHash("sha256").update(body).digest("hex")).toBe(expected);
  });

  test("Copy puts the checksum on the clipboard", async ({ browser, baseURL }) => {
    const context = await browser.newContext({ baseURL, permissions: ["clipboard-read", "clipboard-write"] });
    const page = await context.newPage();
    await page.goto("/download");
    const copy = page.getByRole("button", { name: /SHA-256/i }).first();
    await copy.click();
    const text = await page.evaluate(() => navigator.clipboard.readText());
    expect(text).toMatch(/[0-9a-f]{64}/i);
    await context.close();
  });
});
