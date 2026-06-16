/**
 * MathFormula — fórmula matemática no laudo (Rodada 1: escrever + exportar).
 *
 * Dois nós:
 *   - `mathInline` — inline, atômico (no meio de um parágrafo, ex.: u_P=√2).
 *   - `mathBlock`  — bloco, atômico, centralizado (equação em linha própria).
 *
 * Atributos (ambos):
 *   - `latex`        : string — FONTE canônica (editável no diálogo MathLive).
 *   - `render_png`   : string|null — PNG da fórmula (data URI) usado pra EXIBIR
 *                       em todo lugar (editor, HTML/PDF, DOCX). Gerado quando a
 *                       fórmula é criada/editada. Display = imagem → não depende
 *                       de carregar fontes/CSS de math pra ver o laudo.
 *   - `render_w_cm`  : number|null — largura natural de exibição (cm).
 *   - `render_h_cm`  : number|null — altura natural de exibição (cm).
 *
 * O NodeView exibe a imagem; duplo-clique dispara `sicro:edit-math` (window) —
 * a view abre o diálogo de edição pro nó selecionado. Clique simples só
 * seleciona (NodeSelection), igual ao Shape.
 */

import { Node, mergeAttributes } from "@tiptap/core";
import { NodeSelection } from "@tiptap/pm/state";
import type { Node as PMNode } from "@tiptap/pm/model";
import type { EditorView } from "@tiptap/pm/view";
// Render SÍNCRONO do LaTeX (KaTeX). Estático de propósito — vide buildMathDom.
import { latexToHtml } from "../math/mathRender";

export const EDIT_MATH_EVENT = "sicro:edit-math";

declare module "@tiptap/core" {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface Commands<ReturnType> {
    mathFormula: {
      insertMathInline: (attrs?: Record<string, unknown>) => ReturnType;
      insertMathBlock: (attrs?: Record<string, unknown>) => ReturnType;
    };
  }
}

const COMMON_ATTRS = {
  latex: { default: "" },
  render_png: { default: null as string | null },
  render_w_cm: { default: null as number | null },
  render_h_cm: { default: null as number | null },
  // Versão do motor de render do PNG. Bump quando trocamos de engine pra a
  // hidratação REGERAR PNGs antigos. 2 = KaTeX (1/ausente = MathLive em branco).
  render_v: { default: null as number | null },
};

/** Versão atual do render de PNG (KaTeX). Vide render_v acima. */
export const MATH_RENDER_VERSION = 2;

/**
 * Render do LaTeX importado ESTÁTICO (não lazy). Razão crítica de paginação:
 * o algoritmo de split mede `block.offsetTop` no mount. Se o KaTeX renderizasse
 * async (import lazy), a 1ª medição via o bloco curto (placeholder) → spacer
 * errado → vão + cascata nas páginas seguintes. Com import estático o bloco já
 * nasce na altura final → a paginação mede certo de primeira. O `katex` fica no
 * chunk vendor-math, que carrega junto com o editor de laudo (rota lazy), então
 * não pesa no cold-start do app.
 */

/** Monta o conteúdo visual: renderiza o LaTeX com KaTeX (SÍNCRONO). */
function buildMathDom(
  el: HTMLElement,
  attrs: Record<string, unknown>,
  variant: "inline" | "block",
): void {
  const latex = ((attrs["latex"] as string | null) ?? "").trim();
  // Se o DOM JÁ está renderizado pra este mesmo LaTeX, não refaz nada. O
  // update() do NodeView é chamado pelos próprios spacers da paginação; sem
  // esta guarda o re-render re-dispararia `sicro:math-rendered` → laço
  // paginação↔render.
  if (el.getAttribute("data-rendered-latex") === latex) return;
  el.classList.remove("sicro-math--empty");
  el.setAttribute("data-latex", latex);
  if (!latex) {
    el.removeAttribute("data-rendered-latex");
    el.classList.add("sicro-math--empty");
    el.textContent =
      variant === "block" ? "ƒ(x)  — clique 2× para editar a fórmula" : "ƒ(x)";
    return;
  }
  try {
    el.innerHTML = latexToHtml(latex, { display: variant === "block" });
  } catch {
    el.textContent = latex;
  }
  // Zera a margem do `.katex-display` (default do KaTeX = `1em 0`). Essa margem
  // colapsava pra FORA do bloco e NÃO entrava na medição de altura da paginação
  // → a fórmula renderizava mais alta do que era contada → o texto vazava a
  // margem inferior. O espaçamento vertical agora vem do padding do container
  // (vide buildView), que entra no offsetHeight e é medido corretamente.
  const disp = el.querySelector?.(".katex-display");
  if (disp instanceof HTMLElement) {
    disp.style.margin = "0";
  }
  el.setAttribute("data-rendered-latex", latex);
  // Avisa a paginação que uma fórmula (re)renderizou e mudou de altura. O KaTeX
  // vem em chunk lazy + fontes async; a 1ª medição da paginação pode ter pego
  // altura provisória e, sem este sinal, só um Enter manual forçava o recálculo.
  // A guarda `data-rendered-latex === latex` (topo) evita re-disparo nos updates
  // de spacer (mesmo latex → early return) → sem laço paginação↔render.
  try {
    window.dispatchEvent(new CustomEvent("sicro:math-rendered"));
  } catch {
    /* sem window (SSR/teste) — ok */
  }
}

