/**
 * Ajustes só de tela do reprodutor: brilho, contraste e gama (CSS filter +
 * filtro SVG de gama). Não alteram o vídeo, os quadros coletados nem medições.
 */
import styles from "./VideoPlayerPanel.module.css";

export interface Adjust {
  brightness: number;
  contrast: number;
  gamma: number;
  on: boolean;
}

export const ADJUST_DEFAULT: Adjust = { brightness: 1, contrast: 1, gamma: 1, on: true };

const GAMMA_FILTER_ID = "sicro-video-gamma";

export function isAdjusted(a: Adjust): boolean {
  return a.brightness !== 1 || a.contrast !== 1 || a.gamma !== 1;
}

/** CSS `filter` do <video>, ou undefined quando não há ajuste ativo. `id` =
 *  o do filtro de gama (um por vídeo na tela). */
export function adjustFilter(a: Adjust, id: string = GAMMA_FILTER_ID): string | undefined {
  if (!a.on || !isAdjusted(a)) return undefined;
  const parts: string[] = [];
  if (a.gamma !== 1) parts.push(`url(#${id})`);
  if (a.brightness !== 1) parts.push(`brightness(${a.brightness})`);
  if (a.contrast !== 1) parts.push(`contrast(${a.contrast})`);
  return parts.join(" ");
}

/** Definição do filtro de gama (saída = entrada^(1/gama)). Montar uma vez. */
export function GammaFilterDefs({ gamma, id = GAMMA_FILTER_ID }: { gamma: number; id?: string }) {
  const exp = String(1 / gamma);
  return (
    <svg width="0" height="0" style={{ position: "absolute" }} aria-hidden>
      <filter id={id} colorInterpolationFilters="sRGB">
        <feComponentTransfer>
          <feFuncR type="gamma" amplitude="1" exponent={exp} offset="0" />
          <feFuncG type="gamma" amplitude="1" exponent={exp} offset="0" />
          <feFuncB type="gamma" amplitude="1" exponent={exp} offset="0" />
        </feComponentTransfer>
      </filter>
    </svg>
  );
}

interface Props {
  value: Adjust;
  onChange: (a: Adjust) => void;
  onClose: () => void;
}

export function ImageAdjustPanel({ value, onChange, onClose }: Props) {
  const row = (
    label: string,
    key: "brightness" | "contrast" | "gamma",
    min: number,
    max: number,
    fmt: (n: number) => string,
  ) => (
    <label className={styles.adjRow}>
      <span>{label}</span>
      <input
        type="range"
        min={min}
        max={max}
        step={0.05}
        value={value[key]}
        onChange={(e) => onChange({ ...value, [key]: Number(e.target.value), on: true })}
        onDoubleClick={() => onChange({ ...value, [key]: 1 })}
        // Solta o foco ao largar o mouse: senão os atalhos (A, Espaço…) ficam
        // mudos enquanto o controle estiver focado.
        onPointerUp={(e) => e.currentTarget.blur()}
        title="Duplo clique volta ao original"
      />
      <code>{fmt(value[key])}</code>
    </label>
  );
  const pct = (n: number) => `${Math.round(n * 100)}%`;
  return (
    <div
      className={styles.adjPanel}
      data-no-magnify
      onPointerDown={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
    >
      <div className={styles.adjHead}>
        <strong>Ajustes de tela</strong>
        <button type="button" onClick={onClose} title="Fechar">
          ×
        </button>
      </div>
      {row("Brilho", "brightness", 0.2, 3, pct)}
      {row("Contraste", "contrast", 0.2, 3, pct)}
      {row("Gama", "gamma", 0.3, 3, (n) => n.toFixed(2))}
      <div className={styles.adjFoot}>
        <label>
          <input
            type="checkbox"
            checked={value.on}
            onChange={(e) => {
              onChange({ ...value, on: e.target.checked });
              e.currentTarget.blur();
            }}
          />{" "}
          aplicar (<kbd>A</kbd> compara)
        </label>
        <button type="button" onClick={() => onChange(ADJUST_DEFAULT)}>
          Original
        </button>
      </div>
      <p className={styles.adjNote}>
        Só na tela: não altera o vídeo, os quadros coletados nem as medições.
      </p>
    </div>
  );
}
