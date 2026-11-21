import { Fragment, type ReactNode } from "react";

/**
 * A sentence from the dictionary with marked words: `<b>…</b>` for bold, `<key>…</key>` for a
 * key on the keyboard and `<code>…</code>` for something to type. A translation can move the
 * marks to wherever its sentence needs them. Nothing else is read as markup.
 */
export function Rich({
  text,
  b = (inner) => <strong>{inner}</strong>,
  keyboard = (inner) => <kbd>{inner}</kbd>,
  code = (inner) => <code>{inner}</code>,
}: {
  text: string;
  b?: (inner: string) => ReactNode;
  keyboard?: (inner: string) => ReactNode;
  code?: (inner: string) => ReactNode;
}) {
  const parts: ReactNode[] = [];
  const marks = /<(b|key|code)>([^<]*)<\/\1>/g;
  let last = 0;
  for (const match of text.matchAll(marks)) {
    parts.push(text.slice(last, match.index));
    const [, tag, inner = ""] = match;
    parts.push(tag === "b" ? b(inner) : tag === "key" ? keyboard(inner) : code(inner));
    last = (match.index ?? 0) + match[0].length;
  }
  parts.push(text.slice(last));
  return (
    <>
      {parts.map((part, index) => (
        <Fragment key={index}>{part}</Fragment>
      ))}
    </>
  );
}