interface MathNodeView {
  dom: HTMLElement;
  update: (node: PMNode) => boolean;
  destroy: () => void;
  ignoreMutation: () => boolean;
}

/** Constrói o NodeView (DOM + seleção + duplo-clique → editar). */
function buildView(
  variant: "inline" | "block",
  node: PMNode,
  view: EditorView,
  getPos: () => number | undefined,
): MathNodeView {
  const dom = document.createElement(variant === "block" ? "div" : "span");
  dom.className = variant === "block" ? "sicro-math-block" : "sicro-math-inline";
  dom.setAttribute(
    variant === "block" ? "data-math-block" : "data-math-inline",
    "true",
  );
  dom.setAttribute("contenteditable", "false");
  if (variant === "block") {
    dom.style.textAlign = "center";
    // PADDING (não margin): o padding ENTRA no offsetHeight e NÃO colapsa, então
    // a paginação mede a altura REAL da fórmula. Antes, a margin (0.4em) do bloco
    // + a margem 1em do `.katex-display` (CSS do KaTeX) COLAPSAVAM pra fora da
    // caixa medida → a paginação contava a fórmula MENOR do que ela renderiza →
    // a página empacotava conteúdo demais e o texto VAZAVA a margem inferior.
    dom.style.padding = "0.55em 0";
    dom.style.margin = "0";
  } else {
    dom.style.display = "inline";
  }
  dom.style.cursor = "pointer";
  buildMathDom(dom, node.attrs, variant);

  const selectSelf = () => {
    const pos = getPos();
    if (typeof pos !== "number") return;
    try {
      const sel = NodeSelection.create(view.state.doc, pos);
      view.dispatch(view.state.tr.setSelection(sel));
      view.focus();
    } catch {
      /* posição instável — ignora */
    }
  };
  const onMouseDown = (e: Event) => {
    e.preventDefault();
    selectSelf();
  };
  const onDblClick = (e: Event) => {
    e.preventDefault();
    e.stopPropagation();
    selectSelf();
    window.dispatchEvent(new CustomEvent(EDIT_MATH_EVENT));
  };
  dom.addEventListener("mousedown", onMouseDown);
  dom.addEventListener("dblclick", onDblClick);

  return {
    dom,
    update: (updatedNode: PMNode) => {
      if (updatedNode.type.name !== node.type.name) return false;
      buildMathDom(dom, updatedNode.attrs, variant);
      return true;
    },
    destroy: () => {
      dom.removeEventListener("mousedown", onMouseDown);
      dom.removeEventListener("dblclick", onDblClick);
    },
    ignoreMutation: () => true,
  };
}

export const MathInline = Node.create({
  name: "mathInline",
  inline: true,
  group: "inline",
  atom: true,
  selectable: true,
  draggable: false,

  addAttributes() {
    return COMMON_ATTRS;
  },

  parseHTML() {
    return [{ tag: "span[data-math-inline]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    const png = (node.attrs["render_png"] as string | null) ?? null;
    const latex = (node.attrs["latex"] as string | null) ?? "";
    const attrs = mergeAttributes(HTMLAttributes, {
      "data-math-inline": "true",
      "data-latex": latex,
      class: "sicro-math-inline",
    });
    if (png) {
      const hCm = node.attrs["render_h_cm"] as number | null;
      return [
        "span",
        attrs,
        [
          "img",
          {
            src: png,
            alt: latex,
            class: "sicro-math-img",
            ...(hCm && hCm > 0 ? { style: `height:${hCm}cm` } : {}),
          },
        ],
      ];
    }
    return ["span", attrs, latex];
  },

  addNodeView() {
    return (props) => buildView("inline", props.node, props.view, props.getPos);
  },

  addCommands() {
    return {
      insertMathInline:
        (attrs = {}) =>
        ({ commands }) =>
          commands.insertContent({ type: this.name, attrs }),
    };
  },
});

export const MathBlock = Node.create({
  name: "mathBlock",
  group: "block",
  atom: true,
  selectable: true,
  draggable: true,
  isolating: true,

  addAttributes() {
    return COMMON_ATTRS;
  },

  parseHTML() {
    return [{ tag: "div[data-math-block]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    const png = (node.attrs["render_png"] as string | null) ?? null;
    const latex = (node.attrs["latex"] as string | null) ?? "";
    const attrs = mergeAttributes(HTMLAttributes, {
      "data-math-block": "true",
      "data-latex": latex,
      class: "sicro-math-block",
    });
    if (png) {
      const hCm = node.attrs["render_h_cm"] as number | null;
      return [
        "div",
        attrs,
        [
          "img",
          {
            src: png,
            alt: latex,
            class: "sicro-math-img",
            ...(hCm && hCm > 0 ? { style: `height:${hCm}cm` } : {}),
          },
        ],
      ];
    }
    return ["div", attrs, latex];
  },

  addNodeView() {
    return (props) => buildView("block", props.node, props.view, props.getPos);
  },

  addCommands() {
    return {
      insertMathBlock:
        (attrs = {}) =>
        ({ commands }) =>
          commands.insertContent({ type: this.name, attrs }),
    };
  },
});
