/**
 * SigdocsCoverHost — host invisível que reserva a área onde o webview
 * borderless do SIGDOC vai ficar.
 *
 * J — Quando `coverOpen=true`:
 *   1. Mede sua bounding rect (relativa ao webview principal).
 *   2. Envia bounds pro backend, que cria/reposiciona a window
 *      secundária borderless do SIGDOC EXATAMENTE em cima.
 *   3. ResizeObserver re-envia bounds quando o layout muda.
 *
 * Visualmente o host fica preto/vazio — o webview do SIGDOC cobre
 * por cima e ocupa todo o espaço. Quando `coverOpen=false`, o host
 * some e o conteúdo "verdadeiro" (editor do laudo) reaparece.
 *
 * Inclui um header com botão "Fechar SIGDOC" e instrução visual.
 */

import { useEffect, useRef } from "react";
import { Landmark, X } from "lucide-react";
import { commands } from "@core/commands";
import { useSigdocsStore } from "@stores/sigdocsStore";
import styles from "./SigdocsCoverHost.module.css";

interface Props {
  /** Margem em px reservada para o header (acima da área do webview). */
  headerHeightPx?: number;
}

export function SigdocsCoverHost({ headerHeightPx = 36 }: Props) {
  const coverOpen = useSigdocsStore((s) => s.coverOpen);
  const setCoverOpen = useSigdocsStore((s) => s.setCoverOpen);
  const hostRef = useRef<HTMLDivElement | null>(null);
  const lastBoundsRef = useRef<string>("");

  // J — Sincroniza bounds com o backend (Tauri reposiciona o webview).
  useEffect(() => {
    if (!coverOpen) return undefined;
    const host = hostRef.current;
    if (!host) return undefined;

    const send = () => {
      const rect = host.getBoundingClientRect();
      const bounds = {
        x: rect.left,
        y: rect.top + headerHeightPx,
        width: rect.width,
        height: Math.max(50, rect.height - headerHeightPx),
      };
      const key = `${bounds.x.toFixed(1)}:${bounds.y.toFixed(1)}:${bounds.width.toFixed(1)}:${bounds.height.toFixed(1)}`;
      if (key === lastBoundsRef.current) return;
      lastBoundsRef.current = key;
      void commands.updateSigdocsCoverBounds(bounds).catch(() => {
        /* silent */
      });
    };

    // Primeira atualização imediata.
    send();

    // ResizeObserver — qualquer mudança no layout dispara reposicionamento.
    const ro = new ResizeObserver(() => send());
    ro.observe(host);

    // Window resize / scroll — também afeta clientRect.
    window.addEventListener("resize", send);
    window.addEventListener("scroll", send, true);

    return () => {
      ro.disconnect();
      window.removeEventListener("resize", send);
      window.removeEventListener("scroll", send, true);
    };
  }, [coverOpen, headerHeightPx]);

  const handleClose = async () => {
    setCoverOpen(false);
    try {
      await commands.closeSigdocsCover();
    } catch {
      /* silent */
    }
  };

  if (!coverOpen) return null;

  return (
    <div ref={hostRef} className={styles.host}>
      <header className={styles.head} style={{ height: headerHeightPx }}>
        <span className={styles.title}>
          <Landmark size={13} /> SIGDOC — Estado do Amapá
        </span>
        <span className={styles.hint}>
          Ctrl+V não funciona no SIGDOC — arraste o PDF do Explorer (que já
          foi aberto na pasta correta).
        </span>
        <button
          type="button"
          className={styles.closeBtn}
          onClick={() => void handleClose()}
          aria-label="Fechar SIGDOC"
          title="Fechar SIGDOC e voltar para o laudo"
        >
          <X size={13} /> Fechar
        </button>
      </header>
      <div className={styles.coverArea}>
        {/* O webview borderless do Tauri sobrepõe esta área. */}
      </div>
    </div>
  );
}
