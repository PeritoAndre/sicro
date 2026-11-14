/** Montagem: sequência de trechos, cartelas e quadros congelados para o vídeo do júri. */

import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  Clapperboard,
  Crosshair,
  FolderOpen,
  GripVertical,
  Play,
  Plus,
  Snowflake,
  Trash2,
  Type,
  X,
  Zap,
} from "lucide-react";
import { Button } from "@components/Button/Button";
import { askConfirm } from "@components/Dialog/ask";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { mediaSrc } from "@core/mediaSrc";
import { selectActiveWorkspacePath, useWorkspaceStore } from "@stores/workspaceStore";
import type {
  MontagemItem,
  MontagemTexto,
  MontagemTrecho,
  RenderMontagemResult,
} from "@domain/montagem";
import type { VideoEvent, VideoMedia } from "@domain/video";
import { useVideoStore } from "../store/videoStore";
import { formatDuration } from "../editor/format";
import { useSalvamentoAutomatico } from "@modules/croqui/editor/useSalvamentoAutomatico";
import { useMontagemStore } from "./montagemStore";
import {
  duracaoItem,
  duracaoTotal,
  novaCartela,
  novoCongelar,
  novoTrecho,
  inserirQuadro,
  textoPadrao,
  validarItem,
  validarMontagem,
} from "./montagemModel";
import { AudioPainel } from "./AudioPainel";
import { EfeitosTrecho } from "./EfeitosTrecho";
import { PalcoPrevia, type PreviaItem } from "./PalcoPrevia";
import type { AlvoMarcacao, Rascunho } from "./marcacao";
import styles from "./MontagemView.module.css";
import { NumInput, TempoInput } from "./campos";
import { caminhoDoCaso } from "@core/caminhoDoCaso";

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

