import { CopyButton } from "@/components/ui/copy-button";

/**
 * A command to paste, with what it runs in and a Copy button. Long lines wrap rather than
 * scroll, so a phone never needs sideways scrolling; Copy always copies the exact text.
 */
export function CodeBlock({ code, label, copyLabel }: { code: string; label: string; copyLabel: string }) {
  return (
    <figure className="overflow-hidden rounded-m border border-bank bg-channel">
      <figcaption className="flex min-h-12 items-center justify-between gap-3 border-b border-bank py-1 pl-4 pr-1.5">
        <span className="text-caption text-silt">{label}</span>
        <CopyButton value={code} label={copyLabel} />
      </figcaption>
      <pre className="whitespace-pre-wrap px-4 py-4 font-code text-[0.88rem] leading-[1.7] text-foam [overflow-wrap:anywhere]">
        <code>{code}</code>
      </pre>
    </figure>
  );
}
