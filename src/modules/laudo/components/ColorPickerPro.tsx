/**
 * ColorPickerPro — seletor de cor "de verdade" estilo Word/Photoshop.
 *
 * Componente sem dependências externas. Combina:
 *   - Painel S/V (saturação × brilho) controlado por drag
 *   - Slider de Hue (0–360°)
 *   - Input hex (#RRGGBB)
 *   - Paleta de presets em grade (40 cores)
 *   - Linha de "cores recentes" (persistida em localStorage)
 *   - Botão "Sem cor / Remover"
 *
 * Estratégia: o estado interno é HSV. Quando o usuário escolhe via
 * paleta ou digita um hex, convertemos para HSV pra atualizar a UI.
 * Quando ele arrasta o pad ou o slider, atualizamos HSV diretamente.
 * O `onSelect` recebe sempre um hex `#rrggbb`.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import styles from "./ColorPickerPro.module.css";

// ===========================================================================
// Conversões HSV ↔ RGB ↔ HEX
// ===========================================================================

interface HSV {
  h: number; // 0-360
  s: number; // 0-1
  v: number; // 0-1
}

interface RGB {
  r: number; // 0-255
  g: number; // 0-255
  b: number; // 0-255
}

function hsvToRgb({ h, s, v }: HSV): RGB {
  const c = v * s;
  const hh = (h % 360) / 60;
  const x = c * (1 - Math.abs((hh % 2) - 1));
  let r = 0;
  let g = 0;
  let b = 0;
  if (hh >= 0 && hh < 1) {
    r = c;
    g = x;
  } else if (hh < 2) {
    r = x;
    g = c;
  } else if (hh < 3) {
    g = c;
    b = x;
  } else if (hh < 4) {
    g = x;
    b = c;
  } else if (hh < 5) {
    r = x;
    b = c;
  } else {
    r = c;
    b = x;
  }
  const m = v - c;
  return {
    r: Math.round((r + m) * 255),
    g: Math.round((g + m) * 255),
    b: Math.round((b + m) * 255),
  };
}

function rgbToHex({ r, g, b }: RGB): string {
  const toH = (n: number) => n.toString(16).padStart(2, "0");
  return `#${toH(r)}${toH(g)}${toH(b)}`;
}

function hexToRgb(hex: string): RGB | null {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) {
    // Aceita também #rgb (3 dígitos)
    const m3 = /^#?([0-9a-f]{3})$/i.exec(hex.trim());
    if (!m3) return null;
    const s = m3[1]!;
    const r = parseInt(s[0]! + s[0]!, 16);
    const g = parseInt(s[1]! + s[1]!, 16);
    const b = parseInt(s[2]! + s[2]!, 16);
    return { r, g, b };
  }
  const s = m[1]!;
  return {
    r: parseInt(s.slice(0, 2), 16),
    g: parseInt(s.slice(2, 4), 16),
    b: parseInt(s.slice(4, 6), 16),
  };
}

function rgbToHsv({ r, g, b }: RGB): HSV {
  const rn = r / 255;
  const gn = g / 255;
  const bn = b / 255;
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const d = max - min;
  let h = 0;
  if (d !== 0) {
    if (max === rn) h = ((gn - bn) / d) % 6;
    else if (max === gn) h = (bn - rn) / d + 2;
    else h = (rn - gn) / d + 4;
    h *= 60;
    if (h < 0) h += 360;
  }
  const s = max === 0 ? 0 : d / max;
  const v = max;
  return { h, s, v };
}

function hexToHsv(hex: string): HSV | null {
  const rgb = hexToRgb(hex);
  return rgb ? rgbToHsv(rgb) : null;
}

// ===========================================================================
// Paletas
// ===========================================================================

// Paleta padrão organizada por matiz, 8 colunas × 5 linhas (=40 cores).
// Tons neutros na primeira coluna, depois rampa de hue.
const DEFAULT_PALETTE: readonly string[] = [
  // Neutros
  "#000000", "#262626", "#525252", "#737373", "#a3a3a3", "#d4d4d4", "#f5f5f5", "#ffffff",
  // Vermelhos / Rosa
  "#7f1d1d", "#b91c1c", "#dc2626", "#ef4444", "#f87171", "#fca5a5", "#fecaca", "#fee2e2",
  // Laranja / Amarelo
  "#9a3412", "#c2410c", "#ea580c", "#f97316", "#facc15", "#fde047", "#fef08a", "#fef9c3",
  // Verdes
  "#14532d", "#15803d", "#16a34a", "#22c55e", "#4ade80", "#86efac", "#bbf7d0", "#dcfce7",
  // Azul / Ciano
  "#1e3a8a", "#1d4ed8", "#2563eb", "#3b82f6", "#60a5fa", "#93c5fd", "#bfdbfe", "#dbeafe",
];

const RECENT_KEY = "sicro.color-picker.recent";
const RECENT_MAX = 10;

function loadRecent(): string[] {
  try {
    const raw = localStorage.getItem(RECENT_KEY);
    if (!raw) return [];
    const arr = JSON.parse(raw);
    if (!Array.isArray(arr)) return [];
    return arr.filter((s) => typeof s === "string").slice(0, RECENT_MAX);
  } catch {
    return [];
  }
}

function saveRecent(hex: string) {
  try {
    const curr = loadRecent();
    const next = [hex, ...curr.filter((c) => c.toLowerCase() !== hex.toLowerCase())].slice(
      0,
      RECENT_MAX,
    );
    localStorage.setItem(RECENT_KEY, JSON.stringify(next));
  } catch {
    /* localStorage off — best effort */
  }
}

