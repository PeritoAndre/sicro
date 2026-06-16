/**
 * Registry de templates de laudo.
 *
 * Hoje só há um template ativo:
 *   - "Documento em branco" (`documento_em_branco`)
 *
 * Cada novo laudo já nasce com o **cabeçalho oficial** PCA — Polícia
 * Científica do Amapá — ligado por padrão (ver `NewLaudoDialog`). O
 * `build()` do template só preenche o corpo.
 *
 * ---
 *
 * Como criar novos templates:
 *   1. Crie um arquivo `templates/<id>.ts` exportando uma constante
 *      do tipo `LaudoTemplate`.
 *   2. Use os helpers de `helpers.ts` (`heading`, `paragraph`,
 *      `styledParagraph`, `sentence`, `quesitoList`, `signatureBlock`,
 *      `field({key})`) para montar o `build(title, occurrence)`.
 *   3. Importe e acrescente em `TEMPLATES` abaixo.
 *
 * Como "salvar um laudo editado como template":
 *   Hoje templates são código TypeScript — não há UI pra salvar a
 *   partir do editor. O fluxo é: você cria o laudo, salva, e me
 *   pede pra transformar o `.sicrodoc` num arquivo de template.
 *   No futuro podemos adicionar um botão "Salvar como modelo".
 *
 * Aliases legados: laudos antigos com `template_id` apontando pros
 * 8 modelos removidos (sinistro_transito, arrombamento, etc.)
 * caem no fallback `documento_em_branco` em `findTemplate`.
 */

import { documentoEmBranco } from "./documento-em-branco";

export type { LaudoTemplate, OccurrenceContext } from "./types";

export const TEMPLATES = [documentoEmBranco] as const;

/** Retorna o template pelo `id` ou cai no padrão (`documento_em_branco`). */
export function findTemplate(id: string) {
  return TEMPLATES.find((t) => t.id === id) ?? documentoEmBranco;
}

/**
 * Variante que mapeia IDs legados (templates removidos) para o
 * padrão atual. Útil ao reabrir laudos antigos que ainda guardam
 * `template_id` apontando pra um modelo que não existe mais.
 */
const LEGACY_ALIASES = new Set<string>([
  "documento_livre",
  "em_branco",
  "generico",
  "sinistro_transito",
  "sinistro_transito_simples",
  "arrombamento",
  "local_crime",
  "avaliacao_merceologica",
  "constatacao",
  "exame_veicular",
]);

export function findTemplateWithLegacyAlias(id: string) {
  if (LEGACY_ALIASES.has(id)) return documentoEmBranco;
  return findTemplate(id);
}
