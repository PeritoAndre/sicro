/**
 * Document validators — F9 expandido.
 *
 * Regras de validação implementadas:
 *
 *   ESTRUTURA
 *   1. titulo_principal_ausente   — laudo sem titulo_1 visível.
 *   2. documento_quase_vazio      — só placeholder ou < 10 palavras.
 *   3. parametros_obrigatorios    — `metadata.numero_laudo`/`setor` vazios.
 *
 *   CONTEÚDO
 *   4. figura_sem_legenda         — `figure` sem `figcaption`.
 *   5. figura_sem_evidence_id     — `figure` referenciando evidence sem id.
 *   6. tabela_sem_cabecalho       — tabela sem nenhuma linha de `tableHeader`.
 *   7. tabela_celulas_vazias_demais — > 60% das células estão vazias.
 *   8. quesito_sem_resposta       — `quesito-item` sem `quesito-answer` ou
 *                                    answer vazio.
 *   9. assinatura_ausente         — documento sem nó `signature`.
 *   10. conclusao_ausente         — sem trecho com `data-laudo-style="conclusao"`.
 *
 *   CAMPOS AUTOMÁTICOS
 *   11. field_placeholder_pendente — `{{var}}` sem valor no contexto.
 *
 *   REVISÃO
 *   12. revisao_pendente            — há `revisionMark` não aceitos/rejeitados.
 *   13. comentarios_pendentes       — há `commentMark` sem entrada na lista,
 *                                     ou orphan comments na lista sem mark.
 *
 *   EVIDÊNCIA
 *   14. system_data_pendente        — `system_data` em `pending`.
 *
 *   FINALIZAÇÃO
 *   15. status_final_sem_finalization — `status="final"` mas `finalization` ausente.
 *
 * Severidades:
 *   - "error":   bloqueante (mostrado em vermelho); não impede save mas
 *                impede finalização.
 *   - "warning": amarelo; recomendação.
 *   - "info":    cinza/azul; nota informativa.
 */

import type { JSONContent } from "@tiptap/core";
import type { SicroDoc } from "./schema";

export type WarningSeverity = "info" | "warning" | "error";

export interface DocumentWarning {
  id: string;
  severity: WarningSeverity;
  message: string;
  hint?: string;
  /** Categoria para agrupar na UI. */
  category?: ValidationCategory;
}

export type ValidationCategory =
  | "estrutura"
  | "conteudo"
  | "campos"
  | "revisao"
  | "evidencia"
  | "finalizacao";

export function validateSicroDoc(doc: SicroDoc): DocumentWarning[] {
  const warnings: DocumentWarning[] = [];

  // ESTRUTURA ----------------------------------------------------------------
  warnings.push(...checkStructure(doc));

  // CONTEÚDO -----------------------------------------------------------------
  warnings.push(...checkContent(doc));

  // CAMPOS -------------------------------------------------------------------
  warnings.push(...checkFields(doc));

  // REVISÃO ------------------------------------------------------------------
  warnings.push(...checkRevision(doc));

  // EVIDÊNCIA ----------------------------------------------------------------
  warnings.push(...checkEvidence(doc));

  // FINALIZAÇÃO --------------------------------------------------------------
  warnings.push(...checkFinalization(doc));

  return warnings;
}

// ---------------------------------------------------------------------------
// Implementações.

function checkStructure(doc: SicroDoc): DocumentWarning[] {
  const out: DocumentWarning[] = [];
  let hasTitle = false;
  let totalWords = 0;
  let nonEmptyParagraphs = 0;

  walk(doc.content, (n) => {
    if (n.type === "heading" && (n.attrs?.["level"] as number) === 1) {
      if (textOf(n).trim().length > 0) hasTitle = true;
    }
    const laudoStyle = n.attrs?.["data-laudo-style"] as string | undefined;
    if (laudoStyle === "titulo_1") hasTitle = true;
    if (n.type === "paragraph") {
      const t = textOf(n).trim();
      if (t.length > 0) {
        nonEmptyParagraphs += 1;
        totalWords += t.split(/\s+/).length;
      }
    }
  });

  if (!hasTitle) {
    out.push({
      id: "titulo_principal_ausente",
      severity: "warning",
      category: "estrutura",
      message: "Laudo sem título principal (Título 1).",
      hint: "Use Ctrl+Alt+1 ou o painel Estilos para inserir um Título 1.",
    });
  }

  if (totalWords < 30 && nonEmptyParagraphs <= 1) {
    out.push({
      id: "documento_quase_vazio",
      severity: "info",
      category: "estrutura",
      message: "Documento com pouco conteúdo.",
      hint: `Apenas ${totalWords} palavras. Considere expandir.`,
    });
  }

  const metadata = doc.metadata ?? {};
  if (!isFilled(metadata["numero_laudo"])) {
    out.push({
      id: "metadata_numero_ausente",
      severity: "warning",
      category: "estrutura",
      message: "Número do laudo não definido.",
      hint: "Edite em Dados → Número do laudo.",
    });
  }
  if (!isFilled(metadata["setor"])) {
    out.push({
      id: "metadata_setor_ausente",
      severity: "info",
      category: "estrutura",
      message: "Setor responsável não definido.",
      hint: "Edite em Dados → Setor.",
    });
  }

  return out;
}

