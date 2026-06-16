/**
 * W13.2 — Barra de contexto ("barra de opções da ferramenta").
 *
 * Faixa horizontal no topo do editor que **muda conforme a ferramenta ativa**
 * e centraliza as propriedades que hoje ficavam espalhadas: cor de traço,
 * espessura, preenchimento. Inspiração: barra de opções do Photoshop / opções
 * de ferramenta do GIMP — o maior ganho de "facilidade" da repaginação.
 *
 * Comportamento (igual aos editores da indústria):
 *  - Se há um **objeto selecionado**, a barra edita o estilo DELE ao vivo.
 *  - Senão, edita o **estilo padrão** que será aplicado à PRÓXIMA anotação.
 *  - Ferramentas sem estilo (navegar/medir/escala/cortar) mostram a dica/estado
 *    relevante (ex.: status da calibração de escala).
 */

import type { SicroAnnotation, SicroImageScale } from "../engine/schema";

export interface ToolStyle {
  stroke: string;
  strokeWidth: number;
  fill: string; // "transparent" = sem preenchimento
}

export const DEFAULT_TOOL_STYLE: ToolStyle = {
  stroke: "#ef4444",
  strokeWidth: 2,
  fill: "transparent",
};

/** Ferramentas cujo estilo (cor/espessura/fill) faz sentido editar. */
const STYLEABLE_TOOLS = new Set([
  "arrow",
  "line",
  "rect",
  "ellipse",
  "text",
  "marker",
  "point",
  "measurement",
  "redaction",
]);

/** Kinds que aceitam preenchimento (fill). */
const FILLABLE_KINDS = new Set(["rect", "ellipse"]);

const TOOL_LABEL: Record<string, string> = {
  select: "Selecionar",
  pan: "Mover (pan)",
  arrow: "Seta",
  line: "Linha",
  rect: "Retângulo",
  ellipse: "Elipse",
  text: "Texto",
  marker: "Marcador numerado",
  point: "Ponto",
  measurement: "Medida",
  redaction: "Tarja (anonimização)",
  set_scale: "Definir escala",
  crop: "Cortar",
};

interface Props {
  tool: string;
  /** Estilo padrão da próxima anotação. */
  toolStyle: ToolStyle;
  onToolStyle: (patch: Partial<ToolStyle>) => void;
  /** Objeto selecionado (edita o estilo dele se houver). */
  selected: SicroAnnotation | null;
  onSelectedPatch: (patch: Partial<SicroAnnotation>) => void;
  /** Escala calibrada (px → unidade), para o contexto de medida. */
  scale: SicroImageScale | null;
}

