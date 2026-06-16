/**
 * U — TextBox: nó pra caixa de texto editável (estilo Word).
 *
 * Padrão visual:
 *   - container flutuante (`in_front` por default) com border + fill
 *     opcionais (ambos nulificáveis via `border_enabled` / `fill_enabled`)
 *   - rotação via CSS transform
 *   - conteúdo interno: `block+` (paragraphs, headings, lists com TextStyle
 *     completo) — o user controla fonte/tamanho/cor com a toolbar normal
 *
 * Diferenças vs Shape:
 *   - Shape é `atom: true` (sem conteúdo); TextBox tem `content: "block+"`
 *   - Shape renderiza SVG; TextBox renderiza div estilizado + contentDOM
 *   - TextBox precisa diferenciar click NO BORDER (select node) vs click
 *     NO TEXTO INTERNO (text cursor para edição)
 *
 * Diferenças vs Figure:
 *   - Figure tem figcaption (inline-only); TextBox aceita parágrafos
 *     inteiros lá dentro
 *   - Figure renderiza imagem; TextBox é container de texto puro
 *
 * NodeView dá controle total do DOM (mesmo padrão P21 do Figure / R do
 * Shape) pra estabilidade.
 */

import { Node, mergeAttributes } from "@tiptap/core";
import {
  NodeSelection,
  Plugin,
  PluginKey,
  Selection,
  TextSelection,
} from "@tiptap/pm/state";

export type TextBoxWrapMode = "inline" | "in_front" | "behind";
export type TextBoxBorderStyle = "solid" | "dashed" | "dotted";
/**
 * U2 — Direção do texto dentro da caixa (estilo "direção do texto" do Word).
 *   - `horizontal`     → padrão, texto da esquerda p/ direita.
 *   - `vertical_up`    → texto na vertical lido DE BAIXO P/ CIMA (head-tilt
 *                        à esquerda), como a marca lateral institucional
 *                        "POLÍCIA CIENTÍFICA…". CSS: writing-mode vertical-rl
 *                        + rotate(180deg).
 *   - `vertical_down`  → texto na vertical lido de cima p/ baixo.
 * A caixa mantém posição/tamanho — só o fluxo do texto muda (≠ girar a
 * caixa inteira via `rotation`).
 */
export type TextBoxOrientation = "horizontal" | "vertical_up" | "vertical_down";

/** Gera ID estável pra textbox. */
function generateTextBoxId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return `tbx-${crypto.randomUUID().slice(0, 8)}`;
  }
  return `tbx-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e6).toString(36)}`;
}

declare module "@tiptap/core" {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface Commands<ReturnType> {
    text_box: {
      insertTextBox: (attrs?: {
        width_cm?: number;
        height_cm?: number;
        wrap_mode?: TextBoxWrapMode;
        wrap_x_cm?: number;
        wrap_y_cm?: number;
        border_enabled?: boolean;
        border_color?: string;
        border_width?: number;
        border_style?: TextBoxBorderStyle;
        fill_enabled?: boolean;
        fill_color?: string;
        text_orientation?: TextBoxOrientation;
      }) => ReturnType;
    };
  }
}

/** Pure: monta o style string do container baseado em todos os attrs.
 *  Reusada por renderHTML e applyAttrs (NodeView) pra evitar drift. */
