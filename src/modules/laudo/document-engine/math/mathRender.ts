/**
 * Render de fórmula (KaTeX) — Rodada 1/2.
 *
 * Trocamos o MathLive pelo KaTeX: na WebView2 do Tauri o MathLive renderizava
 * EM BRANCO (fontes não carregavam) e o web component `<math-field>` derrubava
 * a webview (tela preta). O KaTeX é a lib padrão de render de LaTeX — fontes
 * empacotadas pelo Vite de forma confiável, render SÍNCRONO, sem web component,
 * sem áudio, sem teclado virtual.
 *
 * Este módulo é carregado SOB DEMANDA (só quando se vê/edita/insere uma
 * fórmula). Importa o KaTeX + seu CSS (fontes) e o rasterizador html-to-image.
 */

import katex from "katex";
import "katex/dist/katex.min.css";
import { toPng } from "html-to-image";

const PX_PER_CM = 96 / 2.54;
const round2 = (n: number) => Math.round(n * 100) / 100;

export interface MathRender {
  /** PNG da fórmula como data URI (base64) — usado no export DOCX. */
  dataUri: string;
  /** Tamanho natural de exibição em cm (o PNG tem mais pixels p/ nitidez). */
  widthCm: number;
  heightCm: number;
}

/**
 * LaTeX → HTML do KaTeX (síncrono). `throwOnError:false` faz LaTeX inválido
 * renderizar em vermelho (não quebra o app). Usado no editor (NodeView) e no
 * preview do diálogo.
 */
export function latexToHtml(latex: string, opts?: { display?: boolean }): string {
  return katex.renderToString(latex || "", {
    displayMode: !!opts?.display,
    throwOnError: false,
    strict: false,
    trust: false,
    output: "html",
  });
}

/**
 * LaTeX → PNG (data URI) + tamanho natural em cm. Rasteriza o HTML do KaTeX
 * (fontes já carregadas pelo CSS importado) num container fora da tela. Usado
 * pra embutir a fórmula como imagem no DOCX (o PDF do LibreOffice é o canônico).
 */
export async function latexToPng(
  latex: string,
  opts?: { display?: boolean },
): Promise<MathRender> {
  const host = document.createElement("div");
  host.setAttribute("aria-hidden", "true");
  host.style.cssText = [
    "position:fixed",
    "left:-99999px",
    "top:0",
    "padding:2px 4px",
    "margin:0",
    "color:#111111",
    "background:transparent",
    "white-space:nowrap",
    `font-size:${opts?.display ? 22 : 18}px`,
    "line-height:1.2",
  ].join(";");
  host.innerHTML = latexToHtml(latex, opts);
  document.body.appendChild(host);

  try {
    if (typeof document !== "undefined" && document.fonts?.ready) {
      try {
        await document.fonts.ready;
      } catch {
        /* ignore */
      }
    }
    const w = Math.max(1, host.offsetWidth);
    const h = Math.max(1, host.offsetHeight);
    const dataUri = await toPng(host, {
      pixelRatio: 3,
      style: { background: "transparent" },
    });
    return {
      dataUri,
      widthCm: round2(w / PX_PER_CM),
      heightCm: round2(h / PX_PER_CM),
    };
  } finally {
    document.body.removeChild(host);
  }
}
