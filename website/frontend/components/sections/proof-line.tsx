import { Container } from "@/components/ui/container";
import { formatDate } from "@/lib/release";
import { facts } from "@/lib/site";
import { fill } from "@/lib/i18n/languages";
import { getWords } from "@/lib/i18n/server";

/** One sentence of evidence, set as type: what checks the app. */
export async function ProofLine() {
  const { lang, w } = await getWords();
  const proof = facts.proof;
  return (
    <div className="border-t border-bank/70">
      <Container className="py-14 md:py-20">
        <p className="tabular max-w-[46ch] font-display text-[1.6rem] leading-[1.28] font-[560] tracking-[-0.02em] text-balance text-foam md:text-h3 md:leading-[1.22]">
          {fill(w.proof.line, { rust: proof.rustTests, ui: proof.interfaceTests, steps: proof.endToEndSteps })}
        </p>
        <p className="mt-5 text-caption text-silt">
          {fill(w.proof.counted, { date: formatDate(proof.checkedOn, lang) })}
        </p>
      </Container>
    </div>
  );
}
