/** Painel da aba Pessoas: rastrear, lista com edição, opções de desenho e saídas. */
import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  ArrowDown,
  ArrowUp,
  Crosshair,
  Film,
  Image as ImageIcon,
  Images,
  Merge,
  Split,
  Table2,
  Trash2,
  X,
} from "lucide-react";
import { Button } from "@components/Button/Button";
import { askConfirm } from "@components/Dialog/ask";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { revealExported } from "@core/reveal";
import type { VideoEvent, VideoMedia } from "@domain/video";
import type { ArquivoGerado, PessoaView } from "@domain/video_pessoas";
import { useVideoStore } from "../../store/videoStore";
import { formatDuration } from "../format";
import { corContraste } from "./drawPessoa";
import { visiveisEm } from "./pessoasMath";
import { cadenciaDe, usePessoasStore } from "./pessoasStore";
import styles from "./PessoasPanel.module.css";

interface Props {
  workspacePath: string;
  media: VideoMedia;
  currentTime: number;
  events: VideoEvent[];
  onSeek: (t: number) => void;
  hasAudio: boolean;
}

const PASSOS = [
  { v: 1, rotulo: "Todos os quadros" },
  { v: 2, rotulo: "1 a cada 2" },
  { v: 4, rotulo: "1 a cada 4" },
];

/** Limiar usado na última rodada, lido do params_json gravado pelo Rust. */
function limiarDoRun(run: { params_json: string }): string | null {
  try {
    const v = JSON.parse(run.params_json).limiar;
    return typeof v === "number" ? v.toFixed(2) : null;
  } catch {
    return null;
  }
}

