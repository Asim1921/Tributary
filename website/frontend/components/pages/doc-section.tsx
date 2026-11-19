import type { ReactNode } from "react";

import { Container } from "@/components/ui/container";

/**
 * A section of an inner page: its title in the left four columns (it stays in view while the
 * section scrolls on wide screens), the content in the right eight. A hairline above each one.
 */
export function DocSection({
  id,
  title,
  intro,
  children,
}: {
  id: string;
  title: ReactNode;
  intro?: ReactNode;
  children: ReactNode;
}) {
  const headingId = `${id}-title`;
  return (
    <section id={id} aria-labelledby={headingId}>
      <Container>
        <div className="grid gap-x-8 gap-y-7 border-t border-bank py-[clamp(3rem,6vw,4.75rem)] lg:grid-cols-12">
          <div className="lg:col-span-4">
            <div className="lg:sticky lg:top-24">
              <h2 id={headingId} className="max-w-[18ch] font-display text-h3 font-[660] tracking-[-0.026em]">
                {title}
              </h2>
              {intro ? <p className="mt-3 max-w-[38ch] text-silt">{intro}</p> : null}
            </div>
          </div>
          <div className="min-w-0 max-w-[64ch] lg:col-span-8">{children}</div>
        </div>
      </Container>
    </section>
  );
}
