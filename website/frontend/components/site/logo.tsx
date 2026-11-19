/** The app's own icon: two streams join one channel, which ends in a download. */
export function Logo({
  className,
  title,
  id = "logo",
}: {
  className?: string;
  title?: string;
  /** Unique per use on a page, as it names the gradient. */
  id?: string;
}) {
  const gradient = `tributary-${id}-bg`;
  return (
    <svg
      className={className}
      viewBox="0 0 1024 1024"
      role={title ? "img" : undefined}
      aria-hidden={title ? undefined : true}
      aria-label={title}
      focusable="false"
    >
      <defs>
        <linearGradient id={gradient} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#0f766e" />
          <stop offset="1" stopColor="#134e6f" />
        </linearGradient>
      </defs>
      <rect x="48" y="48" width="928" height="928" rx="208" fill={`url(#${gradient})`} />
      <g fill="none" stroke="#ffffff" strokeLinecap="round" strokeWidth="72">
        <path d="M250 212 C 250 420, 512 420, 512 600" strokeOpacity="0.72" />
        <path d="M774 212 C 774 420, 512 420, 512 600" strokeOpacity="0.72" />
        <path d="M512 200 L 512 640" />
        <path d="M336 560 L 512 760 L 688 560" strokeLinejoin="round" />
      </g>
      <rect x="300" y="818" width="424" height="56" rx="28" fill="#ffffff" />
    </svg>
  );
}
