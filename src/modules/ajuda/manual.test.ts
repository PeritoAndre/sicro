import { describe, expect, it } from "vitest";
import manualMd from "../../../docs/MANUAL_SICRO.md?raw";
import { slugify } from "./AjudaModule";

/** Ids dos títulos na ordem em que a Ajuda os gera (h1 a h3, texto sem marcação). */
function idsDosTitulos(md: string): string[] {
  const usados = new Set<string>();
  return md
    .split("\n")
    .filter((l) => /^#{1,3} /.test(l))
    .map((l) => slugify(l.replace(/^#+ /, "").replace(/[*`]/g, ""), usados));
}

describe("manual na Ajuda", () => {
  it("todo link do Sumário leva a um título", () => {
    const ids = new Set(idsDosTitulos(manualMd));
    const sumario = manualMd.slice(manualMd.indexOf("## Sumário"), manualMd.indexOf("\n---", manualMd.indexOf("## Sumário")));
    const links = [...sumario.matchAll(/\]\(#([^)]+)\)/g)].map((m) => m[1] as string);
    expect(links.length).toBeGreaterThan(10);
    for (const l of links) expect(ids.has(l), l).toBe(true);
  });

  it("tem a seção Edição, no Sumário e no índice", () => {
    expect(manualMd).toContain("\n## 8. Edição\n");
    expect(manualMd).toContain("[Edição](#8-edição)");
  });
});
