"use client";

import { useSyncExternalStore } from "react";

import { MoonIcon, SunIcon } from "@/components/ui/icons";

const KEY = "tributary-theme";
const CHANGED = "tributary-theme";
type Theme = "light" | "dark";

function currentTheme(): Theme {
  const set = document.documentElement.dataset.theme;
  if (set === "light" || set === "dark") return set;
  return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

// The theme lives outside React: on the root element, or in the system's setting.
function subscribe(changed: () => void) {
  const query = window.matchMedia("(prefers-color-scheme: light)");
  query.addEventListener("change", changed);
  window.addEventListener(CHANGED, changed);
  return () => {
    query.removeEventListener("change", changed);
    window.removeEventListener(CHANGED, changed);
  };
}

/** Switches between the river by night and by day, and remembers the choice. */
export function ThemeToggle() {
  // Unknown until the page is in the browser, so the server renders no icon.
  const theme = useSyncExternalStore<Theme | null>(subscribe, currentTheme, () => null);

  function toggle() {
    const next: Theme = currentTheme() === "dark" ? "light" : "dark";
    document.documentElement.dataset.theme = next;
    try {
      localStorage.setItem(KEY, next);
    } catch {
      // Storage refused: the choice lasts for this page only.
    }
    window.dispatchEvent(new CustomEvent(CHANGED, { detail: next }));
  }

  const label = theme === "light" ? "Switch to the dark theme" : "Switch to the light theme";
  return (
    <button
      type="button"
      onClick={toggle}
      aria-label={label}
      title={label}
      className="inline-flex size-11 items-center justify-center rounded-s text-[1.15rem] text-silt transition-colors duration-[160ms] hover:text-foam"
    >
      {theme === "light" ? <MoonIcon /> : theme === "dark" ? <SunIcon /> : null}
    </button>
  );
}

/** Runs before the first paint, so a remembered theme never flashes the other one. */
export const themeScript = `try{var t=localStorage.getItem("${KEY}");if(t==="light"||t==="dark")document.documentElement.dataset.theme=t}catch(e){}`;
