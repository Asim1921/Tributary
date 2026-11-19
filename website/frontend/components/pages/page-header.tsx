import type { ReactNode } from "react";

import { Container } from "@/components/ui/container";

/**
 * The opening of an inner page: a large display title and one plain sentence under it,
 * left-aligned on the grid. An optional aside takes the right five columns on wide screens.
 */
export function PageHeader({
  title,
  lede,
  children,
  aside,
}: {
  title: ReactNode;
  lede?: ReactNode;
  children?: ReactNode;
  aside?: ReactNode;
}) {
  return (
    <Container className="pt-[clamp(3.25rem,7vw,6rem)] pb-[clamp(3rem,6vw,4.75rem)]">
      <div className={aside ? "grid gap-12 lg:grid-cols-12 lg:items-start lg:gap-8" : undefined}>
        <div className={aside ? "lg:col-span-7 lg:pt-4" : undefined}>
          <h1 className="max-w-[15ch] font-display text-h1 font-[690] tracking-[-0.034em]">{title}</h1>
          {lede ? <p className="mt-6 max-w-[52ch] text-lead text-silt">{lede}</p> : null}
          {children}
        </div>
        {aside ? <div className="lg:col-span-5">{aside}</div> : null}
      </div>
    </Container>
  );
}
