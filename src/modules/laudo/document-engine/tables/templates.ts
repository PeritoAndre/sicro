/**
 * Tabelas-modelo periciais.
 *
 * F7 — Catálogo de tabelas pré-formatadas para os usos mais comuns
 * em laudo pericial. Cada template gera JSONContent do tipo `table`
 * compatível com `@tiptap/extension-table`.
 *
 * Estrutura: array de linhas; cada linha é um array de células com
 * header? + texto inicial. O comando `insertTableTemplate(id)`
 * monta o nó TipTap correspondente.
 */

import type { JSONContent } from "@tiptap/core";
import { generateTableId } from "../nodes/SicroTable";
import { seedEqualColWidths } from "./tableDefaults";

export interface TableTemplateDefinition {
  id: string;
  /** Nome curto exibido no painel. */
  label: string;
  /** Descrição curta — tooltip. */
  description: string;
  /** Constrói o nó TipTap da tabela. */
  build: () => JSONContent;
}

// ---------------------------------------------------------------------------
// Helpers internos para montar células.

function cell(text: string, header = false): JSONContent {
  return {
    type: header ? "tableHeader" : "tableCell",
    // F1.2 — colwidth semeado depois por `applyColWidths` (precisamos saber
    // quantas colunas a tabela tem). Aqui inicia null e é preenchido em
    // buildTable.
    attrs: { colspan: 1, rowspan: 1, colwidth: null },
    content: [
      {
        type: "paragraph",
        content: text ? [{ type: "text", text }] : [],
      },
    ],
  };
}

function row(cells: JSONContent[]): JSONContent {
  return { type: "tableRow", content: cells };
}

/**
 * Monta o nó `table`, gera um `id` estável e SEMEIA `colwidth` igualmente
 * em cada coluna (F1.2 — com `table-layout: fixed`, colunas sem largura
 * colapsam). A contagem de colunas vem da primeira linha (somando colspans).
 */
function buildTable(rows: JSONContent[]): JSONContent {
  const firstRow = rows[0];
  const cols = (firstRow?.content ?? []).reduce(
    (sum, c) => sum + (Number(c.attrs?.colspan) || 1),
    0,
  );
  const widths = seedEqualColWidths(cols || 1);
  // Atribui a cada célula a largura da(s) coluna(s) que ela ocupa.
  for (const r of rows) {
    let col = 0;
    for (const c of r.content ?? []) {
      const span = Number(c.attrs?.colspan) || 1;
      const slice = widths.slice(col, col + span);
      c.attrs = { ...(c.attrs ?? {}), colwidth: slice.length ? slice : null };
      col += span;
    }
  }
  return {
    type: "table",
    attrs: { id: generateTableId() },
    content: rows,
  };
}

// ---------------------------------------------------------------------------
// Catálogo.

const dadosLocal: TableTemplateDefinition = {
  id: "dados_local",
  label: "Dados do local",
  description:
    "Endereço, coordenadas, condições climáticas, iluminação e tipo de via.",
  build: () =>
    buildTable([
      row([cell("Campo", true), cell("Valor", true)]),
      row([cell("Endereço"), cell("")]),
      row([cell("Município"), cell("")]),
      row([cell("Coordenadas (lat, lon)"), cell("")]),
      row([cell("Condições climáticas"), cell("")]),
      row([cell("Condições de iluminação"), cell("")]),
      row([cell("Tipo de via"), cell("")]),
      row([cell("Sinalização"), cell("")]),
    ]),
};

const dadosVeiculos: TableTemplateDefinition = {
  id: "dados_veiculos",
  label: "Dados dos veículos",
  description: "Marca, modelo, placa, chassi, cor e estado de conservação.",
  build: () =>
    buildTable([
      row([
        cell("Veículo", true),
        cell("Marca/Modelo", true),
        cell("Placa", true),
        cell("Chassi", true),
        cell("Cor", true),
        cell("Estado", true),
      ]),
      row([cell("Veículo 1"), cell(""), cell(""), cell(""), cell(""), cell("")]),
      row([cell("Veículo 2"), cell(""), cell(""), cell(""), cell(""), cell("")]),
    ]),
};

const dadosEnvolvidos: TableTemplateDefinition = {
  id: "dados_envolvidos",
  label: "Dados dos envolvidos",
  description: "Nome, idade, papel (condutor/passageiro/pedestre) e estado.",
  build: () =>
    buildTable([
      row([
        cell("Envolvido", true),
        cell("Nome", true),
        cell("Idade", true),
        cell("Papel", true),
        cell("Estado", true),
      ]),
      row([cell("1"), cell(""), cell(""), cell(""), cell("")]),
      row([cell("2"), cell(""), cell(""), cell(""), cell("")]),
    ]),
};

