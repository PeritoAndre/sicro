import { describe, expect, it } from "vitest";
import {
  bindingParts,
  codeToToken,
  eventToBinding,
  formatBinding,
  isEditableTarget,
  matchesBinding,
  normalizeBinding,
} from "./keymap";

const ev = (
  code: string,
  mods: Partial<{ ctrl: boolean; meta: boolean; alt: boolean; shift: boolean }> = {},
) => ({
  code,
  ctrlKey: !!mods.ctrl,
  metaKey: !!mods.meta,
  altKey: !!mods.alt,
  shiftKey: !!mods.shift,
});

describe("codeToToken", () => {
  it("mapeia letras, dígitos e teclas nomeadas", () => {
    expect(codeToToken("KeyV")).toBe("V");
    expect(codeToToken("Digit1")).toBe("1");
    expect(codeToToken("Numpad3")).toBe("3");
    expect(codeToToken("Escape")).toBe("Esc");
    expect(codeToToken("ArrowUp")).toBe("Up");
    expect(codeToToken("Slash")).toBe("/");
    expect(codeToToken("F5")).toBe("F5");
  });
});

describe("eventToBinding", () => {
  it("tecla simples", () => {
    expect(eventToBinding(ev("KeyV"))).toBe("V");
  });
  it("com modificadores na ordem Ctrl→Alt→Shift", () => {
    expect(eventToBinding(ev("KeyZ", { ctrl: true, shift: true }))).toBe("Ctrl+Shift+Z");
    expect(eventToBinding(ev("KeyS", { meta: true }))).toBe("Ctrl+S"); // meta = Ctrl
  });
  it("ignora pressionamento só-de-modificador", () => {
    expect(eventToBinding(ev("ShiftLeft", { shift: true }))).toBe("");
    expect(eventToBinding(ev("ControlRight", { ctrl: true }))).toBe("");
  });
});

describe("normalizeBinding", () => {
  it("reordena modificadores e normaliza apelidos", () => {
    expect(normalizeBinding("shift+ctrl+z")).toBe("Ctrl+Shift+Z");
    expect(normalizeBinding("Cmd+S")).toBe("Ctrl+S");
    expect(normalizeBinding("alt+1")).toBe("Alt+1");
  });
});

describe("matchesBinding", () => {
  it("casa evento com a combinação independentemente da ordem escrita", () => {
    expect(matchesBinding(ev("KeyZ", { ctrl: true, shift: true }), "shift+ctrl+z")).toBe(true);
    expect(matchesBinding(ev("KeyV"), "V")).toBe(true);
    expect(matchesBinding(ev("KeyV", { ctrl: true }), "V")).toBe(false);
  });
  it("string vazia nunca casa", () => {
    expect(matchesBinding(ev("KeyV"), "")).toBe(false);
  });
});

describe("formatBinding / bindingParts", () => {
  it("parte e formata para exibição", () => {
    expect(bindingParts("Ctrl+S")).toEqual(["Ctrl", "S"]);
    expect(formatBinding("Ctrl+Shift+Z")).toBe("Ctrl + Shift + Z");
    expect(formatBinding("Space")).toBe("Espaço");
  });
});

describe("isEditableTarget", () => {
  const stub = (tagName: string, isContentEditable = false) =>
    ({ tagName, isContentEditable }) as unknown as EventTarget;
  it("detecta campos editáveis", () => {
    expect(isEditableTarget(stub("INPUT"))).toBe(true);
    expect(isEditableTarget(stub("TEXTAREA"))).toBe(true);
    expect(isEditableTarget(stub("SELECT"))).toBe(true);
    expect(isEditableTarget(stub("DIV", true))).toBe(true);
    expect(isEditableTarget(stub("DIV", false))).toBe(false);
    expect(isEditableTarget(null)).toBe(false);
  });
});
