import { useEffect, useState } from "react";
import { Clapperboard, X } from "lucide-react";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import type { BordaEstab, EstadoEstabilizacao } from "@domain/video_realce";
import { useRealceStore } from "./realceStore";
import type { CtxRealce } from "./tipos";
import styles from "./Realce.module.css";

export function EstabTab({ workspacePath, media, onVideoDerivado }: CtxRealce) {
  const s = useRealceStore();
  const [motor, setMotor] = useState<EstadoEstabilizacao | null>(null);
  const [suavizar, setSuavizar] = useState(15);
  const [borda, setBorda] = useState<BordaEstab>("cortar");
  const [feito, setFeito] = useState<{ motor: string; nome: string } | null>(null);
  const t = s.tarefa;
  const emEstab = t?.fase === "estab-analise" || t?.fase === "estab-grava";
  const pct = t && t.total > 0 ? Math.round((t.feitos / t.total) * 100) : 0;

  useEffect(() => {
    void commands.realceEstabilizacaoDisponivel().then(setMotor).catch(() => setMotor(null));
  }, []);

  const gerar = async () => {
    setFeito(null);
    const r = await s.executar("estab-analise", () => commands.realceEstabilizar(workspacePath, media.sha256, { suavizar, borda }));
    if (!r) return;
    setFeito({ motor: r.motor, nome: r.media.filename });
    void s.recarregar(workspacePath, media.sha256);
    onVideoDerivado(r.media);
  };

  return (
    <section className={styles.sec}>
      <div className={styles.line}>
        <label>
          Suavizar
          <input type="range" min={1} max={60} step={1} value={suavizar} disabled={s.tarefa != null} onChange={(e) => setSuavizar(Number(e.target.value))} />
          <code>{suavizar}</code>
        </label>
      </div>
      <div className={styles.line}>
        <div className={styles.seg} role="group" aria-label="Borda">
          <button type="button" aria-pressed={borda === "cortar"} onClick={() => setBorda("cortar")} title="Amplia a imagem até a borda preta sumir (perde os cantos)">Cortar a borda</button>
          <button type="button" aria-pressed={borda === "preencher"} onClick={() => setBorda("preencher")} title="Mantém o enquadramento e preenche o que faltou com o quadro vizinho">Preencher</button>
        </div>
      </div>
      {motor && !motor.vidstab && (
        <div className={styles.dim}>
          {motor.deshake ? "Este ffmpeg não tem o vidstab: a estabilização usa o deshake, mais simples." : "Este ffmpeg não tem filtro de estabilização."}
        </div>
      )}
      {emEstab ? (
        <>
          <div className={styles.bar}>
            <div className={styles.barFill} style={{ width: `${pct}%` }} />
            <span className={styles.barTxt}>{t!.fase === "estab-analise" ? "analisando o movimento" : "gravando a cópia"} · {pct}%</span>
          </div>
          <Button variant="secondary" size="sm" leftIcon={<X size={13} />} onClick={s.cancelar}>Cancelar</Button>
        </>
      ) : (
        <Button variant="primary" leftIcon={<Clapperboard size={14} />} disabled={s.tarefa != null || (motor != null && !motor.vidstab && !motor.deshake)} onClick={() => void gerar()}>
          Gerar cópia estabilizada
        </Button>
      )}
      {feito && <div className={styles.resumo}><strong>{feito.nome}</strong> <span className={styles.dim}>({feito.motor})</span></div>}
      <div className={styles.dim}>A cópia é recomprimida e a borda é cortada ou preenchida: serve para ler a cena, não substitui o vídeo original.</div>
    </section>
  );
}