function buildContainerStyle(args: {
  widthCm: number;
  heightCm: number;
  wrapMode: TextBoxWrapMode;
  wrapXCm: number;
  wrapYCm: number;
  rotation: number;
  borderEnabled: boolean;
  borderColor: string;
  borderWidth: number;
  borderStyle: TextBoxBorderStyle;
  fillEnabled: boolean;
  fillColor: string;
  paddingCm: number;
}): string {
  const {
    widthCm,
    heightCm,
    wrapMode,
    wrapXCm,
    wrapYCm,
    rotation,
    borderEnabled,
    borderColor,
    borderWidth,
    borderStyle,
    fillEnabled,
    fillColor,
    paddingCm,
  } = args;
  const transform = rotation ? ` transform: rotate(${rotation}deg);` : "";
  // Pós-laudo U fix — borda "desligada" usa cor TRANSPARENTE em vez de
  // `border: none`. Razão: `border: none` zera o espaço da borda no
  // box-sizing, e como o conteúdo da textbox vazia também é invisível,
  // o usuário fica sem nenhuma área clicável. Com `transparent`, a
  // borda continua ocupando o layout e o hit-area, só não pinta nada
  // — o user clica em cima dela igual antes. Mesmo princípio do fill
  // (que já era transparent quando desligado).
  const border = ` border: ${borderWidth}px ${borderStyle} ${borderEnabled ? borderColor : "transparent"};`;
  const fill = fillEnabled
    ? ` background-color: ${fillColor};`
    : ` background-color: transparent;`;
  const padding = ` padding: ${paddingCm}cm; box-sizing: border-box;`;
  if (wrapMode === "inline") {
    return (
      `display: inline-block; width: ${widthCm}cm; height: ${heightCm}cm; margin: 0 auto;` +
      padding +
      border +
      fill +
      transform
    );
  }
  const zIndex = wrapMode === "in_front" ? 5 : -1;
  return (
    `position: absolute; left: ${wrapXCm}cm; top: ${wrapYCm}cm; width: ${widthCm}cm; height: ${heightCm}cm; z-index: ${zIndex};` +
    padding +
    border +
    fill +
    transform
  );
}

/** U2 — Pure: monta o style do elemento de CONTEÚDO (wrapper interno) a
 *  partir da orientação do texto. Reusada pelo renderHTML (export) e pelo
 *  NodeView (editor). `vertical_up` lê de baixo p/ cima (marca lateral);
 *  `vertical_down`, de cima p/ baixo. `writing-mode: vertical-rl` é
 *  suportado nativamente pelo WebView2 (Chromium); o rotate(180deg)
 *  inverte o sentido pra leitura ascendente. */
export function buildTextBoxContentStyle(
  orientation: TextBoxOrientation,
): string {
  const base = "width: 100%; height: 100%; outline: none;";
  if (orientation === "vertical_up") {
    return `${base} writing-mode: vertical-rl; transform: rotate(180deg);`;
  }
  if (orientation === "vertical_down") {
    return `${base} writing-mode: vertical-rl;`;
  }
  return base;
}

