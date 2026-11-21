import { Fragment } from "react";

import { CopyButton } from "@/components/ui/copy-button";
import { getWords } from "@/lib/i18n/server";

/**
 * A SHA-256 checksum in groups of eight, so it can be compared a piece at a time. The groups
 * are spaced with margins, not spaces, so a selection copies the exact value; Copy does too.
 */
export async function Checksum({ value, id = "sha256" }: { value: string; id?: string }) {
  const { w } = await getWords();
  const groups = value.match(/.{1,8}/g) ?? [value];
  return (
    <div>
      <div className="flex items-center justify-between gap-3">
        <h3 id={`${id}-label`} className="font-text text-caption font-semibold tracking-normal text-foam">
          {w.panel.checksum}
        </h3>
        <CopyButton value={value} label={w.panel.copyChecksum} copyText={w.site.copy} copiedLabel={w.site.copied} />
      </div>
      <p className="mt-2.5">
        <code dir="ltr" data-checksum={value} className="block font-code text-[0.9rem] leading-[1.75] text-foam">
          {groups.map((group, index) => (
            <Fragment key={index}>
              {index > 0 ? <wbr /> : null}
              <span className="mr-[0.7ch] inline-block last:mr-0">{group}</span>
            </Fragment>
          ))}
        </code>
      </p>
    </div>
  );
}
