import type { ReactNode } from "react";

export type Step = { title: string; body: ReactNode };

/**
 * A real sequence, so it is numbered: each step's number sits on a thin channel that runs
 * down to the next one. The list itself carries the numbering for screen readers.
 */
export function Steps({ steps, className = "" }: { steps: Step[]; className?: string }) {
  return (
    <ol className={className}>
      {steps.map((step, index) => {
        const last = index === steps.length - 1;
        return (
          <li key={step.title} className={`relative grid grid-cols-[2.75rem_minmax(0,1fr)] gap-x-5 ${last ? "" : "pb-9"}`}>
            {last ? null : (
              <span aria-hidden="true" className="absolute bottom-0 left-[1.375rem] top-11 w-px bg-bank" />
            )}
            <span
              aria-hidden="true"
              className="relative flex size-11 items-center justify-center rounded-full border border-bank bg-riverbed font-display text-[1.06rem] font-[650] text-(--current) tabular"
            >
              {index + 1}
            </span>
            <div className="pt-[0.45rem]">
              <h3 className="font-display text-[1.24rem] font-[640] leading-snug tracking-[-0.016em]">{step.title}</h3>
              <div className="mt-2 space-y-3 text-silt">{step.body}</div>
            </div>
          </li>
        );
      })}
    </ol>
  );
}
