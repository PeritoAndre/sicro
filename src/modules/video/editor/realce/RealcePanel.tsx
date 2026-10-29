/**
 * Aba Realce: ferramentas para ler melhor a imagem (média de quadros alinhados, super-resolução,
 * estabilização, lente e perspectiva). Nada substitui o vídeo original; cada saída vira arquivo
 * novo, com SHA-256 e parâmetros, listado no fim.
 */
import { useCallback, useEffect } from "react";
import { FolderOpen, Images } from "lucide-react";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { revealExported } from "@core/reveal";
import type { VideoMedia } from "@domain/video";
import { useVideoStore } from "../../store/videoStore";
import { formatDuration } from "../format";
import { EstabTab } from "./EstabTab";
import { LenteTab } from "./LenteTab";
import { MediaTab } from "./MediaTab";
import { PerspectivaTab } from "./PerspectivaTab";
import { SuperResTab } from "./SuperResTab";
import { useRealceStore, type Aba } from "./realceStore";
import type { CtxRealce } from "./tipos";
import styles from "./Realce.module.css";

const ABAS: { id: Aba; rotulo: string; precisaQuadro: boolean }[] = [
  { id: "media", rotulo: "Média", precisaQuadro: true },
  { id: "superres", rotulo: "Super-res.", precisaQuadro: false },
  { id: "estab", rotulo: "Estabilizar", precisaQuadro: false },
  { id: "lente", rotulo: "Lente", precisaQuadro: true },
  { id: "perspectiva", rotulo: "Perspectiva", precisaQuadro: true },
];

const ROTULO_TIPO: Record<string, string> = {
  media: "Média (região)",
  media_quadro: "Média (quadro inteiro)",
  superres: "Super-resolução",
  retificada: "Vista de cima",
  quadro_lente: "Quadro com lente corrigida",
  estabilizado: "Cópia estabilizada",
  lente_video: "Cópia com lente corrigida",
};

interface Props {
  workspacePath: string;
  media: VideoMedia;
  currentTime: number;
  onVideoDerivado: (media: VideoMedia) => void;
}

export function RealcePanel({ workspacePath, media, currentTime, onVideoDerivado }: Props) {
  const s = useRealceStore();
  const addFrames = useVideoStore((v) => v.addStoryboardFrames);

  useEffect(() => {
    s.bind(media.sha256);
    void s.recarregar(workspacePath, media.sha256);
    void commands
      .realceLenteObter(workspacePath, media.sha256)
      .then((p) => useRealceStore.getState().setTemPerfil(p != null))
      .catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspacePath, media.sha256]);

  const usarQuadro = useCallback(
    async (t: number) => {
      const st = useRealceStore.getState();
      const q = await st.executar("trabalho", () =>
        commands.realceQuadro(workspacePath, media.sha256, t, st.usarLente && st.temPerfil),
      );
      if (q) {
        // mesmo instante (só trocou a lente): mantém a região e o que já foi marcado
        const mesmo = st.quadro && Math.abs(st.quadro.t - q.t) < 1e-6;
        if (mesmo) useRealceStore.setState({ quadro: q });
        else st.setQuadro(q);
      }
    },
    [workspacePath, media.sha256],
  );

  const ctx: CtxRealce = { workspacePath, media, currentTime, onVideoDerivado, usarQuadro };
  const q = s.quadro;
  const aba = ABAS.find((a) => a.id === s.aba)!;
  const aviso = aba.precisaQuadro && !q;

  const paraStoryboard = async (id: string) => {
    try {
      const f = await commands.realceParaStoryboard(workspacePath, media.sha256, id);
      addFrames([f]);
      s.setMsg("Entrou no storyboard.");
    } catch (e) {
      s.setErro(toSicroError(e).message);
    }
  };

  return (
    <div className={styles.panel}>
      <section className={styles.sec}>
        <div className={styles.head}>
          <h3 className={styles.title}>Realce</h3>
          {s.temPerfil && (
            <label className={styles.check} title="Aplica ao quadro e ao realce a correção de lente salva para este vídeo">
              <input
                type="checkbox"
                checked={s.usarLente}
                onChange={(e) => {
                  s.setUsarLente(e.target.checked);
                  if (q) void usarQuadro(q.t);
                }}
              />
              lente corrigida
            </label>
          )}
        </div>
        <div className={styles.line}>
          <Button variant="primary" size="sm" disabled={s.tarefa != null} onClick={() => void usarQuadro(currentTime)}>
            {q ? "Trocar para o quadro atual" : "Usar o quadro atual"} · {formatDuration(currentTime)}
          </Button>
        </div>
      </section>

      <nav className={styles.abas} aria-label="Ferramentas de realce">
        {ABAS.map((a) => (
          <button key={a.id} type="button" aria-pressed={s.aba === a.id} onClick={() => s.setAba(a.id)}>
            {a.rotulo}
          </button>
        ))}
      </nav>

      {aviso ? (
        <section className={styles.sec}>
          <span className={styles.dim}>Pause no instante e clique em “Usar o quadro atual”.</span>
        </section>
      ) : s.aba === "media" ? (
        <MediaTab {...ctx} />
      ) : s.aba === "superres" ? (
        <SuperResTab {...ctx} />
      ) : s.aba === "estab" ? (
        <EstabTab {...ctx} />
      ) : s.aba === "lente" ? (
        <LenteTab {...ctx} />
      ) : (
        <PerspectivaTab {...ctx} />
      )}

      {s.erro && <div className={`${styles.sec} ${styles.error}`}>{s.erro}</div>}
      {s.msg && <div className={`${styles.sec} ${styles.dim}`}>{s.msg}</div>}
      {s.tarefa?.fase === "trabalho" && <div className={`${styles.sec} ${styles.dim}`}>trabalhando…</div>}

      <section className={styles.sec}>
        <h3 className={styles.title}>Resultados{s.resultados.length > 0 ? ` · ${s.resultados.length}` : ""}</h3>
        {s.resultados.length === 0 ? (
          <span className={styles.dim}>Nada gerado para este vídeo ainda.</span>
        ) : (
          <ul className={styles.lista}>
            {s.resultados.map((a) => {
              const r = a.resultado;
              const imagem = r.caminho.toLowerCase().endsWith(".png");
              return (
                <li key={r.id} className={styles.item}>
                  <div className={styles.itemMain}>
                    <strong>{ROTULO_TIPO[r.tipo] ?? r.tipo}</strong>
                    <span className={styles.dim}>
                      {new Date(r.created_at).toLocaleString("pt-BR")}
                      {r.largura && r.altura ? ` · ${r.largura}×${r.altura}` : ""}
                    </span>
                    <code title={r.sha256}>SHA {r.sha256.slice(0, 12)}…</code>
                  </div>
                  <div className={styles.itemAcoes}>
                    {imagem && (
                      <button type="button" title="Levar ao storyboard" onClick={() => void paraStoryboard(r.id)}>
                        <Images size={13} aria-hidden />
                      </button>
                    )}
                    <button type="button" title="Mostrar na pasta" onClick={() => revealExported(workspacePath, r.caminho)}>
                      <FolderOpen size={13} aria-hidden />
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
        <div className={styles.dim}>Realce não substitui o original: cada arquivo guarda o SHA-256, os parâmetros e o aviso ao lado.</div>
      </section>
    </div>
  );
}
