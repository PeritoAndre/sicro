/**
 * Figure — block node that wraps an image with a caption.
 *
 * Used by `image`, `croqui` and `video_frame` blocks in the doc 04
 * schema. Numbering is computed at render time based on document order
 * (see `renderer.ts`).
 *
 * MVP 4 — evidence provenance: when a figure represents an evidence
 * inserted from the Dossiê / Croqui / Vídeo, the following extra
 * attributes are populated so the `.sicrodoc` stays auditable:
 *
 *   - `evidence_id`     — UUID of the source entity (media_asset_id,
 *                          croqui_id, video_storyboard_frame_id, etc.)
 *   - `evidence_kind`   — discriminator (photo|croqui|video_frame|...)
 *   - `relative_path`   — workspace-relative path of the asset (PNG/JPG)
 *   - `source_hash`     — SHA-256 when available
 *   - `metadata_json`   — opaque JSON (categoria, timestamp, etc.)
 *
 * Authoring (text / drawings) still uses `src` directly; the evidence
 * fields default to null. Existing `.sicrodoc` envelopes keep loading.
 */

import { Node, mergeAttributes } from "@tiptap/core";
import { NodeSelection, Plugin, PluginKey } from "@tiptap/pm/state";

/** F12.1 — UUID estável para figures. Permite cross-refs persistentes. */
function generateFigureId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return `fig-${crypto.randomUUID().slice(0, 8)}`;
  }
  return `fig-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e6).toString(36)}`;
}

export interface FigureOptions {
  /** Default placeholder src for spike-time inserts. */
  placeholderSrc: string;
}

export type FigureKind = "image" | "croqui" | "video_frame";

/**
 * F6 — Alinhamento da figura na largura útil da página. Default `"center"`.
 */
export type FigureAlign = "left" | "center" | "right";

/**
 * P13 — Modo de wrap (estilo Word):
 *   - `inline`: figura participa do fluxo de texto (default; equivalente
 *     ao "Alinhado ao Texto" do Word).
 *   - `in_front`: figura flutua sobre o texto. position: absolute,
 *     z-index alto. Coords (wrap_x_cm, wrap_y_cm) relativas ao
 *     contêiner posicionado mais próximo (.editorWrap dentro do laudo).
 *   - `behind`: igual a `in_front` mas com z-index negativo — fica
 *     ATRÁS do texto. Útil pra marca d'água, fundos, etc.
 */
export type FigureWrapMode = "inline" | "in_front" | "behind";

declare module "@tiptap/core" {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface Commands<ReturnType> {
    figure: {
      insertFigure: (attrs?: {
        src?: string;
        alt?: string;
        kind?: FigureKind;
        caption?: string;
        /** F6 — largura em CSS (ex: "60%", "12cm"). Default "70%". */
        width?: string | null;
        /** P11 — Altura *da imagem* (não do figure todo) em CSS.
         *  null = auto (preserva aspect natural). Quando setado, a img
         *  recebe `object-fit: fill` pra esticar/comprimir conforme
         *  necessário (handles laterais N/S/E/W). */
        image_height?: string | null;
        /** P13 — Modo de wrap estilo Word. Default "inline". */
        wrap_mode?: FigureWrapMode | null;
        /** P13 — Offset X em cm quando wrap_mode != "inline". */
        wrap_x_cm?: number | null;
        /** P13 — Offset Y em cm quando wrap_mode != "inline". */
        wrap_y_cm?: number | null;
        /** F6 — alinhamento na página. Default "center". */
        align?: FigureAlign | null;
        evidence_id?: string | null;
        evidence_kind?: string | null;
        relative_path?: string | null;
        source_hash?: string | null;
        metadata_json?: string | null;
      }) => ReturnType;
      /**
       * F6 — Define largura/alinhamento da figura selecionada.
       * No-op se a seleção não estiver dentro de uma figura.
       */
      setFigureSize: (attrs: {
        width?: string | null;
        align?: FigureAlign | null;
      }) => ReturnType;
      /**
       * F6 — Substitui a fonte da imagem mantendo legenda e provenance.
       * Útil para "trocar foto" sem perder a numeração nem a legenda.
       */
      replaceFigureSrc: (next: {
        src?: string;
        relative_path?: string | null;
        source_hash?: string | null;
        evidence_id?: string | null;
        evidence_kind?: string | null;
      }) => ReturnType;
    };
  }
}

