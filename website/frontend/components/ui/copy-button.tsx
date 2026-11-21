"use client";

import { useEffect, useRef, useState } from "react";

import { CheckIcon, CopyIcon } from "./icons";

type Props = {
  value: string;
  /** The word on the button, in the page's language. */
  copyText?: string;
  /** What is copied, for the button's name: "Copy the SHA-256 checksum". */
  label: string;
  copiedLabel?: string;
  className?: string;
};

/** Copies a value, says so on screen and to screen readers, and falls back to selecting it. */
export function CopyButton({ value, label, copyText = "Copy", copiedLabel = "Copied", className = "" }: Props) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => () => {
    if (timer.current) clearTimeout(timer.current);
  }, []);

  async function copy() {
    let done = false;
    try {
      await navigator.clipboard.writeText(value);
      done = true;
    } catch {
      // Older browsers or a refused permission: copy through a selected text field.
      const field = document.createElement("textarea");
      field.value = value;
      field.setAttribute("readonly", "");
      field.style.position = "fixed";
      field.style.opacity = "0";
      document.body.appendChild(field);
      field.select();
      done = document.execCommand("copy");
      field.remove();
    }
    if (!done) return;
    setCopied(true);
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => setCopied(false), 2000);
  }

  return (
    <button
      type="button"
      onClick={copy}
      aria-label={label}
      className={
        "inline-flex min-h-11 min-w-11 items-center justify-center gap-2 rounded-s border border-bank px-3 " +
        "text-caption font-semibold text-foam transition-colors duration-[160ms] hover:border-stream hover:text-stream " +
        className
      }
    >
      {copied ? <CheckIcon className="text-stream" /> : <CopyIcon />}
      <span aria-hidden="true">{copied ? copiedLabel : copyText}</span>
      <span role="status" aria-live="polite" className="sr-only">
        {copied ? copiedLabel : ""}
      </span>
    </button>
  );
}
