import { NextResponse, type NextRequest } from "next/server";

import { DEFAULT_LANG, isLang } from "@/lib/i18n/languages";

/**
 * Every page lives under its language (`app/[lang]`), but English keeps the site's plain
 * addresses: `/download` is drawn from `/en/download`, and `/en/download` sends the browser
 * back to `/download`, so each page has one English address.
 */
export function proxy(request: NextRequest) {
  const { pathname } = request.nextUrl;
  const first = pathname.split("/")[1] ?? "";
  if (first === DEFAULT_LANG) {
    const url = request.nextUrl.clone();
    url.pathname = pathname.slice(DEFAULT_LANG.length + 1) || "/";
    return NextResponse.redirect(url, 308);
  }
  if (isLang(first)) return NextResponse.next();
  const url = request.nextUrl.clone();
  url.pathname = `/${DEFAULT_LANG}${pathname === "/" ? "" : pathname}`;
  return NextResponse.rewrite(url);
}

export const config = {
  // Pages only: not Next's own files, the API, media, or anything with a file extension.
  matcher: ["/((?!_next|api|media|opengraph-image|.*\\..*).*)"],
};
