/**
 * MathEditDialog — editor de fórmula (LaTeX + paleta + preview KaTeX ao vivo).
 *
 * Trocamos o web component `<math-field>` do MathLive (que derrubava a WebView2
 * do Tauri) por um campo LaTeX + paleta de atalhos + pré-visualização ao vivo
 * renderizada com KaTeX. Robusto, sem custom element. Ao salvar: gera o PNG da
 * fórmula (KaTeX → html-to-image) e insere/atualiza o nó math no editor.
 *
 * O KaTeX é carregado SOB DEMANDA (só quando o diálogo abre).
 */

import { useEffect, useRef, useState } from "react";
import type { Editor } from "@tiptap/react";
import { MATH_RENDER_VERSION } from "../document-engine/nodes";

export interface MathDialogContext {
  mode: "insert" | "edit";
  variant: "inline" | "block";
  latex: string;
}

interface MathRenderModule {
  latexToHtml: (latex: string, opts?: { display?: boolean }) => string;
  latexToPng: (
    latex: string,
    opts?: { display?: boolean },
  ) => Promise<{ dataUri: string; widthCm: number; heightCm: number }>;
}

interface Props {
  editor: Editor | null;
  context: MathDialogContext | null;
  onClose: () => void;
}

// Atalhos: rótulo + trecho LaTeX. Inseridos no cursor; o cursor é posicionado
// dentro do primeiro `{}` quando houver.
const PALETTE: Array<{ label: string; insert: string; title: string }> = [
  { label: "a/b", insert: "\\frac{}{}", title: "Fração" },
  { label: "√", insert: "\\sqrt{}", title: "Raiz quadrada" },
  { label: "ⁿ√", insert: "\\sqrt[]{}", title: "Raiz n-ésima" },
  { label: "x²", insert: "^{}", title: "Sobrescrito (potência)" },
  { label: "xₙ", insert: "_{}", title: "Subscrito (índice)" },
  { label: "Σ", insert: "\\sum_{}^{}", title: "Somatório" },
  { label: "∫", insert: "\\int_{}^{}", title: "Integral" },
  { label: "±", insert: "\\pm ", title: "Mais ou menos" },
  { label: "≈", insert: "\\approx ", title: "Aproximadamente" },
  { label: "≤", insert: "\\le ", title: "Menor ou igual" },
  { label: "≥", insert: "\\ge ", title: "Maior ou igual" },
  { label: "×", insert: "\\times ", title: "Multiplicação" },
  { label: "·", insert: "\\cdot ", title: "Produto (ponto)" },
  { label: "÷", insert: "\\div ", title: "Divisão" },
  { label: "π", insert: "\\pi ", title: "Pi" },
  { label: "θ", insert: "\\theta ", title: "Theta" },
  { label: "α", insert: "\\alpha ", title: "Alfa" },
  { label: "Δ", insert: "\\Delta ", title: "Delta" },
];

