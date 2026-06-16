/**
 * W (fase 2) — PageFooterRegion: faixa de RODAPÉ replicada na base de cada
 * page card. Simétrico ao `PageHeaderRegion`, mas ancorado na BASE da página
 * (dentro da banda da margem inferior — por isso NÃO altera a paginação) e
 * com o handle de altura na borda SUPERIOR (arrastar pra cima = cresce).
 *
 *   - modo BODY/HEADER (isEditing=false): clone visual estático do
 *     `doc.footer.content` (mostra o brasão da PC). Double-click ativa edição.
 *   - modo FOOTER (isEditing=true): a 1ª pageCard hospeda o `<EditorContent>`
 *     real; as demais seguem com o clone que atualiza em tempo real.
 *
 * Reusa o CSS de `PageHeaderRegion.module.css`.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { EditorContent, type Editor } from "@tiptap/react";
import styles from "./PageHeaderRegion.module.css";

export interface PageFooterRegionProps {
  pageIndex: number;
  isEditing: boolean;
  enabled: boolean;
  editor: Editor | null;
  footerHtml: string;
  topCm: number;
  widthCm: number;
  pageHeightCm: number;
  footerHeightCm: number;
  paddingLeftCm: number;
  paddingRightCm: number;
  onActivate: () => void;
  /** Drag-to-resize: a borda SUPERIOR vira draggable (pra cima cresce). */
  onHeightChange?: (heightCm: number) => void;
  maxAllowedHeightCm?: number;
  /** "Fechar rodapé" (volta pro body). */
  onClose?: () => void;
  /** "Desativar rodapé" (enabled=false). */
  onDisable?: () => void;
}

export function PageFooterRegion({
  pageIndex,
  isEditing,
  enabled,
  editor,
  footerHtml,
  topCm,
  widthCm,
  pageHeightCm,
  footerHeightCm,
  paddingLeftCm,
  paddingRightCm,
  onActivate,
  onHeightChange,
  maxAllowedHeightCm = 6,
  onClose,
  onDisable,
}: PageFooterRegionProps) {
  const regionRef = useRef<HTMLDivElement | null>(null);
  const [dragPreviewCm, setDragPreviewCm] = useState<number | null>(null);
  const effectiveHeightCm = dragPreviewCm ?? footerHeightCm;

  // Drag na borda SUPERIOR: arrastar pra CIMA aumenta a altura (o rodapé
  // cresce em direção ao corpo). Espelha o handle do header (que cresce pra
  // baixo) com o sinal do delta invertido.
  const handleResizeMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (!onHeightChange) return;
      e.preventDefault();
      e.stopPropagation();
      const regionEl = regionRef.current;
      if (!regionEl) return;
      const rect = regionEl.getBoundingClientRect();
      const pxPerCm = rect.width / widthCm;
      if (!Number.isFinite(pxPerCm) || pxPerCm <= 0) return;

      const startY = e.clientY;
      const startHeightCm = footerHeightCm;
      let didMove = false;

      const computeNewCm = (clientY: number): number => {
        const deltaPx = clientY - startY;
        const deltaCm = deltaPx / pxPerCm;
        // Pra cima (deltaCm < 0) → cresce; pra baixo → encolhe.
        let newCm = startHeightCm - deltaCm;
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
    [onHeightChange, footerHeightCm, widthCm, maxAllowedHeightCm],
  );

  // Limpa o preview quando a prop alinhar (commit confirmado).
  useEffect(() => {
    if (dragPreviewCm === null) return;
    if (Math.abs(footerHeightCm - dragPreviewCm) < 0.005) {
      setDragPreviewCm(null);
    } else {
      setDragPreviewCm(footerHeightCm);
    }
  }, [footerHeightCm, dragPreviewCm]);

  useEffect(() => {
    if (dragPreviewCm === null) return;
    const prev = document.body.style.cursor;
    document.body.style.cursor = "ns-resize";
    return () => {
      document.body.style.cursor = prev;
    };
  }, [dragPreviewCm]);

  if (!enabled || effectiveHeightCm <= 0) return null;

  const footerTopCm = topCm + pageHeightCm - effectiveHeightCm;
  const hostsEditor = isEditing && pageIndex === 0 && editor !== null;
  const showControls = isEditing && pageIndex === 0;
  const showResizeHandle = isEditing && pageIndex === 0 && !!onHeightChange;

  return (
    <div
      ref={pageIndex === 0 ? regionRef : null}
      className={`${styles.region} ${isEditing ? styles.editing : ""}`}
      style={{
        top: `${footerTopCm}cm`,
        left: 0,
        width: `${widthCm}cm`,
        height: `${effectiveHeightCm}cm`,
        paddingLeft: `${paddingLeftCm}cm`,
        paddingRight: `${paddingRightCm}cm`,
      }}
      data-footer-region="true"
      data-page-index={pageIndex}
      data-editing={isEditing ? "true" : "false"}
      onDoubleClick={!isEditing ? onActivate : undefined}
    >
      {hostsEditor ? (
        <div className={styles.editorHost}>
          <EditorContent editor={editor} />
        </div>
      ) : (
        <div
          className={styles.cloneHost}
          // eslint-disable-next-line react/no-danger
          dangerouslySetInnerHTML={{ __html: footerHtml }}
          aria-hidden={!isEditing}
        />
      )}

      {/* Handle de altura na borda SUPERIOR (só pg1 em edição). */}
      {showResizeHandle && (
        <div
          className={styles.resizeHandle}
          style={{ top: -4, bottom: "auto" }}
          onMouseDown={handleResizeMouseDown}
          title="Arraste para ajustar a altura do rodapé"
          aria-label="Redimensionar altura do rodapé"
        >
          <div className={styles.resizeGrip} aria-hidden />
        </div>
      )}

      {/* Badge "Editando rodapé" + controles (pg1 apenas). */}
      {showControls && (
        <div className={styles.editingBadge} style={{ top: -38 }}>
          <span className={styles.editingBadgeLabel}>
            Editando rodapé ·{" "}
            <span className={styles.editingBadgeSub}>
              {effectiveHeightCm.toFixed(1)} cm · todas as páginas
            </span>
          </span>
          {onDisable && (
            <button
              type="button"
              className={styles.badgeBtnDanger}
              onClick={onDisable}
              title="Desativa o rodapé (conteúdo permanece salvo)"
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
              aria-label="Fechar rodapé"
            >
              ✕
            </button>
          )}
        </div>
      )}

      {/* Hint sutil em modo body/header. */}
      {!isEditing && (
        <div className={styles.hint} aria-hidden>
          Clique duas vezes para editar o rodapé
        </div>
      )}
    </div>
  );
}
