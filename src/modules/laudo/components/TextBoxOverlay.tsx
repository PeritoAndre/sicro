/**
 * U — TextBoxOverlay: handles + toolbar pra TextBox selecionada.
 *
 * Híbrido entre ShapeOverlay (drag/resize 4-corner) e FigureOverlay
 * (8 handles + rotação). Pra TextBox vamos full Figure-like:
 *   - 8 handles (4 cantos + 4 lados)
 *   - 1 handle de rotação acima do topo
 *   - DragZone pra mover livremente em modo flutuante
 *   - Toolbar com:
 *       · 3 modos de wrap (Alinhado / Em frente / Atrás)
 *       · Border: on/off + cor + largura + estilo
 *       · Fill: on/off + cor
 *       · Indicador de rotação (graus)
 *       · Botão excluir
 *
 * Diferença chave vs ShapeOverlay: border e fill TÊM TOGGLE INDEPENDENTE
 * (border_enabled / fill_enabled) — o user pode ter border só, fill só,
 * ambos ou nenhum.
 */

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { Editor } from "@tiptap/react";
import { NodeSelection, TextSelection } from "@tiptap/pm/state";
import {
  BringToFront,
  RotateCw,
  SendToBack,
  Square,
  SquareDashed,
  Trash2,
  WrapText,
} from "lucide-react";
import type {
  TextBoxBorderStyle,
  TextBoxOrientation,
  TextBoxWrapMode,
} from "../document-engine/nodes/TextBox";
import type { SelectedTextBox } from "../hooks/useSelectedTextBox";
import styles from "./TextBoxOverlay.module.css";

export interface TextBoxOverlayProps {
  editor: Editor | null;
  selected: SelectedTextBox | null;
  containerRef: React.RefObject<HTMLElement>;
  /** Pós-laudo U fix — Callback chamado ANTES do dispatch de
   *  TextSelection (quando o usuário entra em modo edição de texto via
   *  dblclick). Usado pelo overlay do HEADER pra ativar o
   *  `editingRegion = "header"` no store, deixando o headerEditor
   *  editável antes do cursor cair lá. Sem isso, dblclick no header
   *  textbox em body mode despachava TextSelection num editor
   *  não-editável e nada acontecia. Body overlay não precisa, omite. */
  onBeforeEnterTextEdit?: () => void;
}

type ResizeDir = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";

// NOTA: o antigo `aabbDirToLocalDir`/`DIR_ORDER_CW` (mapeamento AABB→local)
// foi removido. Agora o overlay de seleção é renderizado JÁ ROTACIONADO junto
// com a caixa (transform: rotate no .overlay), então os handles ficam nos
// cantos LOCAIS — o `dir` do handle já é o dir local, sem mapeamento.

/** Pra cada `localDir`, retorna os fatores (lxFactor, lyFactor) do
 *  corner LOCAL OPOSTO — o que deve permanecer fixo na tela durante o
 *  resize. lxFactor: -1 = lado esquerdo, +1 = lado direito.
 *  lyFactor: -1 = topo, +1 = fundo. */
function getOppositeCornerFactors(
  localDir: ResizeDir,
): { lx: -1 | 1; ly: -1 | 1 } {
  switch (localDir) {
    case "ne":
      return { lx: -1, ly: 1 }; // SW fixo
    case "nw":
      return { lx: 1, ly: 1 }; // SE fixo
    case "se":
      return { lx: -1, ly: -1 }; // NW fixo
    case "sw":
      return { lx: 1, ly: -1 }; // NE fixo
    case "n":
      return { lx: -1, ly: 1 }; // edge → fixa um canto do lado oposto
    case "s":
      return { lx: -1, ly: -1 };
    case "e":
      return { lx: -1, ly: -1 };
    case "w":
      return { lx: 1, ly: -1 };
  }
}

