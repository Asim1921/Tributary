import Link from "next/link";
import type { ReactNode } from "react";

type Variant = "primary" | "secondary" | "quiet";
type Size = "md" | "lg";

const base =
  "inline-flex items-center justify-center gap-2.5 rounded-s font-text font-semibold whitespace-nowrap " +
  "transition-[background-color,border-color,color,transform] duration-[160ms] ease-flow active:translate-y-px";

const variants: Record<Variant, string> = {
  primary:
    "bg-stream text-on-current border border-stream hover:bg-[color-mix(in_oklab,var(--current)_88%,var(--foam))]",
  secondary:
    "bg-transparent text-foam border border-bank hover:border-stream hover:text-stream",
  quiet:
    "bg-transparent text-foam border border-transparent underline decoration-bank hover:decoration-stream hover:text-stream",
};

const sizes: Record<Size, string> = {
  md: "min-h-11 px-4 text-[0.95rem]",
  lg: "min-h-13 px-6 text-body",
};

type Props = {
  href: string;
  children: ReactNode;
  variant?: Variant;
  size?: Size;
  icon?: ReactNode;
  /** A file the browser should save rather than open. */
  download?: boolean;
  /** A link to another site, opened in a new tab. */
  external?: boolean;
  className?: string;
};

/** A link that looks like a button. Every action on this site goes somewhere. */
export function ButtonLink({
  href,
  children,
  variant = "primary",
  size = "md",
  icon,
  download,
  external,
  className = "",
}: Props) {
  const classes = `${base} ${variants[variant]} ${sizes[size]} ${className}`;
  const content = (
    <>
      {icon}
      <span>{children}</span>
    </>
  );
  // Downloads and API routes are plain links: Next must not try to render them as pages.
  if (external || download || href.startsWith("/api/")) {
    return (
      <a
        href={href}
        className={classes}
        {...(download ? { download: "" } : {})}
        {...(external ? { target: "_blank", rel: "noreferrer" } : {})}
      >
        {content}
      </a>
    );
  }
  return (
    <Link href={href} className={classes}>
      {content}
    </Link>
  );
}
