/**
 * Tipo compartilhado de template de laudo.
 *
 * F5 — Os templates antigos viviam em `document-engine/templates.ts`
 * (arquivo único). Agora são arquivos separados em `templates/<id>.ts`
 * com este tipo comum. O barrel `templates/index.ts` reexporta o
 * registry.
 */

import type { JSONContent } from "@tiptap/core";

/** Shape mínimo do contexto de ocorrência (compat com legado). */
export interface OccurrenceContext {
  numero_bo?: string | null;
  protocolo?: string | null;
  requisicao?: string | null;
  oficio?: string | null;
  tipo_pericia?: string | null;
  municipio?: string | null;
  data_fato?: string | null;
  peritos?: string[];
}

export interface LaudoTemplate {
  id: string;
  name: string;
  description: string;
  category: string;
  /**
   * F5 — `build(title, occurrence?)`. O `occurrence` permanece na
   * assinatura por compat retro com o caller legado, mas os novos
   * templates devem usar `fieldPlaceholder` (resolvido em runtime)
   * em vez de materializar valores aqui.
   */
  build: (title: string, occurrence?: OccurrenceContext | null) => JSONContent;
}
