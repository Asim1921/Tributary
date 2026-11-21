import type { Metadata, Viewport } from "next";
import {
  Bricolage_Grotesque,
  JetBrains_Mono,
  Noto_Naskh_Arabic,
  Noto_Nastaliq_Urdu,
  Noto_Sans_Devanagari,
  Public_Sans,
} from "next/font/google";

import { Footer } from "@/components/site/footer";
import { Header } from "@/components/site/header";
import { themeScript } from "@/components/site/theme-toggle";
import { LANGUAGES, dirOf } from "@/lib/i18n/languages";
import { alternatesFor } from "@/lib/i18n/metadata";
import { getWords } from "@/lib/i18n/server";
import { site } from "@/lib/site";

import "../globals.css";

// Self-hosted at build time: no request to Google when the page loads.
const display = Bricolage_Grotesque({
  subsets: ["latin"],
  variable: "--font-bricolage",
  display: "swap",
  axes: ["opsz"],
});
const text = Public_Sans({ subsets: ["latin"], variable: "--font-public", display: "swap" });
const code = JetBrains_Mono({ subsets: ["latin"], variable: "--font-jetbrains", display: "swap" });
// The scripts the Latin fonts don't have. A browser fetches one only for a page that uses it.
const nastaliq = Noto_Nastaliq_Urdu({ subsets: ["arabic"], variable: "--font-nastaliq", display: "swap" });
const naskh = Noto_Naskh_Arabic({ subsets: ["arabic"], variable: "--font-naskh", display: "swap" });
const devanagari = Noto_Sans_Devanagari({ subsets: ["devanagari"], variable: "--font-devanagari", display: "swap" });

/** Each language is drawn from the start; any other first segment is a missing page. */
export function generateStaticParams() {
  return LANGUAGES.map((language) => ({ lang: language.code }));
}
export const dynamicParams = false;

export async function generateMetadata(): Promise<Metadata> {
  const { lang, w } = await getWords();
  return {
    metadataBase: new URL(site.url),
    title: { default: w.meta.title, template: "%s | Tributary" },
    description: w.meta.description,
    applicationName: site.name,
    alternates: alternatesFor(lang, "/"),
    openGraph: {
      type: "website",
      siteName: site.name,
      title: w.meta.title,
      description: w.meta.description,
      locale: lang,
    },
    twitter: { card: "summary_large_image" },
  };
}

export const viewport: Viewport = {
  themeColor: [
    { media: "(prefers-color-scheme: dark)", color: "#05161b" },
    { media: "(prefers-color-scheme: light)", color: "#f1f6f5" },
  ],
  colorScheme: "dark light",
};

export default async function RootLayout({ children }: { children: React.ReactNode }) {
  const { lang, w } = await getWords();
  const fonts = [display, text, code, nastaliq, naskh, devanagari].map((font) => font.variable).join(" ");
  return (
    <html lang={lang} dir={dirOf(lang)} suppressHydrationWarning className={fonts}>
      <head>
        <script dangerouslySetInnerHTML={{ __html: themeScript }} />
      </head>
      <body className="min-h-dvh">
        <a
          href="#main"
          className="sr-only-focusable fixed start-4 top-3 z-50 rounded-s bg-stream px-4 py-2 font-semibold text-on-current"
        >
          {w.site.skip}
        </a>
        <Header />
        <main id="main">{children}</main>
        <Footer />
      </body>
    </html>
  );
}
