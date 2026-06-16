/**
 * N — PageHeaderRegion: faixa de cabeçalho replicada em cada page card.
 *
 * Semântica:
 *   - Em modo BODY (`isEditing === false`): renderiza um clone visual
 *     estático do `doc.header.content`. Não-editável. Double-click ativa
 *     o modo header (chamando `onActivate`).
 *   - Em modo HEADER (`isEditing === true`):
 *       - Na PRIMEIRA pageCard (pageIndex === 0): renderiza o
 *         <EditorContent> do `headerEditor` real (interativo).
 *       - Nas demais (pageIndex >= 1): continua mostrando o clone visual,
 *         que atualiza em tempo real conforme o user edita (porque o
 *         `headerHtml` é recomputado pelo pai a cada update do header).
 *   - Quando `enabled === false`: renderiza placeholder discreto. Double-
 *     click ativa o cabeçalho (vai ligar enabled e entrar em modo header).
 *
 * Posicionamento: o componente espera ser inserido DENTRO do `.pageStack`
 * (posicionado absolutamente). O caller passa `topCm` (offset desde o topo
 * do pageStack) e `widthCm` (largura do page card). A altura é
 * `headerHeightCm`.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { EditorContent, type Editor } from "@tiptap/react";
import styles from "./PageHeaderRegion.module.css";

export interface PageHeaderRegionProps {
  /** Índice da página (0-based). Determina se hospeda o editor real ou só
   *  o clone visual quando em modo edição. */
  pageIndex: number;
  /** Modo edição global. */
  isEditing: boolean;
  /** Header está habilitado no documento (doc.header.enabled). */
  enabled: boolean;
  /** Instância TipTap do header (passada pelo EditorPage). Só usada
   *  na PRIMEIRA pageCard quando `isEditing === true`. */
  editor: Editor | null;
  /** HTML estático pré-renderizado do conteúdo do header. Usado pelas
   *  pageCards que NÃO hospedam o editor real (todas exceto a primeira
   *  quando em modo header, e todas quando em modo body). */
  headerHtml: string;
  /** Top do pageCard em cm (dentro do pageStack). */
  topCm: number;
  /** Largura do pageCard em cm (= pageWidthCm). */
  widthCm: number;
  /** Altura do cabeçalho em cm. */
  headerHeightCm: number;
  /** Padding lateral interno do cabeçalho (geralmente igual às margens
   *  do body — `margins.left`/`margins.right`). */
  paddingLeftCm: number;
  paddingRightCm: number;
  /** Padding top (offset do topo da página até a faixa do header).
   *  Por padrão zero — o header começa colado no topo da página. */
  paddingTopCm?: number;
  /** Callback disparado quando o usuário dá double-click no cabeçalho.
   *  O EditorPage seta `editingRegion = "header"` (e liga `enabled` se
   *  estiver desligado). */
  onActivate: () => void;
  /** N — Drag-to-resize da altura. Quando passado, a borda inferior da
   *  região (a "régua azul") vira draggável: pra cima encolhe, pra baixo
   *  aumenta. Limitado por `maxAllowedHeightCm` e clamped >= 0. */
  onHeightChange?: (heightCm: number) => void;
  /** Limite superior pra altura (geralmente = margens.top). Default 6cm. */
  maxAllowedHeightCm?: number;
  /** Callback "Fechar cabeçalho" (volta pro modo body). Mostrado como
   *  botão dentro do badge quando isEditing && pageIndex === 0. */
  onClose?: () => void;
  /** Callback "Desativar cabeçalho" (enabled=false + sai do modo).
   *  Mostrado como botão dentro do badge. */
  onDisable?: () => void;
}

