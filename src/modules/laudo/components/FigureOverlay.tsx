/**
 * P4–P7 — FigureOverlay: handles + floating toolbar pra figuras selecionadas.
 *
 * Renderizado sobre o `.editorRegion`. Quando há uma figura selecionada
 * (useSelectedFigure), posiciona-se em cima do bounding rect dela e
 * oferece:
 *
 *   - **8 handles de resize** (4 cantos + 4 lados):
 *     - Cantos: preservam aspect ratio do natural da imagem
 *     - Lados: redimensionam apenas o eixo correspondente (sem aspect)
 *     - Shift inverte o comportamento (cantos free, lados aspect)
 *
 *   - **1 handle circular de rotação** acima da figura:
 *     - Drag livre; Shift trava em múltiplos de 15°
 *     - Indicador "X°" em tempo real
 *
 *   - **Floating toolbar** acima de tudo:
 *     - Botões esquerda/centro/direita (muda attr `align`)
 *     - Indicador numérico "W% · A°"
 *     - Botão Editar (abre Editor de Imagem Pericial — placeholder por agora)
 *     - Botão Excluir (deleteSelection)
 *
 * Estados de drag usam o padrão M7 sticky preview: dragPreview local
 * persiste até o prop alinhar com o store. Sem flicker.
 */

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { Editor } from "@tiptap/react";
import {
  AlignCenter,
  AlignLeft,
  AlignRight,
  BringToFront,
  ImageIcon,
  SendToBack,
  Trash2,
  WrapText,
} from "lucide-react";
import { NodeSelection } from "@tiptap/pm/state";
import type { FigureWrapMode } from "../document-engine/nodes/Figure";
import { resolveFigureEl, type SelectedFigure } from "../hooks/useSelectedFigure";
import styles from "./FigureOverlay.module.css";

export interface FigureOverlayProps {
  editor: Editor | null;
  selected: SelectedFigure | null;
  /** Container relativo onde o overlay vive (geralmente `.editorRegion`).
   *  As coords absolutas dos handles são calculadas relativas a ele. */
  containerRef: React.RefObject<HTMLElement>;
  /** Callback opcional pra abrir o Editor de Imagem Pericial. Quando
   *  ausente, o botão Editar fica oculto. */
  onEditPhoto?: (relativePath: string) => void;
}

type ResizeDir = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";

/** Limite mínimo absoluto da figura em pixels (visual). */
const MIN_WIDTH_PX = 60;
const MIN_HEIGHT_PX = 40;
/** Limites de width em porcentagem (relativa ao contêiner do laudo). */
const MIN_WIDTH_PCT = 5;
const MAX_WIDTH_PCT = 100;
/** Snap angular quando Shift está pressionado (graus). */
const ROTATION_SNAP_DEG = 15;