export function ToolOptionsBar({
  tool,
  toolStyle,
  onToolStyle,
  selected,
  onSelectedPatch,
  scale,
}: Props) {
  // Alvo do estilo: objeto selecionado tem prioridade (edição ao vivo).
  const target = selected;
  const editingSelected = !!target;

  const stroke = target?.stroke ?? toolStyle.stroke;
  const strokeWidth = target?.stroke_width ?? toolStyle.strokeWidth;
  const fill =
    target?.fill ?? (toolStyle.fill === "transparent" ? "" : toolStyle.fill);
  const fillOn = (target ? target.fill : toolStyle.fill) !== "transparent" &&
    !!(target ? target.fill : toolStyle.fill);

  const setStroke = (v: string) =>
    editingSelected ? onSelectedPatch({ stroke: v }) : onToolStyle({ stroke: v });
  const setWidth = (v: number) =>
    editingSelected
      ? onSelectedPatch({ stroke_width: v })
      : onToolStyle({ strokeWidth: v });
  const setFill = (v: string) =>
    editingSelected ? onSelectedPatch({ fill: v }) : onToolStyle({ fill: v });

  // Mostra controles de estilo quando: edita um objeto selecionado OU a
  // ferramenta ativa é "desenhável".
  const showStyle =
    editingSelected || STYLEABLE_TOOLS.has(tool);
  const showFill =
    (target && FILLABLE_KINDS.has(target.kind)) ||
    (!target && (tool === "rect" || tool === "ellipse"));
  // Texto usa "stroke" como cor do próprio texto (sem espessura/fill).
  const isText = target ? target.kind === "text" : tool === "text";

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 14,
        padding: "5px 12px",
        minHeight: 34,
        background: "var(--sicro-bg-elev, rgba(17,24,39,0.6))",
        borderBottom: "1px solid var(--sicro-border, rgba(148,163,184,0.18))",
        fontSize: 12,
        color: "var(--sicro-fg, #e2e8f0)",
      }}
      role="toolbar"
      aria-label="Opções da ferramenta"
    >
      <span style={{ fontWeight: 600, minWidth: 130, color: "var(--sicro-fg, #e2e8f0)" }}>
        {editingSelected ? "Objeto selecionado" : (TOOL_LABEL[tool] ?? tool)}
        {editingSelected && (
          <span style={{ color: "var(--sicro-fg-dim, #94a3b8)", fontWeight: 400 }}>
            {" "}· {TOOL_LABEL[target!.kind] ?? target!.kind}
          </span>
        )}
      </span>

      {showStyle ? (
        <>
          <Field label={isText ? "Cor do texto" : "Cor"}>
            <input
              type="color"
              value={normalizeColor(stroke)}
              onChange={(e) => setStroke(e.target.value)}
              style={swatch}
              aria-label="Cor"
            />
          </Field>

          {!isText && (
            <Field label="Espessura">
              <input
                type="range"
                min={1}
                max={20}
                step={1}
                value={strokeWidth}
                onChange={(e) => setWidth(Number(e.target.value))}
                style={{ width: 90 }}
              />
              <code style={{ minWidth: 22, textAlign: "right" }}>{strokeWidth}</code>
            </Field>
          )}

          {showFill && (
            <Field label="Preenchimento">
              <label style={{ display: "flex", alignItems: "center", gap: 4 }}>
                <input
                  type="checkbox"
                  checked={fillOn}
                  onChange={(e) =>
                    setFill(e.target.checked ? fill || "#ffff00" : "transparent")
                  }
                />
                <span style={{ color: "var(--sicro-fg-dim, #94a3b8)" }}>
                  {fillOn ? "" : "transparente"}
                </span>
              </label>
              {fillOn && (
                <input
                  type="color"
                  value={normalizeColor(fill || "#ffff00")}
                  onChange={(e) => setFill(e.target.value)}
                  style={swatch}
                  aria-label="Cor de preenchimento"
                />
              )}
            </Field>
          )}
        </>
      ) : (
        <ContextHint tool={tool} scale={scale} />
      )}
    </div>
  );
}

function ContextHint({
  tool,
  scale,
}: {
  tool: string;
  scale: SicroImageScale | null;
}) {
  if (tool === "set_scale" || tool === "measurement") {
    return (
      <span style={{ color: "var(--sicro-fg-dim, #94a3b8)" }}>
        {scale
          ? `Escala calibrada: ${scale.px_per_unit.toFixed(2)} px/${scale.unit}`
          : "Escala não calibrada — use 'Definir escala' (clique 2 pontos de distância conhecida)."}
        {tool === "measurement" && " · clique 2 pontos para medir."}
      </span>
    );
  }
  if (tool === "crop") {
    return (
      <span style={{ color: "var(--sicro-fg-dim, #94a3b8)" }}>
        Arraste o retângulo ou as alças; depois <strong>Aplicar</strong>.
      </span>
    );
  }
  if (tool === "pan") {
    return (
      <span style={{ color: "var(--sicro-fg-dim, #94a3b8)" }}>
        Arraste para mover a imagem; roda do mouse = zoom.
      </span>
    );
  }
  return (
    <span style={{ color: "var(--sicro-fg-dim, #94a3b8)" }}>
      Selecione um objeto para editar, ou escolha uma ferramenta na barra
      lateral.
    </span>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
      <span style={{ color: "var(--sicro-fg-dim, #94a3b8)" }}>{label}</span>
      {children}
    </span>
  );
}

const swatch: React.CSSProperties = {
  width: 26,
  height: 20,
  padding: 0,
  border: "1px solid rgba(148,163,184,0.4)",
  borderRadius: 4,
  background: "transparent",
  cursor: "pointer",
};

/** `<input type=color>` exige #rrggbb; "transparent"/nomes → fallback. */
function normalizeColor(c: string): string {
  if (/^#[0-9a-fA-F]{6}$/.test(c)) return c;
  if (/^#[0-9a-fA-F]{3}$/.test(c)) {
    return `#${c[1]}${c[1]}${c[2]}${c[2]}${c[3]}${c[3]}`;
  }
  return "#ef4444";
}
