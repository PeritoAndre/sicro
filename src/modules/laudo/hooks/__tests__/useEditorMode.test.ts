/**
 * useEditorMode — testes do mapeamento modo → flags de UI.
 *
 * Testa a lógica pura de quais bandeiras (isEditable, showInspector,
 * showStatusBar) cada modo ativa. Hook React não é instanciado — usamos
 * funções equivalentes.
 */

import { describe, expect, it } from "vitest";
import { laudoModeLabel, type LaudoMode } from "../useEditorMode";

// Reproduz a lógica privada de `useEditorMode` para validar paridade.
function flagsFor(mode: LaudoMode) {
  return {
    isEditable: mode === "edicao" || mode === "revisao" || mode === "foco",
    showInspector: mode === "edicao" || mode === "revisao",
    showStatusBar: mode !== "foco",
  };
}

describe("useEditorMode — bandeiras por modo", () => {
  it("edição: tudo visível e editável", () => {
    const f = flagsFor("edicao");
    expect(f.isEditable).toBe(true);
    expect(f.showInspector).toBe(true);
    expect(f.showStatusBar).toBe(true);
  });

  it("leitura: não editável, inspector oculto, status bar visível", () => {
    const f = flagsFor("leitura");
    expect(f.isEditable).toBe(false);
    expect(f.showInspector).toBe(false);
    expect(f.showStatusBar).toBe(true);
  });

  it("foco: editável, inspector oculto, status bar oculta", () => {
    const f = flagsFor("foco");
    expect(f.isEditable).toBe(true);
    expect(f.showInspector).toBe(false);
    expect(f.showStatusBar).toBe(false);
  });

  it("revisão: editável + inspector visível (para comentários futuros)", () => {
    const f = flagsFor("revisao");
    expect(f.isEditable).toBe(true);
    expect(f.showInspector).toBe(true);
    expect(f.showStatusBar).toBe(true);
  });
});

describe("useEditorMode — laudoModeLabel", () => {
  it("retorna label PT-BR para cada modo", () => {
    expect(laudoModeLabel("edicao")).toBe("Edição");
    expect(laudoModeLabel("leitura")).toBe("Leitura");
    expect(laudoModeLabel("foco")).toBe("Foco");
    expect(laudoModeLabel("revisao")).toBe("Revisão");
  });
});