/** `embutida`: como sexto modo da análise (sem "Voltar"; sair é trocar de modo). */
export function MontagemView({ embutida = false }: { embutida?: boolean } = {}) {
  const ws = useWorkspaceStore(selectActiveWorkspacePath);
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
  const [previa, setPrevia] = useState<PreviaItem | null>(null);
  const [previaMont, setPreviaMont] = useState<{ src: string; key: string } | null>(null);
  const [alvo, setAlvo] = useState<AlvoMarcacao | null>(null);
  const [rascunho, setRascunho] = useState<Rascunho | null>(null);
  const [tAtual, setTAtual] = useState(0);
  const [erro, setErro] = useState<string | null>(null);
  const [render, setRender] = useState<{ pct: number; previa: boolean } | null>(null);
  const [arr, setArr] = useState<{ de: number; para: number } | null>(null);
  const itemRefs = useRef(new Map<string, HTMLLIElement>());
  const montRef = useRef<HTMLVideoElement | null>(null);
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
      if (e.payload.montagem_id === doc.id) setRender((r) => ({ pct: e.payload.pct, previa: r?.previa ?? false }));
    }).then((f) => (un = f));
    return () => un?.();
  }, [doc?.id]);

  const porId = useMemo(() => new Map(videos.map((v) => [v.id, v])), [videos]);
  const total = doc ? duracaoTotal(doc.itens) : 0;
  const problema = doc ? validarMontagem(doc) : null;

  const sair = async () => {
    if (ws && m.dirty) await m.salvar(ws);
    m.fechar();
  };
  // Embutida: "Voltar" vai para a lista de edições, sem sair do modo.
  const voltarLista = async () => {
    if (ws && m.dirty) await m.salvar(ws);
    m.fechar();
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
    const abs = caminhoDoCaso(ws, v.relative_path);
    setPreviaMont(null);
    setPrevia({
      src: mediaSrc(abs),
      t: it.tipo === "trecho" ? it.inicio_s : it.instante_s,
      fim: it.tipo === "trecho" ? it.fim_s : null,
      vel: it.tipo === "trecho" ? it.velocidade : 1,
      key: `${it.id}-${Date.now()}`,
      itemId: it.id,
    });
    setAlvo(null);
    setRascunho(null);
  };

  const marcar = (it: MontagemItem, a: AlvoMarcacao) => {
    if (it.tipo !== "trecho") return;
    if (previa?.itemId !== it.id || previaMont) ver(it);
    setAlvo(a);
    setRascunho(null);
  };

  const trechoAlvo = alvo && doc ? doc.itens.find((i) => i.id === alvo.item && i.tipo === "trecho") : null;
  const rascunhoOk =
    !!rascunho &&
    (alvo?.tipo === "destaque" && alvo.ponto ? true : (rascunho.w ?? 0) > 0.005 && (rascunho.h ?? 0) > 0.005);

  /** Grava o rascunho como quadro-chave em t = tempo do player − início do trecho. */
  const marcarAgora = () => {
    if (!alvo || !rascunho || !trechoAlvo || trechoAlvo.tipo !== "trecho" || !rascunhoOk) return;
    const t = Math.max(0, Math.min(tAtual - trechoAlvo.inicio_s, trechoAlvo.fim_s - trechoAlvo.inicio_s));
    const tr = Math.round(t * 1000) / 1000;
    if (alvo.tipo === "zoom") {
      const reg = { x: rascunho.x, y: rascunho.y, w: rascunho.w ?? 1, h: rascunho.h ?? 1 };
      const z = trechoAlvo.zoom ?? { inicio: { x: 0, y: 0, w: 1, h: 1 }, fim: { x: 0, y: 0, w: 1, h: 1 } };
      m.alterar(trechoAlvo.id, { zoom: { ...z, [alvo.ponta]: reg } });
    } else if (alvo.tipo === "destaque") {
      const q = { t: tr, x: rascunho.x, y: rascunho.y, ...(alvo.ponto ? {} : { w: rascunho.w, h: rascunho.h }) };
      m.alterar(trechoAlvo.id, {
        destaques: (trechoAlvo.destaques ?? []).map((d) => (d.id === alvo.id ? { ...d, quadros: inserirQuadro(d.quadros, q) } : d)),
      });
    } else {
      const q = { t: tr, x: rascunho.x, y: rascunho.y, w: rascunho.w, h: rascunho.h };
      m.alterar(trechoAlvo.id, {
        desfoques: (trechoAlvo.desfoques ?? []).map((d) => (d.id === alvo.id ? { ...d, quadros: inserirQuadro(d.quadros, q) } : d)),
      });
    }
    setRascunho(null);
  };

  const renderizar = async (rapida = false) => {
    if (!ws || !doc) return;
    setErro(null);
    if (!rapida) setPronto(null);
    setRender({ pct: 0, previa: rapida });
    try {
      if (!(await m.salvar(ws))) throw new Error("não foi possível salvar a montagem");
      const r = await commands.renderMontagem(ws, doc.id, rapida);
      if (rapida) {
        setPrevia(null);
        setAlvo(null);
        setPreviaMont({ src: mediaSrc(caminhoDoCaso(ws, r.output_relative_path)), key: String(Date.now()) });
      } else {
        setPronto(r);
      }
    } catch (e) {
      setErro(toSicroError(e).message);
    } finally {
      setRender(null);
    }
  };

  // Reordenar arrastando a alça (pointer events: o arrastar HTML5 não funciona com dragDrop do Tauri no Windows).
  const iniciaArrasto = (e: React.PointerEvent, de: number) => {
    e.preventDefault();
    setArr({ de, para: de });
  };
  useEffect(() => {
    if (!arr || !doc) return;
    const ids = doc.itens.map((i) => i.id);
    const mover = (e: PointerEvent) => {
      let para = ids.length - 1;
      for (let i = 0; i < ids.length; i++) {
        const r = itemRefs.current.get(ids[i] as string)?.getBoundingClientRect();
        if (r && e.clientY < r.top + r.height / 2) {
          para = i;
          break;
        }
      }
      setArr((a) => (a ? { ...a, para } : a));
    };
    const soltar = () => {
      setArr((a) => {
        if (a && a.de !== a.para) m.moverItem(a.de, a.para);
        return null;
      });
    };
    window.addEventListener("pointermove", mover);
    window.addEventListener("pointerup", soltar);
    window.addEventListener("pointercancel", soltar);
    return () => {
      window.removeEventListener("pointermove", mover);
      window.removeEventListener("pointerup", soltar);
      window.removeEventListener("pointercancel", soltar);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [arr !== null, doc?.itens.length]);

  const abrirPasta = () => {
    if (!ws || !pronto) return;
    void commands.revealPathInExplorer(caminhoDoCaso(ws, pronto.output_relative_path)).catch((e) =>
      setErro(toSicroError(e).message),
    );
  };

  const excluirMontagem = async (id: string) => {
    if (!ws) return;
    const ok = await askConfirm({ title: "Excluir edição", message: "Excluir esta edição?", confirmLabel: "Excluir", danger: true });
    if (ok) await m.excluir(ws, id);
  };

  // ---- lista de montagens ----
  if (!doc) {
    return (
      <div className={styles.wrap}>
        <header className={styles.top}>
          {!embutida && (
            <button type="button" className={styles.back} onClick={() => void sair()}>
              <ArrowLeft size={14} /> Voltar
            </button>
          )}
          <h1 className={styles.titulo}>Edição</h1>
          <Button variant="primary" leftIcon={<Plus size={14} />} onClick={m.nova} style={{ marginLeft: "auto" }}>
            Nova edição
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
                aria-label="Excluir edição"
                title="Excluir edição"
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

  const trechoDaPrevia =
    previa && !previaMont
      ? ((doc.itens.find((i) => i.id === previa.itemId && i.tipo === "trecho") as MontagemTrecho | undefined) ?? null)
      : null;
  const nomeVideo = (id: string) => porId.get(id)?.filename ?? "vídeo removido";
  const fpsDe = (id: string) => porId.get(id)?.fps_declared ?? null;

  return (
    <div className={styles.wrap}>
      <header className={styles.top}>
        <button type="button" className={styles.back} onClick={() => void (embutida ? voltarLista() : sair())}>
          <ArrowLeft size={14} /> Voltar
        </button>
        <input className={`${styles.input} ${styles.tituloInput}`} value={doc.titulo} onChange={(e) => m.titulo(e.target.value)} />
        <span className={styles.dim}>{m.dirty ? "Alterado" : "Salvo"}</span>
        <Button variant="ghost" aria-label="Excluir edição" title="Excluir edição" onClick={() => void excluirMontagem(doc.id)}>
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
                <li
                  key={it.id}
                  ref={(el) => {
                    if (el) itemRefs.current.set(it.id, el);
                    else itemRefs.current.delete(it.id);
                  }}
                  className={`${styles.item} ${prob ? styles.itemRuim : ""} ${arr?.de === i ? styles.arrastando : ""} ${
                    arr && arr.para === i && arr.de !== i ? (arr.para > arr.de ? styles.soltarDepois : styles.soltarAntes) : ""
                  }`}
                >
                  <div className={styles.itemTopo}>
                    <span
                      className={styles.alca}
                      title="Arrastar para reordenar"
                      aria-label="Arrastar para reordenar"
                      onPointerDown={(e) => iniciaArrasto(e, i)}
                    >
                      <GripVertical size={14} />
                    </span>
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
                  {it.tipo === "trecho" && (
                    <EfeitosTrecho it={it} alvo={alvo} onAlvo={(a) => marcar(it, a)} onChange={(p) => m.alterar(it.id, p)} />
                  )}
                </li>
              );
            })}
          </ol>
        </section>

        <aside className={styles.lateral}>
          <div className={styles.previa}>
            {previaMont ? (
              <video key={previaMont.key} ref={montRef} src={previaMont.src} controls autoPlay />
            ) : previa ? (
              <PalcoPrevia
                previa={previa}
                trecho={trechoDaPrevia}
                tRel={trechoDaPrevia ? tAtual - trechoDaPrevia.inicio_s : 0}
                onTempo={setTAtual}
                videoRef={videoRef}
                alvo={alvo}
                rascunho={rascunho}
                onRascunho={setRascunho}
                aspectoSaida={doc.saida.largura / doc.saida.altura}
              />
            ) : (
              <div className={styles.semPrevia} />
            )}
          </div>
          {previaMont && (
            <div className={styles.barra}>
              <Button variant="ghost" leftIcon={<X size={14} />} onClick={() => setPreviaMont(null)}>
                Fechar prévia
              </Button>
            </div>
          )}
          {alvo && !previaMont && (
            <div className={styles.barra}>
              <Button variant="primary" leftIcon={<Crosshair size={14} />} disabled={!rascunhoOk} onClick={marcarAgora}>
                Marcar neste instante
              </Button>
              <Button variant="ghost" onClick={() => (setAlvo(null), setRascunho(null))}>
                Concluir
              </Button>
            </div>
          )}
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
          {ws && doc.saida.incluir_audio && (
            <AudioPainel ws={ws} doc={doc} inicioAtual={() => montRef.current?.currentTime ?? 0} onErro={setErro} />
          )}
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
            {render.previa ? "Gerando prévia…" : "Renderizando…"} {Math.round(render.pct)}%
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
          style={{ marginLeft: "auto" }}
          leftIcon={<Zap size={14} />}
          onClick={() => void renderizar(true)}
          disabled={!!render || doc.itens.length === 0 || !!problema}
        >
          Prévia rápida
        </Button>
        <Button
          variant="primary"
          onClick={() => void renderizar()}
          disabled={!!render || doc.itens.length === 0 || !!problema}
        >
          Renderizar vídeo do júri
        </Button>
      </footer>
    </div>
  );
}
