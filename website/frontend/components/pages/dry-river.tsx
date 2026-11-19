/**
 * The icon's confluence with the water gone: the streams are there, but the channel is only an
 * outline and no download arrives. Drawn in hairline colours; purely decorative.
 */
export function DryRiver({ className = "" }: { className?: string }) {
  return (
    <svg
      aria-hidden="true"
      focusable="false"
      className={className}
      viewBox="150 120 724 800"
      fill="none"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <g stroke="var(--bank)" strokeWidth="44">
        <path d="M250 212 C 250 420, 512 420, 512 600" />
        <path d="M774 212 C 774 420, 512 420, 512 600" />
      </g>
      <g stroke="var(--bank)" strokeWidth="10" strokeDasharray="2 30">
        <path d="M512 200 L 512 640" />
        <path d="M336 560 L 512 760 L 688 560" />
        <rect x="300" y="818" width="424" height="56" rx="28" />
      </g>
      <g stroke="var(--silt)" strokeWidth="6" strokeOpacity="0.55">
        <path d="M250 212 C 250 420, 512 420, 512 600" />
        <path d="M774 212 C 774 420, 512 420, 512 600" />
      </g>
    </svg>
  );
}
