/** Montagem: sequência de trechos, cartelas e quadros congelados para o vídeo do júri. */

import { useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  Clapperboard,
  FolderOpen,
  Play,
  Plus,
  Snowflake,
  Trash2,
  Type,
  X,
} from "lucide-react";
import { Button } from "@components/Button/Button";
import { askConfirm } from "@components/Dialog/ask";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { selectActiveWorkspacePath, useWorkspaceStore } from "@stores/workspaceStore";
import type {
  MontagemItem,
  MontagemTexto,
  RenderMontagemResult,
} from "@domain/montagem";
import type { VideoEvent, VideoMedia } from "@domain/video";
import { useVideoStore } from "../store/videoStore";
import { formatDuration, parseTimeInput } from "../editor/format";
import { useSalvamentoAutomatico } from "@modules/croqui/editor/useSalvamentoAutomatico";
import { useMontagemStore } from "./montagemStore";
import {
  duracaoItem,
  duracaoTotal,
  novaCartela,
  novoCongelar,
  novoTrecho,
  textoPadrao,
  validarItem,
} from "./montagemModel";
import styles from "./MontagemView.module.css";

const segFmt = (s: number) => (Number.isInteger(s) ? String(s) : s.toFixed(3).replace(/0+$/, ""));

/** Tempo em segundos ou "1:02.5"; confirma ao sair do campo ou no Enter. */
function TempoInput({ valor, onChange, fps }: { valor: number; onChange: (v: number) => void; fps?: number | null }) {
  const [txt, setTxt] = useState(segFmt(valor));
  const [ruim, setRuim] = useState(false);
  useEffect(() => setTxt(segFmt(valor)), [valor]);
  const confirmar = () => {
    const v = parseTimeInput(txt, fps);
    if (v == null) {
      setRuim(true);
      return;
    }
    setRuim(false);
    if (v !== valor) onChange(v);
    else setTxt(segFmt(valor));
  };
  return (
    <input
      className={`${styles.input} ${ruim ? styles.bad : ""}`}
      value={txt}
      onChange={(e) => setTxt(e.target.value)}
      onBlur={confirmar}
      onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
      inputMode="decimal"
    />
  );
}

function NumInput({ valor, onChange, min }: { valor: number; onChange: (v: number) => void; min?: number }) {
  return (
    <input
      className={styles.input}
      type="number"
      step="0.1"
      min={min}
      value={valor}
      onChange={(e) => Number.isFinite(e.target.valueAsNumber) && onChange(e.target.valueAsNumber)}
    />
  );
}

function TextoEditor({ texto, onChange }: { texto: MontagemTexto | null | undefined; onChange: (t: MontagemTexto) => void }) {
  const t = texto ?? textoPadrao();
  return (
    <div className={styles.textoBox}>
      <textarea
        className={styles.input}
        rows={2}
        placeholder="Texto sobre o vídeo"
        value={t.conteudo}
        onChange={(e) => onChange({ ...t, conteudo: e.target.value })}
      />
      <select className={styles.input} value={t.posicao} onChange={(e) => onChange({ ...t, posicao: e.target.value as MontagemTexto["posicao"] })}>
        <option value="topo">Topo</option>
        <option value="centro">Centro</option>
        <option value="rodape">Rodapé</option>
      </select>
      <select className={styles.input} value={t.tamanho} onChange={(e) => onChange({ ...t, tamanho: e.target.value as MontagemTexto["tamanho"] })}>
        <option value="p">Pequeno</option>
        <option value="m">Médio</option>
        <option value="g">Grande</option>
      </select>
      <label className={styles.check}>
        <input type="checkbox" checked={t.carimbo_tempo} onChange={(e) => onChange({ ...t, carimbo_tempo: e.target.checked })} />
        Tempo do original
      </label>
    </div>
  );
}

type Previa = { src: string; t: number; fim: number | null; vel: number; key: string };

