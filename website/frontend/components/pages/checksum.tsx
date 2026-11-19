import { Fragment } from "react";

import { CopyButton } from "@/components/ui/copy-button";

/**
 * A SHA-256 checksum in groups of eight, so it can be compared a piece at a time. The groups
 * are spaced with margins, not spaces, so a selection copies the exact value; Copy does too.
 */
export function Checksum({ value, id = "sha256" }: { value: string; id?: string }) {
  const groups = value.match(/.{1,8}/g) ?? [value];
  return (
    <div>
      <div className="flex items-center justify-between gap-3">
        <h3 id={`${id}-label`} className="font-text text-caption font-semibold tracking-normal text-foam">
          SHA-256 checksum
        </h3>
        <CopyButton value={value} label="Copy the SHA-256 checksum" />
      </div>
      <p className="mt-2.5">
        <code data-checksum={value} className="block font-code text-[0.9rem] leading-[1.75] text-foam">
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