export function PessoasPanel({ workspacePath, media, currentTime, events, onSeek, hasAudio }: Props) {
  const navigate = useNavigate();
  const st = usePessoasStore();
  const addFrames = useVideoStore((s) => s.addStoryboardFrames);
  const [pronto, setPronto] = useState<boolean | null>(null);
  const [soVisiveis, setSoVisiveis] = useState(false);
  const [editandoNum, setEditandoNum] = useState<string | null>(null);
  const [comAudio, setComAudio] = useState(true);
  const [saidas, setSaidas] = useState<{ rotulo: string; arq: ArquivoGerado }[]>([]);
  const [msg, setMsg] = useState<string | null>(null);
  const hash = media.sha256;

  useEffect(() => {
    void usePessoasStore.getState().carregar(workspacePath, hash);
    setSaidas([]);
    setMsg(null);
  }, [workspacePath, hash]);

  useEffect(() => {
    void commands
      .getAiStatus()
      .then((s) => setPronto(s.pessoas_ok))
      .catch(() => setPronto(false));
  }, []);

  const estado = st.estado;
  const cad = cadenciaDe(estado);
  const pessoas = estado?.pessoas ?? [];
  const visiveis = useMemo(
    () => new Set(cad ? visiveisEm(pessoas, currentTime, cad).map((p) => p.id) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [pessoas, currentTime, cad?.frameDur, cad?.passo],
  );
  const lista = soVisiveis ? pessoas.filter((p) => visiveis.has(p.id)) : pessoas;
  const ocupado = st.tarefa != null;

  const editar = (e: Parameters<typeof st.editar>[2]) => st.editar(workspacePath, hash, e);

  const rastrear = async () => {
    if (pronto === false) {
      navigate("/configuracoes?aba=iaocr");
      return;
    }
    if (pessoas.length > 0) {
      const editadas = pessoas.some((p) => p.estado === "editado");
      const ok = await askConfirm({
        title: "Rastrear de novo?",
        message: editadas
          ? "As pessoas atuais, com as suas edições, serão substituídas."
          : "As pessoas atuais serão substituídas.",
        confirmLabel: "Rastrear",
        danger: editadas,
      });
      if (!ok) return;
    }
    setSaidas([]);
    await st.rastrear(workspacePath, hash);
  };

  const apagar = async (p: PessoaView) => {
    const ok = await askConfirm({
      title: `Apagar a pessoa ${p.numero}?`,
      message: "Use para falso positivo. As outras mantêm os números.",
      confirmLabel: "Apagar",
      danger: true,
    });
    if (ok) await editar({ op: "apagar", id: p.id });
  };

  const mover = (p: PessoaView, dir: -1 | 1) => {
    const ids = pessoas.map((x) => x.id);
    const i = ids.indexOf(p.id);
    const j = i + dir;
    if (j < 0 || j >= ids.length) return;
    [ids[i], ids[j]] = [ids[j]!, ids[i]!];
    void editar({ op: "ordem", ids });
  };

  const executar = async (rotulo: string, fn: () => Promise<ArquivoGerado[]>) => {
    st.setErro(null);
    setMsg(null);
    try {
      const arqs = await fn();
      setSaidas((s) => [...arqs.map((arq) => ({ rotulo, arq })), ...s].slice(0, 6));
    } catch (e) {
      const m = toSicroError(e).message;
      if (!/cancelado/.test(m)) st.setErro(m);
    } finally {
      st.setTarefa(null);
    }
  };

  const gravarVideo = () =>
    executar("Vídeo", async () => {
      st.setTarefa({ fase: "exportar", feitos: 0, total: 0, pessoas: 0 });
      return [await commands.pessoasExportarVideo(workspacePath, hash, st.desenho, comAudio && hasAudio)];
    });

  const exportarQuadros = (tempos: number[], rotulo: string) =>
    executar(rotulo, async () => {
      const r = await commands.pessoasExportarQuadros(workspacePath, hash, tempos, st.desenho);
      addFrames(r.map((q) => q.frame));
      setMsg(`${r.length} ${r.length === 1 ? "quadro entrou" : "quadros entraram"} no storyboard`);
      return [];
    });

  const tabela = () =>
    executar("Tabela", async () => {
      const t = await commands.pessoasExportarTabela(workspacePath, hash);
      return [t.csv, t.html];
    });

  const tarefa = st.tarefa;
  const pct = tarefa && tarefa.total > 0 ? Math.round((tarefa.feitos / tarefa.total) * 100) : 0;
  const run = estado?.run;

  if (pronto === false && pessoas.length === 0) {
    return (
      <div className={styles.panel}>
        <section className={styles.sec}>
          <div className={styles.head}>
            <h3 className={styles.title}>Pessoas</h3>
          </div>
          <Button variant="primary" onClick={() => navigate("/configuracoes?aba=iaocr")}>
            Baixar no Gerenciador de IA
          </Button>
        </section>
      </div>
    );
  }

  return (
    <div className={styles.panel}>
      <section className={styles.sec}>
        <div className={styles.head}>
          <h3 className={styles.title}>Pessoas{pessoas.length > 0 ? ` · ${pessoas.length}` : ""}</h3>
          <label className={styles.dim} style={{ display: "inline-flex", gap: 6, alignItems: "center" }} title="Mostrar na imagem (P)">
            <input type="checkbox" checked={st.mostrar} onChange={(e) => st.setMostrar(e.target.checked)} /> na imagem
          </label>
        </div>
        <div className={styles.line}>
          <label>
            Quadros
            <select value={st.passo} onChange={(e) => st.setPasso(Number(e.target.value))} disabled={ocupado}>
              {PASSOS.map((p) => (
                <option key={p.v} value={p.v}>
                  {p.rotulo}
                </option>
              ))}
            </select>
          </label>
          <label title="Confiança mínima da detecção">
            Confiança
            <input
              type="range"
              min={0.15}
              max={0.85}
              step={0.05}
              value={st.limiar}
              disabled={ocupado}
              onChange={(e) => st.setLimiar(Number(e.target.value))}
            />
            <code>{st.limiar.toFixed(2)}</code>
          </label>
        </div>
        {tarefa?.fase === "rastrear" ? (
          <>
            <div className={styles.bar}>
              <div className={styles.barFill} style={{ width: `${pct}%` }} />
              <span className={styles.barTxt}>
                {tarefa.feitos} / {tarefa.total || "…"} · {tarefa.pessoas} na cena
              </span>
            </div>
            <Button variant="secondary" size="sm" leftIcon={<X size={13} />} onClick={st.cancelar}>
              Cancelar
            </Button>
          </>
        ) : (
          <Button variant="primary" onClick={() => void rastrear()} disabled={ocupado || pronto == null}>
            {pessoas.length > 0 ? "Rastrear de novo" : "Rastrear"}
          </Button>
        )}
        {run && !tarefa && (
          <span className={styles.dim}>
            {run.frames_analyzed} de {run.frames_total} quadros · <code>{run.elapsed_s.toFixed(0)} s</code> ·{" "}
            {run.model.split(" ")[0]}
          </span>
        )}
        {st.erro && <span className={styles.error}>{st.erro}</span>}
        {run && !tarefa && !st.erro && pessoas.length === 0 && (
          <span className={styles.dim}>
            Nenhuma pessoa encontrada com confiança {limiarDoRun(run) ?? st.limiar.toFixed(2)}. À noite, de longe ou em
            baixa resolução, baixe a confiança (0,25 a 0,35) e rastreie de novo.
          </span>
        )}
      </section>

      {pessoas.length > 0 && (
        <>
          <section className={styles.sec}>
            <div className={styles.line}>
              <div className={styles.seg} role="group" aria-label="O que desenhar">
                {(
                  [
                    ["numero", "Número"],
                    ["numero_seta", "+ seta"],
                    ["completo", "+ caixa"],
                  ] as const
                ).map(([m, r]) => (
                  <button key={m} type="button" aria-pressed={st.desenho.modo === m} onClick={() => st.setDesenho({ modo: m })}>
                    {r}
                  </button>
                ))}
              </div>
              <label title="Tamanho do número e da seta">
                Tamanho
                <input
                  type="range"
                  min={0.5}
                  max={3}
                  step={0.1}
                  value={st.desenho.fonte}
                  onChange={(e) => st.setDesenho({ fonte: Number(e.target.value) })}
                />
              </label>
            </div>
          </section>

          <section className={styles.sec}>
            <div className={styles.head}>
              <h3 className={styles.title}>Lista</h3>
              <label className={styles.dim} style={{ display: "inline-flex", gap: 6, alignItems: "center" }}>
                <input type="checkbox" checked={soVisiveis} onChange={(e) => setSoVisiveis(e.target.checked)} /> só na imagem agora
              </label>
              <button
                type="button"
                className={styles.link}
                title="Numerar na ordem em que entram em cena"
                onClick={() => {
                  const ids = [...pessoas].sort((a, b) => a.primeiro_s - b.primeiro_s).map((p) => p.id);
                  void editar({ op: "ordem", ids });
                }}
              >
                numerar por entrada
              </button>
            </div>
            {st.juntando && (
              <div className={styles.line}>
                <span className={styles.dim}>Clique na pessoa que é a mesma</span>
                <button type="button" className={styles.link} onClick={() => st.setJuntando(null)}>
                  cancelar
                </button>
              </div>
            )}
            <ul className={styles.list}>
              {lista.map((p) => (
                <li
                  key={p.id}
                  className={`${styles.row} ${st.selecionada === p.id ? styles.rowOn : ""} ${
                    visiveis.has(p.id) ? styles.rowVisible : ""
                  } ${st.juntando === p.id ? styles.rowJoin : ""}`}
                  onClick={() => {
                    if (st.juntando && st.juntando !== p.id) {
                      void editar({ op: "juntar", destino: st.juntando, origem: p.id });
                      st.setJuntando(null);
                      return;
                    }
                    st.selecionar(p.id);
                  }}
                  onDoubleClick={() => onSeek(p.primeiro_s)}
                >
                  {editandoNum === p.id ? (
                    <input
                      className={styles.numInput}
                      autoFocus
                      defaultValue={p.numero}
                      onClick={(e) => e.stopPropagation()}
                      onBlur={() => setEditandoNum(null)}
                      onKeyDown={(e) => {
                        if (e.key === "Escape") setEditandoNum(null);
                        if (e.key === "Enter") {
                          const n = Number(e.currentTarget.value);
                          setEditandoNum(null);
                          if (Number.isInteger(n) && n >= 1 && n !== p.numero) void editar({ op: "numero", id: p.id, numero: n });
                        }
                      }}
                    />
                  ) : (
                    <button
                      type="button"
                      className={styles.num}
                      style={{ background: p.cor, color: corContraste(p.cor) }}
                      title="Clique para mudar o número"
                      onClick={(e) => {
                        e.stopPropagation();
                        setEditandoNum(p.id);
                      }}
                    >
                      {p.numero}
                    </button>
                  )}
                  <input
                    className={styles.name}
                    defaultValue={p.nome}
                    key={`${p.id}-${p.nome}`}
                    placeholder="nome"
                    onClick={(e) => e.stopPropagation()}
                    onBlur={(e) => {
                      if (e.currentTarget.value.trim() !== p.nome) void editar({ op: "nome", id: p.id, nome: e.currentTarget.value });
                    }}
                    onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                  />
                  <span className={styles.acts} onClick={(e) => e.stopPropagation()}>
                    <button type="button" title="Ir para a entrada" onClick={() => onSeek(p.primeiro_s)}>
                      <Crosshair size={13} />
                    </button>
                    <button type="button" title="É a mesma pessoa que outra…" onClick={() => st.setJuntando(p.id)}>
                      <Merge size={13} />
                    </button>
                    <button
                      type="button"
                      title="Separar aqui: do instante atual em diante vira outra pessoa"
                      disabled={!(currentTime > p.primeiro_s && currentTime < p.ultimo_s)}
                      onClick={() => void editar({ op: "separar", id: p.id, t: currentTime })}
                    >
                      <Split size={13} />
                    </button>
                    <button type="button" title="Subir um número" onClick={() => mover(p, -1)}>
                      <ArrowUp size={13} />
                    </button>
                    <button type="button" title="Descer um número" onClick={() => mover(p, 1)}>
                      <ArrowDown size={13} />
                    </button>
                    <button type="button" title="Apagar (falso positivo)" onClick={() => void apagar(p)}>
                      <Trash2 size={13} />
                    </button>
                  </span>
                  <span className={styles.meta}>
                    <span>{formatDuration(p.primeiro_s)}</span>
                    <span>→ {formatDuration(p.ultimo_s)}</span>
                    <span>{p.quadros} q</span>
                    {p.estado === "editado" && <span className={styles.edited}>editada</span>}
                    {p.ajustes_t.length > 0 && <span className={styles.edited}>{p.ajustes_t.length} ajuste(s)</span>}
                  </span>
                </li>
              ))}
            </ul>
          </section>

          <section className={styles.sec}>
            <div className={styles.head}>
              <h3 className={styles.title}>Saídas</h3>
            </div>
            {tarefa?.fase === "exportar" && (
              <>
                <div className={styles.bar}>
                  <div className={styles.barFill} style={{ width: `${pct}%` }} />
                  <span className={styles.barTxt}>
                    {tarefa.feitos} / {tarefa.total || "…"}
                  </span>
                </div>
                <Button variant="secondary" size="sm" leftIcon={<X size={13} />} onClick={st.cancelar}>
                  Cancelar
                </Button>
              </>
            )}
            <div className={styles.outBtns}>
              <Button size="sm" leftIcon={<Film size={13} />} disabled={ocupado} onClick={() => void gravarVideo()} title="MP4 com a sobreposição gravada, no mesmo tamanho do original">
                Vídeo MP4
              </Button>
              <Button size="sm" leftIcon={<ImageIcon size={13} />} disabled={ocupado} onClick={() => void exportarQuadros([currentTime], "Quadro")} title="Quadro atual com a sobreposição, no storyboard">
                Este quadro
              </Button>
              <Button
                size="sm"
                leftIcon={<Images size={13} />}
                disabled={ocupado || events.length === 0}
                onClick={() => void exportarQuadros(events.map((e) => e.timestamp_s), "Eventos")}
                title="Um quadro por evento marcado, no storyboard"
              >
                Eventos ({events.length})
              </Button>
              <Button size="sm" leftIcon={<Table2 size={13} />} disabled={ocupado} onClick={() => void tabela()} title="Tabela por pessoa em CSV e relatório HTML">
                Tabela
              </Button>
            </div>
            <label className={styles.dim} style={{ display: "inline-flex", gap: 6, alignItems: "center" }}>
              <input type="checkbox" checked={comAudio && hasAudio} disabled={!hasAudio} onChange={(e) => setComAudio(e.target.checked)} /> com o som no MP4
            </label>
            {msg && <span className={styles.dim}>{msg}</span>}
            {saidas.length > 0 && (
              <div className={styles.done}>
                {saidas.map((s, i) => (
                  <div key={i}>
                    {s.rotulo}:{" "}
                    <button type="button" className={styles.link} onClick={() => revealExported(workspacePath, s.arq.absoluto)} title={s.arq.caminho}>
                      {s.arq.caminho.split("/").pop()}
                    </button>{" "}
                    <code title={s.arq.sha256}>{s.arq.sha256.slice(0, 12)}…</code>
                  </div>
                ))}
              </div>
            )}
          </section>
        </>
      )}
    </div>
  );
}