export function MontagemView() {
  const ws = useWorkspaceStore(selectActiveWorkspacePath);
  const fechar = useVideoStore((s) => s.fecharMontagem);
  const videos = useVideoStore((s) => s.list);
  const loadList = useVideoStore((s) => s.loadList);
  const m = useMontagemStore();
  const doc = m.doc;

  const [adicionando, setAdicionando] = useState<null | "trecho" | "cartela" | "congelar">(null);
  const [fMedia, setFMedia] = useState("");
  const [fIni, setFIni] = useState(0);
  const [fFim, setFFim] = useState(5);
  const [fTexto, setFTexto] = useState("");
  const [fDur, setFDur] = useState(3);
  const [eventos, setEventos] = useState<VideoEvent[]>([]);
  const [previa, setPrevia] = useState<Previa | null>(null);
  const [erro, setErro] = useState<string | null>(null);
  const [render, setRender] = useState<{ pct: number } | null>(null);
  const [pronto, setPronto] = useState<RenderMontagemResult | null>(null);
  const videoRef = useRef<HTMLVideoElement | null>(null);

  useEffect(() => {
    if (!ws) return;
    void loadList(ws);
    void m.carregar(ws);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ws]);

  // Ao sair (ou trocar de ocorrência) grava o que faltava no caso de origem e limpa o documento.
  useEffect(() => {
    const origem = ws;
    return () => {
      const st = useMontagemStore.getState();
      if (origem && st.dirty) void st.salvar(origem).finally(() => useMontagemStore.getState().fechar());
      else st.fechar();
    };
  }, [ws]);

  useSalvamentoAutomatico(m.dirty, () => (ws ? m.salvar(ws) : Promise.resolve(true)), !!doc);

  // Eventos marcados no vídeo escolhido: atalhos para início e fim.
  useEffect(() => {
    if (!ws || !fMedia) {
      setEventos([]);
      return;
    }
    let vivo = true;
    void commands
      .openVideoMedia(ws, fMedia)
      .then((b) => vivo && setEventos(b.events))
      .catch(() => vivo && setEventos([]));
    return () => {
      vivo = false;
    };
  }, [ws, fMedia]);

  useEffect(() => {
    if (!doc) return;
    let un: (() => void) | null = null;
    void listen<{ montagem_id: string; pct: number }>("montagem:progresso", (e) => {
      if (e.payload.montagem_id === doc.id) setRender({ pct: e.payload.pct });
    }).then((f) => (un = f));
    return () => un?.();
  }, [doc?.id]);

  const porId = useMemo(() => new Map(videos.map((v) => [v.id, v])), [videos]);
  const total = doc ? duracaoTotal(doc.itens) : 0;
  const problema = doc?.itens.map(validarItem).find((p) => p) ?? null;

  const sair = async () => {
    if (ws && m.dirty) await m.salvar(ws);
    m.fechar();
    fechar();
  };

  const abrirForm = (tipo: "trecho" | "cartela" | "congelar") => {
    setAdicionando(tipo);
    setFMedia((x) => x || videos[0]?.id || "");
    setFIni(0);
    setFFim(5);
    setFTexto("");
    setFDur(tipo === "cartela" ? 3 : 2);
  };

  const confirmarForm = () => {
    if (!adicionando) return;
    if (adicionando === "cartela") m.adicionar(novaCartela(fTexto, fDur));
    else if (!fMedia) return;
    else if (adicionando === "trecho") m.adicionar(novoTrecho(fMedia, fIni, fFim));
    else m.adicionar(novoCongelar(fMedia, fIni, fDur));
    setAdicionando(null);
  };

  const ver = (it: MontagemItem) => {
    if (!ws || it.tipo === "cartela") return;
    const v = porId.get(it.media_id);
    if (!v) return;
    const sep = ws.includes("\\") ? "\\" : "/";
    const abs = `${ws}${sep}${v.relative_path.replace(/\//g, sep)}`;
    setPrevia({
      src: convertFileSrc(abs),
      t: it.tipo === "trecho" ? it.inicio_s : it.instante_s,
      fim: it.tipo === "trecho" ? it.fim_s : null,
      vel: it.tipo === "trecho" ? it.velocidade : 1,
      key: `${it.id}-${Date.now()}`,
    });
  };

  const renderizar = async () => {
    if (!ws || !doc) return;
    setErro(null);
    setPronto(null);
    setRender({ pct: 0 });
    try {
      if (!(await m.salvar(ws))) throw new Error("não foi possível salvar a montagem");
      setPronto(await commands.renderMontagem(ws, doc.id));
    } catch (e) {
      setErro(toSicroError(e).message);
    } finally {
      setRender(null);
    }
  };

  const abrirPasta = () => {
    if (!ws || !pronto) return;
    const sep = ws.includes("\\") ? "\\" : "/";
    void commands.revealPathInExplorer(`${ws}${sep}${pronto.output_relative_path.replace(/\//g, sep)}`).catch((e) =>
      setErro(toSicroError(e).message),
    );
  };

  const excluirMontagem = async (id: string) => {
    if (!ws) return;
    const ok = await askConfirm({ title: "Excluir montagem", message: "Excluir esta montagem?", confirmLabel: "Excluir", danger: true });
    if (ok) await m.excluir(ws, id);
  };

  // ---- lista de montagens ----
  if (!doc) {
    return (
      <div className={styles.wrap}>
        <header className={styles.top}>
          <button type="button" className={styles.back} onClick={() => void sair()}>
            <ArrowLeft size={14} /> Voltar
          </button>
          <h1 className={styles.titulo}>Montagem</h1>
          <Button variant="primary" leftIcon={<Plus size={14} />} onClick={m.nova} style={{ marginLeft: "auto" }}>
            Nova montagem
          </Button>
        </header>
        {m.erro && <p className={styles.erro}>{m.erro}</p>}
        <ul className={styles.lista}>
          {m.lista.map((x) => (
            <li key={x.id} className={styles.linha} onClick={() => m.abrir(x.id)}>
              <Clapperboard size={16} />
              <strong>{x.titulo}</strong>
              <span className={styles.dim}>
                {x.itens.length} itens · {formatDuration(duracaoTotal(x.itens))}
              </span>
              <Button
                variant="ghost"
                aria-label="Excluir montagem"
                title="Excluir montagem"
                style={{ marginLeft: "auto" }}
                onClick={(e) => {
                  e.stopPropagation();
                  void excluirMontagem(x.id);
                }}
              >
                <Trash2 size={14} />
              </Button>
            </li>
          ))}
        </ul>
      </div>
    );
  }

  const nomeVideo = (id: string) => porId.get(id)?.filename ?? "vídeo removido";
  const fpsDe = (id: string) => porId.get(id)?.fps_declared ?? null;

  return (
    <div className={styles.wrap}>
      <header className={styles.top}>
        <button type="button" className={styles.back} onClick={() => void sair()}>
          <ArrowLeft size={14} /> Voltar
        </button>
        <input className={`${styles.input} ${styles.tituloInput}`} value={doc.titulo} onChange={(e) => m.titulo(e.target.value)} />
        <span className={styles.dim}>{m.dirty ? "Alterado" : "Salvo"}</span>
        <Button variant="ghost" aria-label="Excluir montagem" title="Excluir montagem" onClick={() => void excluirMontagem(doc.id)}>
          <Trash2 size={14} />
        </Button>
      </header>

      <div className={styles.corpo}>
        <section className={styles.coluna}>
          <div className={styles.barra}>
            <Button leftIcon={<Plus size={14} />} onClick={() => abrirForm("trecho")} disabled={videos.length === 0}>
              Trecho…
            </Button>
            <Button leftIcon={<Type size={14} />} onClick={() => abrirForm("cartela")}>
              Cartela…
            </Button>
            <Button leftIcon={<Snowflake size={14} />} onClick={() => abrirForm("congelar")} disabled={videos.length === 0}>
              Congelar quadro…
            </Button>
          </div>

          {adicionando && (
            <div className={styles.form}>
              {adicionando !== "cartela" && (
                <label>
                  Vídeo
                  <select className={styles.input} value={fMedia} onChange={(e) => setFMedia(e.target.value)}>
                    {videos.map((v: VideoMedia) => (
                      <option key={v.id} value={v.id}>
                        {v.filename}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              {adicionando === "trecho" && (
                <>
                  <label>
                    Início
                    <TempoInput valor={fIni} onChange={setFIni} fps={fpsDe(fMedia)} />
                  </label>
                  <label>
                    Fim
                    <TempoInput valor={fFim} onChange={setFFim} fps={fpsDe(fMedia)} />
                  </label>
                  {eventos.length > 0 && (
                    <>
                      <label>
                        Início no evento
                        <select className={styles.input} value="" onChange={(e) => e.target.value && setFIni(Number(e.target.value))}>
                          <option value="" />
                          {eventos.map((ev) => (
                            <option key={ev.id} value={ev.timestamp_s}>
                              {formatDuration(ev.timestamp_s)} {ev.title}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label>
                        Fim no evento
                        <select className={styles.input} value="" onChange={(e) => e.target.value && setFFim(Number(e.target.value))}>
                          <option value="" />
                          {eventos.map((ev) => (
                            <option key={ev.id} value={ev.timestamp_s}>
                              {formatDuration(ev.timestamp_s)} {ev.title}
                            </option>
                          ))}
                        </select>
                      </label>
                    </>
                  )}
                </>
              )}
              {adicionando === "congelar" && (
                <>
                  <label>
                    Instante
                    <TempoInput valor={fIni} onChange={setFIni} fps={fpsDe(fMedia)} />
                  </label>
                  <label>
                    Duração (s)
                    <NumInput valor={fDur} onChange={setFDur} min={0.1} />
                  </label>
                </>
              )}
              {adicionando === "cartela" && (
                <>
                  <label className={styles.larga}>
                    Texto
                    <textarea className={styles.input} rows={3} value={fTexto} onChange={(e) => setFTexto(e.target.value)} />
                  </label>
                  <label>
                    Duração (s)
                    <NumInput valor={fDur} onChange={setFDur} min={0.1} />
                  </label>
                </>
              )}
              <div className={styles.formBotoes}>
                <Button variant="primary" onClick={confirmarForm}>
                  Adicionar
                </Button>
                <Button variant="ghost" onClick={() => setAdicionando(null)}>
                  Cancelar
                </Button>
              </div>
            </div>
          )}

          <ol className={styles.itens}>
            {doc.itens.map((it, i) => {
              const prob = validarItem(it);
              return (
                <li key={it.id} className={`${styles.item} ${prob ? styles.itemRuim : ""}`}>
                  <div className={styles.itemTopo}>
                    <span className={styles.num}>{i + 1}</span>
                    <strong>{it.tipo === "trecho" ? "Trecho" : it.tipo === "cartela" ? "Cartela" : "Quadro congelado"}</strong>
                    <span className={styles.dim}>
                      {it.tipo !== "cartela" ? nomeVideo(it.media_id) : ""} · {formatDuration(duracaoItem(it))}
                      {prob ? ` · ${prob}` : ""}
                    </span>
                    <span className={styles.acoes}>
                      {it.tipo !== "cartela" && (
                        <button type="button" title="Ver no player" aria-label="Ver no player" onClick={() => ver(it)}>
                          <Play size={13} />
                        </button>
                      )}
                      <button type="button" title="Subir" aria-label="Subir" disabled={i === 0} onClick={() => m.moverItem(i, i - 1)}>
                        <ArrowUp size={13} />
                      </button>
                      <button type="button" title="Descer" aria-label="Descer" disabled={i === doc.itens.length - 1} onClick={() => m.moverItem(i, i + 1)}>
                        <ArrowDown size={13} />
                      </button>
                      <button type="button" title="Remover" aria-label="Remover" onClick={() => m.remover(it.id)}>
                        <X size={13} />
                      </button>
                    </span>
                  </div>

                  <div className={styles.campos}>
                    {it.tipo === "trecho" && (
                      <>
                        <label>
                          Início
                          <TempoInput valor={it.inicio_s} onChange={(v) => m.alterar(it.id, { inicio_s: v })} fps={fpsDe(it.media_id)} />
                        </label>
                        <label>
                          Fim
                          <TempoInput valor={it.fim_s} onChange={(v) => m.alterar(it.id, { fim_s: v })} fps={fpsDe(it.media_id)} />
                        </label>
                        <label>
                          Velocidade
                          <select
                            className={styles.input}
                            value={String(it.velocidade)}
                            onChange={(e) => m.alterar(it.id, { velocidade: Number(e.target.value) })}
                          >
                            {[0.1, 0.25, 0.5, 0.75, 1, 1.5, 2, 4].map((v) => (
                              <option key={v} value={v}>
                                {v}×
                              </option>
                            ))}
                            {![0.1, 0.25, 0.5, 0.75, 1, 1.5, 2, 4].includes(it.velocidade) && (
                              <option value={it.velocidade}>{it.velocidade}×</option>
                            )}
                          </select>
                        </label>
                      </>
                    )}
                    {it.tipo === "congelar" && (
                      <>
                        <label>
                          Instante
                          <TempoInput valor={it.instante_s} onChange={(v) => m.alterar(it.id, { instante_s: v })} fps={fpsDe(it.media_id)} />
                        </label>
                        <label>
                          Duração (s)
                          <NumInput valor={it.duracao_s} min={0.1} onChange={(v) => m.alterar(it.id, { duracao_s: v })} />
                        </label>
                      </>
                    )}
                    {it.tipo === "cartela" && (
                      <>
                        <label className={styles.larga}>
                          Texto
                          <textarea className={styles.input} rows={2} value={it.texto} onChange={(e) => m.alterar(it.id, { texto: e.target.value })} />
                        </label>
                        <label>
                          Duração (s)
                          <NumInput valor={it.duracao_s} min={0.1} onChange={(v) => m.alterar(it.id, { duracao_s: v })} />
                        </label>
                        <label>
                          Tamanho
                          <select className={styles.input} value={it.tamanho} onChange={(e) => m.alterar(it.id, { tamanho: e.target.value as "p" | "m" | "g" })}>
                            <option value="p">Pequeno</option>
                            <option value="m">Médio</option>
                            <option value="g">Grande</option>
                          </select>
                        </label>
                        <label>
                          Fundo
                          <input className={styles.cor} type="color" value={it.cor_fundo} onChange={(e) => m.alterar(it.id, { cor_fundo: e.target.value })} />
                        </label>
                        <label>
                          Texto
                          <input className={styles.cor} type="color" value={it.cor_texto} onChange={(e) => m.alterar(it.id, { cor_texto: e.target.value })} />
                        </label>
                      </>
                    )}
                    {it.tipo !== "congelar" && (
                      <label>
                        Entrada
                        <select
                          className={styles.input}
                          value={it.transicao_entrada ?? "corte"}
                          onChange={(e) => m.alterar(it.id, { transicao_entrada: e.target.value as "corte" | "fade" })}
                        >
                          <option value="corte">Corte</option>
                          <option value="fade">Fade</option>
                        </select>
                      </label>
                    )}
                  </div>
                  {it.tipo !== "cartela" && (
                    <TextoEditor texto={it.texto} onChange={(t) => m.alterar(it.id, { texto: t })} />
                  )}
                </li>
              );
            })}
          </ol>
        </section>

        <aside className={styles.lateral}>
          <div className={styles.previa}>
            {previa ? (
              <video
                key={previa.key}
                ref={videoRef}
                src={previa.src}
                controls
                autoPlay
                onLoadedMetadata={(e) => {
                  e.currentTarget.currentTime = previa.t;
                  e.currentTarget.playbackRate = previa.vel;
                }}
                onTimeUpdate={(e) => {
                  if (previa.fim != null && e.currentTarget.currentTime >= previa.fim) e.currentTarget.pause();
                }}
              />
            ) : (
              <div className={styles.semPrevia} />
            )}
          </div>
          <div className={styles.saida}>
            <label>
              Largura
              <NumInput valor={doc.saida.largura} onChange={(v) => m.saida({ largura: Math.round(v / 2) * 2 })} min={64} />
            </label>
            <label>
              Altura
              <NumInput valor={doc.saida.altura} onChange={(v) => m.saida({ altura: Math.round(v / 2) * 2 })} min={64} />
            </label>
            <label>
              Quadros/s
              <NumInput valor={doc.saida.fps} onChange={(v) => m.saida({ fps: Math.max(1, Math.round(v)) })} min={1} />
            </label>
            <label className={styles.check}>
              <input type="checkbox" checked={doc.saida.incluir_audio} onChange={(e) => m.saida({ incluir_audio: e.target.checked })} />
              Incluir áudio
            </label>
          </div>
        </aside>
      </div>

      <footer className={styles.rodape}>
        <strong>{formatDuration(total)}</strong>
        <span className={styles.dim}>{doc.itens.length} itens</span>
        {render && (
          <span className={styles.progresso}>
            <span className={styles.barraProg}>
              <span style={{ width: `${render.pct}%` }} />
            </span>
            Renderizando… {Math.round(render.pct)}%
          </span>
        )}
        {pronto && !render && (
          <span className={styles.pronto}>
            Pronto: {pronto.output_relative_path.split("/").pop()} · {formatDuration(pronto.duracao_s)} · SHA-256{" "}
            <code title={pronto.sha256}>{pronto.sha256.slice(0, 16)}…</code>
            <Button variant="ghost" leftIcon={<FolderOpen size={14} />} onClick={abrirPasta}>
              Abrir pasta
            </Button>
          </span>
        )}
        {(erro || m.erro) && <span className={styles.erroLinha}>{erro ?? m.erro}</span>}
        <Button
          variant="primary"
          style={{ marginLeft: "auto" }}
          onClick={() => void renderizar()}
          disabled={!!render || doc.itens.length === 0 || !!problema}
        >
          Renderizar vídeo do júri
        </Button>
      </footer>
    </div>
  );
}