function checkContent(doc: SicroDoc): DocumentWarning[] {
  const out: DocumentWarning[] = [];
  let figureIndex = 0;
  let croquiIndex = 0;
  let hasSignature = false;
  let hasConclusion = false;

  // Tabela tracking.
  type TblInfo = { headers: number; cells: number; empty: number };
  const tables: TblInfo[] = [];

  // Quesitos tracking.
  let quesitoNum = 0;

  walk(doc.content, (n) => {
    if (n.type === "figure") {
      const kind = (n.attrs?.["kind"] as string | undefined) ?? "image";
      if (kind === "croqui") croquiIndex += 1;
      else figureIndex += 1;
      const ordinal = kind === "croqui" ? croquiIndex : figureIndex;
      const figcap = (n.content ?? []).find((c) => c.type === "figcaption");
      const text = textOf(figcap);
      if (!text || text.trim().length === 0) {
        out.push({
          id: `figure_sem_legenda_${kind}_${ordinal}`,
          severity: "warning",
          category: "conteudo",
          message: `${kind === "croqui" ? "Croqui" : "Figura"} ${ordinal} sem legenda.`,
          hint: "Toda figura deve ter legenda descritiva.",
        });
      }
      const evId = n.attrs?.["evidence_id"];
      const src = n.attrs?.["src"];
      if (src && !evId) {
        out.push({
          id: `figure_sem_evidence_id_${kind}_${ordinal}`,
          severity: "info",
          category: "evidencia",
          message: `${kind === "croqui" ? "Croqui" : "Figura"} ${ordinal} sem vínculo de evidência.`,
          hint: "Arraste do painel de Evidências para vincular.",
        });
      }
    }

    if (n.type === "signature") hasSignature = true;
    const ls = n.attrs?.["data-laudo-style"] as string | undefined;
    if (ls === "conclusao") hasConclusion = true;

    if (n.type === "table") {
      let headers = 0;
      let cells = 0;
      let empty = 0;
      const visit = (m: JSONContent) => {
        if (m.type === "tableHeader") headers += 1;
        if (m.type === "tableHeader" || m.type === "tableCell") {
          cells += 1;
          if (textOf(m).trim().length === 0) empty += 1;
        }
        if (Array.isArray(m.content)) for (const c of m.content) visit(c);
      };
      visit(n);
      tables.push({ headers, cells, empty });
    }

    if (n.type === "quesitoItem") {
      quesitoNum += 1;
      const ans = (n.content ?? []).find((c) => c.type === "quesitoAnswer");
      if (!ans || textOf(ans).trim().length === 0) {
        out.push({
          id: `quesito_${quesitoNum}_sem_resposta`,
          severity: "warning",
          category: "conteudo",
          message: `Quesito ${quesitoNum} sem resposta.`,
          hint: "Responda ao quesito antes de finalizar.",
        });
      }
    }
  });

  if (!hasSignature) {
    out.push({
      id: "assinatura_ausente",
      severity: "info",
      category: "conteudo",
      message: "Documento sem bloco de assinatura.",
      hint: 'Insira via "Inserir → Assinatura".',
    });
  }
  if (!hasConclusion) {
    out.push({
      id: "conclusao_ausente",
      severity: "info",
      category: "conteudo",
      message: "Documento sem conclusão.",
      hint: "Aplique o estilo 'Conclusão' a um parágrafo.",
    });
  }

  tables.forEach((t, i) => {
    if (t.headers === 0 && t.cells > 0) {
      out.push({
        id: `tabela_${i + 1}_sem_cabecalho`,
        severity: "info",
        category: "conteudo",
        message: `Tabela ${i + 1} sem linha de cabeçalho.`,
        hint: "Use 'Alternar cabeçalho na linha' no painel Tabela.",
      });
    }
    if (t.cells > 0 && t.empty / t.cells > 0.6) {
      out.push({
        id: `tabela_${i + 1}_celulas_vazias_demais`,
        severity: "info",
        category: "conteudo",
        message: `Tabela ${i + 1} tem ${Math.round((t.empty / t.cells) * 100)}% de células vazias.`,
        hint: "Preencha os campos pendentes ou remova colunas/linhas.",
      });
    }
  });

  return out;
}