/** Offset (em cm) do `wrap_x_cm`/`wrap_y_cm` até a posição screen do
 *  corner LOCAL especificado, considerando a rotação ao redor do centro.
 *
 *  Mat: corner LOCAL relativo ao centro = (lxFactor * W/2, lyFactor * H/2).
 *  Após rotação por r ao redor do centro:
 *    rel_x = cos(r) * lxFactor * W/2 - sin(r) * lyFactor * H/2
 *    rel_y = sin(r) * lxFactor * W/2 + cos(r) * lyFactor * H/2
 *  Screen pos = wrap_pos + center_offset + rel
 *  Como `center_offset = (W/2, H/2)`, offset total do wrap até screen é
 *  (W/2 + rel_x, H/2 + rel_y). */
function cornerScreenOffsetCm(
  lxFactor: number,
  lyFactor: number,
  W: number,
  H: number,
  rRad: number,
): { x: number; y: number } {
  const cos = Math.cos(rRad);
  const sin = Math.sin(rRad);
  return {
    x: W / 2 + cos * lxFactor * (W / 2) - sin * lyFactor * (H / 2),
    y: H / 2 + sin * lxFactor * (W / 2) + cos * lyFactor * (H / 2),
  };
}

const MIN_WIDTH_CM = 1;
const MIN_HEIGHT_CM = 0.8;
const PX_PER_CM = 37.7952755906;
const ROTATION_SNAP_DEG = 15;

