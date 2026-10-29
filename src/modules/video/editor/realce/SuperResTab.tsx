import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { FolderOpen, Sparkles, X } from "lucide-react";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { revealExported } from "@core/reveal";
import { assetSrc, useRealceStore } from "./realceStore";
import type { CtxRealce } from "./tipos";
import styles from "./Realce.module.css";

const AVISO = "Realce por rede neural: pode inventar detalhe; não usar para identificação sem o original ao lado.";

export function SuperResTab({ workspacePath, media }: CtxRealce) {
  const navigate = useNavigate();
  const s = useRealceStore();
  const [pronto, setPronto] = useState<boolean | null>(null);
  const [escala, setEscala] = useState<2 | 4>(2);
  const [fonte, setFonte] = useState<"media" | "quadro">("media");
  const [ultimo, setUltimo] = useState<{ id: string; caminho: string; tempo: number; w: number; h: number } | null>(null);
  const ocupado = s.tarefa != null;
  const pct = s.tarefa && s.tarefa.total > 0 ? Math.round((s.tarefa.feitos / s.tarefa.total) * 100) : 0;

  useEffect(() => {
    void commands.getAiStatus().then((a) => setPronto(a.superres_ok)).catch(() => setPronto(false));
  }, []);

  const deMedia = fonte === "media" && s.ultimaMedia != null;
  const q = s.quadro;
  const pxEntrada = deMedia
    ? (s.ultimaMedia!.resultado.largura ?? 0) * (s.ultimaMedia!.resultado.altura ?? 0)
    : s.roi
      ? s.roi.w * s.roi.h
      : (q?.largura ?? 0) * (q?.altura ?? 0);
  const grande = pxEntrada * escala * escala * (escala === 2 ? 4 : 1) > 64_000_000;
  const lento = pxEntrada > 600_000;

  const ampliar = async () => {
    if (!q && !deMedia) return;
    const r = await s.executar("superres", () =>
      commands.realceSuperres(workspacePath, media.sha256, {
        resultado_id: deMedia ? s.ultimaMedia!.resultado.id : null,
        t: q?.t ?? 0,
        roi: deMedia ? null : s.roi,
        escala,
        com_lente: s.usarLente && s.temPerfil,
      }),
    );
    if (!r) return;
    const res = r.arquivo.resultado;
    setUltimo({ id: res.id, caminho: res.caminho, tempo: r.tempo_s, w: res.largura ?? 0, h: res.altura ?? 0 });
    s.setComparacao({
      esquerda: { src: assetSrc(r.original), rotulo: "Original" },
      direita: { src: assetSrc(r.arquivo.absoluto), rotulo: `Realce · super-resolução ${escala}x` },
      nota: `${res.largura}×${res.altura} px · ${r.tempo_s.toFixed(1).replace(".", ",")} s`,
      aviso: res.aviso || AVISO,
    });
    void s.recarregar(workspacePath, media.sha256);
  };

  if (pronto === false) {
    return (
      <section className={styles.sec}>
        <Button variant="primary" leftIcon={<Sparkles size={14} />} onClick={() => navigate("/configuracoes?aba=iaocr")}>
          Baixar no Gerenciador de IA
        </Button>
      </section>
    );
  }

  return (
    <section className={styles.sec}>
      {s.ultimaMedia && (
        <div className={styles.seg} role="group" aria-label="Sobre o quê">
          <button type="button" aria-pressed={fonte === "media"} onClick={() => setFonte("media")}>Sobre a média</button>
          <button type="button" aria-pressed={fonte === "quadro"} onClick={() => setFonte("quadro")}>Sobre o quadro</button>
        </div>
      )}
      {!deMedia && (
        <div className={styles.passo}>
          {s.roi ? <>Região <code>{s.roi.w}×{s.roi.h}</code> px</> : <span className={styles.dim}>Sem região: o quadro inteiro. Marque uma região no quadro para ser rápido.</span>}
        </div>
      )}
      <div className={styles.line}>
        <div className={styles.seg} role="group" aria-label="Escala">
          <button type="button" aria-pressed={escala === 2} onClick={() => setEscala(2)}>2x</button>
          <button type="button" aria-pressed={escala === 4} onClick={() => setEscala(4)}>4x</button>
        </div>
      </div>
      {lento && !grande && <div className={styles.dim}>Imagem grande: pode levar alguns minutos na CPU.</div>}
      {grande && <div className={styles.error}>Grande demais para ampliar: marque uma região menor.</div>}
      {s.tarefa?.fase === "superres" ? (
        <>
          <div className={styles.bar}>
            <div className={styles.barFill} style={{ width: `${pct}%` }} />
            <span className={styles.barTxt}>ladrilho {s.tarefa.feitos} / {s.tarefa.total || "…"}</span>
          </div>
          <Button variant="secondary" size="sm" leftIcon={<X size={13} />} onClick={s.cancelar}>Cancelar</Button>
        </>
      ) : (
        <Button variant="primary" leftIcon={<Sparkles size={14} />} disabled={ocupado || grande || (!q && !deMedia) || pronto == null} onClick={() => void ampliar()}>
          Ampliar {escala}x
        </Button>
      )}
      <div className={styles.avisoRede}>{AVISO}</div>
      {ultimo && (
        <div className={styles.line}>
          <Button variant="ghost" size="sm" leftIcon={<FolderOpen size={13} />} onClick={() => revealExported(workspacePath, ultimo.caminho)}>Mostrar na pasta</Button>
        </div>
      )}
    </section>
  );
}
