/**
 * Release information from the API. Pages are rendered on the server, so this runs there:
 * inside the stack the API is at INTERNAL_API_URL. When it can't be reached (at build time, or
 * if the API is down), callers get null or an empty list and the page renders without the
 * live figures.
 */

export type Release = {
  version: string;
  file: string;
  size: number;
  sha256: string;
  published: string;
  notes: string[];
  requirements: string[];
};

const API = process.env.INTERNAL_API_URL || "http://api:8000";
const TIMEOUT_MS = 2500;

async function getJson<T>(path: string): Promise<T | null> {
  try {
    const reply = await fetch(`${API}${path}`, {
      next: { revalidate: 60 },
      signal: AbortSignal.timeout(TIMEOUT_MS),
    });
    if (!reply.ok) return null;
    return (await reply.json()) as T;
  } catch {
    return null;
  }
}

export async function getLatestRelease(): Promise<Release | null> {
  return getJson<Release>("/api/releases/latest");
}

export async function getReleases(): Promise<Release[]> {
  return (await getJson<Release[]>("/api/releases")) ?? [];
}

/** 5461084 -> "5.2 MB" */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "";
  const units = ["bytes", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${value} bytes` : `${value.toFixed(1)} ${units[unit]}`;
}

/** "2026-11-19" -> "19 November 2026" */
export function formatDate(iso: string): string {
  const date = new Date(`${iso.slice(0, 10)}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) return iso;
  return new Intl.DateTimeFormat("en-GB", {
    day: "numeric",
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  }).format(date);
}