export const Figure = Node.create<FigureOptions>({
  name: "figure",
  group: "block",
  content: "figcaption",
  draggable: true,
  isolating: true,

  addOptions() {
    return {
      placeholderSrc:
        "data:image/svg+xml;utf8," +
        encodeURIComponent(
          `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 360">
             <rect width="100%" height="100%" fill="#e8eaee"/>
             <text x="50%" y="50%" dominant-baseline="middle" text-anchor="middle"
                   font-family="Inter, sans-serif" font-size="20" fill="#5a6471">
               Imagem placeholder
             </text>
           </svg>`,
        ),
    };
  },

  addAttributes() {
    return {
      // F12.1 — ID estável (UUID) gerado no insert. Permite cross-refs
      // que mantêm a referência mesmo se a figura for movida/renumerada.
      id: { default: null },
      src: { default: null },
      alt: { default: null },
      kind: { default: "image" },
      // ----- F6 — apresentação na página -----
      /** Largura CSS (ex: "70%", "12cm"). Default "70%". */
      width: { default: "70%" },
      /** P11 — Altura *da imagem* (não do figure). null = auto (preserva
       *  aspect natural). Quando setado (ex: "5.20cm"), os handles N/S e
       *  E/W passam a controlar dimensões independentes, e a img recebe
       *  `object-fit: fill` pra esticar/comprimir. Persistido via
       *  `data-image-height` no figure. */
      image_height: {
        default: null,
        parseHTML: (el: HTMLElement) => el.getAttribute("data-image-height") ?? null,
        renderHTML: (attrs: { image_height?: string | null }) => {
          if (!attrs.image_height) return {};
          return { "data-image-height": attrs.image_height };
        },
      },
      /** Alinhamento horizontal. Default "center". */
      align: { default: "center" },
      // ----- P13 — Wrap mode (estilo Word) -----
      /** "inline" (default) | "in_front" | "behind". Quando != inline,
       *  a figure vira `position: absolute` ancorada em coords
       *  (wrap_x_cm, wrap_y_cm) relativas ao .editorWrap. Persistido via
       *  `data-wrap-mode`. Docs antigos abrem com "inline" → mesmo
       *  comportamento de antes. */
      wrap_mode: {
        default: "inline",
        parseHTML: (el: HTMLElement) =>
          (el.getAttribute("data-wrap-mode") as FigureWrapMode | null) ??
          "inline",
        renderHTML: (attrs: { wrap_mode?: FigureWrapMode | null }) => {
          const m = attrs.wrap_mode ?? "inline";
          if (m === "inline") return {};
          return { "data-wrap-mode": m };
        },
      },
      /** Offset X em cm (relativo ao .editorWrap). Só usado quando
       *  wrap_mode != "inline". */
      wrap_x_cm: {
        default: 0,
        parseHTML: (el: HTMLElement) => {
          const raw = el.getAttribute("data-wrap-x-cm");
          if (!raw) return 0;
          const n = parseFloat(raw);
          return Number.isFinite(n) ? n : 0;
        },
        renderHTML: (attrs: { wrap_x_cm?: number }) => {
          const v = attrs.wrap_x_cm ?? 0;
          if (!v) return {};
          return { "data-wrap-x-cm": String(v) };
        },
      },
      /** Offset Y em cm (relativo ao .editorWrap). Só usado quando
       *  wrap_mode != "inline". */
      wrap_y_cm: {
        default: 0,
        parseHTML: (el: HTMLElement) => {
          const raw = el.getAttribute("data-wrap-y-cm");
          if (!raw) return 0;
          const n = parseFloat(raw);
          return Number.isFinite(n) ? n : 0;
        },
        renderHTML: (attrs: { wrap_y_cm?: number }) => {
          const v = attrs.wrap_y_cm ?? 0;
          if (!v) return {};
          return { "data-wrap-y-cm": String(v) };
        },
      },
      // ----- P — Handles de manipulação visual -----
      /** Rotação em graus (positivo = horário). Default 0. Aditivo:
       *  docs antigos sem este attr abrem com 0. Persistido via attr
       *  HTML `data-rotation` no figure. */
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
      // ----- MVP 4 evidence provenance -----
      evidence_id: { default: null },
      evidence_kind: { default: null },
      relative_path: { default: null },
      source_hash: { default: null },
      metadata_json: { default: null },
    };
  },

  parseHTML() {
    return [{ tag: "figure[data-sicro-figure]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    const kind = (node.attrs.kind as string) ?? "image";
    const width = (node.attrs.width as string) ?? "70%";
    const align = (node.attrs.align as string) ?? "center";
    const rotation = Number(node.attrs.rotation) || 0;
    const imageHeight =
      (node.attrs.image_height as string | null | undefined) ?? null;
    const wrapMode =
      (node.attrs.wrap_mode as FigureWrapMode | null | undefined) ?? "inline";
    const wrapX = Number(node.attrs.wrap_x_cm) || 0;
    const wrapY = Number(node.attrs.wrap_y_cm) || 0;
    // Composição estilística: width controla o tamanho da imagem, align
    // controla a posição da figura inteira na largura da página.
    const alignMargin =
      align === "left"
        ? "0 auto 0 0"
        : align === "right"
          ? "0 0 0 auto"
          : "0 auto";
    // P — Aplica rotate via transform. transform-origin: center pra
    // rotacionar em torno do meio. Suaviza a transição quando NÃO está
    // arrastando (durante drag, a classe `.sicro-figure-rotating` no
    // body remove a transition pra responder em tempo real).
    const transform = rotation ? ` transform: rotate(${rotation}deg);` : "";
    // P25 — Wrapper-âncora. Style depende do wrap_mode. Inline = fluxo
    // normal (figure flutua no fluxo com margin:auto). Floating
    // (in_front/behind) = position: absolute ancorado em wrap_x_cm/y_cm
    // RELATIVO AO WRAPPER (que é position:relative; height:0; em fluxo) —
    // não mais relativo ao .editorWrap global. Isso faz o editor e o PDF
    // posicionarem a foto IDENTICAMENTE (ver wrapper abaixo). z-index
    // controla in_front vs behind.
    let figStyle: string;
    if (wrapMode === "inline") {
      figStyle = `width: ${width}; margin: ${alignMargin};${transform}`;
    } else {
      const zIndex = wrapMode === "in_front" ? 5 : -1;
      figStyle =
        `width: ${width}; position: absolute; left: ${wrapX}cm; top: ${wrapY}cm; z-index: ${zIndex}; margin: 0;${transform}`;
    }
    const extra: Record<string, string> = {
      "data-sicro-figure": "true",
      "data-kind": kind,
      "data-align": align,
      "data-width": width,
      style: figStyle,
    };
    if (rotation) extra["data-rotation"] = String(rotation);
    if (imageHeight) extra["data-image-height"] = imageHeight;
    // P15 — Em modo flutuante, desabilita o `draggable="true"` que o PM
    // adicionaria por causa de `draggable: true` no schema. Sem isso,
    // o browser interpreta clicks como drag-start e suprime os events
    // click/select da figure (foto fica inclicável).
    if (wrapMode !== "inline") {
      extra["draggable"] = "false";
    }
    // wrap_mode/wrap_x_cm/wrap_y_cm já emitidos via renderHTML por attr.
    if (node.attrs.id) extra["data-fig-id"] = String(node.attrs.id);
    if (node.attrs.evidence_id)
      extra["data-evidence-id"] = String(node.attrs.evidence_id);
    if (node.attrs.evidence_kind)
      extra["data-evidence-kind"] = String(node.attrs.evidence_kind);
    if (node.attrs.relative_path)
      extra["data-relative-path"] = String(node.attrs.relative_path);
    if (node.attrs.source_hash)
      extra["data-source-hash"] = String(node.attrs.source_hash);
    // P11 — Style da imagem depende de ter image_height explícito ou não:
    //  - Sem image_height: `height: auto` → preserva aspect natural
    //    (handles de canto sem Shift produzem este estado)
    //  - Com image_height: `height: Xcm; object-fit: fill` → permite
    //    distorcer (handles laterais ou cantos+Shift produzem este estado)
    const imgStyle = imageHeight
      ? `width: 100%; height: ${imageHeight}; display: block; object-fit: fill;`
      : "width: 100%; height: auto; display: block;";
    // P25 — Wrapper-âncora: a <figure> SEMPRE vive dentro de um <div
    // data-figure-anchor>. Isso garante que o editor (NodeView) e o
    // export (renderHTML → PDF/HTML/clones estáticos) usem o MESMO frame
    // de coordenada para `position:absolute`.
    //   - Inline: o wrapper é INERTE (display:block, static, sem tamanho
    //     próprio) — a figure flui normalmente com margin:auto. Layout
    //     idêntico ao de antes.
    //   - Floating (in_front/behind): o wrapper é `position:relative;
    //     height:0` em fluxo (ocupa zero espaço vertical, não empurra
    //     texto). A figure interna é `position:absolute` ancorada ao
    //     wrapper → `top: wrap_y_cm` agora é offset a partir da âncora da
    //     figura no fluxo (página correta), não do topo do documento.
    const figureNode = [
      "figure",
      mergeAttributes(HTMLAttributes, extra),
      [
        "img",
        {
          src: node.attrs.src ?? this.options.placeholderSrc,
          alt: node.attrs.alt ?? "",
          style: imgStyle,
        },
      ],
      ["div", { "data-sicro-figcaption-slot": "true" }, 0],
    ];
    const wrapperAttrs: Record<string, string> = {
      "data-figure-anchor": "true",
    };
    if (wrapMode === "inline") {
      wrapperAttrs["style"] = "display: block;";
    } else {
      wrapperAttrs["style"] = "display: block; position: relative; height: 0;";
      // data-wrap-mode no wrapper só pra debug/inspeção (espelha o NodeView).
      wrapperAttrs["data-wrap-mode"] = wrapMode;
    }
    return ["div", wrapperAttrs, figureNode];
  },

  /**
   * P21 — NodeView: dá controle TOTAL do DOM do figure. PM agora chama
   * o nosso `update()` em vez de re-renderizar do zero quando attrs
   * mudam. Resultado:
   *  - O elemento DOM da figure é ESTÁVEL (não é substituído por PM
   *    quando wrap_mode muda) → `selected.domEl` permanece válido,
   *    overlay não pisca/some;
   *  - Click handler direto no DOM (sem depender de PM coord mapping
   *    nem handleClickOn) → figure sempre selecionável;
   *  - O renderHTML continua existindo para serialização (HTML/PDF/
   *    DOCX export), mas o NodeView é o que controla a visualização
   *    no editor.
   */
  addNodeView() {
    const placeholderSrc = this.options.placeholderSrc;
    return ({ node, editor, getPos }) => {
      let currentNode = node;

      // P25 — `dom` é o WRAPPER-ÂNCORA <div>, ESTÁVEL pela vida toda do
      // NodeView (não recriado ao trocar wrap_mode → preserva P21:
      // `selected.domEl` continua válido, overlay não pisca). A <figure>
      // real vive DENTRO do wrapper. Espelha o `renderHTML` (export/PDF)
      // pra editor e PDF posicionarem a foto flutuante no MESMO frame.
      //
      // O `useSelectedFigure` / `FigureOverlay` resolvem o `nodeDOM` do
      // wrapper de volta pra <figure> interna (via [data-sicro-figure]),
      // pois é a figure — não o wrapper de altura zero — que mede o rect
      // dos handles e serve de âncora pro drag/resize.
      const dom = document.createElement("div");
      dom.setAttribute("data-figure-anchor", "true");

      const figureEl = document.createElement("figure");
      const imgEl = document.createElement("img");
      figureEl.appendChild(imgEl);

      const contentDOM = document.createElement("div");
      contentDOM.setAttribute("data-sicro-figcaption-slot", "true");
      figureEl.appendChild(contentDOM);

      dom.appendChild(figureEl);

      const applyAttrs = () => {
        const a = currentNode.attrs;
        const wrapMode = (a.wrap_mode as string) ?? "inline";
        const width = (a.width as string) ?? "70%";
        const align = (a.align as string) ?? "center";
        const rotation = Number(a.rotation) || 0;
        const imageHeight = (a.image_height as string | null) ?? null;
        const wrapX = Number(a.wrap_x_cm) || 0;
        const wrapY = Number(a.wrap_y_cm) || 0;
        const kind = (a.kind as string) ?? "image";

        // Data attrs — vão na <figure> interna (figureEl), NÃO no wrapper.
        // O wrapper só carrega data-figure-anchor + data-wrap-mode (debug)
        // + o style de ancoragem.
        figureEl.setAttribute("data-sicro-figure", "true");
        figureEl.setAttribute("data-kind", kind);
        figureEl.setAttribute("data-align", align);
        figureEl.setAttribute("data-width", width);
        if (rotation) figureEl.setAttribute("data-rotation", String(rotation));
        else figureEl.removeAttribute("data-rotation");
        if (imageHeight) figureEl.setAttribute("data-image-height", imageHeight);
        else figureEl.removeAttribute("data-image-height");
        if (wrapMode !== "inline") {
          figureEl.setAttribute("data-wrap-mode", wrapMode);
          figureEl.setAttribute("data-wrap-x-cm", String(wrapX));
          figureEl.setAttribute("data-wrap-y-cm", String(wrapY));
          figureEl.setAttribute("draggable", "false");
        } else {
          figureEl.removeAttribute("data-wrap-mode");
          figureEl.removeAttribute("data-wrap-x-cm");
          figureEl.removeAttribute("data-wrap-y-cm");
          figureEl.setAttribute("draggable", "true");
        }
        if (a.id) figureEl.setAttribute("data-fig-id", String(a.id));
        if (a.evidence_id)
          figureEl.setAttribute("data-evidence-id", String(a.evidence_id));
        if (a.evidence_kind)
          figureEl.setAttribute("data-evidence-kind", String(a.evidence_kind));
        if (a.relative_path)
          figureEl.setAttribute("data-relative-path", String(a.relative_path));
        if (a.source_hash)
          figureEl.setAttribute("data-source-hash", String(a.source_hash));

        // Wrapper-âncora style + data-wrap-mode (debug/seleção por coords).
        if (wrapMode !== "inline") {
          dom.setAttribute("data-wrap-mode", wrapMode);
          dom.setAttribute(
            "style",
            "display: block; position: relative; height: 0;",
          );
        } else {
          dom.removeAttribute("data-wrap-mode");
          dom.setAttribute("style", "display: block;");
        }

        // Figure style
        const transform = rotation
          ? ` transform: rotate(${rotation}deg);`
          : "";
        let figStyle: string;
        if (wrapMode === "inline") {
          const alignMargin =
            align === "left"
              ? "0 auto 0 0"
              : align === "right"
                ? "0 0 0 auto"
                : "0 auto";
          figStyle = `width: ${width}; margin: ${alignMargin};${transform}`;
        } else {
          const zIndex = wrapMode === "in_front" ? 5 : -1;
          figStyle = `width: ${width}; position: absolute; left: ${wrapX}cm; top: ${wrapY}cm; z-index: ${zIndex}; margin: 0;${transform}`;
        }
        figureEl.setAttribute("style", figStyle);

        // Img
        imgEl.setAttribute("src", (a.src as string) ?? placeholderSrc);
        imgEl.setAttribute("alt", (a.alt as string) ?? "");
        const imgStyle = imageHeight
          ? `width: 100%; height: ${imageHeight}; display: block; object-fit: fill;`
          : "width: 100%; height: auto; display: block;";
        imgEl.setAttribute("style", imgStyle);
      };

      applyAttrs();

      // Click direto no DOM: garante seleção mesmo quando posAtCoords
      // do PM bater errado (figure flutuante com DOM ancorado num pos
      // diferente do visual). Usamos mousedown (mais confiável que click
      // pra elementos draggable) com preventDefault pra evitar drag
      // nativo do browser.
      const onMouseDown = (e: MouseEvent) => {
        if (e.button !== 0) return;
        const target = e.target as HTMLElement | null;
        if (target && target.closest("figcaption")) return; // editar legenda
        const pos = getPos();
        if (typeof pos !== "number") return;
        // setSelection direto via view dispatch (sem .focus() async).
        const view = editor.view;
        try {
          const sel = NodeSelection.create(view.state.doc, pos);
          view.dispatch(view.state.tr.setSelection(sel));
        } catch {
          // pos invalida ou nao-selecionavel — ignora
        }
      };
      dom.addEventListener("mousedown", onMouseDown);

      // Bloqueia drag&drop nativo pra figures flutuantes.
      const onDragStart = (e: DragEvent) => {
        const wrapMode = currentNode.attrs.wrap_mode ?? "inline";
        if (wrapMode !== "inline") e.preventDefault();
      };
      dom.addEventListener("dragstart", onDragStart);

      return {
        dom,
        contentDOM,
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
        // PM precisa saber que NÃO devemos ignorar mutations no
        // contentDOM (= onde figcaption renderiza). Retornar false
        // significa "essa mutation pode ser processada pelo PM".
        ignoreMutation(mutation) {
          if (mutation.type === "selection") return false;
          const target = mutation.target as unknown as globalThis.Node;
          // Mutations dentro do contentDOM (figcaption editing)
          // devem ser propagadas pro PM.
          if (
            mutation.type === "childList" ||
            mutation.type === "characterData"
          ) {
            return !contentDOM.contains(target);
          }
          // Mutations nos attrs do wrapper/figure/img (style/data-*) que
          // NÓS setamos via applyAttrs — PM deve ignorar pra não tentar
          // re-render.
          if (mutation.type === "attributes") {
            return target === dom || target === figureEl || target === imgEl;
          }
          return false;
        },
      };
    };
  },

  addCommands() {
    return {
      insertFigure:
        (attrs) =>
        ({ commands }) => {
          const src = attrs?.src ?? this.options.placeholderSrc;
          const caption = attrs?.caption ?? "Descrição da figura.";
          // F12.1 — Gera UUID estável para esta figura.
          const id = generateFigureId();
          return commands.insertContent({
            type: this.name,
            attrs: {
              id,
              src,
              alt: attrs?.alt ?? "",
              kind: attrs?.kind ?? "image",
              width: attrs?.width ?? "70%",
              image_height: attrs?.image_height ?? null,
              align: attrs?.align ?? "center",
              wrap_mode: attrs?.wrap_mode ?? "inline",
              wrap_x_cm: attrs?.wrap_x_cm ?? 0,
              wrap_y_cm: attrs?.wrap_y_cm ?? 0,
              evidence_id: attrs?.evidence_id ?? null,
              evidence_kind: attrs?.evidence_kind ?? null,
              relative_path: attrs?.relative_path ?? null,
              source_hash: attrs?.source_hash ?? null,
              metadata_json: attrs?.metadata_json ?? null,
            },
            content: [
              {
                type: "figcaption",
                content: [{ type: "text", text: caption }],
              },
            ],
          });
        },
      setFigureSize:
        ({ width, align }) =>
        ({ chain }) => {
          const patch: Record<string, unknown> = {};
          if (width !== undefined) patch["width"] = width;
          if (align !== undefined) patch["align"] = align;
          if (Object.keys(patch).length === 0) return false;
          return chain().updateAttributes(this.name, patch).run();
        },
      replaceFigureSrc:
        (next) =>
        ({ chain }) => {
          const patch: Record<string, unknown> = {};
          if (next.src !== undefined) patch["src"] = next.src;
          if (next.relative_path !== undefined)
            patch["relative_path"] = next.relative_path;
          if (next.source_hash !== undefined)
            patch["source_hash"] = next.source_hash;
          if (next.evidence_id !== undefined)
            patch["evidence_id"] = next.evidence_id;
          if (next.evidence_kind !== undefined)
            patch["evidence_kind"] = next.evidence_kind;
          return chain().updateAttributes(this.name, patch).run();
        },
    };
  },

  // P — Plugin que captura clique em figure → cria NodeSelection sobre
  // ele. Sem isso, o ProseMirror trata o clique como "posiciona cursor
  // de texto" e o `useSelectedFigure` nunca vê o figure selecionado.
  // Clique no figcaption permanece tratado como text selection (pra
  // edição da legenda funcionar normal).
  addProseMirrorPlugins() {
    const figureName = this.name;
    // Helper: seleciona uma figure por NodeSelection.
    const selectFigureViaDOM = (
      view: import("@tiptap/pm/view").EditorView,
      figEl: HTMLElement,
      event: Event,
    ): boolean => {
      try {
        const innerPos = view.posAtDOM(figEl, 0);
        if (typeof innerPos !== "number" || innerPos < 0) return false;
        const figurePos = Math.max(0, innerPos - 1);
        const sel = NodeSelection.create(view.state.doc, figurePos);
        view.dispatch(view.state.tr.setSelection(sel));
        event.preventDefault();
        return true;
      } catch {
        return false;
      }
    };

    // P15 — Helper: dado um event do DOM, anda pra cima procurando uma
    // figure FLUTUANTE (data-wrap-mode != inline) e dispara
    // NodeSelection se achar. Usado por click E mouseup pra defender
    // contra suppression de click events em draggable elements.
    const selectFloatingFigureFromEvent = (
      view: import("@tiptap/pm/view").EditorView,
      event: Event,
    ): boolean => {
      const target = event.target as HTMLElement | null;
      if (!target) return false;
      const figEl = target.closest(
        `figure[data-sicro-figure]`,
      ) as HTMLElement | null;
      if (!figEl) return false;
      const wrapMode = figEl.getAttribute("data-wrap-mode");
      // Inline já é coberto pelo handleClickOn.
      if (!wrapMode || wrapMode === "inline") return false;
      // Cliques no figcaption seguem como text selection.
      if (target.closest("figcaption")) return false;
      return selectFigureViaDOM(view, figEl, event);
    };

    // P23 — Fallback BASEADO EM COORDENADAS pra figures "Atrás do texto"
    // que estão ATRÁS dos parágrafos (z-index: -1). O click não cai
    // direto na figure (cai no parágrafo acima), então walk-up DOM não
    // acha. Aqui iteramos todas figures flutuantes e checamos se as
    // coords do click estão dentro do bbox de alguma. Só faz isso se
    // o target NÃO contém texto (parágrafo vazio) — pra não roubar
    // clicks de edição de texto real.
    const selectFloatingFigureByCoords = (
      view: import("@tiptap/pm/view").EditorView,
      event: MouseEvent,
    ): boolean => {
      const target = event.target as HTMLElement | null;
      if (!target) return false;
      // Se target tem texto real, não roubamos o click — user quer texto.
      const textblock = target.closest(
        "p, h1, h2, h3, h4, h5, h6, li, blockquote",
      ) as HTMLElement | null;
      if (textblock && (textblock.textContent ?? "").trim() !== "") {
        return false;
      }
      // Procura figure flutuante cujo bbox contenha o click. Em caso de
      // overlap, prefere a com z-index maior (in_front > behind).
      const figs = view.dom.querySelectorAll(
        `figure[data-sicro-figure][data-wrap-mode]`,
      );
      let best: HTMLElement | null = null;
      let bestZ = -Infinity;
      for (let i = 0; i < figs.length; i++) {
        const fig = figs[i] as HTMLElement;
        const wrapMode = fig.getAttribute("data-wrap-mode");
        if (!wrapMode || wrapMode === "inline") continue;
        const rect = fig.getBoundingClientRect();
        if (
          event.clientX >= rect.left &&
          event.clientX <= rect.right &&
          event.clientY >= rect.top &&
          event.clientY <= rect.bottom
        ) {
          const z = wrapMode === "in_front" ? 5 : -1;
          if (z > bestZ) {
            bestZ = z;
            best = fig;
          }
        }
      }
      if (best) return selectFigureViaDOM(view, best, event);
      return false;
    };

    return [
      new Plugin({
        key: new PluginKey("figureClickSelect"),
        props: {
          handleClickOn(view, _pos, node, nodePos, event, direct) {
            if (!direct) return false;
            if (node.type.name !== figureName) return false;
            // Se o clique caiu dentro de um <figcaption>, deixa o
            // ProseMirror tratar normalmente (cursor de texto na legenda).
            const target = event.target as HTMLElement | null;
            if (target && target.closest("figcaption")) return false;
            // Caso contrário, seleciona o figure inteiro como nó.
            const sel = NodeSelection.create(view.state.doc, nodePos);
            view.dispatch(view.state.tr.setSelection(sel));
            return true;
          },
          // P14 — Fallback robusto pra cliques em figures FLUTUANTES
          // (wrap_mode = in_front | behind). Quando a figure é
          // position: absolute, o DOM dela continua ancorado no parágrafo
          // de origem mas o visual está em outro lugar — o handleClickOn
          // acima depende do `posAtCoords` do PM, que mapeia o click
          // pra posição do parágrafo (não da figure), então a figure
          // nunca era selecionada de volta.
          // Aqui andamos pra cima do `event.target` procurando o
          // figure DOM e usamos `posAtDOM` que mapeia DOM→pos diretamente
          // (sem depender de coords visuais).
          handleDOMEvents: {
            // P15 — Helper compartilhado entre `click` e `mouseup`.
            // Tentamos pelos DOIS porque, em algumas situações com
            // draggable=true / position:absolute, o browser SUPRIME
            // o evento `click`. mouseup sempre dispara.
            mouseup(view, event) {
              // Primeira tentativa: target é dentro de uma figure flutuante
              if (selectFloatingFigureFromEvent(view, event)) return true;
              // P23 — Segunda tentativa: target NÃO é figure mas click
              // pode estar sobre o bbox de uma figure "Atrás do texto"
              // que está atrás de um parágrafo vazio.
              if (event instanceof MouseEvent) {
                return selectFloatingFigureByCoords(view, event);
              }
              return false;
            },
            click(view, event) {
              if (selectFloatingFigureFromEvent(view, event)) return true;
              if (event instanceof MouseEvent) {
                return selectFloatingFigureByCoords(view, event);
              }
              return false;
            },
          },
        },
      }),
    ];
  },
});

/**
 * Inline caption that sits inside `<figure>`. Plain block of text only.
 */
export const FigCaption = Node.create({
  name: "figcaption",
  content: "inline*",
  marks: "_",
  defining: true,

  parseHTML() {
    return [{ tag: "figcaption" }];
  },

  renderHTML({ HTMLAttributes }) {
    return ["figcaption", mergeAttributes(HTMLAttributes), 0];
  },
});