export function TextBoxOverlay({
  editor,
  selected,
  containerRef,
  onBeforeEnterTextEdit,
}: TextBoxOverlayProps) {
  const [rect, setRect] = useState<DOMRect | null>(null);
  const [dragTooltip, setDragTooltip] = useState<{
    x: number;
    y: number;
    label: string;
  } | null>(null);
  const [previewRotation, setPreviewRotation] = useState<number | null>(null);
  // Pós-laudo U fix — ref direto ao div overlay pra atualização DOM
  // imediata durante drag/resize. `setRect` continua sendo chamado pra
  // sincronizar o React state pós-drag, mas durante o drag o React tem
  // latência de render e os handles ficam "pra trás" da caixa. Esse
  // ref dá update instantâneo, igual o `el.style.left/top` faz no node.
  const overlayRef = useRef<HTMLDivElement | null>(null);

  /** Atualiza a posição+tamanho do overlay div via DOM direto, sem
   *  passar pelo React. Usado durante drag/resize pra evitar lag dos
   *  handles. Lê o bounding rect ATUAL do node + o container, calcula
   *  o offset e escreve no style do overlay. */
  const syncOverlayFromDom = useCallback(
    (nodeEl: HTMLElement) => {
      const overlay = overlayRef.current;
      const containerEl = containerRef.current;
      if (!overlay || !containerEl) return;
      const r = nodeEl.getBoundingClientRect();
      const c = containerEl.getBoundingClientRect();
      // Caixa NÃO-rotacionada centrada no centro do AABB — a rotação vem do
      // `transform` aplicado no render. Sem isto o overlay seguiria o AABB (a
      // caixa maior alinhada à tela) e os handles não girariam junto.
      const zoom =
        containerEl.offsetWidth > 0 ? c.width / containerEl.offsetWidth : 1;
      const w = (nodeEl.offsetWidth || 1) * zoom;
      const h = (nodeEl.offsetHeight || 1) * zoom;
      const cx = r.left - c.left + r.width / 2;
      const cy = r.top - c.top + r.height / 2;
      overlay.style.left = `${cx - w / 2}px`;
      overlay.style.top = `${cy - h / 2}px`;
      overlay.style.width = `${w}px`;
      overlay.style.height = `${h}px`;
    },
    [containerRef],
  );

  const recompute = useCallback(() => {
    if (!selected || !containerRef.current || !editor) {
      setRect(null);
      return;
    }
    // Pós-laudo U fix — sempre tenta DOM fresco PRIMEIRO via view.nodeDOM.
    // Só usa o cached selected.domEl se estiver conectado E nodeDOM
    // não retornou nada conectado. Sem essa ordem, o overlay segurava
    // um domEl detached (do header editor desmontado ao sair do modo
    // header) e renderizava com rect cached em posição errada.
    let domEl: HTMLElement | null = null;
    try {
      const dom = editor.view.nodeDOM(selected.pos);
      if (dom instanceof HTMLElement && dom.isConnected) domEl = dom;
    } catch {
      // ignore
    }
    if (!domEl && selected.domEl.isConnected) {
      domEl = selected.domEl;
    }
    if (!domEl) {
      // Nada conectado — esconde o overlay até voltar a um estado
      // válido (ex: re-mount do EditorContent ao re-entrar header mode).
      setRect(null);
      return;
    }
    const r = domEl.getBoundingClientRect();
    const c = containerRef.current.getBoundingClientRect();
    // Caixa NÃO-rotacionada centrada no centro do AABB (o render aplica
    // `transform: rotate`, então os handles giram junto com a caixa).
    const contEl = containerRef.current;
    const zoom = contEl.offsetWidth > 0 ? c.width / contEl.offsetWidth : 1;
    const w = (domEl.offsetWidth || 1) * zoom;
    const h = (domEl.offsetHeight || 1) * zoom;
    const cx = r.left - c.left + r.width / 2;
    const cy = r.top - c.top + r.height / 2;
    setRect(new DOMRect(cx - w / 2, cy - h / 2, w, h));
  }, [selected, containerRef, editor]);

  useLayoutEffect(() => {
    recompute();
  }, [recompute]);

  // Scroll/resize listeners — igual ShapeOverlay/FigureOverlay.
  useEffect(() => {
    if (!selected || !containerRef.current) return undefined;
    const el = containerRef.current;
    const onScroll = () => recompute();
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

  const updateAttrs = useCallback(
    (patch: Record<string, unknown>) => {
      if (!editor || !selected) return;
      const { state } = editor;
      const sel = state.selection;
      if (!(sel instanceof NodeSelection)) return;
      const pos = sel.from;
      const node = sel.node;
      if (node.type.name !== "text_box") return;
      const tr = state.tr.setNodeMarkup(pos, undefined, {
        ...node.attrs,
        ...patch,
      });
      tr.setSelection(NodeSelection.create(tr.doc, pos));
      editor.view.dispatch(tr);
    },
    [editor, selected],
  );

  const deleteTextBox = useCallback(() => {
    if (!editor) return;
    editor.chain().focus().deleteSelection().run();
  }, [editor]);

  // Zoom robusto pra elementos rotacionados.
  //
  // ANTES: `zoom = el.getBoundingClientRect().height / el.offsetHeight`.
  // Pra elementos COM rotação, `getBoundingClientRect()` retorna o AABB
  // do elemento JÁ ROTACIONADO — então uma caixa 6×2cm rotacionada -90°
  // tem AABB ~2×6cm. Dividir por offsetHeight (2cm não-rotacionado)
  // dá zoom ~3, e os deltas do drag/resize ficam multiplicados por 3,
  // jogando a caixa pra longe a cada movimento.
  //
  // DEPOIS: usa um ancestral NÃO-rotacionado (`containerRef` = região
  // do editor) pra calcular o zoom da página. Mesmo zoom independente
  // de quanto a caixa está rotacionada.
  const getZoom = useCallback((): number => {
    const containerEl = containerRef.current;
    if (!containerEl) return 1;
    const w = containerEl.offsetWidth;
    if (w <= 0) return 1;
    return containerEl.getBoundingClientRect().width / w;
  }, [containerRef]);

  // Pós-laudo U fix — ref pra timing de dblclick no DragZone.
  // Quando a textbox tá selecionada, o overlay com DragZone fica
  // EM CIMA dela cobrindo o conteúdo — capturando todos os mousedown
  // antes de chegarem no NodeView. Isso quebra a detecção de dblclick
  // do NodeView. Fix: detectamos dblclick AQUI também (timing entre 2
  // mousedowns no DragZone) e convertemos pra TextSelection, entrando
  // em modo de edição.
  const DBLCLICK_THRESHOLD_MS = 500;
  const lastDragMouseDownAt = useRef(0);

  // ---- Drag-to-reposition ----
  const handleDragMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (!editor || !selected) return;
      if (e.button !== 0) return;
      const wrapMode =
        (selected.attrs.wrap_mode as TextBoxWrapMode | null) ?? "in_front";
      if (wrapMode === "inline") return;

      // Detecção de dblclick no DragZone: dois mousedowns rápidos →
      // entra em modo de edição de texto na posição clicada.
      const now = performance.now();
      const isDoubleClick =
        now - lastDragMouseDownAt.current < DBLCLICK_THRESHOLD_MS;
      lastDragMouseDownAt.current = now;
      if (isDoubleClick) {
        // Pós-laudo U fix — ativa header mode ANTES do dispatch (no-op
        // pro body). Sem isso, dblclick no header textbox quando o
        // user tá em body mode despachava TextSelection num editor
        // não-editável e nada acontecia.
        if (onBeforeEnterTextEdit) onBeforeEnterTextEdit();
        // Defensiva — garante editor editável (race com o useEffect
        // de setEditable na mudança de editingRegion).
        try {
          editor.setEditable(true);
        } catch {
          // ignora
        }
        const view = editor.view;
        // Tenta resolver a posição exata clicada. Se falhar (caso
        // comum em headers absolutos onde caretRangeFromPoint às vezes
        // retorna null), cai no fallback: posição no início do
        // conteúdo do textbox.
        let targetPos: number | null = null;
        const coords = view.posAtCoords({
          left: e.clientX,
          top: e.clientY,
        });
        if (coords) {
          try {
            const $pos = view.state.doc.resolve(coords.pos);
            for (let d = $pos.depth; d >= 0; d--) {
              if ($pos.node(d).type.name === "text_box") {
                targetPos = coords.pos;
                break;
              }
            }
          } catch {
            // ignora
          }
        }
        // Fallback: início do conteúdo do textbox (pos + 1 = primeiro
        // child do textbox, geralmente um <p>).
        if (targetPos === null) {
          targetPos = selected.pos + 1;
        }
        try {
          const textSel = TextSelection.create(view.state.doc, targetPos);
          view.dispatch(view.state.tr.setSelection(textSel));
          view.focus();
          e.preventDefault();
          e.stopPropagation();
          return;
        } catch {
          // pos inválido — abandona dblclick, deixa fall-through pro drag
        }
      }

      e.preventDefault();
      e.stopPropagation();

      const startX = e.clientX;
      const startY = e.clientY;
      const startWrapX = Number(selected.attrs.wrap_x_cm) || 0;
      const startWrapY = Number(selected.attrs.wrap_y_cm) || 0;
      const el = selected.domEl;
      const zoom = getZoom();
      let hasMoved = false;
      let latestX = startWrapX;
      let latestY = startWrapY;

      const onMove = (ev: MouseEvent) => {
        const dx = ev.clientX - startX;
        const dy = ev.clientY - startY;
        if (!hasMoved && Math.abs(dx) + Math.abs(dy) < 5) return;
        hasMoved = true;
        ev.preventDefault();
        const dxLayout = dx / Math.max(0.0001, zoom);
        const dyLayout = dy / Math.max(0.0001, zoom);
        const newX = startWrapX + dxLayout / PX_PER_CM;
        const newY = startWrapY + dyLayout / PX_PER_CM;
        el.style.left = `${newX.toFixed(2)}cm`;
        el.style.top = `${newY.toFixed(2)}cm`;
        latestX = newX;
        latestY = newY;
        setDragTooltip({
          x: ev.clientX,
          y: ev.clientY,
          label: `${newX.toFixed(2)}cm, ${newY.toFixed(2)}cm`,
        });
        // Atualização DOM direta — handles seguem instantaneamente
        // sem esperar o ciclo de render do React.
        syncOverlayFromDom(el);
      };

      const onUp = () => {
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
        if (hasMoved) {
          setDragTooltip(null);
          updateAttrs({
            wrap_x_cm: Number(latestX.toFixed(2)),
            wrap_y_cm: Number(latestY.toFixed(2)),
          });
        }
      };

      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [editor, selected, updateAttrs, getZoom, syncOverlayFromDom],
  );

  // ---- Resize ----
  //
  // Resize correto com rotação:
  //   1. O usuário clica um handle posicionado no AABB do textbox
  //      (a bounding box ALINHADA AOS EIXOS DA TELA após rotação).
  //      Esse é o `dir` AABB.
  //   2. Convertemos pro `localDir` (qual corner LOCAL do textbox
  //      não-rotacionado está sendo arrastado) via `aabbDirToLocalDir`.
  //   3. O delta do mouse vem em coords de TELA. Inverso-rotacionamos
  //      pra obter o delta em coords LOCAIS do textbox.
  //   4. Aplicamos o delta LOCAL na W/H do box conforme o localDir.
  //   5. Recalculamos wrap_x_cm/wrap_y_cm pra manter o corner OPOSTO
  //      no mesmo lugar na TELA — sem isso, a rotação ao redor do
  //      centro faria o box "escorregar" durante o resize porque o
  //      centro muda quando W/H mudam.
  const handleResizeMouseDown = useCallback(
    (dir: ResizeDir) => (e: React.MouseEvent) => {
      if (!editor || !selected) return;
      if (e.button !== 0) return;
      e.preventDefault();
      e.stopPropagation();

      const startX = e.clientX;
      const startY = e.clientY;
      const startW = Number(selected.attrs.width_cm) || 1;
      const startH = Number(selected.attrs.height_cm) || 1;
      const startWrapX = Number(selected.attrs.wrap_x_cm) || 0;
      const startWrapY = Number(selected.attrs.wrap_y_cm) || 0;
      const rotationDeg = Number(selected.attrs.rotation) || 0;
      const rRad = (rotationDeg * Math.PI) / 180;
      const cosR = Math.cos(rRad);
      const sinR = Math.sin(rRad);
      const el = selected.domEl;
      const zoom = getZoom();

      // Handles giram JUNTO com a caixa (overlay rotacionado), então o dir do
      // handle JÁ É o dir local — sem mapeamento AABB→local.
      const localDir = dir;

      // Pré-calcula o screen position do corner FIXO (oposto ao
      // localDir). Manteremos esse ponto constante durante o resize.
      const fixedFactors = getOppositeCornerFactors(localDir);
      const startFixedOffset = cornerScreenOffsetCm(
        fixedFactors.lx,
        fixedFactors.ly,
        startW,
        startH,
        rRad,
      );
      const fixedScreenX = startWrapX + startFixedOffset.x;
      const fixedScreenY = startWrapY + startFixedOffset.y;

      let latestW = startW;
      let latestH = startH;
      let latestX = startWrapX;
      let latestY = startWrapY;

      const onMove = (ev: MouseEvent) => {
        const dx = (ev.clientX - startX) / Math.max(0.0001, zoom);
        const dy = (ev.clientY - startY) / Math.max(0.0001, zoom);

        // Inverso-rotaciona o delta do mouse pra coords LOCAIS.
        // R(-r) * (dx, dy):
        //   ldx = cos(r) * dx + sin(r) * dy
        //   ldy = -sin(r) * dx + cos(r) * dy
        const ldx = cosR * dx + sinR * dy;
        const ldy = -sinR * dx + cosR * dy;
        const ldxCm = ldx / PX_PER_CM;
        const ldyCm = ldy / PX_PER_CM;

        let newW = startW;
        let newH = startH;
        // Aplica o delta LOCAL no W/H baseado no localDir.
        if (
          localDir === "e" ||
          localDir === "ne" ||
          localDir === "se"
        ) {
          newW = Math.max(MIN_WIDTH_CM, startW + ldxCm);
        } else if (
          localDir === "w" ||
          localDir === "nw" ||
          localDir === "sw"
        ) {
          newW = Math.max(MIN_WIDTH_CM, startW - ldxCm);
        }
        if (
          localDir === "s" ||
          localDir === "se" ||
          localDir === "sw"
        ) {
          newH = Math.max(MIN_HEIGHT_CM, startH + ldyCm);
        } else if (
          localDir === "n" ||
          localDir === "ne" ||
          localDir === "nw"
        ) {
          newH = Math.max(MIN_HEIGHT_CM, startH - ldyCm);
        }

        // Recalcula wrap_x_cm/wrap_y_cm pra manter o corner FIXO no
        // mesmo screen position. Como W e H mudaram, o offset do
        // corner muda — compensamos no wrap.
        const newFixedOffset = cornerScreenOffsetCm(
          fixedFactors.lx,
          fixedFactors.ly,
          newW,
          newH,
          rRad,
        );
        const newX = fixedScreenX - newFixedOffset.x;
        const newY = fixedScreenY - newFixedOffset.y;

        latestW = newW;
        latestH = newH;
        latestX = newX;
        latestY = newY;
        el.style.width = `${newW.toFixed(2)}cm`;
        el.style.height = `${newH.toFixed(2)}cm`;
        el.style.left = `${newX.toFixed(2)}cm`;
        el.style.top = `${newY.toFixed(2)}cm`;
        setDragTooltip({
          x: ev.clientX,
          y: ev.clientY,
          label: `${newW.toFixed(1)} × ${newH.toFixed(1)} cm`,
        });
        // DOM direto: overlay segue instantaneamente. recompute()
        // dispara React update pros HANDLES (cujas posições dependem
        // de rect.width/height — que pra rotated box é o AABB).
        syncOverlayFromDom(el);
        recompute();
      };

      const onUp = () => {
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
        setDragTooltip(null);
        updateAttrs({
          width_cm: Number(latestW.toFixed(2)),
          height_cm: Number(latestH.toFixed(2)),
          wrap_x_cm: Number(latestX.toFixed(2)),
          wrap_y_cm: Number(latestY.toFixed(2)),
        });
      };

      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [editor, selected, updateAttrs, recompute, getZoom, syncOverlayFromDom],
  );

  // ---- Rotation ----
  const handleRotationMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (!editor || !selected) return;
      if (e.button !== 0) return;
      e.preventDefault();
      e.stopPropagation();
      const startRotation = Number(selected.attrs.rotation) || 0;
      const el = selected.domEl;
      const elRect = el.getBoundingClientRect();
      const cx = elRect.left + elRect.width / 2;
      const cy = elRect.top + elRect.height / 2;
      const startAngle =
        (Math.atan2(e.clientY - cy, e.clientX - cx) * 180) / Math.PI;
      let latest = startRotation;

      const onMove = (ev: MouseEvent) => {
        ev.preventDefault();
        const angle =
          (Math.atan2(ev.clientY - cy, ev.clientX - cx) * 180) / Math.PI;
        let delta = angle - startAngle;
        // Normaliza pra -180..180
        if (delta > 180) delta -= 360;
        if (delta < -180) delta += 360;
        let next = startRotation + delta;
        if (ev.shiftKey) {
          next = Math.round(next / ROTATION_SNAP_DEG) * ROTATION_SNAP_DEG;
        }
        // Normaliza pra -180..180 pra evitar growth descontrolado
        while (next > 180) next -= 360;
        while (next < -180) next += 360;
        latest = next;
        el.style.transform = `rotate(${next}deg)`;
        setPreviewRotation(next);
        setDragTooltip({
          x: ev.clientX,
          y: ev.clientY,
          label: `${Math.round(next)}°${ev.shiftKey ? " (snap)" : ""}`,
        });
        // Como a rotação muda o AABB do node, o overlay também precisa
        // seguir. DOM direto pra responsividade + recompute pros handles
        // (cujas posições dependem do rect width/height do AABB).
        syncOverlayFromDom(el);
        recompute();
      };

      const onUp = () => {
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
        setDragTooltip(null);
        setPreviewRotation(null);
        updateAttrs({ rotation: Math.round(latest) });
      };

      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [editor, selected, updateAttrs, recompute, syncOverlayFromDom],
  );

  // ---- Cálculos derivados ----
  const effectiveWrapMode =
    (selected?.attrs.wrap_mode as TextBoxWrapMode | null) ?? "in_front";
  const borderEnabled = selected?.attrs.border_enabled !== false;
  const borderColor =
    (selected?.attrs.border_color as string | null) ?? "#1f2937";
  const borderWidth = Number(selected?.attrs.border_width ?? 1);
  const borderStyle =
    (selected?.attrs.border_style as TextBoxBorderStyle | null) ?? "solid";
  const fillEnabled = selected?.attrs.fill_enabled === true;
  const fillColor =
    (selected?.attrs.fill_color as string | null) ?? "#ffffff";
  const rotation =
    previewRotation ?? (Number(selected?.attrs.rotation) || 0);
  const textOrientation =
    (selected?.attrs.text_orientation as TextBoxOrientation | null) ??
    "horizontal";

  if (!selected || !rect) return null;

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
        ref={overlayRef}
        className={styles.overlay}
        style={{
          left: `${rect.left}px`,
          top: `${rect.top}px`,
          width: `${rect.width}px`,
          height: `${rect.height}px`,
          // A caixa de seleção + handles giram JUNTO com a textbox (em vez de
          // formar a AABB maior alinhada à tela). `rect` já é a caixa
          // não-rotacionada centrada; aqui só aplicamos a rotação.
          transform: `rotate(${rotation}deg)`,
          transformOrigin: "center",
        }}
        onMouseDown={(e) => e.stopPropagation()}
      >
        {/* Toolbar acima */}
        <div
          className={styles.toolbar}
          style={{
            top: -42,
            left: rect.width / 2,
            // Contra-rotaciona pra manter a toolbar legível mesmo com a caixa
            // girada (o .overlay tem `rotate`; aqui desfazemos no texto).
            transform: `translateX(-50%) rotate(${-rotation}deg)`,
          }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <button
            type="button"
            className={`${styles.toolBtn} ${effectiveWrapMode === "inline" ? styles.toolBtnActive : ""}`}
            onClick={() => updateAttrs({ wrap_mode: "inline" })}
            title="Alinhado ao texto"
          >
            <WrapText size={14} />
          </button>
          <button
            type="button"
            className={`${styles.toolBtn} ${effectiveWrapMode === "in_front" ? styles.toolBtnActive : ""}`}
            onClick={() => updateAttrs({ wrap_mode: "in_front" })}
            title="Em frente ao texto"
          >
            <BringToFront size={14} />
          </button>
          <button
            type="button"
            className={`${styles.toolBtn} ${effectiveWrapMode === "behind" ? styles.toolBtnActive : ""}`}
            onClick={() => updateAttrs({ wrap_mode: "behind" })}
            title="Atrás do texto"
          >
            <SendToBack size={14} />
          </button>
          <div className={styles.toolDivider} />
          {/* Border controls */}
          <button
            type="button"
            className={`${styles.toolBtn} ${borderEnabled ? styles.toolBtnActive : ""}`}
            onClick={() => updateAttrs({ border_enabled: !borderEnabled })}
            title={
              borderEnabled
                ? "Borda visível (clique pra deixar transparente)"
                : "Borda transparente (clique pra deixar visível)"
            }
          >
            <Square size={14} />
          </button>
          {borderEnabled && (
            <>
              <label
                className={styles.colorSwatch}
                style={{ background: borderColor }}
                title="Cor da borda"
              >
                <input
                  type="color"
                  value={normalizeColor(borderColor)}
                  onChange={(e) =>
                    updateAttrs({ border_color: e.target.value })
                  }
                />
              </label>
              <input
                type="number"
                min="0.5"
                max="20"
                step="0.5"
                className={styles.numInput}
                value={borderWidth}
                onChange={(e) =>
                  updateAttrs({
                    border_width: Number(e.target.value) || 1,
                  })
                }
                title="Espessura da borda (px)"
              />
              <select
                className={styles.selectInput}
                value={borderStyle}
                onChange={(e) =>
                  updateAttrs({
                    border_style: e.target.value as TextBoxBorderStyle,
                  })
                }
                title="Estilo da borda"
              >
                <option value="solid">Sólida</option>
                <option value="dashed">Tracejada</option>
                <option value="dotted">Pontilhada</option>
              </select>
            </>
          )}
          <div className={styles.toolDivider} />
          {/* Fill controls */}
          <button
            type="button"
            className={`${styles.toolBtn} ${fillEnabled ? styles.toolBtnActive : ""}`}
            onClick={() => updateAttrs({ fill_enabled: !fillEnabled })}
            title={
              fillEnabled
                ? "Preenchimento visível (clique pra deixar transparente)"
                : "Preenchimento transparente (clique pra deixar visível)"
            }
          >
            <SquareDashed size={14} />
          </button>
          {fillEnabled && (
            <label
              className={styles.colorSwatch}
              style={{ background: fillColor }}
              title="Cor do preenchimento"
            >
              <input
                type="color"
                value={normalizeColor(fillColor)}
                onChange={(e) =>
                  updateAttrs({ fill_color: e.target.value })
                }
              />
            </label>
          )}
          <div className={styles.toolDivider} />
          {/* Rotation indicator (read-only — use rotation handle pra mudar) */}
          <span className={styles.labelSmall} title="Rotação atual (use o handle acima)">
            {Math.round(rotation)}°
          </span>
          <div className={styles.toolDivider} />
          {/* U2 — Direção do texto: horizontal / vertical de baixo p/ cima
              (marca lateral) / vertical de cima p/ baixo. */}
          <select
            className={styles.selectInput}
            value={textOrientation}
            onChange={(e) =>
              updateAttrs({
                text_orientation: e.target.value as TextBoxOrientation,
              })
            }
            title="Direção do texto na caixa"
          >
            <option value="horizontal">Texto horizontal</option>
            <option value="vertical_up">Vertical ↑ (baixo→cima)</option>
            <option value="vertical_down">Vertical ↓ (cima→baixo)</option>
          </select>
          <div className={styles.toolDivider} />
          <button
            type="button"
            className={`${styles.toolBtn} ${styles.toolBtnDanger}`}
            onClick={deleteTextBox}
            title="Excluir caixa"
          >
            <Trash2 size={14} />
          </button>
        </div>

        {/* Borda de seleção */}
        <div className={styles.selectionFrame} aria-hidden />

        {/* DragZone (modo flutuante) */}
        {effectiveWrapMode !== "inline" && (
          <div
            className={styles.dragZone}
            onMouseDown={handleDragMouseDown}
            aria-hidden
          />
        )}

        {/* 8 handles */}
        {(Object.keys(handlePos) as ResizeDir[]).map((dir) => (
          <div
            key={dir}
            className={styles.handle}
            style={{
              left: `${handlePos[dir].left}px`,
              top: `${handlePos[dir].top}px`,
              width: `${HSIZE}px`,
              height: `${HSIZE}px`,
              cursor: cursorOf[dir],
            }}
            onMouseDown={handleResizeMouseDown(dir)}
          />
        ))}

        {/* Rotation handle — círculo acima do topo, ligado por linha curta. */}
        <div
          className={styles.rotationHandle}
          style={{
            left: rect.width / 2 - 8,
            top: -28,
          }}
          onMouseDown={handleRotationMouseDown}
          title="Arrastar pra rotacionar. Segure Shift pra snap a 15°."
        >
          <RotateCw size={12} />
        </div>
        <div
          className={styles.rotationLine}
          style={{
            left: rect.width / 2 - 0.5,
            top: -20,
          }}
          aria-hidden
        />
      </div>

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

/** Normaliza cor pra `#RRGGBB` aceito por `<input type="color">`. */
function normalizeColor(color: string): string {
  if (!color) return "#000000";
  if (color.startsWith("#") && color.length === 7) return color;
  if (color.startsWith("#") && color.length === 4) {
    const r = color[1];
    const g = color[2];
    const b = color[3];
    return `#${r}${r}${g}${g}${b}${b}`;
  }
  return "#000000";
}
