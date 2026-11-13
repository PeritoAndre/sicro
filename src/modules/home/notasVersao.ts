/** Notas de versão (docs/releases/v*.md) e o resumo de uma linha do aviso de atualização. */

import { notasEmTexto } from "@/core/updater/notasTexto";

/** Tira a seção "Downloads" (tabela de instaladores), que não interessa dentro do app. */
export function semDownloads(md: string): string {
  const saida: string[] = [];
  let pulando = false;
  for (const linha of md.split("\n")) {
    if (/^#{1,6}\s/.test(linha)) pulando = /^#{1,6}\s*downloads\b/i.test(linha);
    if (!pulando) saida.push(linha);
  }
  return saida.join("\n");
}

function partes(v: string): number[] {
  return v.split(".").map((n) => parseInt(n, 10) || 0);
}

function cmp(a: string, b: string): number {
  const pa = partes(a);
  const pb = partes(b);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d) return d;
  }
  return 0;
}

/** Chave do arquivo de notas da versão (ou da mais próxima que não passe dela). */
export function escolherNotas(chaves: string[], versao: string): { chave: string; versao: string } | null {
  const achadas = chaves
    .map((chave) => ({ chave, versao: /v(\d+(?:\.\d+)*)\.md$/.exec(chave)?.[1] ?? "" }))
    .filter((x) => x.versao);
  const exata = achadas.find((x) => cmp(x.versao, versao) === 0);
  if (exata) return exata;
  const abaixo = achadas.filter((x) => cmp(x.versao, versao) <= 0).sort((a, b) => cmp(b.versao, a.versao));
  return abaixo[0] ?? null;
}

/** "5.3.0" → "5.3". */
export function versaoCurta(v: string): string {
  return partes(v).slice(0, 2).join(".");
}

/** Primeira frase útil das notas, em uma linha. */
export function resumoNotas(md: string, max = 140): string {
  const linhas = notasEmTexto(semDownloads(md))
    .split("\n")
    .map((l) => l.replace(/^\s*[-*]\s+/, "").trim())
    .filter((l) => l.length > 0 && !/^SICRO\s+\d/i.test(l));
  const l = linhas[0] ?? "";
  return l.length > max ? l.slice(0, max - 1).trimEnd() + "…" : l;
}
