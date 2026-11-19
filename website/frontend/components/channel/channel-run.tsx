import type { ReactNode } from "react";

import { ChannelLine } from "./channel-line";

/**
 * The stretch of the landing page the main channel runs through, from the features to the
 * download. Sections inside indent their content on wide screens (lg:pl-24) to leave the
 * channel its track.
 */
export function ChannelRun({ children }: { children: ReactNode }) {
  return (
    <div className="relative">
      <ChannelLine />
      {children}
    </div>
  );
}