export function PageHeaderRegion({
  pageIndex,
  isEditing,
  enabled,
  editor,
  headerHtml,
  topCm,
  widthCm,
  headerHeightCm,
  paddingLeftCm,
  paddingRightCm,
  paddingTopCm = 0,
  onActivate,
  onHeightChange,
  maxAllowedHeightCm = 6,
  onClose,
  onDisable,
}: PageHeaderRegionProps) {
  const handleDoubleClick = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      onActivate();
    },
    [onActivate],
  );

  // N — Drag-to-resize na borda inferior (régua azul). Pattern espelhado
  // do HorizontalRuler (M7): mantém um dragPreview local até a prop
  // alinhar com o valor commitado, evitando flicker entre mouseup e a
  // propagação do store.
  const regionRef = useRef<HTMLDivElement | null>(null);
  const [dragPreviewCm, setDragPreviewCm] = useState<number | null>(null);
  const effectiveHeightCm = dragPreviewCm ?? headerHeightCm;

  const handleResizeMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (!onHeightChange) return;
      e.preventDefault();
      e.stopPropagation();

      const regionEl = regionRef.current;
      if (!regionEl) return;
      const rect = regionEl.getBoundingClientRect();
      // px/cm é estável durante o drag — use a largura visual (= widthCm)
      // como referência. Funciona em qualquer zoom porque pega o estado
      // visual atual.
      const pxPerCm = rect.width / widthCm;
      if (!Number.isFinite(pxPerCm) || pxPerCm <= 0) return;

      const startY = e.clientY;
      const startHeightCm = headerHeightCm;
      let didMove = false;

      const computeNewCm = (clientY: number): number => {
        const deltaPx = clientY - startY;
        const deltaCm = deltaPx / pxPerCm;
        let newCm = startHeightCm + deltaCm;
        if (newCm < 0) newCm = 0;
        if (newCm > maxAllowedHeightCm) newCm = maxAllowedHeightCm;
        return newCm;
      };

      const onMove = (ev: MouseEvent) => {
        didMove = true;
        setDragPreviewCm(computeNewCm(ev.clientY));
      };
      const onUp = (ev: MouseEvent) => {
        if (didMove) {
          const final = computeNewCm(ev.clientY);
          onHeightChange(final);
          setDragPreviewCm(final);
        } else {
          setDragPreviewCm(null);
        }
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
      };
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [onHeightChange, headerHeightCm, widthCm, maxAllowedHeightCm],
  );

  // M7-like: limpa dragPreview quando a prop chegar (commit confirmado).
  useEffect(() => {
    if (dragPreviewCm === null) return;
    if (Math.abs(headerHeightCm - dragPreviewCm) < 0.005) {
      setDragPreviewCm(null);
    } else {
      // Hardcap recuou — snap preview à prop pra clear no próximo render.
      setDragPreviewCm(headerHeightCm);
    }
  }, [headerHeightCm, dragPreviewCm]);

  // Cursor durante drag.
  useEffect(() => {
    if (dragPreviewCm === null) return;
    const prev = document.body.style.cursor;
    document.body.style.cursor = "ns-resize";
    return () => {
      document.body.style.cursor = prev;
    };
  }, [dragPreviewCm]);

  // Se o header está desligado, renderizamos uma fita SUPER discreta na
  // borda superior do page card. Double-click liga.
  if (!enabled) {
    return (
      <div
        className={styles.disabledStrip}
        style={{
          top: `${topCm}cm`,
          left: 0,
          width: `${widthCm}cm`,
        }}
        onDoubleClick={handleDoubleClick}
        title="Clique duas vezes para ativar o cabeçalho"
        aria-hidden
      />
    );
  }

  const hostsEditor = isEditing && pageIndex === 0 && editor !== null;
  const showResizeHandle = isEditing && pageIndex === 0 && !!onHeightChange;
  const showControls = isEditing && pageIndex === 0;

  return (
    <div
      ref={pageIndex === 0 ? regionRef : null}
      className={`${styles.region} ${isEditing ? styles.editing : ""}`}
      style={{
        top: `${topCm}cm`,
        left: 0,
        width: `${widthCm}cm`,
        height: `${effectiveHeightCm}cm`,
        paddingTop: `${paddingTopCm}cm`,
        paddingLeft: `${paddingLeftCm}cm`,
        paddingRight: `${paddingRightCm}cm`,
      }}
      onDoubleClick={!isEditing ? handleDoubleClick : undefined}
      data-page-index={pageIndex}
      data-editing={isEditing ? "true" : "false"}
    >
      {hostsEditor ? (
        <div className={styles.editorHost}>
          <EditorContent editor={editor} />
        </div>
      ) : (
        // Clone visual estático — atualizado pelo pai a cada keystroke
        // do header editor (via prop headerHtml).
        <div
          className={styles.cloneHost}
          // eslint-disable-next-line react/no-danger
          dangerouslySetInnerHTML={{ __html: headerHtml }}
          aria-hidden={!isEditing}
        />
      )}
      {/* Badge "Editando cabeçalho" + controles integrados (pg 1 apenas). */}
      {showControls && (
        <div className={styles.editingBadge}>
          <span className={styles.editingBadgeLabel}>
            Editando cabeçalho ·{" "}
            <span className={styles.editingBadgeSub}>
              {effectiveHeightCm.toFixed(1)} cm · todas as páginas
            </span>
          </span>
          {onDisable && (
            <button
              type="button"
              className={styles.badgeBtnDanger}
              onClick={onDisable}
              title="Desativa o cabeçalho (conteúdo permanece salvo)"
            >
              Desativar
            </button>
          )}
          {onClose && (
            <button
              type="button"
              className={styles.badgeBtnClose}
              onClick={onClose}
              title="Voltar ao corpo (Esc)"
              aria-label="Fechar cabeçalho"
            >
              ✕
            </button>
          )}
        </div>
      )}
      {/* Régua de altura: a borda azul inferior vira draggable. Sai do
          espaço da borda 4px pra cada lado pra ter hit-area confortável. */}
      {showResizeHandle && (
        <div
          className={styles.resizeHandle}
          onMouseDown={handleResizeMouseDown}
          title="Arraste para ajustar a altura do cabeçalho"
          aria-label="Redimensionar altura do cabeçalho"
        >
          <div className={styles.resizeGrip} aria-hidden />
        </div>
      )}
      {/* Hint sutil em modo body quando o header está habilitado. */}
      {!isEditing && (
        <div className={styles.hint} aria-hidden>
          Clique duas vezes para editar o cabeçalho
        </div>
      )}
    </div>
  );
}
