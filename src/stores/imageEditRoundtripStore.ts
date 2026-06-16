/**
 * Pós-laudo S — Image Edit Round-trip Store
 *
 * Coordena a ida-e-volta entre o módulo Laudo e o módulo Imagem quando
 * o perito clica em "EDITAR" numa foto do laudo:
 *
 *   1. Laudo (FigureOverlay) → `startEdit({...})` → seta `state = "editing"`
 *      + `request` com workspace, source path e match key da figure;
 *      navega para `/imagem`.
 *
 *   2. Imagem (ImagemModule) lê o store ao montar; se `state === "editing"`,
 *      cria uma análise nova a partir de `request.source_relative_path`
 *      e abre o `ImageEditor` direto, mostrando um banner "Editando foto
 *      do laudo X — clique Salvar para voltar".
 *
 *   3. Imagem (ImageEditor) → usuário edita (crop, filtros, etc.) → clica
 *      "Salvar e voltar" → store recebe `completeEdit(output)` com o path
 *      do PNG renderizado → navega para `/laudo`.
 *
 *   4. Laudo (LaudoEditorView) lê o store no mount; se `state === "completed"`,
 *      atualiza o(s) Figure node(s) que apontam para `source_relative_path`
 *      pra usar `output_relative_path` em vez disso; depois chama
 *      `clearRoundtrip()` pra resetar o store.
 *
 * Idle states: `state === "idle"` e ambos `request`/`result` null. Após
 * `clearRoundtrip`, volta pra esse estado.
 *
 * Não persiste entre sessões — round-trip é transação curta (minutos).
 */

import { create } from "zustand";

export type RoundtripState = "idle" | "editing" | "completed";

export interface RoundtripRequest {
  /** Workspace ativo no momento do click. Imagem usa pra criar análise. */
  workspace_path: string;
  /** Path relativo (workspace) da foto que está no Figure do laudo. */
  source_relative_path: string;
  /** ID do laudo de origem (informativo). */
  laudo_id: string | null;
  /** Título do laudo (banner UX). */
  laudo_title: string | null;
}

export interface RoundtripResult {
  /** Path relativo (workspace) do PNG renderizado após edição/save. */
  output_relative_path: string;
  /** Fonte original (echo do request) — permite buscar a figure a atualizar. */
  source_relative_path: string;
}

interface ImageEditRoundtripState {
  state: RoundtripState;
  request: RoundtripRequest | null;
  result: RoundtripResult | null;

  startEdit: (req: RoundtripRequest) => void;
  completeEdit: (res: RoundtripResult) => void;
  /** Reseta para idle. Chamar após o laudo aplicar o resultado. */
  clearRoundtrip: () => void;
}

export const useImageEditRoundtripStore = create<ImageEditRoundtripState>(
  (set) => ({
    state: "idle",
    request: null,
    result: null,

    startEdit: (req) =>
      set({ state: "editing", request: req, result: null }),
    completeEdit: (res) => set({ state: "completed", result: res }),
    clearRoundtrip: () =>
      set({ state: "idle", request: null, result: null }),
  }),
);