export const TextBox = Node.create({
  name: "text_box",
  group: "block",
  content: "block+",
  draggable: true,
  selectable: true,
  // isolating evita que selection arraste pra fora da textbox via cursor
  // navigation, e que conteúdo de fora seja "mergido" com o de dentro.
  isolating: true,

  addAttributes() {
    return {
      id: { default: null },
      width_cm: {
        default: 6,
        parseHTML: (el: HTMLElement) => {
          const raw = el.getAttribute("data-width-cm");
          if (!raw) return 6;
          const n = parseFloat(raw);
          return Number.isFinite(n) && n > 0 ? n : 6;
        },
        renderHTML: (attrs: { width_cm?: number }) => ({
          "data-width-cm": String(attrs.width_cm ?? 6),
        }),
      },
      height_cm: {
        default: 3,
        parseHTML: (el: HTMLElement) => {
          const raw = el.getAttribute("data-height-cm");
          if (!raw) return 3;
          const n = parseFloat(raw);
          return Number.isFinite(n) && n > 0 ? n : 3;
        },
        renderHTML: (attrs: { height_cm?: number }) => ({
          "data-height-cm": String(attrs.height_cm ?? 3),
        }),
      },
      wrap_mode: {
        default: "in_front",
        parseHTML: (el: HTMLElement) =>
          (el.getAttribute("data-wrap-mode") as TextBoxWrapMode | null) ??
          "in_front",
        renderHTML: (attrs: { wrap_mode?: TextBoxWrapMode | null }) => ({
          "data-wrap-mode": attrs.wrap_mode ?? "in_front",
        }),
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
      // Border — independente, com flag separada pra "desaparecer".
      border_enabled: {
        default: true,
        parseHTML: (el: HTMLElement) =>
          el.getAttribute("data-border-enabled") !== "false",
        renderHTML: (attrs: { border_enabled?: boolean }) => ({
          "data-border-enabled": String(attrs.border_enabled ?? true),
        }),
      },
      border_color: {
        default: "#1f2937",
        parseHTML: (el: HTMLElement) =>
          el.getAttribute("data-border-color") ?? "#1f2937",
        renderHTML: (attrs: { border_color?: string }) => ({
          "data-border-color": attrs.border_color ?? "#1f2937",
        }),
      },
      border_width: {
        default: 1,
        parseHTML: (el: HTMLElement) => {
          const raw = el.getAttribute("data-border-width");
          if (!raw) return 1;
          const n = parseFloat(raw);
          return Number.isFinite(n) && n >= 0 ? n : 1;
        },
        renderHTML: (attrs: { border_width?: number }) => ({
          "data-border-width": String(attrs.border_width ?? 1),
        }),
      },
      border_style: {
        default: "solid",
        parseHTML: (el: HTMLElement) =>
          (el.getAttribute("data-border-style") as TextBoxBorderStyle | null) ??
          "solid",
        renderHTML: (attrs: { border_style?: TextBoxBorderStyle }) => ({
          "data-border-style": attrs.border_style ?? "solid",
        }),
      },
      // Fill — também independente.
      fill_enabled: {
        default: false,
        parseHTML: (el: HTMLElement) =>
          el.getAttribute("data-fill-enabled") === "true",
        renderHTML: (attrs: { fill_enabled?: boolean }) => ({
          "data-fill-enabled": String(attrs.fill_enabled ?? false),
        }),
      },
      fill_color: {
        default: "#ffffff",
        parseHTML: (el: HTMLElement) =>
          el.getAttribute("data-fill-color") ?? "#ffffff",
        renderHTML: (attrs: { fill_color?: string }) => ({
          "data-fill-color": attrs.fill_color ?? "#ffffff",
        }),
      },
      padding_cm: {
        default: 0.3,
        parseHTML: (el: HTMLElement) => {
          const raw = el.getAttribute("data-padding-cm");
          if (!raw) return 0.3;
          const n = parseFloat(raw);
          return Number.isFinite(n) && n >= 0 ? n : 0.3;
        },
        renderHTML: (attrs: { padding_cm?: number }) => ({
          "data-padding-cm": String(attrs.padding_cm ?? 0.3),
        }),
      },
      // U2 — Direção do texto (horizontal / vertical de baixo p/ cima /
      // vertical de cima p/ baixo). Persistida em data-attr; aplicada via
      // writing-mode no wrapper de conteúdo.
      text_orientation: {
        default: "horizontal",
        parseHTML: (el: HTMLElement) =>
          (el.getAttribute("data-text-orientation") as
            | TextBoxOrientation
            | null) ?? "horizontal",
        renderHTML: (attrs: { text_orientation?: TextBoxOrientation }) => ({
          "data-text-orientation": attrs.text_orientation ?? "horizontal",
        }),
      },
    };
  },

  parseHTML() {
    // U2 — `contentElement` localiza o wrapper interno (novos docs) e cai
    // de volta no próprio elemento (docs antigos, sem wrapper) — evita
    // perda de conteúdo na migração.
    return [
      {
        tag: "div[data-sicro-textbox]",
        contentElement: (dom: globalThis.Node) =>
          (dom as HTMLElement).querySelector<HTMLElement>(
            "[data-sicro-textbox-content]",
          ) ?? (dom as HTMLElement),
      },
    ];
  },

  renderHTML({ node, HTMLAttributes }) {
    const widthCm = Number(node.attrs.width_cm) || 6;
    const heightCm = Number(node.attrs.height_cm) || 3;
    const wrapMode = (node.attrs.wrap_mode as TextBoxWrapMode) ?? "in_front";
    const wrapXCm = Number(node.attrs.wrap_x_cm) || 0;
    const wrapYCm = Number(node.attrs.wrap_y_cm) || 0;
    const rotation = Number(node.attrs.rotation) || 0;
    const borderEnabled = node.attrs.border_enabled !== false;
    const borderColor = (node.attrs.border_color as string) ?? "#1f2937";
    const borderWidth = Number(node.attrs.border_width) || 1;
    const borderStyle =
      (node.attrs.border_style as TextBoxBorderStyle) ?? "solid";
    const fillEnabled = node.attrs.fill_enabled === true;
    const fillColor = (node.attrs.fill_color as string) ?? "#ffffff";
    const paddingCm = Number(node.attrs.padding_cm) || 0.3;
    const orientation =
      (node.attrs.text_orientation as TextBoxOrientation) ?? "horizontal";

    const style = buildContainerStyle({
      widthCm,
      heightCm,
      wrapMode,
      wrapXCm,
      wrapYCm,
      rotation,
      borderEnabled,
      borderColor,
      borderWidth,
      borderStyle,
      fillEnabled,
      fillColor,
      paddingCm,
    });

    const extra: Record<string, string> = {
      "data-sicro-textbox": "true",
      style,
    };

    return [
      "div",
      mergeAttributes(HTMLAttributes, extra),
      // U2 — wrapper interno carrega a direção do texto (writing-mode).
      // O content slot (0) fica dentro dele, espelhando o NodeView
      // (dom > contentDOM) pra paridade editor↔export.
      [
        "div",
        {
          "data-sicro-textbox-content": "true",
          style: buildTextBoxContentStyle(orientation),
        },
        0,
      ],
    ];
  },

  addNodeView() {
    return ({ node, editor, getPos }) => {
      let currentNode = node;
      const dom = document.createElement("div");
      const contentDOM = document.createElement("div");
      contentDOM.setAttribute("data-sicro-textbox-content", "true");
      contentDOM.style.width = "100%";
      contentDOM.style.height = "100%";
      contentDOM.style.outline = "none";
      // Pra o cursor de texto aparecer naturalmente dentro do conteúdo.
      contentDOM.style.cursor = "text";
      dom.appendChild(contentDOM);

      const applyAttrs = () => {
        const a = currentNode.attrs;
        const widthCm = Number(a.width_cm) || 6;
        const heightCm = Number(a.height_cm) || 3;
        const wrapMode = (a.wrap_mode as TextBoxWrapMode) ?? "in_front";
        const wrapXCm = Number(a.wrap_x_cm) || 0;
        const wrapYCm = Number(a.wrap_y_cm) || 0;
        const rotation = Number(a.rotation) || 0;
        const borderEnabled = a.border_enabled !== false;
        const borderColor = (a.border_color as string) ?? "#1f2937";
        const borderWidth = Number(a.border_width) || 1;
        const borderStyle =
          (a.border_style as TextBoxBorderStyle) ?? "solid";
        const fillEnabled = a.fill_enabled === true;
        const fillColor = (a.fill_color as string) ?? "#ffffff";
        const paddingCm = Number(a.padding_cm) || 0.3;
        const id = (a.id as string) ?? "anon";
        const orientation =
          (a.text_orientation as TextBoxOrientation) ?? "horizontal";

        // Data attrs (espelham os attrs do node)
        dom.setAttribute("data-sicro-textbox", "true");
        dom.setAttribute("data-width-cm", String(widthCm));
        dom.setAttribute("data-height-cm", String(heightCm));
        dom.setAttribute("data-wrap-mode", wrapMode);
        dom.setAttribute("data-wrap-x-cm", String(wrapXCm));
        dom.setAttribute("data-wrap-y-cm", String(wrapYCm));
        dom.setAttribute("data-border-enabled", String(borderEnabled));
        dom.setAttribute("data-border-color", borderColor);
        dom.setAttribute("data-border-width", String(borderWidth));
        dom.setAttribute("data-border-style", borderStyle);
        dom.setAttribute("data-fill-enabled", String(fillEnabled));
        dom.setAttribute("data-fill-color", fillColor);
        dom.setAttribute("data-padding-cm", String(paddingCm));
        if (rotation) dom.setAttribute("data-rotation", String(rotation));
        else dom.removeAttribute("data-rotation");
        if (id) dom.setAttribute("data-textbox-id", id);
        dom.setAttribute("data-text-orientation", orientation);

        // U2 — direção do texto: aplica writing-mode no wrapper de
        // conteúdo (mantém width/height/outline/cursor já setados).
        if (orientation === "vertical_up") {
          contentDOM.style.writingMode = "vertical-rl";
          contentDOM.style.transform = "rotate(180deg)";
        } else if (orientation === "vertical_down") {
          contentDOM.style.writingMode = "vertical-rl";
          contentDOM.style.transform = "";
        } else {
          contentDOM.style.writingMode = "";
          contentDOM.style.transform = "";
        }

        dom.setAttribute(
          "style",
          buildContainerStyle({
            widthCm,
            heightCm,
            wrapMode,
            wrapXCm,
            wrapYCm,
            rotation,
            borderEnabled,
            borderColor,
            borderWidth,
            borderStyle,
            fillEnabled,
            fillColor,
            paddingCm,
          }),
        );
        // Em modos floating, desabilita drag&drop nativo (a overlay
        // controla o drag via mousedown manual).
        dom.setAttribute(
          "draggable",
          wrapMode === "inline" ? "true" : "false",
        );
      };

      applyAttrs();

      // Click semântico estilo Word — hitbox = a caixa inteira:
      //   1º click numa textbox NÃO selecionada → seleciona o node
      //      (hitbox = caixa toda, independente de border/padding/texto).
      //   2º click rápido (double-click) DENTRO do contentDOM → entra
      //      em modo de edição de texto (TextSelection na posição
      //      exata clicada).
      //
      // Por que tracking manual de dblclick em vez de `dblclick` event?
      // Porque o NodeView faz `dispatch + preventDefault` no 1º
      // mousedown pra impor a NodeSelection — isso quebra o tracking
      // interno de double-click do browser/PM, e o `dblclick` event
      // não dispara nunca. Detectando dois mousedowns sucessivos
      // dentro do threshold a gente contorna isso.
      const DBLCLICK_THRESHOLD_MS = 500;
      let lastMouseDownAt = 0;

      const onMouseDown = (e: MouseEvent) => {
        if (e.button !== 0) return;
        const pos = getPos();
        if (typeof pos !== "number") return;
        const { state } = editor;
        const sel = state.selection;
        const isAlreadySelected =
          sel instanceof NodeSelection &&
          sel.from === pos &&
          sel.node.type.name === "text_box";
        // Pós-laudo U fix — Detecta se a textbox JÁ está em modo de
        // edição de texto (TextSelection com from/to dentro do range
        // do node). Conteúdo do textbox vai de `pos+1` até
        // `pos + nodeSize - 1` (boundaries do node em si).
        const nodeSize = currentNode.nodeSize;
        const isTextEditingThis =
          sel instanceof TextSelection &&
          sel.from > pos &&
          sel.to < pos + nodeSize;
        const target = e.target as globalThis.Node | null;
        const isInsideContent =
          !!target && (contentDOM.contains(target) || target === contentDOM);

        // Detecção manual de double-click via timing.
        const now = performance.now();
        const isDoubleClick =
          now - lastMouseDownAt < DBLCLICK_THRESHOLD_MS;
        lastMouseDownAt = now;

        // Dblclick SÓ pra ENTRAR em modo texto (vindo de NodeSelection ou
        // selecionado nenhum). Se já tá em text-edit, dblclick é word
        // selection nativo do browser/PM — deixa passar.
        if (isDoubleClick && isInsideContent && !isTextEditingThis) {
          const view = editor.view;
          const coords = view.posAtCoords({
            left: e.clientX,
            top: e.clientY,
          });
          if (coords) {
            try {
              const $pos = view.state.doc.resolve(coords.pos);
              let inTextBox = false;
              for (let d = $pos.depth; d >= 0; d--) {
                if ($pos.node(d).type.name === "text_box") {
                  inTextBox = true;
                  break;
                }
              }
              if (inTextBox) {
                const textSel = TextSelection.create(
                  view.state.doc,
                  coords.pos,
                );
                view.dispatch(view.state.tr.setSelection(textSel));
                view.focus();
                e.preventDefault();
                e.stopPropagation();
                return;
              }
            } catch {
              // ignora — pos inválido
            }
          }
        }

        // Pós-laudo U fix — Em modo texto, click no INTERIOR = deixa o
        // PM/browser cuidar (cursor, drag-select de texto, word select
        // por dblclick, etc.). Click na BORDA/PADDING = re-seleciona o
        // node inteiro pra recolocar handles e overlay.
        if (isTextEditingThis) {
          if (isInsideContent) {
            return; // PM/browser cuida
          }
          // Click na borda → re-seleciona como node (cai no else).
        } else if (isAlreadySelected && isInsideContent) {
          // Já selecionada como node + click no interior (1º click do
          // dblclick) → deixa o PM cuidar; o próximo click rápido vai
          // cair no branch de double-click acima.
          return;
        }

        // Resto dos casos → seleciona o node inteiro.
        try {
          const newSel = NodeSelection.create(state.doc, pos);
          editor.view.dispatch(state.tr.setSelection(newSel));
          // Bloqueia o handling padrão pra evitar que o PM coloque um
          // cursor de texto onde o user clicou.
          e.preventDefault();
        } catch {
          // ignora — ex: pos inválido por reordenação concorrente
        }
      };
      dom.addEventListener("mousedown", onMouseDown);

      // Bloqueia drag&drop nativo em modo flutuante.
      const onDragStart = (e: DragEvent) => {
        const wrapMode = currentNode.attrs.wrap_mode ?? "in_front";
        if (wrapMode !== "inline") e.preventDefault();
      };
      dom.addEventListener("dragstart", onDragStart);

      // Pós-laudo U fix — Listener contínuo de `selectionchange` que
      // limpa a DOM Selection range sempre que ela intersecta o
      // textbox E o textbox está em NodeSelection (data-textbox-
      // selected="true"). Razão: ao aplicar NodeSelection, o PM seta
      // uma DOM Selection cobrindo o node, e o browser pinta o
      // highlight nativo opaco em cima do texto. Limpar dentro do
      // `selectNode` não basta — o PM RE-APLICA a seleção em
      // transações posteriores (typing no body, save, etc.), e o
      // texto some de novo. Limpar continuamente garante que enquanto
      // a textbox está selecionada, NUNCA tem range coberto pelo
      // browser highlight.
      //
      // Quando entra em modo edição de texto (TextSelection dentro),
      // `deselectNode` é chamado, o atributo sai e esse handler vira
      // no-op — usuário consegue selecionar texto normalmente.
      const onSelectionChange = () => {
        if (!dom.isConnected) return;
        if (!dom.hasAttribute("data-textbox-selected")) return;
        try {
          const sel = window.getSelection();
          if (!sel || sel.rangeCount === 0) return;
          const range = sel.getRangeAt(0);
          // Só limpa se a seleção realmente cobre/intersecta este
          // textbox (não toca seleções em outras partes do doc).
          if (range.intersectsNode(dom)) {
            sel.removeAllRanges();
          }
        } catch {
          // ignora — ex: range inválido em ambiente de teste
        }
      };
      document.addEventListener("selectionchange", onSelectionChange);

      return {
        dom,
        contentDOM,
        update(newNode) {
          if (newNode.type !== currentNode.type) return false;
          currentNode = newNode;
          applyAttrs();
          return true;
        },
        // Pós-laudo U fix — Override do selectNode/deselectNode do PM.
        //
        // Por padrão, quando o PM aplica NodeSelection num node, ele:
        //   1. Adiciona a classe `.ProseMirror-selectednode` no dom
        //   2. Aplica estilos default (outline azul, ::after overlay)
        //   3. Coloca a DOM Selection range cobrindo o node
        // O passo (3) faz o browser pintar o highlight nativo cobrindo
        // todo o conteúdo — escondendo o texto dentro da textbox.
        //
        // Aqui sobrescrevemos com um atributo NOSSO (`data-textbox-selected`)
        // que serve só pra debug — o feedback visual real fica todo
        // por conta do `TextBoxOverlay` React (border dashed + handles
        // + toolbar) que detecta a NodeSelection via useSelectedTextBox.
        // Assim a classe do PM nunca é adicionada e os estilos default
        // (incluindo o highlight nativo do browser) não escondem o
        // texto.
        selectNode() {
          dom.setAttribute("data-textbox-selected", "true");
        },
        deselectNode() {
          dom.removeAttribute("data-textbox-selected");
        },
        destroy() {
          dom.removeEventListener("mousedown", onMouseDown);
          dom.removeEventListener("dragstart", onDragStart);
          document.removeEventListener(
            "selectionchange",
            onSelectionChange,
          );
        },
        ignoreMutation(mutation) {
          if (mutation.type === "selection") return false;
          const target = mutation.target as unknown as globalThis.Node;
          // Mutations dentro do contentDOM (edição de texto) DEVEM
          // ser processadas pelo PM.
          if (contentDOM.contains(target)) return false;
          if (target === contentDOM) return false;
          // Nossas próprias mutations no dom (data-attrs/style) podem
          // ser ignoradas.
          return true;
        },
      };
    };
  },

  addCommands() {
    return {
      insertTextBox:
        (attrs) =>
        ({ commands }) => {
          const id = generateTextBoxId();
          return commands.insertContent({
            type: this.name,
            attrs: {
              id,
              width_cm: attrs?.width_cm ?? 6,
              height_cm: attrs?.height_cm ?? 3,
              wrap_mode: attrs?.wrap_mode ?? "in_front",
              wrap_x_cm: attrs?.wrap_x_cm ?? 3,
              wrap_y_cm: attrs?.wrap_y_cm ?? 3,
              rotation: 0,
              border_enabled: attrs?.border_enabled ?? true,
              border_color: attrs?.border_color ?? "#1f2937",
              border_width: attrs?.border_width ?? 1,
              border_style: attrs?.border_style ?? "solid",
              fill_enabled: attrs?.fill_enabled ?? false,
              fill_color: attrs?.fill_color ?? "#ffffff",
              padding_cm: 0.3,
              text_orientation: attrs?.text_orientation ?? "horizontal",
            },
            content: [
              {
                type: "paragraph",
                content: [
                  { type: "text", text: "Texto da caixa" },
                ],
              },
            ],
          });
        },
    };
  },

  addProseMirrorPlugins() {
    const nodeName = this.name;
    return [
      new Plugin({
        key: new PluginKey("textBoxClickSelect"),
        props: {
          handleClickOn(view, _pos, node, nodePos, event, direct) {
            if (!direct) return false;
            if (node.type.name !== nodeName) return false;
            // Mesma lógica do NodeView mas em fallback (PM pode chamar
            // isso quando o NodeView não pegou o evento, ex: modo behind
            // coberto por texto).
            const sel = view.state.selection;
            const isAlreadySelected =
              sel instanceof NodeSelection &&
              sel.from === nodePos &&
              sel.node.type.name === nodeName;
            const target = event.target as HTMLElement | null;
            const isInsideContent =
              !!target && !!target.closest("div[data-sicro-textbox-content]");
            if (isAlreadySelected && isInsideContent) return false; // PM cuida
            // Seleciona o node inteiro.
            const nodeSel = NodeSelection.create(view.state.doc, nodePos);
            view.dispatch(view.state.tr.setSelection(nodeSel));
            return true;
          },
          // Pós-laudo U fix — Esc deseleciona a TextBox. Antes Esc
          // não fazia nada porque o PM não tem default handler pra
          // limpar NodeSelection sobre o textbox.
          //
          // ATENÇÃO: `TextSelection.create(doc, sel.from)` ingênuo
          // CAI DENTRO da textbox, porque `sel.from` é a posição
          // "between blocks" (entre o bloco anterior e o textbox), e
          // o PM faz bias automático pra achar a text-position mais
          // próxima — que é o início do conteúdo da textbox.
          // Usamos `Selection.near($pos, -1)` com bias backward pra
          // forçar que o cursor caia ANTES do textbox (no bloco
          // anterior, ou no início do doc se for o primeiro node).
          handleKeyDown(view, event) {
            if (event.key !== "Escape") return false;
            const sel = view.state.selection;
            if (
              !(sel instanceof NodeSelection) ||
              sel.node.type.name !== nodeName
            )
              return false;
            try {
              const $pos = view.state.doc.resolve(sel.from);
              const newSel = Selection.near($pos, -1);
              view.dispatch(view.state.tr.setSelection(newSel));
              // Foco no editor pra garantir que próxima tecla vá pro
              // documento, não pro browser/sistema.
              view.focus();
              event.preventDefault();
              return true;
            } catch {
              return false;
            }
          },
        },
      }),
    ];
  },
});
