import { useEffect, useMemo, useState } from "react";
import { FolderOpen, Grid3x3 } from "lucide-react";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { revealExported } from "@core/reveal";
import type { ParIn } from "@domain/video_realce";
import { useVideoStore } from "../../store/videoStore";
import { assetSrc, useRealceStore } from "./realceStore";
import type { CtxRealce } from "./tipos";
import styles from "./Realce.module.css";

const num = (t: string): number => Number(t.replace(",", "."));

export function PerspectivaTab({ workspacePath, media }: CtxRealce) {
  const s = useRealceStore();
  const cals = useVideoStore((v) => v.speedCalibrations);
  const loadSpeedData = useVideoStore((v) => v.loadSpeedData);
  const [ultimo, setUltimo] = useState<{ caminho: string; ppm: number } | null>(null);
  const p = s.persp;
  const q = s.quadro;

  useEffect(() => {
    void loadSpeedData(workspacePath, media.sha256);
  }, [workspacePath, media.sha256, loadSpeedData]);

  const planas = useMemo(() => cals.filter((c) => c.method === "plane" && c.control_points.length === 4), [cals]);
  const cal = planas.find((c) => c.id === p.calId) ?? null;

  const L = num(p.largura);
  const D = num(p.profundidade);
  const manualOk = p.pts.length === 4 && L > 0 && D > 0;
  const pronto = !!q && (cal ? true : manualOk);

  const escolherCal = (id: string) => {
    const c = planas.find((x) => x.id === id);
    if (!c) return s.patchPersp({ calId: null });
    s.patchPersp({ calId: id, pts: c.control_points.map((cp) => ({ x: cp.px, y: cp.py })) });
  };

  const gerar = async () => {
    if (!q) return;
    let pontos: ParIn[];
    if (cal) {
      pontos = cal.control_points.map((cp) => ({ px: cp.px, py: cp.py, mx: cp.world_x_m, my: cp.world_y_m }));
    } else {
      const m: [number, number][] = [[0, 0], [L, 0], [L, D], [0, D]];
      pontos = p.pts.map((pt, i) => ({ px: pt.x, py: pt.y, mx: m[i]![0], my: m[i]![1] }));
    }
    const r = await s.executar("trabalho", () =>
      commands.realcePerspectiva(workspacePath, media.sha256, {
        t: q.t,
        com_lente: s.usarLente && s.temPerfil,
        pontos,
        margem: p.margem,
        px_por_m: null,
        origem: cal ? `calibracao:${cal.id}` : "manual",
      }),
    );
    if (!r) return;
    setUltimo({ caminho: r.arquivo.resultado.caminho, ppm: r.info.px_por_m });
    s.setComparacao({
      esquerda: { src: assetSrc(q.absoluto), rotulo: "Quadro" },
      direita: { src: assetSrc(r.arquivo.absoluto), rotulo: "Realce · vista de cima" },
      nota: `${r.info.largura}×${r.info.altura} px · 1 m = ${r.info.px_por_m.toFixed(0)} px`,
    });
    void s.recarregar(workspacePath, media.sha256);
  };

  return (
    <section className={styles.sec}>
      {planas.length > 0 && (
        <label className={styles.line}>
          <span className={styles.dim}>Usar a calibração da Velocidade</span>
          <select value={p.calId ?? ""} onChange={(e) => escolherCal(e.target.value)}>
            <option value="">não, marcar aqui</option>
            {planas.map((c) => (
              <option key={c.id} value={c.id}>{new Date(c.created_at).toLocaleDateString("pt-BR")} · {c.author}</option>
            ))}
          </select>
        </label>
      )}
      {!cal && (
        <>
          <div className={styles.passo}>
            {p.pts.length < 4 ? <span className={styles.dim}>Marque no quadro os 4 cantos de um retângulo do plano, na ordem 1 → 4.</span> : <>4 pontos marcados <button type="button" className={styles.link} onClick={() => s.patchPersp({ pts: [] })}>refazer</button></>}
          </div>
          <div className={styles.line}>
            <label>
              Lado 1→2 (m)
              <input className={styles.num} inputMode="decimal" value={p.largura} onChange={(e) => s.patchPersp({ largura: e.target.value })} placeholder="3,50" />
            </label>
            <label>
              Lado 2→3 (m)
              <input className={styles.num} inputMode="decimal" value={p.profundidade} onChange={(e) => s.patchPersp({ profundidade: e.target.value })} placeholder="6,00" />
            </label>
          </div>
        </>
      )}
      {cal && <div className={styles.dim}>Pontos e medidas da calibração ({cal.control_points.length} pontos).</div>}
      <label className={styles.line}>
        <span className={styles.dim}>Margem em volta</span>
        <input type="range" min={0} max={1} step={0.05} value={p.margem} onChange={(e) => s.patchPersp({ margem: Number(e.target.value) })} />
        <code>{Math.round(p.margem * 100)}%</code>
      </label>
      <Button variant="primary" leftIcon={<Grid3x3 size={14} />} disabled={!pronto || s.tarefa != null} onClick={() => void gerar()}>
        Gerar a vista de cima
      </Button>
      {ultimo && (
        <div className={styles.line}>
          <span className={styles.dim}>1 m = {ultimo.ppm.toFixed(0)} px</span>
          <Button variant="ghost" size="sm" leftIcon={<FolderOpen size={13} />} onClick={() => revealExported(workspacePath, ultimo.caminho)}>Mostrar na pasta</Button>
        </div>
      )}
      <div className={styles.dim}>Vale só para o que está no plano marcado: tudo que tem altura (pessoas, carros) fica deformado.</div>
    </section>
  );
}
