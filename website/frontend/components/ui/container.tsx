import type { ElementType, ReactNode } from "react";

/** The content column: 1200px at most, with a side gutter that clears phone edges. */
export function Container({
  as: Tag = "div",
  className = "",
  children,
}: {
  as?: ElementType;
  className?: string;
  children: ReactNode;
}) {
  return (
    <Tag
      className={`mx-auto w-full max-w-[1200px] pl-[max(1rem,env(safe-area-inset-left))] pr-[max(1rem,env(safe-area-inset-right))] sm:px-6 lg:px-8 ${className}`}
    >
      {children}
    </Tag>
  );
}

/** A page section with the standard vertical rhythm, named by its heading. */
export function Section({
  id,
  labelledBy,
  className = "",
  children,
}: {
  id?: string;
  labelledBy: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <section id={id} aria-labelledby={labelledBy} className={`relative py-[clamp(4.5rem,9vw,8rem)] ${className}`}>
      {children}
    </section>
  );
}
