/**
 * Template "Documento em branco" — único modelo atual.
 *
 * Corpo: VAZIO (um único parágrafo), como um documento novo do Word —
 * sem título pré-inserido, sem cabeçalho e sem timbre. O título do laudo
 * vive em `doc.title` (não no corpo); o cabeçalho é uma camada à parte
 * (`doc.header`, hoje desligada por padrão) e o timbre institucional
 * será reconstruído depois, nos MODELOS periciais.
 *
 * Esta folha em branco também é a base para a IMPORTAÇÃO de .docx: o
 * conteúdo importado substitui o corpo vazio.
 *
 * Para criar novos templates no futuro, basta acrescentar um arquivo
 * irmão exportando `LaudoTemplate` e registrá-lo em `index.ts`.
 */

import type { LaudoTemplate } from "./types";

export const documentoEmBranco: LaudoTemplate = {
  id: "documento_em_branco",
  name: "Documento em branco",
  description:
    "Documento limpo, como uma folha em branco do Word. Você escreve o " +
    "laudo do zero (ou importa de um .docx).",
  category: "Genérico",
  // Corpo VAZIO: um único parágrafo. Ignora o título (vai em doc.title).
  build: () => ({
    type: "doc",
    content: [{ type: "paragraph" }],
  }),
};
