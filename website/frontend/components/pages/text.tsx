import type { ReactNode } from "react";

/** The name of something on screen (a button, a tab, a menu item), as the reader will see it. */
export function UiName({ children }: { children: ReactNode }) {
  return <strong className="font-semibold text-foam">{children}</strong>;
}

/** A key on the keyboard. Set in the text face: the code face is kept for code blocks. */
export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="mx-[0.1em] inline-block rounded-[5px] border border-b-2 border-bank bg-channel px-[0.45em] py-px font-text text-[0.88em] font-semibold leading-[1.45] text-foam">
      {children}
    </kbd>
  );
}

/** Something the reader types or pastes: an address or a folder. */
export function InlineCode({ children }: { children: ReactNode }) {
  return (
    <code className="rounded-[5px] border border-bank bg-channel px-[0.4em] py-px font-code text-[0.86em] text-foam [overflow-wrap:anywhere]">
      {children}
    </code>
  );
}

/** A plain list whose items are marked with a short stream line rather than a bullet. */
export function FlowList({ items, className = "" }: { items: readonly ReactNode[]; className?: string }) {
  return (
    <ul className={`space-y-3 ${className}`}>
      {items.map((item, index) => (
        <li key={index} className="relative pl-6">
          <span
            aria-hidden="true"
            className="absolute left-0 top-[0.8em] h-px w-3 bg-(--current)"
          />
          {item}
        </li>
      ))}
    </ul>
  );
}
