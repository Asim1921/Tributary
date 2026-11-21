import { notFound } from "next/navigation";

/** Any address the site doesn't have, in any language, gets that language's missing page. */
export default function Missing() {
  notFound();
}
