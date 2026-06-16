/**
 * Núcleo de atalhos de teclado (puro, sem React/DOM além do tipo de evento).
 *
 * Canonicaliza teclas por `KeyboardEvent.code` (físico, independente de layout)
 * — assim "V" é sempre a tecla V, e não muda com acento/teclado. Captura e
 * casamento usam EXATAMENTE a mesma função, então o que o usuário grava é o
 * que casa depois (determinístico). Modificadores explícitos: Ctrl, Alt, Shift.
 *
 * §13: atalhos são conveniência de UI — não alteram nenhuma lógica forense.
 */

/** Combinação canônica, ex.: "V", "Ctrl+S", "Ctrl+Shift+Z", "Shift+/". */
export type Binding = string;

const MODIFIER_CODES = new Set([
  "ShiftLeft",
  "ShiftRight",
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
]);

/** Mapa de `KeyboardEvent.code` → token legível e estável. */
const CODE_TOKENS: Record<string, string> = {
  Minus: "-",
  Equal: "=",
  Slash: "/",
  Backslash: "\\",
  Comma: ",",
  Period: ".",
  Semicolon: ";",
  Quote: "'",
  BracketLeft: "[",
  BracketRight: "]",
  Backquote: "`",
  Space: "Space",
  Escape: "Esc",
  Enter: "Enter",
  Tab: "Tab",
  Backspace: "Backspace",
  Delete: "Del",
  Insert: "Ins",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Home: "Home",
  End: "End",
  PageUp: "PgUp",
  PageDown: "PgDn",
};

/** Converte um `code` físico no token canônico do atalho. */
export function codeToToken(code: string): string {
  if (code.startsWith("Key")) return code.slice(3); // KeyV → V
  if (code.startsWith("Digit")) return code.slice(5); // Digit1 → 1
  if (code.startsWith("Numpad")) return code.slice(6); // Numpad1 → 1
  if (/^F\d{1,2}$/.test(code)) return code; // F1..F12
  return CODE_TOKENS[code] ?? code;
}

/**
 * Deriva a combinação canônica de um evento de teclado. Retorna `""` para
 * pressionamentos só-de-modificador (Shift/Ctrl/Alt sozinhos) — úteis ignorar.
 */
export function eventToBinding(e: {
  code: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}): Binding {
  if (MODIFIER_CODES.has(e.code)) return "";
  const parts: string[] = [];
  if (e.ctrlKey || e.metaKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  parts.push(codeToToken(e.code));
  return parts.join("+");
}

const MOD_ORDER: Record<string, number> = { Ctrl: 0, Alt: 1, Shift: 2 };

/**
 * Normaliza uma combinação digitada à forma canônica (ordem dos modificadores
 * Ctrl→Alt→Shift, tecla por último). Tolerante a "Cmd"/"Control"/"Option".
 */
export function normalizeBinding(binding: Binding): Binding {
  const raw = binding
    .split("+")
    .map((p) => p.trim())
    .filter(Boolean);
  if (raw.length === 0) return "";
  const mods: string[] = [];
  let key = "";
  for (const p of raw) {
    const low = p.toLowerCase();
    if (low === "ctrl" || low === "control" || low === "cmd" || low === "meta") {
      if (!mods.includes("Ctrl")) mods.push("Ctrl");
    } else if (low === "alt" || low === "option") {
      if (!mods.includes("Alt")) mods.push("Alt");
    } else if (low === "shift") {
      if (!mods.includes("Shift")) mods.push("Shift");
    } else {
      key = p.length === 1 ? p.toUpperCase() : p;
    }
  }
  mods.sort((a, b) => (MOD_ORDER[a] ?? 9) - (MOD_ORDER[b] ?? 9));
  return key ? [...mods, key].join("+") : mods.join("+");
}

/** `true` se o evento corresponde à combinação (ambos canonizados). */
export function matchesBinding(
  e: {
    code: string;
    ctrlKey: boolean;
    metaKey: boolean;
    altKey: boolean;
    shiftKey: boolean;
  },
  binding: Binding,
): boolean {
  if (!binding) return false;
  return eventToBinding(e) === normalizeBinding(binding);
}

/** Partes de uma combinação para renderizar (cada uma vira um <kbd>). */
export function bindingParts(binding: Binding): string[] {
  if (!binding) return [];
  return normalizeBinding(binding).split("+");
}

/** Rótulo amigável de um token isolado (para exibição). */
export function prettyToken(token: string): string {
  const map: Record<string, string> = {
    Space: "Espaço",
    Esc: "Esc",
    Del: "Del",
    Up: "↑",
    Down: "↓",
    Left: "←",
    Right: "→",
    Ctrl: "Ctrl",
    Alt: "Alt",
    Shift: "Shift",
  };
  return map[token] ?? token;
}

/** Combinação inteira para exibição compacta, ex.: "Ctrl + S". */
export function formatBinding(binding: Binding): string {
  return bindingParts(binding).map(prettyToken).join(" + ");
}

/** `true` se o alvo do evento é um campo editável (input/textarea/cE). */
export function isEditableTarget(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el) return false;
  const tag = el.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    el.isContentEditable === true
  );
}
