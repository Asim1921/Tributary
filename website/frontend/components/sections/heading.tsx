import type { ReactNode } from "react";

/** A landing-page section's title. Its id names the section (Section's labelledBy). */
export function SectionTitle({ id, children, className = "" }: { id: string; children: ReactNode; className?: string }) {
  return (
    <h2 id={id} className={`font-display text-h2 font-[680] tracking-[-0.03em] text-foam ${className}`}>
      {children}
    </h2>
  );
}

/** The paragraph that opens a section, under its title. */
export function Lede({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <p className={`mt-5 max-w-[62ch] text-lead text-silt ${className}`}>{children}</p>;
}

/** A feature's or a block's title, one step below the section's. */
export function BlockTitle({ children, className = "" }: { children: ReactNode; className?: string }) {
  return (
    <h3 className={`font-display text-h4 font-[650] tracking-[-0.022em] text-foam ${className}`}>{children}</h3>
  );
}

/** A key, as written on the keyboard. */
export function Key({ children }: { children: ReactNode }) {
  return (
    <kbd className="rounded-[4px] border border-bank bg-channel px-1.5 py-px font-text text-[0.88em] font-semibold text-foam">
      {children}
    </kbd>
  );
}

/** Something to type exactly as shown: an address or a file name. */
export function Typed({ children }: { children: ReactNode }) {
  return <code className="font-code text-[0.88em] text-foam">{children}</code>;
}