const vestigios: TableTemplateDefinition = {
  id: "vestigios",
  label: "Vestígios identificados",
  description: "Tipo, posição, dimensão e observações.",
  build: () =>
    buildTable([
      row([
        cell("Nº", true),
        cell("Tipo de vestígio", true),
        cell("Posição", true),
        cell("Dimensão", true),
        cell("Observações", true),
      ]),
      row([cell("1"), cell(""), cell(""), cell(""), cell("")]),
      row([cell("2"), cell(""), cell(""), cell(""), cell("")]),
      row([cell("3"), cell(""), cell(""), cell(""), cell("")]),
    ]),
};

const medicoes: TableTemplateDefinition = {
  id: "medicoes",
  label: "Medições",
  description: "Medições de distância, altura, ângulo e similares.",
  build: () =>
    buildTable([
      row([
        cell("Nº", true),
        cell("Descrição da medição", true),
        cell("Valor", true),
        cell("Unidade", true),
      ]),
      row([cell("1"), cell(""), cell(""), cell("m")]),
      row([cell("2"), cell(""), cell(""), cell("m")]),
      row([cell("3"), cell(""), cell(""), cell("°")]),
    ]),
};

const cronologia: TableTemplateDefinition = {
  id: "cronologia",
  label: "Cronologia de eventos",
  description: "Sequência temporal dos eventos relevantes.",
  build: () =>
    buildTable([
      row([
        cell("Hora", true),
        cell("Evento", true),
        cell("Fonte da informação", true),
      ]),
      row([cell(""), cell(""), cell("")]),
      row([cell(""), cell(""), cell("")]),
      row([cell(""), cell(""), cell("")]),
    ]),
};

const materiaisExaminados: TableTemplateDefinition = {
  id: "materiais_examinados",
  label: "Materiais examinados",
  description:
    "Catálogo de materiais entregues à perícia (avaliação/constatação).",
  build: () =>
    buildTable([
      row([
        cell("Item", true),
        cell("Descrição", true),
        cell("Quantidade", true),
        cell("Estado", true),
      ]),
      row([cell("1"), cell(""), cell(""), cell("")]),
      row([cell("2"), cell(""), cell(""), cell("")]),
    ]),
};

const condicoesAmbientais: TableTemplateDefinition = {
  id: "condicoes_ambientais",
  label: "Condições ambientais",
  description: "Temperatura, umidade, vento, visibilidade.",
  build: () =>
    buildTable([
      row([cell("Parâmetro", true), cell("Valor / observação", true)]),
      row([cell("Temperatura"), cell("")]),
      row([cell("Umidade relativa"), cell("")]),
      row([cell("Vento"), cell("")]),
      row([cell("Visibilidade"), cell("")]),
      row([cell("Precipitação"), cell("")]),
    ]),
};

const midiasVideo: TableTemplateDefinition = {
  id: "midias_video",
  label: "Mídias / vídeos analisados",
  description: "Origem, duração, qualidade, conteúdo.",
  build: () =>
    buildTable([
      row([
        cell("Nº", true),
        cell("Origem", true),
        cell("Duração", true),
        cell("Qualidade", true),
        cell("Conteúdo relevante", true),
      ]),
      row([cell("1"), cell(""), cell(""), cell(""), cell("")]),
      row([cell("2"), cell(""), cell(""), cell(""), cell("")]),
    ]),
};

// ---------------------------------------------------------------------------
// Registry.
//
// F7.1 — Removido `simples_3x3` em favor do `InsertTableDialog`, que
// permite ao perito definir N×M dinamicamente. Os templates restantes
// continuam servindo o fluxo "insira um modelo pré-formatado".

export const TABLE_TEMPLATES: ReadonlyArray<TableTemplateDefinition> = [
  dadosLocal,
  dadosVeiculos,
  dadosEnvolvidos,
  vestigios,
  medicoes,
  cronologia,
  materiaisExaminados,
  condicoesAmbientais,
  midiasVideo,
];

/** Lookup rápido por id. */
export const TABLE_TEMPLATES_BY_ID: ReadonlyMap<string, TableTemplateDefinition> =
  new Map(TABLE_TEMPLATES.map((t) => [t.id, t]));

/** Resolve um template ou null. */
export function findTableTemplate(
  id: string | null | undefined,
): TableTemplateDefinition | null {
  if (!id) return null;
  return TABLE_TEMPLATES_BY_ID.get(id) ?? null;
}
