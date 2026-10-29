/**
 * Palco da aba Realce: o quadro de trabalho (PNG exato do ffmpeg) para marcar a região, os pontos do
 * plano e a grade da lente; ou a comparação original × realce quando há resultado.
 */
import { useMemo } from "react";
import { ArrowLeft, Columns2, Image as ImageIcon } from "lucide-react";
import type { VideoMedia } from "@domain/video";
import { FrameCanvas, type FrameGuide, type FrameMarker } from "../speed/FrameCanvas";
import { formatDuration } from "../format";
import { CompareView } from "./CompareView";
import { assetSrc, useRealceStore } from "./realceStore";
import styles from "./Realce.module.css";

const GOLD = "#d7a84f";

export function RealceStage({ media, onVoltar }: { media: VideoMedia; onVoltar: () => void }) {
  const s = useRealceStore();
  const { quadro, aba } = s;

  const noLente = aba === "lente" && s.lentePrevia != null;
  const src = quadro ? assetSrc(noLente ? s.lentePrevia!.absoluto : quadro.absoluto) : null;
  const dims = noLente ? { w: s.lentePrevia!.largura, h: s.lentePrevia!.altura } : { w: quadro?.largura ?? media.width ?? 1280, h: quadro?.altura ?? media.height ?? 720 };

  const { markers, polyline, closed, guides, hint, onAddPoint } = useMemo(() => {
    const markers: FrameMarker[] = [];
    let polyline: { x: number; y: number }[] | undefined;
    let closed = false;
    const guides: FrameGuide[] = [];
    let hint: string | null = null;
    let onAddPoint: ((x: number, y: number) => void) | undefined;
    if (aba === "media" || aba === "superres") {
      if (s.roi) {
        const r = s.roi;
        polyline = [
          { x: r.x, y: r.y },
          { x: r.x + r.w, y: r.y },
          { x: r.x + r.w, y: r.y + r.h },
          { x: r.x, y: r.y + r.h },
        ];
        closed = true;
        hint = `Região ${r.w}×${r.h} px · clique para marcar outra`;
      } else if (s.cantos.length === 1) {
        markers.push({ ...s.cantos[0]!, color: GOLD, label: "1" });
        hint = "Clique no canto oposto da região";
      } else {
        hint = aba === "media" ? "Clique num canto da região (placa, rosto, objeto)" : "Clique num canto da região, ou deixe o quadro inteiro";
      }
      onAddPoint = (x, y) => {
        const st = useRealceStore.getState();
        if (st.roi || st.cantos.length >= 2) st.limparRegiao();
        useRealceStore.getState().addCanto({ x, y });
      };
    } else if (aba === "lente" && s.grade) {
      const { w, h } = dims;
      const passo = Math.max(24, Math.round(w / 14));
      for (let x = passo; x < w; x += passo) guides.push({ points: [x, 0, x, h], color: "rgba(34,211,238,0.55)", width: 1 });
      for (let y = passo; y < h; y += passo) guides.push({ points: [0, y, w, y], color: "rgba(34,211,238,0.55)", width: 1 });
    } else if (aba === "perspectiva") {
      const pts = s.persp.pts;
      pts.forEach((p, i) => markers.push({ ...p, color: GOLD, label: String(i + 1) }));
      polyline = pts;
      closed = pts.length === 4;
      const dica = ["1: canto perto à esquerda", "2: canto perto à direita", "3: canto longe à direita", "4: canto longe à esquerda"];
      hint = pts.length < 4 ? `Clique no ponto ${dica[pts.length]}` : "Os 4 pontos estão marcados";
      onAddPoint = (x, y) => {
        const st = useRealceStore.getState();
        const atual = st.persp.pts;
        st.patchPersp({ pts: atual.length >= 4 ? [{ x, y }] : [...atual, { x, y }], calId: null });
      };
    }
    return { markers, polyline, closed, guides, hint, onAddPoint };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [aba, s.roi, s.cantos, s.grade, s.persp.pts, dims.w, dims.h]);

  if (!quadro) return null;
  const mostraResultado = s.vista === "resultado" && s.comparacao;

  return (
    <div className={styles.stage}>
      <div className={styles.stageBar}>
        <button type="button" onClick={onVoltar} title="Voltar ao vídeo para escolher outro quadro">
          <ArrowLeft size={13} aria-hidden /> Vídeo
        </button>
        <span className={styles.stageT}>
          quadro {formatDuration(quadro.t)}
          {quadro.com_lente ? " · lente corrigida" : ""}
        </span>
        <span style={{ flex: 1 }} />
        {s.comparacao && (
          <div className={styles.seg}>
            <button type="button" aria-pressed={s.vista === "quadro"} onClick={() => s.setVista("quadro")}>
              <ImageIcon size={12} aria-hidden /> Quadro
            </button>
            <button type="button" aria-pressed={s.vista === "resultado"} onClick={() => s.setVista("resultado")}>
              <Columns2 size={12} aria-hidden /> Original × realce
            </button>
          </div>
        )}
      </div>
      <div className={styles.canvasBox}>
        {mostraResultado ? (
          <CompareView c={s.comparacao!} />
        ) : (
          <FrameCanvas
            src={src}
            naturalWidth={dims.w}
            naturalHeight={dims.h}
            markers={markers}
            polyline={polyline}
            closed={closed}
            guides={guides}
            hint={hint}
            lupa={!!onAddPoint}
            onAddPoint={onAddPoint}
          />
        )}
      </div>
    </div>
  );
}