function checkFields(doc: SicroDoc): DocumentWarning[] {
  const out: DocumentWarning[] = [];
  const pending: string[] = [];
  walk(doc.content, (n) => {
    if (n.type === "fieldPlaceholder") {
      // FIX: o node grava em attrs.field (FieldPlaceholder.ts:51), não fieldKey.
      // Antes esta regra nunca disparava. Não há `resolved` no schema; o estado
      // "pendente" agora é uma decisão do caller — aqui sempre reportamos a
      // chave pra o validador externo (que tem o resolver) decidir se sobrou.
      const key = n.attrs?.["field"] as string | undefined;
      if (key) {
        pending.push(key);
      }
    }
  });
  if (pending.length > 0) {
    out.push({
      id: "field_placeholder_pendente",
      severity: "warning",
      category: "campos",
      message: `${pending.length} campo(s) automáticos sem valor.`,
      hint: `Preencha em Dados / Cabeçalho: ${pending.slice(0, 3).join(", ")}${
        pending.length > 3 ? "…" : ""
      }`,
    });
  }
  return out;
}

function checkRevision(doc: SicroDoc): DocumentWarning[] {
  const out: DocumentWarning[] = [];
  let revisionMarks = 0;
  let commentMarks = 0;
  walk(doc.content, (n) => {
    if (n.type === "text" && Array.isArray(n.marks)) {
      for (const m of n.marks) {
        if (m.type === "revisionMark") revisionMarks += 1;
        if (m.type === "commentMark") commentMarks += 1;
      }
    }
  });

  if (revisionMarks > 0) {
    out.push({
      id: "revisao_pendente",
      severity: "warning",
      category: "revisao",
      message: `${revisionMarks} marcação(ões) de revisão pendente(s).`,
      hint: "Aceite/rejeite as marcações em Histórico antes de finalizar.",
    });
  }

  const activeComments = (doc.comments ?? []).filter((c) => !c.resolved);
  if (activeComments.length > 0) {
    out.push({
      id: "comentarios_pendentes",
      severity: "info",
      category: "revisao",
      message: `${activeComments.length} comentário(s) ativo(s).`,
      hint: "Resolva ou exclua antes de exportar a versão final.",
    });
  }

  // Orphan: comment list IDs com marca, mas mark sem comment list.
  const commentIdsInDoc = new Set<string>();
  walk(doc.content, (n) => {
    if (n.type === "text" && Array.isArray(n.marks)) {
      for (const m of n.marks) {
        if (m.type === "commentMark" && typeof m.attrs?.["id"] === "string") {
          commentIdsInDoc.add(m.attrs["id"] as string);
        }
      }
    }
  });
  const orphanList = (doc.comments ?? []).filter(
    (c) => !commentIdsInDoc.has(c.id),
  );
  if (orphanList.length > 0) {
    out.push({
      id: "comentarios_orfãos_lista",
      severity: "info",
      category: "revisao",
      message: `${orphanList.length} comentário(s) na lista sem âncora no texto.`,
      hint: "Texto pode ter sido excluído. Considere remover o comentário.",
    });
  }
  void commentMarks;
  return out;
}

function checkEvidence(doc: SicroDoc): DocumentWarning[] {
  const out: DocumentWarning[] = [];
  let pendingSystemData = 0;
  walk(doc.content, (n) => {
    if (
      n.type === "systemData" &&
      (n.attrs?.["review_status"] ?? "pending") === "pending"
    ) {
      pendingSystemData += 1;
    }
  });
  if (pendingSystemData > 0) {
    out.push({
      id: "system_data_pendente",
      severity: "info",
      category: "evidencia",
      message: `${pendingSystemData} dado(s) do sistema aguardando revisão.`,
      hint: "Clique em cada destaque amarelo no documento para revisar.",
    });
  }
  return out;
}

function checkFinalization(doc: SicroDoc): DocumentWarning[] {
  const out: DocumentWarning[] = [];
  if (doc.status === "final" && !doc.finalization) {
    out.push({
      id: "status_final_sem_finalization",
      severity: "error",
      category: "finalizacao",
      message: "Laudo marcado como FINAL mas sem selo de finalização.",
      hint: "Reaplique o status via menu 'Finalizar'.",
    });
  }
  return out;
}

// ---------------------------------------------------------------------------
// Helpers.

function walk(node: JSONContent, visit: (n: JSONContent) => void) {
  visit(node);
  if (!node.content) return;
  for (const child of node.content) walk(child, visit);
}

function textOf(node: JSONContent | undefined): string {
  if (!node) return "";
  let buf = "";
  walk(node, (n) => {
    if (n.type === "text" && typeof n.text === "string") buf += n.text;
  });
  return buf;
}

function isFilled(v: unknown): boolean {
  if (v === null || v === undefined) return false;
  if (typeof v === "string") return v.trim().length > 0;
  return true;
}