export function FigureOverlay({
  editor,
  selected,
  containerRef,
  onEditPhoto,
}: FigureOverlayProps) {
  /** Bounding rect do figure relativo ao container (top/left/width/height). */
  const [rect, setRect] = useState<DOMRect | null>(null);
  /** dragPreview persiste width/rotation durante o drag pra evitar flicker. */
  const [previewWidth, setPreviewWidth] = useState<string | null>(null);
  const [previewRotation, setPreviewRotation] = useState<number | null>(null);
  /** P11 — Preview da altura da imagem durante drag. `undefined` = não
   *  override (usa o attr do store); `null` = limpa (= auto/aspect
   *  natural); `string` = CSS height explícito (ex: "5.20cm"). */
  const [previewImageHeight, setPreviewImageHeight] = useState<
    string | null | undefined
  >(undefined);
  /** Tooltip flutuante perto do cursor durante o drag (resize/rotate).
   *  `null` quando ocioso. Coords são relativas à viewport. */
  const [dragTooltip, setDragTooltip] = useState<{
    x: number;
    y: number;
    label: string;
  } | null>(null);
  /** Texto da legenda no input do menu. Controlado, semeado a partir do
   *  figcaption da figura selecionada e mantido em sincronia quando o
   *  perito edita a legenda inline (ver effect abaixo). */
  const [captionText, setCaptionText] = useState("");
  /** Marca que a última escrita do figcaption partiu do PRÓPRIO input —
   *  evita que o effect de sincronização sobrescreva o que o usuário acabou
   *  de digitar (e mova o cursor) num ciclo de transação. */
  const captionSelfEditRef = useRef(false);

  // Recalcula o rect quando: a seleção muda, o container muda, ou
  // o container faz scroll/resize. Usamos getBoundingClientRect do
  // domEl + do container — coords locais ao viewport atual do
  // container. Não somamos scrollTop porque o overlay está DENTRO
  // do containerRef que NÃO rola (overflow: hidden); o scroll real
  // acontece num descendente (.scroll do EditorPage), e o efeito
  // visual desse scroll já está refletido em figRect.top.
  //
  // P14 — `selected.domEl` pode ficar STALE quando a figure muda
  // significativamente (ex: troca de wrap_mode inline ↔ in_front). PM
  // pode rebuildar o DOM substituindo o elemento, e o ref capturado
  // no `useSelectedFigure` aponta pro elemento DETACHED. O
  // getBoundingClientRect retorna (0,0,0,0) → overlay vai pra fora da
  // tela → some. Solução: re-buscar via `editor.view.nodeDOM(pos)` a
  // cada recompute pra pegar o elemento ATUAL.
  const recompute = useCallback(() => {
    if (!selected || !containerRef.current || !editor) {
      setRect(null);
      return;
    }
    let figEl: HTMLElement | null = selected.domEl;
    try {
      const dom = editor.view.nodeDOM(selected.pos);
      // P25 — `nodeDOM` devolve o wrapper-âncora; resolve pra <figure>.
      if (dom instanceof HTMLElement) figEl = resolveFigureEl(dom);
    } catch {
      // ignore — fallback abaixo
    }
    if (!figEl || !figEl.isConnected) {
      // P19 — Defensivo: NÃO seta rect=null se figEl está em estado
      // transitório. Mantém rect anterior pra o overlay não piscar
      // (handles continuam visíveis na posição antiga até o próximo
      // recompute pegar o DOM atualizado). Schedule um retry em RAF
      // pra tentar pegar o DOM novo quando PM terminar de atualizar.
      requestAnimationFrame(() => {
        const dom2 = (() => {
          try {
            const d = editor.view.nodeDOM(selected.pos);
            // P25 — resolve wrapper-âncora → <figure>.
            return d instanceof HTMLElement ? resolveFigureEl(d) : null;
          } catch {
            return null;
          }
        })();
        if (dom2 && dom2.isConnected && containerRef.current) {
          const figRect2 = dom2.getBoundingClientRect();
          const conRect2 = containerRef.current.getBoundingClientRect();
          setRect(
            new DOMRect(
              figRect2.left - conRect2.left,
              figRect2.top - conRect2.top,
              figRect2.width,
              figRect2.height,
            ),
          );
        }
      });
      return;
    }
    const figRect = figEl.getBoundingClientRect();
    const conRect = containerRef.current.getBoundingClientRect();
    const local = new DOMRect(
      figRect.left - conRect.left,
      figRect.top - conRect.top,
      figRect.width,
      figRect.height,
    );
    setRect(local);
  }, [selected, containerRef, editor]);

  useLayoutEffect(() => {
    recompute();
  }, [recompute]);

  // Observa scroll/resize. Como o scroll real pode estar em qualquer
  // ancestral da figure (ex: .scroll do EditorPage), percorremos a
  // árvore DOM a partir da figure pra cima coletando TODOS os elementos
  // com overflow-y/x auto/scroll, e anexamos listener em cada um. Sem
  // isso, scrollar o conteúdo do laudo movia a figure mas o overlay
  // ficava parado.
  useEffect(() => {
    if (!selected || !containerRef.current) return undefined;
    const el = containerRef.current;
    const onScroll = () => recompute();

    // Coleta scrollers desde a figure até o body.
    const scrollers: HTMLElement[] = [];
    let cur: HTMLElement | null = selected.domEl.parentElement;
    while (cur && cur !== document.body) {
      const cs = window.getComputedStyle(cur);
      if (
        cs.overflowY === "auto" ||
        cs.overflowY === "scroll" ||
        cs.overflowX === "auto" ||
        cs.overflowX === "scroll"
      ) {
        scrollers.push(cur);
      }
      cur = cur.parentElement;
    }
    for (const sc of scrollers) {
      sc.addEventListener("scroll", onScroll, { passive: true });
    }
    // editorRegion em si não rola, mas mantemos o listener por garantia
    // (em caso de mudança futura no CSS).
    el.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", onScroll);
    window.addEventListener("scroll", onScroll, { passive: true });
    const ro = new ResizeObserver(onScroll);
    ro.observe(selected.domEl);
    ro.observe(el);
    return () => {
      for (const sc of scrollers) {
        sc.removeEventListener("scroll", onScroll);
      }
      el.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", onScroll);
      window.removeEventListener("scroll", onScroll);
      ro.disconnect();
    };
  }, [selected, containerRef, recompute]);

  // Limpa preview quando a prop alinha (igual M7).
  useEffect(() => {
    if (!selected) return;
    if (previewWidth !== null && previewWidth === selected.attrs.width) {
      setPreviewWidth(null);
    }
    if (
      previewRotation !== null &&
      Math.abs(previewRotation - Number(selected.attrs.rotation ?? 0)) < 0.5
    ) {
      setPreviewRotation(null);
    }
    if (previewImageHeight !== undefined) {
      const storeVal = (selected.attrs.image_height as string | null) ?? null;
      if (previewImageHeight === storeVal) {
        setPreviewImageHeight(undefined);
      }
    }
  }, [selected, previewWidth, previewRotation, previewImageHeight]);

  // Helpers pra updates de attrs.
  /** P20 — Atualiza attrs do figure E re-afirma NodeSelection na MESMA
   *  transação. Evita usar `editor.chain().focus().updateAttributes()`
   *  porque o `.focus()` async do TipTap (setTimeout interno) pode
   *  causar selection state issues em mudanças significativas (ex:
   *  wrap_mode inline → in_front). Atomic dispatch garante que:
   *  1) attrs mudam (setNodeMarkup);
   *  2) selection permanece NodeSelection na figure (mesmo pos);
   *  3) tudo num único transaction event → useSelectedFigure só
   *     vê o resultado final, nunca um estado intermediário com
   *     selection "perdida". */
  const updateFigureAttrs = useCallback(
    (patch: Record<string, unknown>) => {
      if (!editor || !selected) return;
      const { state } = editor;
      const sel = state.selection;
      if (!(sel instanceof NodeSelection)) return;
      const pos = sel.from;
      const node = sel.node;
      if (node.type.name !== "figure") return;
      const tr = state.tr.setNodeMarkup(pos, undefined, {
        ...node.attrs,
        ...patch,
      });
      // Re-afirma NodeSelection no MESMO transaction (sem .focus() async).
      tr.setSelection(NodeSelection.create(tr.doc, pos));
      editor.view.dispatch(tr);
    },
    [editor, selected],
  );

  const deleteFigure = useCallback(() => {
    if (!editor || !selected) return;
    editor.chain().focus().deleteSelection().run();
  }, [editor, selected]);

  // ---- Legenda manual da foto (campo no menu flutuante) ----
  /**
   * Localiza o nó `figure` selecionado e seu filho `figcaption`, devolvendo
   * as posições úteis pra ler/escrever a legenda. Robusto a deslocamentos:
   * relê o nó ao vivo do `editor.state` em vez de confiar no snapshot de
   * `selected.attrs`. Retorna `null` se a seleção não for mais uma figura.
   */
  const resolveFigcaption = useCallback(() => {
    if (!editor || !selected) return null;
    const { state } = editor;
    const sel = state.selection;
    if (!(sel instanceof NodeSelection)) return null;
    const figurePos = sel.from;
    const figureNode = sel.node;
    if (figureNode.type.name !== "figure") return null;
    const cap = figureNode.firstChild;
    if (!cap || cap.type.name !== "figcaption") return null;
    // figcaption começa logo após a abertura do figure (figurePos + 1).
    const capPos = figurePos + 1;
    return {
      figurePos,
      cap,
      // Range do CONTEÚDO interno do figcaption (entre as tags de abertura
      // e fechamento): [capStart, capEnd).
      contentStart: capPos + 1,
      contentEnd: capPos + cap.nodeSize - 1,
    };
  }, [editor, selected]);

  /** Texto puro do figcaption da figura selecionada (vazio se não houver). */
  const readCaptionText = useCallback((): string => {
    const ctx = resolveFigcaption();
    if (!ctx) return "";
    return ctx.cap.textContent ?? "";
  }, [resolveFigcaption]);

  /**
   * Escreve `text` no figcaption da figura selecionada. Substitui apenas o
   * CONTEÚDO INTERNO do figcaption (não o nó inteiro) por um único text node
   * — ou esvazia, se `text` for vazio. Operar no nível de texto deixa o
   * history plugin do ProseMirror AGRUPAR digitação adjacente num único passo
   * de undo (em vez de um passo por tecla). A NodeSelection na figura é
   * re-afirmada na MESMA transação pra o overlay não perder a seleção.
   */
  const writeCaptionText = useCallback(
    (text: string) => {
      if (!editor) return;
      const ctx = resolveFigcaption();
      if (!ctx) return;
      const { state } = editor;
      const { schema } = state;
      const tr = state.tr;
      if (text.length === 0) {
        // Legenda vazia → esvazia o figcaption (inline* aceita vazio).
        tr.delete(ctx.contentStart, ctx.contentEnd);
      } else {
        tr.replaceWith(ctx.contentStart, ctx.contentEnd, schema.text(text));
      }
      if (!tr.docChanged) return;
      // Mantém a figura selecionada (não move o foco pro texto da legenda).
      tr.setSelection(NodeSelection.create(tr.doc, ctx.figurePos));
      // Marca que a próxima sincronização vem de uma auto-edição.
      captionSelfEditRef.current = true;
      editor.view.dispatch(tr);
    },
    [editor, resolveFigcaption],
  );

  // Semeia o input ao TROCAR de foto (seleção). Aqui sempre relemos do doc
  // ao vivo — é uma figura diferente, então o estado do input deve refletir
  // a legenda dela. (A flag de auto-edição NÃO é consumida aqui; ela só vale
  // pro listener de transação abaixo.)
  useEffect(() => {
    if (!selected) {
      setCaptionText("");
      return;
    }
    const live = readCaptionText();
    setCaptionText((prev) => (prev === live ? prev : live));
  }, [selected, readCaptionText]);

  // Mantém o input em sincronia quando o perito edita a legenda INLINE (ou
  // desfaz/refaz): mudanças no figcaption não alteram `selected.attrs`, então
  // o effect acima não dispara. Escutamos as transações do editor e relemos o
  // figcaption ao vivo. A transação cuja escrita partiu do PRÓPRIO input é
  // pulada UMA vez (e a flag é resetada aqui) pra não reescrever o que o
  // usuário acabou de digitar nem mover o cursor do input.
  useEffect(() => {
    if (!editor || !selected) return undefined;
    const onTx = () => {
      if (captionSelfEditRef.current) {
        captionSelfEditRef.current = false;
        return;
      }
      const live = readCaptionText();
      setCaptionText((prev) => (prev === live ? prev : live));
    };
    editor.on("transaction", onTx);
    return () => {
      editor.off("transaction", onTx);
    };
  }, [editor, selected, readCaptionText]);

  const onCaptionInputChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const next = e.target.value;
      setCaptionText(next);
      writeCaptionText(next);
    },
    [writeCaptionText],
  );

  // ---- P13 — Troca de modo de wrap (Alinhado / Em frente / Atrás) ----
  /**
   * P25 — Ao trocar pra in_front / behind: snapshot da posição visual
   * atual da figure RELATIVA AO WRAPPER-ÂNCORA (o <div data-figure-anchor>
   * que é o pai direto da <figure>) → vira wrap_x_cm / wrap_y_cm. Como o
   * wrapper já está em fluxo na âncora da figura (mesmo quando inline), o
   * offset resultante é pequeno e a foto NÃO PULA — fica visualmente onde
   * estava. Após o toggle o wrapper vira position:relative no MESMO lugar,
   * então o `top: wrap_y_cm` mantém a foto no lugar (e bate com o PDF).
   * Ao voltar pra inline: zera as coords e o nó volta pro fluxo.
   *
   * Fallback: se o wrapper-âncora não for encontrado (ex.: DOM legado),
   * sobe até o ancestral posicionado mais próximo.
   */
  const findAnchorContext = useCallback(
    (el: HTMLElement | null): HTMLElement | null => {
      // Pai direto = wrapper-âncora (P25).
      const parent = el?.parentElement ?? null;
      if (parent && parent.matches("[data-figure-anchor]")) return parent;
      // Fallback legado: ancestral posicionado mais próximo.
      let cur: HTMLElement | null = parent;
      while (cur && cur !== document.body) {
        const cs = window.getComputedStyle(cur);
        if (
          cs.position === "relative" ||
          cs.position === "absolute" ||
          cs.position === "fixed" ||
          cs.position === "sticky"
        ) {
          return cur;
        }
        cur = cur.parentElement;
      }
      return cur;
    },
    [],
  );

  const setWrapMode = useCallback(
    (nextMode: FigureWrapMode) => {
      if (!editor || !selected) return;
      const currentMode =
        ((selected.attrs.wrap_mode as FigureWrapMode | null) ?? "inline");
      if (nextMode === currentMode) return;

      if (nextMode === "inline") {
        updateFigureAttrs({
          wrap_mode: "inline",
          wrap_x_cm: 0,
          wrap_y_cm: 0,
        });
        return;
      }

      // Snapshot visual position pra evitar pulo.
      const figEl = selected.domEl;
      const ctxEl = findAnchorContext(figEl);
      if (!ctxEl) {
        updateFigureAttrs({ wrap_mode: nextMode });
        return;
      }
      const figRect = figEl.getBoundingClientRect();
      const ctxRect = ctxEl.getBoundingClientRect();
      const layoutH = figEl.offsetHeight || 1;
      const zoom = figRect.height / Math.max(1, layoutH);
      const PX_PER_CM = 37.7952755906;
      const offsetLeftLayoutPx =
        (figRect.left - ctxRect.left) / Math.max(0.0001, zoom);
      const offsetTopLayoutPx =
        (figRect.top - ctxRect.top) / Math.max(0.0001, zoom);
      const xCm = offsetLeftLayoutPx / PX_PER_CM;
      const yCm = offsetTopLayoutPx / PX_PER_CM;
      updateFigureAttrs({
        wrap_mode: nextMode,
        wrap_x_cm: Number(xCm.toFixed(2)),
        wrap_y_cm: Number(yCm.toFixed(2)),
      });
    },
    [editor, selected, updateFigureAttrs, findAnchorContext],
  );

  // ---- Handles de resize ----
  /**
   * P11 — Cada handle controla DIMENSÕES INDEPENDENTES:
   *
   *   - Cantos (sem Shift): preserva aspect ratio natural → muda só
   *     `width`, e limpa `image_height` (img volta a `height: auto`).
   *   - Cantos (com Shift): livre → muda `width` e `image_height` indep.
   *   - E/W (sem Shift): muda só `width`. Trava `image_height` no valor
   *     visual atual pra altura não mudar (imagem fica esticada).
   *   - E/W (com Shift): preserva aspect natural → muda só `width` e
   *     limpa `image_height`.
   *   - N/S (sem Shift): muda só `image_height`. `width` permanece.
   *   - N/S (com Shift): preserva aspect natural → muda `image_height`
   *     E também atualiza `width` proporcionalmente.
   *
   * O elemento de referência é a `<img>` dentro do figure, NÃO o figure
   * inteiro (que inclui caption). Assim os deltas correspondem ao
   * tamanho real da foto, não da foto+legenda.
   */
  const handleResizeMouseDown = useCallback(
    (dir: ResizeDir) => (e: React.MouseEvent) => {
      if (!editor || !selected || !containerRef.current) return;
      e.preventDefault();
      e.stopPropagation();

      const figEl = selected.domEl;
      const imgEl = figEl.querySelector("img") as HTMLImageElement | null;
      const parentEl = figEl.parentElement;
      if (!parentEl || !imgEl) return;

      // Snapshot inicial. Tudo em VISUAL px (getBoundingClientRect inclui
      // o transform do zoom). offsetWidth/Height são LAYOUT px (zoom-
      // independentes), usados pra converter pra cm.
      const startImgRect = imgEl.getBoundingClientRect();
      const startParentRect = parentEl.getBoundingClientRect();
      const parentVisualWidth = startParentRect.width || 1;
      const startVisualW = startImgRect.width;
      const startVisualH = startImgRect.height;
      const startLayoutH = imgEl.offsetHeight || 1;
      // Zoom factor: visual / layout. Aplicado ao delta vertical pra
      // converter movimento de mouse (visual px) em layout px.
      const zoom = startVisualH / Math.max(1, startLayoutH);
      const PX_PER_CM = 37.7952755906;
      const startHeightCm = startLayoutH / PX_PER_CM;

      const naturalRatio =
        imgEl.naturalWidth && imgEl.naturalHeight
          ? imgEl.naturalWidth / imgEl.naturalHeight
          : startVisualW / Math.max(1, startVisualH);

      const startX = e.clientX;
      const startY = e.clientY;
      const isCornerDir = ["nw", "ne", "se", "sw"].includes(dir);

      document.body.classList.add("sicro-figure-resizing");

      // Latest values pra commit no mouseup (refs locais, sem React).
      let latestWidth: string | null = null;
      let latestImageHeight: string | null | undefined = undefined;
      // Click puro (sem arrastar) não deve commitar nada. Sem isso, o
      // path "default E/W" travaria image_height na altura atual em cm
      // mesmo num click sem drag, criando attr explícito sem motivo.
      let hasMoved = false;

      const onMove = (ev: MouseEvent) => {
        const dxVisual = ev.clientX - startX;
        const dyVisual = ev.clientY - startY;
        if (dxVisual === 0 && dyVisual === 0) return;
        hasMoved = true;
        const xSign =
          dir === "e" || dir === "ne" || dir === "se"
            ? 1
            : dir === "w" || dir === "nw" || dir === "sw"
              ? -1
              : 0;
        const ySign =
          dir === "s" || dir === "sw" || dir === "se"
            ? 1
            : dir === "n" || dir === "nw" || dir === "ne"
              ? -1
              : 0;

        // Novas dimensões propostas (independentes).
        const newVisualW = Math.max(
          MIN_WIDTH_PX,
          startVisualW + xSign * dxVisual,
        );
        const newLayoutH = Math.max(
          MIN_HEIGHT_PX,
          startLayoutH + (ySign * dyVisual) / Math.max(0.0001, zoom),
        );
        const newPct = Math.max(
          MIN_WIDTH_PCT,
          Math.min(MAX_WIDTH_PCT, (newVisualW / parentVisualWidth) * 100),
        );
        const newHeightCm = newLayoutH / PX_PER_CM;

        // Decisão por direção:
        // - nextWidth (null = não mudar)
        // - nextImageHeight (undefined = não mudar; null = limpar)
        const shiftPressed = ev.shiftKey;
        let nextWidth: string | null = null;
        let nextImageHeight: string | null | undefined = undefined;
        let tooltipLabel = "";

        if (isCornerDir) {
          if (shiftPressed) {
            // Free corner: ambos independentes
            nextWidth = `${newPct.toFixed(1)}%`;
            nextImageHeight = `${newHeightCm.toFixed(2)}cm`;
            tooltipLabel = `${nextWidth} × ${nextImageHeight}`;
          } else {
            // Aspect preservado: width muda, height volta pra auto
            nextWidth = `${newPct.toFixed(1)}%`;
            nextImageHeight = null;
            tooltipLabel = nextWidth;
          }
        } else if (dir === "e" || dir === "w") {
          if (shiftPressed) {
            // Shift no E/W = libera aspect natural
            nextWidth = `${newPct.toFixed(1)}%`;
            nextImageHeight = null;
            tooltipLabel = nextWidth;
          } else {
            // Default: muda só width, trava height no valor inicial
            nextWidth = `${newPct.toFixed(1)}%`;
            nextImageHeight = `${startHeightCm.toFixed(2)}cm`;
            tooltipLabel = nextWidth;
          }
        } else {
          // n ou s
          if (shiftPressed) {
            // Shift no N/S = preserva aspect (mexe width junto)
            const newWidthVisualFromH = newLayoutH * naturalRatio * zoom;
            const newPctFromH = Math.max(
              MIN_WIDTH_PCT,
              Math.min(
                MAX_WIDTH_PCT,
                (newWidthVisualFromH / parentVisualWidth) * 100,
              ),
            );
            nextWidth = `${newPctFromH.toFixed(1)}%`;
            nextImageHeight = null;
            tooltipLabel = nextWidth;
          } else {
            // Default: muda só height
            nextImageHeight = `${newHeightCm.toFixed(2)}cm`;
            tooltipLabel = nextImageHeight;
          }
        }

        // Aplica no DOM síncrono.
        if (nextWidth !== null) {
          figEl.style.width = nextWidth;
          latestWidth = nextWidth;
          setPreviewWidth(nextWidth);
        }
        if (nextImageHeight === null) {
          imgEl.style.height = "auto";
          imgEl.style.objectFit = "";
          latestImageHeight = null;
          setPreviewImageHeight(null);
        } else if (nextImageHeight !== undefined) {
          imgEl.style.height = nextImageHeight;
          imgEl.style.objectFit = "fill";
          latestImageHeight = nextImageHeight;
          setPreviewImageHeight(nextImageHeight);
        }

        setDragTooltip({
          x: ev.clientX,
          y: ev.clientY,
          label: tooltipLabel,
        });

        // Recomputa rect do overlay pra handles seguirem a borda.
        recompute();
      };

      const onUp = (ev: MouseEvent) => {
        onMove(ev);
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
        document.body.classList.remove("sicro-figure-resizing");
        setDragTooltip(null);
        if (!hasMoved) return; // click sem drag — não commita nada
        // Commit num único updateAttributes (atomic, gera 1 entry no undo).
        const patch: Record<string, unknown> = {};
        if (latestWidth !== null) patch["width"] = latestWidth;
        if (latestImageHeight !== undefined)
          patch["image_height"] = latestImageHeight;
        if (Object.keys(patch).length > 0) updateFigureAttrs(patch);
      };

      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [editor, selected, containerRef, updateFigureAttrs, recompute],
  );

  // ---- Handle de rotação ----
  const handleRotateMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (!editor || !selected || !rect || !containerRef.current) return;
      e.preventDefault();
      e.stopPropagation();

      // Centro do figure em coords da viewport.
      const figRect = selected.domEl.getBoundingClientRect();
      const cx = figRect.left + figRect.width / 2;
      const cy = figRect.top + figRect.height / 2;
      const startRotation = Number(selected.attrs.rotation ?? 0);
      // Ângulo inicial do cursor relativo ao centro (deg, 0 = "12h").
      const angleFromCursor = (clientX: number, clientY: number) => {
        const dx = clientX - cx;
        const dy = clientY - cy;
        return (Math.atan2(dy, dx) * 180) / Math.PI + 90;
      };
      const startAngle = angleFromCursor(e.clientX, e.clientY);

      // Body class: remove transition CSS pra rotação responder em tempo
      // real + impede seleção de texto enquanto arrasta.
      document.body.classList.add("sicro-figure-rotating");

      const onMove = (ev: MouseEvent) => {
        const currentAngle = angleFromCursor(ev.clientX, ev.clientY);
        let delta = currentAngle - startAngle;
        let next = startRotation + delta;
        // Normaliza pra (-180..180].
        while (next > 180) next -= 360;
        while (next <= -180) next += 360;
        if (ev.shiftKey) {
          next = Math.round(next / ROTATION_SNAP_DEG) * ROTATION_SNAP_DEG;
        } else {
          next = Math.round(next);
        }
        setPreviewRotation(next);
        // Tooltip flutuante mostrando o ângulo perto do cursor.
        setDragTooltip({
          x: ev.clientX,
          y: ev.clientY,
          label: `${next}°`,
        });
      };
      const onUp = (ev: MouseEvent) => {
        onMove(ev);
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
        document.body.classList.remove("sicro-figure-rotating");
        setDragTooltip(null);
        setPreviewRotation((curr) => {
          if (curr !== null) updateFigureAttrs({ rotation: curr });
          return curr;
        });
      };
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [editor, selected, rect, containerRef, updateFigureAttrs],
  );

  // ---- P24 — Drag-to-reposition handler ----
  /**
   * Handler de mousedown pra iniciar drag-to-reposition de figures
   * flutuantes. Aplicado ao `<div className={styles.dragZone}>` que
   * fica dentro do overlay, ACIMA do texto (z-index 10 do overlay no
   * editorRegion stacking context). É a única forma confiável de
   * capturar drag em modo "Atrás do texto" onde a figure está
   * em z-index: -1 (atrás do parágrafo vazio que cobre a área).
   */
  const handleDragMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (!editor || !selected) return;
      if (e.button !== 0) return;
      const wrapMode =
        (selected.attrs.wrap_mode as FigureWrapMode | null) ?? "inline";
      if (wrapMode === "inline") return;
      e.preventDefault();
      e.stopPropagation();

      const figEl = selected.domEl;
      const PX_PER_CM = 37.7952755906;
      const figRect = figEl.getBoundingClientRect();
      const layoutH = figEl.offsetHeight || 1;
      const zoom = figRect.height / Math.max(1, layoutH);
      const startX = e.clientX;
      const startY = e.clientY;
      const startWrapX = Number(selected.attrs.wrap_x_cm) || 0;
      const startWrapY = Number(selected.attrs.wrap_y_cm) || 0;
      let hasMoved = false;
      let latestX = startWrapX;
      let latestY = startWrapY;

      const onMove = (ev: MouseEvent) => {
        const dx = ev.clientX - startX;
        const dy = ev.clientY - startY;
        if (!hasMoved && Math.abs(dx) + Math.abs(dy) < 5) return;
        if (!hasMoved) {
          document.body.classList.add("sicro-figure-resizing");
        }
        hasMoved = true;
        ev.preventDefault();
        const dxLayout = dx / Math.max(0.0001, zoom);
        const dyLayout = dy / Math.max(0.0001, zoom);
        const newX = startWrapX + dxLayout / PX_PER_CM;
        const newY = startWrapY + dyLayout / PX_PER_CM;
        figEl.style.left = `${newX.toFixed(2)}cm`;
        figEl.style.top = `${newY.toFixed(2)}cm`;
        latestX = newX;
        latestY = newY;
        setDragTooltip({
          x: ev.clientX,
          y: ev.clientY,
          label: `${newX.toFixed(2)}cm, ${newY.toFixed(2)}cm`,
        });
        recompute();
      };

      const onUp = () => {
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
        if (hasMoved) {
          document.body.classList.remove("sicro-figure-resizing");
          setDragTooltip(null);
          updateFigureAttrs({
            wrap_x_cm: Number(latestX.toFixed(2)),
            wrap_y_cm: Number(latestY.toFixed(2)),
          });
        }
      };

      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [editor, selected, updateFigureAttrs, recompute],
  );

  // Durante drag, atualiza o estilo DOM diretamente (em vez de esperar
  // commit + re-render via TipTap), pra resposta imediata.
  useEffect(() => {
    if (!selected) return;
    if (previewWidth !== null) {
      selected.domEl.style.width = previewWidth;
    }
    if (previewRotation !== null) {
      selected.domEl.style.transform = `rotate(${previewRotation}deg)`;
    }
    if (previewImageHeight !== undefined) {
      const imgEl = selected.domEl.querySelector(
        "img",
      ) as HTMLImageElement | null;
      if (imgEl) {
        if (previewImageHeight === null) {
          imgEl.style.height = "auto";
          imgEl.style.objectFit = "";
        } else {
          imgEl.style.height = previewImageHeight;
          imgEl.style.objectFit = "fill";
        }
      }
    }
  }, [selected, previewWidth, previewRotation, previewImageHeight]);

  // ---- Atalhos (Delete / Esc / setas) ----
  useEffect(() => {
    if (!editor || !selected) return undefined;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Delete" || e.key === "Backspace") {
        // Só deleta quando o foco NÃO está em um input/contenteditable
        // (pra não engolir delete normal de texto).
        const target = e.target as HTMLElement | null;
        if (
          target &&
          (target.tagName === "INPUT" ||
            target.tagName === "TEXTAREA" ||
            target.isContentEditable)
        ) {
          // Quando o editor tá com NodeSelection, contenteditable é true
          // mas a tecla deve deletar o nó. Deixa o TipTap tratar.
        }
        // O TipTap já trata Delete em NodeSelection deletando o nó.
        // Não interceptamos.
      } else if (e.key === "Escape") {
        editor.chain().focus().setTextSelection(selected.pos).run();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [editor, selected]);

  // ---- Cálculos derivados pra UI ----
  const effectiveWidth = previewWidth ?? (selected?.attrs.width as string) ?? "70%";
  const effectiveRotation =
    previewRotation ?? Number(selected?.attrs.rotation ?? 0);
  const effectiveAlign =
    (selected?.attrs.align as string | undefined) ?? "center";
  const effectiveImageHeight =
    previewImageHeight !== undefined
      ? previewImageHeight
      : ((selected?.attrs.image_height as string | null) ?? null);
  const effectiveWrapMode =
    ((selected?.attrs.wrap_mode as FigureWrapMode | null) ?? "inline");
  const relativePath = (selected?.attrs.relative_path as string) ?? "";

  const widthLabel = useMemo(() => {
    if (typeof effectiveWidth === "string" && effectiveWidth.endsWith("%")) {
      return effectiveWidth;
    }
    return effectiveWidth;
  }, [effectiveWidth]);

  if (!selected || !rect) return null;

  // Coords de cada handle. Os de canto/lado ficam ALÉM da borda
  // (-5px) pra ficarem visualmente sobre a borda do rect.
  const HSIZE = 10;
  const half = HSIZE / 2;
  const handlePos: Record<ResizeDir, { left: number; top: number }> = {
    nw: { left: -half, top: -half },
    n: { left: rect.width / 2 - half, top: -half },
    ne: { left: rect.width - half, top: -half },
    e: { left: rect.width - half, top: rect.height / 2 - half },
    se: { left: rect.width - half, top: rect.height - half },
    s: { left: rect.width / 2 - half, top: rect.height - half },
    sw: { left: -half, top: rect.height - half },
    w: { left: -half, top: rect.height / 2 - half },
  };
  const cursorOf: Record<ResizeDir, string> = {
    nw: "nwse-resize",
    n: "ns-resize",
    ne: "nesw-resize",
    e: "ew-resize",
    se: "nwse-resize",
    s: "ns-resize",
    sw: "nesw-resize",
    w: "ew-resize",
  };

  return (
    <>
    <div
      className={styles.overlay}
      style={{
        left: `${rect.left}px`,
        top: `${rect.top}px`,
        width: `${rect.width}px`,
        height: `${rect.height}px`,
      }}
      // Permite eventos NOS handles/toolbar mas deixa cliques no centro
      // (na própria imagem) passarem para o TipTap.
      onMouseDown={(e) => e.stopPropagation()}
    >
      {/* Floating toolbar acima da figura */}
      <div
        className={styles.toolbar}
        style={{
          top: -52,
          left: rect.width / 2,
          transform: "translateX(-50%)",
        }}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <button
          type="button"
          className={`${styles.toolBtn} ${effectiveAlign === "left" ? styles.toolBtnActive : ""}`}
          onClick={() => updateFigureAttrs({ align: "left" })}
          title="Alinhar à esquerda"
        >
          <AlignLeft size={14} />
        </button>
        <button
          type="button"
          className={`${styles.toolBtn} ${effectiveAlign === "center" ? styles.toolBtnActive : ""}`}
          onClick={() => updateFigureAttrs({ align: "center" })}
          title="Centralizar"
        >
          <AlignCenter size={14} />
        </button>
        <button
          type="button"
          className={`${styles.toolBtn} ${effectiveAlign === "right" ? styles.toolBtnActive : ""}`}
          onClick={() => updateFigureAttrs({ align: "right" })}
          title="Alinhar à direita"
        >
          <AlignRight size={14} />
        </button>
        <div className={styles.toolDivider} />
        {/* P13 — Wrap mode: Alinhado / Em frente / Atrás do texto */}
        <button
          type="button"
          className={`${styles.toolBtn} ${effectiveWrapMode === "inline" ? styles.toolBtnActive : ""}`}
          onClick={() => setWrapMode("inline")}
          title="Alinhado ao texto"
        >
          <WrapText size={14} />
        </button>
        <button
          type="button"
          className={`${styles.toolBtn} ${effectiveWrapMode === "in_front" ? styles.toolBtnActive : ""}`}
          onClick={() => setWrapMode("in_front")}
          title="Em frente ao texto"
        >
          <BringToFront size={14} />
        </button>
        <button
          type="button"
          className={`${styles.toolBtn} ${effectiveWrapMode === "behind" ? styles.toolBtnActive : ""}`}
          onClick={() => setWrapMode("behind")}
          title="Atrás do texto"
        >
          <SendToBack size={14} />
        </button>
        <div className={styles.toolDivider} />
        {/* Campo de legenda manual da foto. O perito digita aqui; o texto é
         *  escrito no figcaption (sem prefixo automático). Controlado e em
         *  sincronia com a edição inline da legenda. */}
        <input
          type="text"
          className={styles.captionInput}
          value={captionText}
          onChange={onCaptionInputChange}
          placeholder="Legenda (opcional)"
          title="Legenda da foto"
          aria-label="Legenda da foto"
          // Não deixa o mousedown borbulhar pro overlay/editor (evita perder
          // a seleção da figura ao clicar no campo).
          onMouseDown={(e) => e.stopPropagation()}
          // Setas/Backspace etc. ficam no input — não dispara os atalhos do
          // overlay (Delete apaga a foto; setas movem etc.).
          onKeyDown={(e) => e.stopPropagation()}
        />
        <div className={styles.toolDivider} />
        <span className={styles.toolReadout}>
          {widthLabel}
          {effectiveImageHeight ? ` × ${effectiveImageHeight}` : ""} ·{" "}
          {effectiveRotation}°
        </span>
        <div className={styles.toolDivider} />
        {onEditPhoto && relativePath && (
          <button
            type="button"
            className={styles.toolBtn}
            onClick={() => onEditPhoto(relativePath)}
            title="Abrir no Editor de Imagem Pericial"
          >
            <ImageIcon size={14} />
          </button>
        )}
        <button
          type="button"
          className={`${styles.toolBtn} ${styles.toolBtnDanger}`}
          onClick={deleteFigure}
          title="Excluir foto (Delete)"
        >
          <Trash2 size={14} />
        </button>
      </div>

      {/* Handle de rotação acima da figura */}
      <div
        className={styles.rotateHandle}
        style={{ left: rect.width / 2, top: -28 }}
        onMouseDown={handleRotateMouseDown}
        title="Arraste para rotacionar (Shift = snap em 15°)"
      />
      {/* Linha conectando rotation handle à borda superior */}
      <div
        className={styles.rotateStem}
        style={{ left: rect.width / 2, top: -18 }}
      />

      {/* Borda da seleção */}
      <div className={styles.selectionFrame} aria-hidden />

      {/* P24 — DragZone: superfície clicável pra iniciar drag-to-reposition
       *  quando a foto é flutuante. Crítica pra modo Atrás onde a figure
       *  está em z-index: -1 — o overlay (z-index 10 no editorRegion)
       *  fica acima do texto, então a dragZone aqui também. */}
      {effectiveWrapMode !== "inline" && (
        <div
          className={styles.dragZone}
          onMouseDown={handleDragMouseDown}
          aria-hidden
        />
      )}

      {/* 8 handles de resize */}
      {(Object.keys(handlePos) as ResizeDir[]).map((dir) => (
        <div
          key={dir}
          className={`${styles.handle} ${styles[`handle_${dir}`]}`}
          style={{
            left: `${handlePos[dir].left}px`,
            top: `${handlePos[dir].top}px`,
            width: `${HSIZE}px`,
            height: `${HSIZE}px`,
            cursor: cursorOf[dir],
          }}
          onMouseDown={handleResizeMouseDown(dir)}
          title={
            ["nw", "ne", "se", "sw"].includes(dir)
              ? "Arraste (Shift = livre, sem proporção)"
              : "Arraste (Shift = preservar proporção)"
          }
        />
      ))}
    </div>
    {/* Tooltip flutuante perto do cursor durante drag (resize/rotate).
     *  Renderizado FORA do overlay (position: fixed) pra usar as coords
     *  de viewport diretamente, sem se preocupar com o contexto de
     *  posicionamento do `.editorRegion`. */}
    {dragTooltip && (
      <div
        className={styles.dragTooltip}
        style={{
          left: `${dragTooltip.x + 14}px`,
          top: `${dragTooltip.y + 14}px`,
        }}
      >
        {dragTooltip.label}
      </div>
    )}
    </>
  );
}
