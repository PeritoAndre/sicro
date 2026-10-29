import { FolderOpen, Images, Wand2, X } from "lucide-react";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { revealExported } from "@core/reveal";
import { useVideoStore } from "../../store/videoStore";
import { useState } from "react";
import { assetSrc, useRealceStore } from "./realceStore";
import type { CtxRealce } from "./tipos";
import styles from "./Realce.module.css";

export function MediaTab({ workspacePath, media }: CtxRealce) {
  const s = useRealceStore();
  const addFrames = useVideoStore((v) => v.addStoryboardFrames);
  const [quadros, setQuadros] = useState(20);
  const [mediana, setMediana] = useState(false);
  const [homografia, setHomografia] = useState(false);
  const [ultima, setUltima] = useState<{ usados: number; total: number; tempo: number; descartados: number[] } | null>(null);
  const ocupado = s.tarefa != null;
  const pct = s.tarefa && s.tarefa.total > 0 ? Math.round((s.tarefa.feitos / s.tarefa.total) * 100) : 0;

  const calcular = async () => {
    const q = s.quadro;
    if (!q || !s.roi) return;
    const r = await s.executar("ler", () =>
      commands.realceMedia(workspacePath, media.sha256, {
        t: q.t,
        roi: s.roi!,
        quadros,
        mediana,
        homografia,
        com_lente: s.usarLente && s.temPerfil,
      }),
    );
    if (!r) return;
    setUltima({ usados: r.usados, total: r.total, tempo: r.tempo_s, descartados: r.quadros.filter((x) => !x.aceito).map((x) => x.indice + 1) });
    s.setUltimaMedia(r.roi);
    s.setComparacao({
      esquerda: { src: assetSrc(r.crua), rotulo: "Quadro cru" },
      direita: { src: assetSrc(r.roi.absoluto), rotulo: `Realce · ${mediana ? "mediana" : "média"} de ${r.usados} quadros alinhados` },
      nota: `${r.usados} de ${r.total} quadros entraram · ${r.tempo_s.toFixed(1).replace(".", ",")} s`,
    });
    void s.recarregar(workspacePath, media.sha256);
  };

  const guardar = async () => {
    const m = s.ultimaMedia;
    if (!m) return;
    try {
      const f = await commands.realceParaStoryboard(workspacePath, media.sha256, m.resultado.id, "Realce: média de quadros");
      addFrames([f]);
      s.setMsg("A região entrou no storyboard.");
    } catch (e) {
      s.setErro(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <section className={styles.sec}>
      <div className={styles.passo}>
        {s.roi ? (
          <>
            Região <code>{s.roi.w}×{s.roi.h}</code> px
            <button type="button" className={styles.link} onClick={s.limparRegiao}>marcar outra</button>
          </>
        ) : (
          <span className={styles.dim}>Marque no quadro a região de interesse (dois cliques).</span>
        )}
      </div>
      <div className={styles.line}>
        <label>
          Quadros em volta
          <input type="range" min={5} max={60} step={1} value={quadros} disabled={ocupado} onChange={(e) => setQuadros(Number(e.target.value))} />
          <code>{quadros}</code>
        </label>
      </div>
      <div className={styles.line}>
        <div className={styles.seg} role="group" aria-label="Combinação">
          <button type="button" aria-pressed={!mediana} onClick={() => setMediana(false)} title="Mais limpa quando o ruído é parecido em todos os quadros">Média</button>
          <button type="button" aria-pressed={mediana} onClick={() => setMediana(true)} title="Ignora o que aparece só em alguns quadros (reflexo, passante)">Mediana</button>
        </div>
      </div>
      <label className={styles.check}>
        <input type="checkbox" checked={homografia} disabled={ocupado} onChange={(e) => setHomografia(e.target.checked)} />
        Alinhar também giro e mudança de escala
      </label>
      {s.tarefa && (s.tarefa.fase === "ler" || s.tarefa.fase === "alinhar") ? (
        <>
          <div className={styles.bar}>
            <div className={styles.barFill} style={{ width: `${pct}%` }} />
            <span className={styles.barTxt}>{s.tarefa.fase === "ler" ? "lendo os quadros…" : `alinhando ${s.tarefa.feitos} / ${s.tarefa.total}`}</span>
          </div>
          <Button variant="secondary" size="sm" leftIcon={<X size={13} />} onClick={s.cancelar}>Cancelar</Button>
        </>
      ) : (
        <Button variant="primary" leftIcon={<Wand2 size={14} />} disabled={!s.roi || !s.quadro || ocupado} onClick={() => void calcular()}>
          Calcular a {mediana ? "mediana" : "média"}
        </Button>
      )}
      {ultima && (
        <div className={styles.resumo}>
          <strong>{ultima.usados} de {ultima.total}</strong> quadros entraram
          {ultima.descartados.length > 0 && <span className={styles.dim}> · fora: {ultima.descartados.join(", ")}</span>}
          <span className={styles.dim}> · {ultima.tempo.toFixed(1).replace(".", ",")} s</span>
        </div>
      )}
      {s.ultimaMedia && (
        <div className={styles.line}>
          <Button variant="secondary" size="sm" leftIcon={<Images size={13} />} onClick={() => void guardar()}>Guardar no storyboard</Button>
          <Button variant="ghost" size="sm" leftIcon={<FolderOpen size={13} />} onClick={() => revealExported(workspacePath, s.ultimaMedia!.resultado.caminho)}>Mostrar na pasta</Button>
        </div>
      )}
    </section>
  );
}