// ===========================================================================
// Componente
// ===========================================================================

interface ColorPickerProProps {
  /** Cor atual em hex (ex.: "#dc2626") — null/undefined = sem cor. */
  current?: string | null;
  /** Chamada quando o usuário escolhe uma cor (hex). */
  onSelect: (hex: string) => void;
  /** Chamada quando o usuário clica em "Remover cor". */
  onClear: () => void;
  /** Cores extras da paleta (acrescentadas à paleta padrão). */
  extraSwatches?: readonly string[];
}

export function ColorPickerPro({
  current,
  onSelect,
  onClear,
  extraSwatches,
}: ColorPickerProProps) {
  // HSV ativo no pad/slider. Inicia do `current` se for hex válido,
  // senão começa em vermelho puro.
  const [hsv, setHsv] = useState<HSV>(() => {
    if (current) {
      const fromHex = hexToHsv(current);
      if (fromHex) return fromHex;
    }
    return { h: 0, s: 1, v: 1 };
  });
  const [hexInput, setHexInput] = useState<string>(current ?? "#ff0000");
  const [recent, setRecent] = useState<string[]>(() => loadRecent());

  const rgb = useMemo(() => hsvToRgb(hsv), [hsv]);
  const hex = useMemo(() => rgbToHex(rgb), [rgb]);

  // Sync hex input quando o usuário mexe no pad/slider — só sobreescreve
  // se o input atual NÃO está sendo editado (heurística: se contém um valor
  // que parse e bate com o estado atual, ele está em sync).
  useEffect(() => {
    setHexInput(hex);
  }, [hex]);

  const commit = useCallback(
    (h: string) => {
      onSelect(h);
      saveRecent(h);
      setRecent(loadRecent());
    },
    [onSelect],
  );

  // ---- S/V pad drag ----
  const padRef = useRef<HTMLDivElement>(null);
  const updateFromPad = useCallback(
    (e: MouseEvent | React.MouseEvent) => {
      const el = padRef.current;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      const x = Math.min(Math.max(0, e.clientX - rect.left), rect.width);
      const y = Math.min(Math.max(0, e.clientY - rect.top), rect.height);
      setHsv((prev) => ({
        ...prev,
        s: rect.width === 0 ? 0 : x / rect.width,
        v: rect.height === 0 ? 0 : 1 - y / rect.height,
      }));
    },
    [],
  );
  const startPadDrag = (e: React.MouseEvent) => {
    e.preventDefault();
    updateFromPad(e);
    const onMove = (ev: MouseEvent) => updateFromPad(ev);
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  // ---- Hue slider drag ----
  const hueRef = useRef<HTMLDivElement>(null);
  const updateFromHue = useCallback((e: MouseEvent | React.MouseEvent) => {
    const el = hueRef.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const x = Math.min(Math.max(0, e.clientX - rect.left), rect.width);
    setHsv((prev) => ({
      ...prev,
      h: rect.width === 0 ? 0 : (x / rect.width) * 360,
    }));
  }, []);
  const startHueDrag = (e: React.MouseEvent) => {
    e.preventDefault();
    updateFromHue(e);
    const onMove = (ev: MouseEvent) => updateFromHue(ev);
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  // ---- Hex input ----
  const onHexChange = (raw: string) => {
    setHexInput(raw);
    const parsed = hexToHsv(raw);
    if (parsed) setHsv(parsed);
  };
  const commitHexInput = () => {
    const parsed = hexToHsv(hexInput);
    if (parsed) {
      commit(rgbToHex(hsvToRgb(parsed)));
    }
  };

  const allSwatches = useMemo(() => {
    if (!extraSwatches || extraSwatches.length === 0) return DEFAULT_PALETTE;
    return [...DEFAULT_PALETTE, ...extraSwatches];
  }, [extraSwatches]);

  // Cor pura do matiz atual (para o gradient do pad).
  const hueHex = rgbToHex(hsvToRgb({ h: hsv.h, s: 1, v: 1 }));

  // Posição (%) dos cursores no pad e no slider.
  const padCursorX = `${hsv.s * 100}%`;
  const padCursorY = `${(1 - hsv.v) * 100}%`;
  const hueCursorX = `${(hsv.h / 360) * 100}%`;

  return (
    <div className={styles.pickerRoot}>
      {/* S/V pad */}
      <div
        ref={padRef}
        className={styles.svPad}
        style={{ background: hueHex }}
        onMouseDown={startPadDrag}
      >
        <div className={styles.svPadWhite} />
        <div className={styles.svPadBlack} />
        <div
          className={styles.svCursor}
          style={{ left: padCursorX, top: padCursorY }}
        />
      </div>

      {/* Hue slider */}
      <div
        ref={hueRef}
        className={styles.hueSlider}
        onMouseDown={startHueDrag}
      >
        <div
          className={styles.hueCursor}
          style={{ left: hueCursorX }}
        />
      </div>

      {/* Hex + RGB readout */}
      <div className={styles.inputRow}>
        <label className={styles.hexLabel}>
          HEX
          <input
            type="text"
            className={styles.hexInput}
            value={hexInput}
            onChange={(e) => onHexChange(e.target.value)}
            onBlur={commitHexInput}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                commitHexInput();
                e.currentTarget.blur();
              }
            }}
            spellCheck={false}
          />
        </label>
        <div className={styles.rgbReadout}>
          <span>R {rgb.r}</span>
          <span>G {rgb.g}</span>
          <span>B {rgb.b}</span>
        </div>
        <button
          type="button"
          className={styles.applyBtn}
          onClick={() => commit(hex)}
          title="Aplicar cor selecionada"
        >
          Aplicar
        </button>
      </div>

      {/* Preset palette */}
      <div className={styles.section}>
        <span className={styles.sectionLabel}>Cores padrão</span>
        <div className={styles.swatchGrid}>
          {allSwatches.map((c) => (
            <button
              key={c}
              type="button"
              className={styles.swatch}
              style={{ background: c }}
              title={c}
              aria-label={c}
              onClick={() => {
                const parsed = hexToHsv(c);
                if (parsed) setHsv(parsed);
                commit(c.toLowerCase());
              }}
            />
          ))}
        </div>
      </div>

      {/* Recent colors */}
      {recent.length > 0 && (
        <div className={styles.section}>
          <span className={styles.sectionLabel}>Cores recentes</span>
          <div className={styles.swatchGrid}>
            {recent.map((c) => (
              <button
                key={`recent-${c}`}
                type="button"
                className={styles.swatch}
                style={{ background: c }}
                title={c}
                aria-label={c}
                onClick={() => {
                  const parsed = hexToHsv(c);
                  if (parsed) setHsv(parsed);
                  commit(c);
                }}
              />
            ))}
          </div>
        </div>
      )}

      {/* Clear */}
      <button
        type="button"
        className={styles.clearBtn}
        onClick={onClear}
      >
        Remover cor
      </button>
    </div>
  );
}
