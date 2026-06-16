/**
 * Q — Shape: nó pra inserir formas geométricas (rectangle, ellipse, arrow, line).
 *
 * Usado pra ANOTAÇÃO sobre fotos em laudos forenses — apontar coisas
 * com setas, destacar áreas com retângulos, marcar pontos com elipses,
 * conectar elementos com linhas.
 *
 * Renderiza SVG dentro de um div posicionado. Default `wrap_mode` é
 * "in_front" (flutua sobre o texto/foto). Suporta:
 *   - kind: rectangle | ellipse | arrow | line
 *   - dimensões em cm (width_cm, height_cm)
 *   - posição flutuante (wrap_x_cm, wrap_y_cm) — segue mesmo padrão do Figure
 *   - rotação (rotation, graus)
 *   - borda: stroke_color (hex), stroke_width (viewBox units, ~mm)
 *   - preenchimento: fill_color (hex+alpha) — só rect/ellipse, ignorado em arrow/line
 *
 * NodeView controla DOM diretamente (mesmo padrão P21 do Figure) pra
 * estabilidade e click handler direto.
 */

import { Node, mergeAttributes } from "@tiptap/core";
import { NodeSelection, Plugin, PluginKey } from "@tiptap/pm/state";

export type ShapeKind = "rectangle" | "ellipse" | "arrow" | "line";
export type ShapeWrapMode = "inline" | "in_front" | "behind";

/** Gera ID estável pra shape. Permite refs persistentes. */
function generateShapeId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return `shape-${crypto.randomUUID().slice(0, 8)}`;
  }
  return `shape-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e6).toString(36)}`;
}

declare module "@tiptap/core" {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface Commands<ReturnType> {
    shape: {
      insertShape: (attrs: {
        kind: ShapeKind;
        width_cm?: number;
        height_cm?: number;
        wrap_mode?: ShapeWrapMode;
        wrap_x_cm?: number;
        wrap_y_cm?: number;
        stroke_color?: string;
        stroke_width?: number;
        fill_color?: string;
      }) => ReturnType;
    };
  }
}

/**
 * Renderiza o conteúdo SVG da shape. viewBox dimensionado em cm × 10
 * (ex: 5cm × 3cm = viewBox "0 0 50 30") pra que stroke_width em "viewBox
 * units" corresponda ~mm — intuitivo pro usuário.
 */
function renderShapeSvgString(
  kind: ShapeKind,
  widthCm: number,
  heightCm: number,
  strokeColor: string,
  strokeWidth: number,
  fillColor: string,
  uniqueId: string,
): string {
  const vbW = Math.max(1, widthCm * 10);
  const vbH = Math.max(1, heightCm * 10);
  const sw = Math.max(0.1, strokeWidth);
  const inset = sw / 2;
  switch (kind) {
    case "rectangle":
      return (
        `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${vbW} ${vbH}" preserveAspectRatio="none" width="100%" height="100%">` +
        `<rect x="${inset}" y="${inset}" width="${vbW - sw}" height="${vbH - sw}" ` +
        `stroke="${strokeColor}" stroke-width="${sw}" fill="${fillColor}" />` +
        `</svg>`
      );
    case "ellipse":
      return (
        `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${vbW} ${vbH}" preserveAspectRatio="none" width="100%" height="100%">` +
        `<ellipse cx="${vbW / 2}" cy="${vbH / 2}" rx="${vbW / 2 - inset}" ry="${vbH / 2 - inset}" ` +
        `stroke="${strokeColor}" stroke-width="${sw}" fill="${fillColor}" />` +
        `</svg>`
      );
    case "arrow": {
      // Linha horizontal com arrowhead no fim direito. User rotaciona pra outras direções.
      // markerUnits="strokeWidth" → arrowhead escala com stroke. refX=9 alinha a ponta no final da linha.
      const tipX = vbW - sw * 4; // recua a ponta da linha pra arrowhead caber dentro do viewBox
      return (
        `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${vbW} ${vbH}" preserveAspectRatio="none" width="100%" height="100%">` +
        `<defs>` +
        `<marker id="arrowhead-${uniqueId}" markerWidth="10" markerHeight="10" ` +
        `refX="9" refY="5" orient="auto" markerUnits="strokeWidth">` +
        `<polygon points="0 0, 10 5, 0 10" fill="${strokeColor}" />` +
        `</marker>` +
        `</defs>` +
        `<line x1="0" y1="${vbH / 2}" x2="${tipX}" y2="${vbH / 2}" ` +
        `stroke="${strokeColor}" stroke-width="${sw}" ` +
        `marker-end="url(#arrowhead-${uniqueId})" />` +
        `</svg>`
      );
    }
    case "line":
      // Linha simples horizontal — user rotaciona se quiser diagonal.
      return (
        `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${vbW} ${vbH}" preserveAspectRatio="none" width="100%" height="100%">` +
        `<line x1="0" y1="${vbH / 2}" x2="${vbW}" y2="${vbH / 2}" ` +
        `stroke="${strokeColor}" stroke-width="${sw}" />` +
        `</svg>`
      );
  }
}

