/**
 * numberOutline — atribui números hierárquicos automáticos a um outline.
 *
 * Regras:
 *   - Subtítulos (level 0) NUNCA recebem número.
 *   - Título 1 (level 1) recebe número inteiro: 1, 2, 3, …
 *   - Título 2 (level 2) recebe número.subnúmero: 1.1, 1.2, 2.1, …
 *   - Título 3 (level 3) recebe três níveis: 1.1.1, 1.1.2, 1.2.1, …
 *   - O contador de cada nível ZERA quando um nível superior aparece.
 *     Exemplo: "Título 1: Histórico" → resetar contador de level 2 e 3.
 *
 * **Não modifica o documento.** Retorna um array paralelo ao outline
 * original com a string formatada em `numero` (ou null para subtítulos).
 *
 * Quando o documento começa direto em Título 2 (sem Título 1 acima),
 * o contador de level 2 inicia em 1.1 ainda — o "1" implícito é o
 * próprio número 1. Esta é a convenção pericial brasileira.
 */

import type { OutlineEntry } from "./extractOutline";

export interface NumberedOutlineEntry extends OutlineEntry {
  /** Número formatado ("1", "1.1", "2.3.1") ou null para subtítulos. */
  numero: string | null;
}

export function numberOutline(
  outline: ReadonlyArray<OutlineEntry>,
): NumberedOutlineEntry[] {
  const counters: [number, number, number] = [0, 0, 0];
  const out: NumberedOutlineEntry[] = [];

  for (const entry of outline) {
    if (entry.level === 0) {
      // Subtítulo: não numera.
      out.push({ ...entry, numero: null });
      continue;
    }

    // Incrementa o contador do nível atual e zera os filhos.
    const idx = entry.level - 1;
    counters[idx]! += 1;
    for (let i = idx + 1; i < counters.length; i++) {
      counters[i] = 0;
    }

    // Convenção pericial: se um nível filho aparece SEM um pai numerado
    // (ex: documento começa direto em Título 2 → "1.1" virtual), garante
    // que os pais à esquerda estejam pelo menos em 1.
    for (let i = 0; i < idx; i++) {
      if ((counters[i] ?? 0) === 0) counters[i] = 1;
    }

    const parts: string[] = [];
    for (let i = 0; i <= idx; i++) {
      parts.push(String(counters[i]));
    }
    out.push({ ...entry, numero: parts.join(".") });
  }

  return out;
}
