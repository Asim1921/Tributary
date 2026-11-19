import { Container } from "@/components/ui/container";
import { formatDate } from "@/lib/release";
import { facts } from "@/lib/site";

/** One sentence of evidence, set as type: what checks the app. */
export function ProofLine() {
  const proof = facts.proof;
  const browsers = proof.browsers.join(" and ");
  return (
    <div className="border-t border-bank/70">
      <Container className="py-14 md:py-20">
        <p className="tabular max-w-[46ch] font-display text-[1.6rem] leading-[1.28] font-[560] tracking-[-0.02em] text-balance text-foam md:text-h3 md:leading-[1.22]">
          Checked by {proof.rustTests} Rust tests, {proof.interfaceTests} interface and extension tests and{" "}
          {proof.endToEndSteps} end-to-end steps, with the extension tested in real {browsers}.
        </p>
        <p className="mt-5 text-caption text-silt">
          Counted on {formatDate(proof.checkedOn)}.
        </p>
      </Container>
    </div>
  );
}