export const Shape = Node.create({
  name: "shape",
  group: "block",
  // Atom: nó sem conteúdo interno (diferente do Figure que tem figcaption).
  atom: true,
  draggable: true,
  selectable: true,
  isolating: true,

  addAttributes() {
    return {
      id: { default: null },
      kind: { default: "rectangle" },
      width_cm: { default: 4 },
      height_cm: { default: 3 },
      wrap_mode: {
        default: "in_front",
        parseHTML: (el: HTMLElement) =>
          (el.getAttribute("data-wrap-mode") as ShapeWrapMode | null) ??
          "in_front",
        renderHTML: (attrs: { wrap_mode?: ShapeWrapMode | null }) => {
          const m = attrs.wrap_mode ?? "in_front";
          return { "data-wrap-mode": m };
        },
      },
      wrap_x_cm: {
        default: 2,
        parseHTML: (el: HTMLElement) => {
          const raw = el.getAttribute("data-wrap-x-cm");
          if (!raw) return 2;
          const n = parseFloat(raw);
          return Number.isFinite(n) ? n : 2;
        },
        renderHTML: (attrs: { wrap_x_cm?: number }) => ({
          "data-wrap-x-cm": String(attrs.wrap_x_cm ?? 2),
        }),
      },
      wrap_y_cm: {
        default: 2,
        parseHTML: (el: HTMLElement) => {
          const raw = el.getAttribute("data-wrap-y-cm");
          if (!raw) return 2;
          const n = parseFloat(raw);
          return Number.isFinite(n) ? n : 2;
        },
        renderHTML: (attrs: { wrap_y_cm?: number }) => ({
          "data-wrap-y-cm": String(attrs.wrap_y_cm ?? 2),
        }),
      },
      rotation: {
        default: 0,
        parseHTML: (el: HTMLElement) => {
          const raw = el.getAttribute("data-rotation");
          if (!raw) return 0;
          const n = parseFloat(raw);
          return Number.isFinite(n) ? n : 0;
        },
        renderHTML: (attrs: { rotation?: number }) => {
          const v = attrs.rotation ?? 0;
          if (!v) return {};
          return { "data-rotation": String(v) };
        },
      },
      stroke_color: { default: "#d92626" },
      stroke_width: { default: 3 },
      fill_color: { default: "rgba(255,255,255,0)" },
    };
  },

  parseHTML() {
    return [{ tag: "div[data-sicro-shape]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    const kind = (node.attrs.kind as ShapeKind) ?? "rectangle";
    const widthCm = Number(node.attrs.width_cm) || 4;
    const heightCm = Number(node.attrs.height_cm) || 3;
    const wrapMode = (node.attrs.wrap_mode as ShapeWrapMode) ?? "in_front";
    const wrapX = Number(node.attrs.wrap_x_cm) || 0;
    const wrapY = Number(node.attrs.wrap_y_cm) || 0;
    const rotation = Number(node.attrs.rotation) || 0;
    const strokeColor = (node.attrs.stroke_color as string) ?? "#d92626";
    const strokeWidth = Number(node.attrs.stroke_width) || 3;
    const fillColor = (node.attrs.fill_color as string) ?? "rgba(255,255,255,0)";
    const id = (node.attrs.id as string) ?? "anon";
    const transform = rotation ? ` transform: rotate(${rotation}deg);` : "";
    let containerStyle: string;
    if (wrapMode === "inline") {
      containerStyle = `display: inline-block; width: ${widthCm}cm; height: ${heightCm}cm; margin: 0 auto;${transform}`;
    } else {
      const zIndex = wrapMode === "in_front" ? 5 : -1;
      containerStyle = `position: absolute; left: ${wrapX}cm; top: ${wrapY}cm; width: ${widthCm}cm; height: ${heightCm}cm; z-index: ${zIndex};${transform}`;
    }
    const svgString = renderShapeSvgString(
      kind,
      widthCm,
      heightCm,
      strokeColor,
      strokeWidth,
      fillColor,
      id,
    );
    const extra: Record<string, string> = {
      "data-sicro-shape": "true",
      "data-kind": kind,
      "data-width-cm": String(widthCm),
      "data-height-cm": String(heightCm),
      "data-stroke-color": strokeColor,
      "data-stroke-width": String(strokeWidth),
      "data-fill-color": fillColor,
      style: containerStyle,
      draggable: wrapMode === "inline" ? "true" : "false",
    };
    if (id) extra["data-shape-id"] = id;
    // Serialização ESTÁTICA (clone do cabeçalho/rodapé fora do modo edição +
    // export HTML/PDF/DOCX via generateHTML): emitimos o SVG como um <img> com
    // data-URI. Antes o SVG ia só no atributo `data-svg-string` e o div saía
    // VAZIO — então a forma sumia no clone e no export, aparecendo apenas no
    // editor AO VIVO (onde o NodeView injeta o SVG via innerHTML). Um <img> com
    // o SVG embutido renderiza em qualquer contexto HTML e evita o problema de
    // namespace que o SVG inline teria no DOMOutputSpec do ProseMirror.
    const svgDataUri = `data:image/svg+xml,${encodeURIComponent(svgString)}`;
    return [
      "div",
      mergeAttributes(HTMLAttributes, extra, { "data-svg-string": svgString }),
      [
        "img",
        {
          src: svgDataUri,
          alt: "",
          "aria-hidden": "true",
          style: "display:block; width:100%; height:100%; pointer-events:none;",
        },
      ],
    ];
  },

  addNodeView() {
    return ({ node, editor, getPos }) => {
      let currentNode = node;
      const dom = document.createElement("div");

      const applyAttrs = () => {
        const a = currentNode.attrs;
        const kind = (a.kind as ShapeKind) ?? "rectangle";
        const widthCm = Number(a.width_cm) || 4;
        const heightCm = Number(a.height_cm) || 3;
        const wrapMode = (a.wrap_mode as ShapeWrapMode) ?? "in_front";
        const wrapX = Number(a.wrap_x_cm) || 0;
        const wrapY = Number(a.wrap_y_cm) || 0;
        const rotation = Number(a.rotation) || 0;
        const strokeColor = (a.stroke_color as string) ?? "#d92626";
        const strokeWidth = Number(a.stroke_width) || 3;
        const fillColor = (a.fill_color as string) ?? "rgba(255,255,255,0)";
        const id = (a.id as string) ?? "anon";

        // Data attrs
        dom.setAttribute("data-sicro-shape", "true");
        dom.setAttribute("data-kind", kind);
        dom.setAttribute("data-width-cm", String(widthCm));
        dom.setAttribute("data-height-cm", String(heightCm));
        dom.setAttribute("data-wrap-mode", wrapMode);
        dom.setAttribute("data-wrap-x-cm", String(wrapX));
        dom.setAttribute("data-wrap-y-cm", String(wrapY));
        dom.setAttribute("data-stroke-color", strokeColor);
        dom.setAttribute("data-stroke-width", String(strokeWidth));
        dom.setAttribute("data-fill-color", fillColor);
        if (rotation) dom.setAttribute("data-rotation", String(rotation));
        else dom.removeAttribute("data-rotation");
        if (id) dom.setAttribute("data-shape-id", id);

        // Style
        const transform = rotation ? ` transform: rotate(${rotation}deg);` : "";
        let containerStyle: string;
        if (wrapMode === "inline") {
          containerStyle = `display: inline-block; width: ${widthCm}cm; height: ${heightCm}cm; margin: 0 auto;${transform}`;
        } else {
          const zIndex = wrapMode === "in_front" ? 5 : -1;
          containerStyle = `position: absolute; left: ${wrapX}cm; top: ${wrapY}cm; width: ${widthCm}cm; height: ${heightCm}cm; z-index: ${zIndex};${transform}`;
        }
        dom.setAttribute("style", containerStyle);
        dom.setAttribute("draggable", wrapMode === "inline" ? "true" : "false");

        // Conteúdo SVG
        dom.innerHTML = renderShapeSvgString(
          kind,
          widthCm,
          heightCm,
          strokeColor,
          strokeWidth,
          fillColor,
          id,
        );
      };

      applyAttrs();

      // Click direto seleciona o shape (mesmo padrão P21 do Figure).
      const onMouseDown = (e: MouseEvent) => {
        if (e.button !== 0) return;
        const pos = getPos();
        if (typeof pos !== "number") return;
        const view = editor.view;
        try {
          const sel = NodeSelection.create(view.state.doc, pos);
          view.dispatch(view.state.tr.setSelection(sel));
        } catch {
          // ignora
        }
      };
      dom.addEventListener("mousedown", onMouseDown);

      // Bloqueia drag&drop nativo em modo flutuante.
      const onDragStart = (e: DragEvent) => {
        const wrapMode = currentNode.attrs.wrap_mode ?? "in_front";
        if (wrapMode !== "inline") e.preventDefault();
      };
      dom.addEventListener("dragstart", onDragStart);

      return {
        dom,
        update(newNode) {
          if (newNode.type !== currentNode.type) return false;
          currentNode = newNode;
          applyAttrs();
          return true;
        },
        destroy() {
          dom.removeEventListener("mousedown", onMouseDown);
          dom.removeEventListener("dragstart", onDragStart);
        },
        ignoreMutation(mutation) {
          if (mutation.type === "selection") return false;
          // Ignora mutations que nós mesmos fazemos via applyAttrs.
          return true;
        },
      };
    };
  },

  addCommands() {
    return {
      insertShape:
        (attrs) =>
        ({ commands }) => {
          const id = generateShapeId();
          const kind = attrs.kind ?? "rectangle";
          // Defaults sensatos por tipo de shape
          let defaultW = 4;
          let defaultH = 3;
          if (kind === "arrow" || kind === "line") {
            defaultW = 5;
            defaultH = 1;
          }
          return commands.insertContent({
            type: this.name,
            attrs: {
              id,
              kind,
              width_cm: attrs.width_cm ?? defaultW,
              height_cm: attrs.height_cm ?? defaultH,
              wrap_mode: attrs.wrap_mode ?? "in_front",
              wrap_x_cm: attrs.wrap_x_cm ?? 3,
              wrap_y_cm: attrs.wrap_y_cm ?? 3,
              rotation: 0,
              stroke_color: attrs.stroke_color ?? "#d92626",
              stroke_width: attrs.stroke_width ?? 3,
              fill_color: attrs.fill_color ?? "rgba(255,255,255,0)",
            },
          });
        },
    };
  },

  addProseMirrorPlugins() {
    const shapeName = this.name;
    // Helper: seleciona shape por NodeSelection.
    const selectShapeViaDOM = (
      view: import("@tiptap/pm/view").EditorView,
      shapeEl: HTMLElement,
      event: Event,
    ): boolean => {
      try {
        const innerPos = view.posAtDOM(shapeEl, 0);
        if (typeof innerPos !== "number" || innerPos < 0) return false;
        const shapePos = Math.max(0, innerPos - 1);
        const sel = NodeSelection.create(view.state.doc, shapePos);
        view.dispatch(view.state.tr.setSelection(sel));
        event.preventDefault();
        return true;
      } catch {
        return false;
      }
    };

    return [
      new Plugin({
        key: new PluginKey("shapeClickSelect"),
        props: {
          handleClickOn(view, _pos, node, nodePos, _event, direct) {
            if (!direct) return false;
            if (node.type.name !== shapeName) return false;
            const sel = NodeSelection.create(view.state.doc, nodePos);
            view.dispatch(view.state.tr.setSelection(sel));
            return true;
          },
          handleDOMEvents: {
            // Fallback: walking up DOM ou coord-based pra shapes flutuantes.
            mouseup(view, event) {
              if (!(event instanceof MouseEvent)) return false;
              const target = event.target as HTMLElement | null;
              if (target) {
                const shapeEl = target.closest(
                  `div[data-sicro-shape]`,
                ) as HTMLElement | null;
                if (shapeEl) return selectShapeViaDOM(view, shapeEl, event);
              }
              // Coord-based fallback pra modo Atrás coberto por parágrafo.
              const textblock = target?.closest(
                "p, h1, h2, h3, h4, h5, h6, li, blockquote",
              ) as HTMLElement | null;
              if (
                !textblock ||
                (textblock.textContent ?? "").trim() === ""
              ) {
                const shapes = view.dom.querySelectorAll(
                  `div[data-sicro-shape][data-wrap-mode]`,
                );
                let best: HTMLElement | null = null;
                let bestZ = -Infinity;
                for (let i = 0; i < shapes.length; i++) {
                  const sh = shapes[i] as HTMLElement;
                  const wrapMode = sh.getAttribute("data-wrap-mode");
                  if (!wrapMode || wrapMode === "inline") continue;
                  const rect = sh.getBoundingClientRect();
                  if (
                    event.clientX >= rect.left &&
                    event.clientX <= rect.right &&
                    event.clientY >= rect.top &&
                    event.clientY <= rect.bottom
                  ) {
                    const z = wrapMode === "in_front" ? 5 : -1;
                    if (z > bestZ) {
                      bestZ = z;
                      best = sh;
                    }
                  }
                }
                if (best) return selectShapeViaDOM(view, best, event);
              }
              return false;
            },
          },
        },
      }),
    ];
  },
});