export function MathEditDialog({ editor, context, onClose }: Props) {
  const taRef = useRef<HTMLTextAreaElement>(null);
  const modRef = useRef<MathRenderModule | null>(null);
  const [latex, setLatex] = useState("");
  const [variant, setVariant] = useState<"inline" | "block">("block");
  const [previewHtml, setPreviewHtml] = useState<string>("");
  const [ready, setReady] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Ao abrir: reseta o estado e carrega o KaTeX (lazy).
  useEffect(() => {
    if (!context) return undefined;
    setLatex(context.latex ?? "");
    setVariant(context.variant);
    setError(null);
    setReady(false);
    let cancelled = false;
    void import("../document-engine/math/mathRender")
      .then((m) => {
        if (cancelled) return;
        modRef.current = m as MathRenderModule;
        setReady(true);
      })
      .catch(() => {
        if (!cancelled) setError("Não foi possível carregar o renderizador.");
      });
    return () => {
      cancelled = true;
    };
  }, [context]);

  // Foca o campo ao abrir.
  useEffect(() => {
    if (context && ready) {
      window.setTimeout(() => taRef.current?.focus(), 30);
    }
  }, [context, ready]);

  // Preview ao vivo sempre que o LaTeX / variante / lib mudam.
  useEffect(() => {
    if (!ready || !modRef.current) return;
    try {
      setPreviewHtml(
        modRef.current.latexToHtml(latex, { display: variant === "block" }),
      );
    } catch {
      setPreviewHtml("");
    }
  }, [latex, variant, ready]);

  if (!context) return null;

  const insertSnippet = (snippet: string) => {
    const ta = taRef.current;
    const cur = latex;
    let start = cur.length;
    let end = cur.length;
    if (ta) {
      start = ta.selectionStart ?? cur.length;
      end = ta.selectionEnd ?? cur.length;
    }
    const next = cur.slice(0, start) + snippet + cur.slice(end);
    setLatex(next);
    // Cursor dentro do primeiro `{}` do snippet, se houver.
    const braceIdx = snippet.indexOf("{}");
    const caret =
      braceIdx >= 0 ? start + braceIdx + 1 : start + snippet.length;
    window.setTimeout(() => {
      if (ta) {
        ta.focus();
        ta.setSelectionRange(caret, caret);
      }
    }, 0);
  };

  const save = async () => {
    const trimmed = latex.trim();
    if (!editor || !modRef.current || !trimmed) {
      onClose();
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const png = await modRef.current.latexToPng(trimmed, {
        display: variant === "block",
      });
      const attrs = {
        latex: trimmed,
        render_png: png.dataUri,
        render_w_cm: png.widthCm,
        render_h_cm: png.heightCm,
        render_v: MATH_RENDER_VERSION,
      };
      if (context.mode === "edit") {
        const name = context.variant === "block" ? "mathBlock" : "mathInline";
        editor.chain().focus().updateAttributes(name, attrs).run();
      } else if (variant === "block") {
        editor.chain().focus().insertMathBlock(attrs).run();
      } else {
        editor.chain().focus().insertMathInline(attrs).run();
      }
      onClose();
    } catch (err) {
      setError("Falha ao renderizar a fórmula. Verifique o LaTeX.");
      // eslint-disable-next-line no-console
      console.error("[math] render failed", err);
    } finally {
      setBusy(false);
    }
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void save();
    }
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Editor de fórmula matemática"
      onKeyDown={onKeyDown}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 1000,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(15,23,42,0.45)",
      }}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        style={{
          width: "min(640px, 92vw)",
          background: "#fff",
          borderRadius: 12,
          boxShadow: "0 10px 40px rgba(0,0,0,0.3)",
          padding: 18,
          display: "flex",
          flexDirection: "column",
          gap: 12,
        }}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <strong style={{ fontSize: 15, color: "#0f172a" }}>
            {context.mode === "edit" ? "Editar fórmula" : "Inserir fórmula"}
          </strong>
          {context.mode === "insert" && (
            <div
              style={{
                marginLeft: "auto",
                display: "inline-flex",
                border: "1px solid #cbd5e1",
                borderRadius: 8,
                overflow: "hidden",
              }}
            >
              {(["block", "inline"] as const).map((v) => (
                <button
                  key={v}
                  type="button"
                  onClick={() => setVariant(v)}
                  style={{
                    padding: "4px 10px",
                    fontSize: 12,
                    border: "none",
                    cursor: "pointer",
                    background: variant === v ? "#2563eb" : "#fff",
                    color: variant === v ? "#fff" : "#334155",
                  }}
                >
                  {v === "block" ? "Bloco (centralizada)" : "Em linha"}
                </button>
              ))}
            </div>
          )}
        </div>

        {/* Preview ao vivo (KaTeX) */}
        <div
          style={{
            minHeight: 56,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            padding: "10px 12px",
            border: "1px solid #e2e8f0",
            borderRadius: 8,
            background: "#f8fafc",
            color: "#0f172a",
            overflowX: "auto",
          }}
        >
          {ready ? (
            previewHtml ? (
              <span dangerouslySetInnerHTML={{ __html: previewHtml }} />
            ) : (
              <span style={{ color: "#94a3b8", fontSize: 13 }}>
                Digite a fórmula em LaTeX abaixo…
              </span>
            )
          ) : (
            <span style={{ color: "#64748b", fontSize: 13 }}>
              Carregando renderizador…
            </span>
          )}
        </div>

        {/* Paleta rápida */}
        <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
          {PALETTE.map((p) => (
            <button
              key={p.label}
              type="button"
              title={p.title}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => insertSnippet(p.insert)}
              style={{
                minWidth: 30,
                height: 28,
                padding: "0 6px",
                fontSize: 14,
                border: "1px solid #e2e8f0",
                borderRadius: 6,
                background: "#fff",
                cursor: "pointer",
                color: "#0f172a",
              }}
            >
              {p.label}
            </button>
          ))}
        </div>

        {/* Campo LaTeX */}
        <textarea
          ref={taRef}
          value={latex}
          onChange={(e) => setLatex(e.target.value)}
          spellCheck={false}
          placeholder="Ex.: k = \frac{L}{P} = 0{,}00709 \\ \text{m/pixel}"
          style={{
            width: "100%",
            minHeight: 70,
            resize: "vertical",
            fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
            fontSize: 14,
            padding: "8px 10px",
            border: "1px solid #cbd5e1",
            borderRadius: 8,
            color: "#0f172a",
            background: "#fff",
          }}
        />

        {error && <div style={{ color: "#b91c1c", fontSize: 13 }}>{error}</div>}

        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ fontSize: 11, color: "#94a3b8" }}>
            LaTeX: <code>frac, sqrt, ^, _, \pm, \approx…</code> · use a paleta ·
            Ctrl+Enter salva
          </span>
          <div style={{ marginLeft: "auto", display: "flex", gap: 8 }}>
            <button
              type="button"
              onClick={onClose}
              disabled={busy}
              style={{
                padding: "7px 14px",
                fontSize: 13,
                border: "1px solid #cbd5e1",
                borderRadius: 8,
                background: "#fff",
                cursor: "pointer",
                color: "#334155",
              }}
            >
              Cancelar
            </button>
            <button
              type="button"
              onClick={() => void save()}
              disabled={busy || !ready}
              style={{
                padding: "7px 16px",
                fontSize: 13,
                fontWeight: 600,
                border: "none",
                borderRadius: 8,
                background: busy || !ready ? "#93c5fd" : "#2563eb",
                color: "#fff",
                cursor: busy || !ready ? "default" : "pointer",
              }}
            >
              {busy ? "Renderizando…" : "Salvar"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
