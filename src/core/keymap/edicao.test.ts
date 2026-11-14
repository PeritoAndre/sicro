import { describe, expect, it } from "vitest";

import { normalizeBinding } from "../keymap";
import { ACTION_BY_ID, colisoes, mesmaTela, SHORTCUT_ACTIONS, type ShortcutAction } from "../keymapActions";
import { EDICAO_ACTIONS, VIDEO_NA_EDICAO } from "./edicao";

const padrao = (id: string) => ACTION_BY_ID[id]?.defaultBinding ?? "";
const acao = (id: string): ShortcutAction => {
  const a = ACTION_BY_ID[id];
  if (!a) throw new Error(`sem ${id}`);
  return a;
};

describe("atalhos da Edição", () => {
  it("nenhum par de ações da mesma tela tem a mesma combinação padrão", () => {
    const pares: string[] = [];
    SHORTCUT_ACTIONS.forEach((a, i) => {
      for (const b of SHORTCUT_ACTIONS.slice(i + 1)) {
        if (a.defaultBinding && normalizeBinding(a.defaultBinding) === normalizeBinding(b.defaultBinding) && mesmaTela(a, b)) {
          pares.push(`${a.id} × ${b.id} (${a.defaultBinding})`);
        }
      }
    });
    expect(pares).toEqual([]);
  });

  it("ids únicos, escopo edicao e combinação canônica", () => {
    const ids = SHORTCUT_ACTIONS.map((a) => a.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const a of EDICAO_ACTIONS) {
      expect(a.id.startsWith("edicao.")).toBe(true);
      expect(a.scope).toBe("edicao");
    }
    // `colisoes` compara o texto: o catálogo inteiro tem de estar na forma canônica.
    for (const a of SHORTCUT_ACTIONS) expect(normalizeBinding(a.defaultBinding), a.id).toBe(a.defaultBinding);
  });

  it("os ids reaproveitados existem no Vídeo", () => {
    expect(VIDEO_NA_EDICAO.length).toBe(24);
    for (const id of VIDEO_NA_EDICAO) expect(ACTION_BY_ID[id]?.scope).toBe("video");
  });

  it("a regra de conflito junta a Edição com os ids reaproveitados", () => {
    // Na Edição o Espaço é "Reproduzir / pausar".
    expect(colisoes(acao("edicao.dividir"), "Space", padrao)).toEqual(["video.playPause"]);
    expect(colisoes(acao("video.playPause"), "Ctrl+B", (id) => (id === "video.playPause" ? "Ctrl+B" : padrao(id)))).toEqual([
      "edicao.dividir",
    ]);
    // "M": Marcar evento na análise, Marcador na Edição; telas diferentes.
    expect(colisoes(acao("edicao.marcador"), "M", padrao)).toEqual([]);
    expect(colisoes(acao("video.quickEvent"), "M", padrao)).toEqual([]);
    // Ação só da análise não colide com a Edição.
    expect(colisoes(acao("video.speedUp"), "Ctrl+B", padrao)).toEqual([]);
    // "geral" vale em toda tela.
    expect(colisoes(acao("edicao.salvar"), "Ctrl+O", padrao)).toEqual(["geral.trocarCaso"]);
    expect(colisoes(acao("geral.novoCaso"), "Ctrl+E", padrao)).toContain("edicao.exportar");
    // Duas ações da Edição.
    expect(colisoes(acao("edicao.remover"), "Ctrl+D", padrao)).toEqual(["edicao.duplicar"]);
  });
});
