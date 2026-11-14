/**
 * Formulários de item novo da Sequência (§8.1, §8.6): "Trecho…" (Vídeo, Início, Fim, Evento), "Título…" (Modelo,
 * Título, Subtítulo) e "Dos eventos…" (Vídeo e os eventos dele; um trecho por par). Nenhum texto de exemplo.
 * Os tempos ficam presos ao quadro pela tabela da mídia (ou pelo Rust depois, em `Analise.prender`).
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { Plus } from "lucide-react";
import { commands } from "@core/commands";
import { Button } from "@components/Button/Button";
import { Dialog } from "@components/Dialog/Dialog";
import { CampoTempo } from "@components/CampoTempo/CampoTempo";
import { formatarTempo } from "@components/CampoTempo/tempoTexto";
import type { MidiaInfo, MidiaRef, Pts, Seg, TabelaQuadros, Titulo } from "@domain/edicao";
import type { VideoEvent, VideoMedia } from "@domain/video";
import { midiaDoCaso } from "../store/coleta";
import { exibido, fimDaMidia, fimDoPar, ptsDaTabela, tempoDoQuadro } from "../modelo/tempo";
import { garantirMidia, recursoDe, useRecursos } from "../palco/recursos";
import { MODELOS, novoTitulo, paresDeEventos, tempoDoEvento, type ModeloNovo } from "./novos";
import styles from "./Sequencia.module.css";

export type Formulario = "trecho" | "titulo" | "eventos";

export interface TrechoNovo {
  midia: MidiaRef;
  inicio_s: Seg;
  inicio_pts: Pts | null;
  fim_s: Seg;
  fim_pts: Pts | null;
}

interface Props {
  qual: Formulario | null;
  ws: string;
  onFechar(): void;
  onTrechos(t: TrechoNovo[]): void;
  onTitulo(t: Titulo): void;
}

/** Ponta presa ao quadro exibido (início) ou à fronteira do fim; sem tabela, só os segundos. */
export function pontas(tab: TabelaQuadros | null, ini: Seg, fim: Seg): Omit<TrechoNovo, "midia"> | null {
  if (!(fim > ini)) return null;
  if (!tab) return { inicio_s: ini, inicio_pts: null, fim_s: fim, fim_pts: null };
  const pts = ptsDaTabela(tab);
  const a = exibido(tab, ini);
  const b = fimDoPar(tab, fim, null);
  if (b <= a) return null;
  return { inicio_s: tempoDoQuadro(tab, a), inicio_pts: pts[a] as number, fim_s: b < pts.length ? tempoDoQuadro(tab, b) : fimDaMidia(tab), fim_pts: b < pts.length ? (pts[b] as number) : null };
}

function useVideos(ws: string, aberto: boolean) {
  const [lista, setLista] = useState<VideoMedia[]>([]);
  useEffect(() => {
    if (!aberto) return;
    let vivo = true;
    commands
      .listVideoMedia(ws)
      .then((l) => vivo && setLista(l))
      .catch(() => vivo && setLista([]));
    return () => {
      vivo = false;
    };
  }, [ws, aberto]);
  return lista;
}

function useEventos(ws: string, video: VideoMedia | null) {
  const [eventos, setEventos] = useState<VideoEvent[]>([]);
  useEffect(() => {
    if (!video) return setEventos([]);
    let vivo = true;
    commands
      .openVideoMedia(ws, video.id)
      .then((b) => vivo && setEventos([...b.events].sort((a, c) => tempoDoEvento(a) - tempoDoEvento(c))))
      .catch(() => vivo && setEventos([]));
    return () => {
      vivo = false;
    };
  }, [ws, video]);
  return eventos;
}

/** `tabela` só quando a mídia é marcável (contratos (h) 17): a de pacotes de contêiner sem tempo não dá pts nem #n;
 *  `travado` = não marcável (Início, Fim e eventos ficam como no Inspetor). */
function useMidia(ws: string, v: VideoMedia | null): { info: MidiaInfo | null; tabela: TabelaQuadros | null; travado: boolean } {
  useRecursos((s) => s.versao);
  useEffect(() => {
    if (v) garantirMidia(ws, v.sha256);
  }, [ws, v]);
  const r = recursoDe(v?.sha256);
  return { info: r.info, tabela: r.info?.marcavel ? r.tabela : null, travado: r.info?.marcavel === false };
}

function CampoVideo({ lista, valor, onChange }: { lista: VideoMedia[]; valor: string; onChange(id: string): void }) {
  return (
    <label className={`${styles.campo} ${styles.larga}`}>
      Vídeo
      <select className={styles.input} value={valor} onChange={(e) => onChange(e.target.value)}>
        <option value="" />
        {lista.map((v) => (
          <option key={v.id} value={v.id}>
            {v.filename}
          </option>
        ))}
      </select>
    </label>
  );
}

function FormTrecho({ ws, onFechar, onTrechos }: Omit<Props, "qual" | "onTitulo">) {
  const lista = useVideos(ws, true);
  const [id, setId] = useState("");
  const video = lista.find((v) => v.id === id) ?? null;
  const { info, tabela, travado } = useMidia(ws, video);
  const eventos = useEventos(ws, video);
  const de = info?.de_s ?? 0;
  const ate = info?.ate_s ?? video?.duration_s ?? 0;
  const [ini, setIni] = useState<Seg | null>(null);
  const [fim, setFim] = useState<Seg | null>(null);
  const [evento, setEvento] = useState("");
  // o que o perito digitou ou escolheu não é trocado pela faixa da mídia quando a MidiaInfo chega (h 27)
  const tocado = useRef(false);
  useEffect(() => {
    tocado.current = false;
    setEvento("");
  }, [video?.id]);
  useEffect(() => {
    if (tocado.current) return;
    setIni(video ? de : null);
    setFim(video ? ate : null);
  }, [video?.id, de, ate]); // eslint-disable-line react-hooks/exhaustive-deps
  const p = video && ini !== null && fim !== null ? pontas(tabela, ini, fim) : null;
  const adicionar = () => {
    if (!video || !p) return;
    onTrechos([{ midia: midiaDoCaso(video, lista, info), ...p }]);
  };
  return (
    <Dialog
      open
      title="Trecho"
      onClose={onFechar}
      footer={
        <>
          <Button variant="ghost" onClick={onFechar}>
            Cancelar
          </Button>
          <Button variant="primary" leftIcon={<Plus size={14} />} disabled={!p} onClick={adicionar}>
            Adicionar
          </Button>
        </>
      }
    >
      <div className={styles.form}>
        <CampoVideo lista={lista} valor={id} onChange={setId} />
        <CampoTempo
          label="Início"
          value={ini}
          base="fonte"
          tabela={tabela}
          min={de}
          max={ate}
          disabled={!video || travado}
          onChange={(t) => {
            tocado.current = true;
            setIni(t);
          }}
        />
        <CampoTempo
          label="Fim"
          value={fim}
          base="fonte"
          tabela={tabela}
          min={de}
          max={ate}
          disabled={!video || travado}
          onChange={(t) => {
            tocado.current = true;
            setFim(t);
          }}
        />
        <label className={`${styles.campo} ${styles.larga}`}>
          Evento
          <select
            className={styles.input}
            value={evento}
            disabled={eventos.length === 0 || travado}
            onChange={(e) => {
              setEvento(e.target.value);
              const ev = eventos.find((x) => x.id === e.target.value);
              if (!ev) return;
              tocado.current = true;
              const t = tempoDoEvento(ev);
              setIni(t);
              if (fim === null || fim <= t) setFim(Math.min(ate, t + 5));
            }}
          >
            <option value="" />
            {eventos.map((ev) => (
              <option key={ev.id} value={ev.id}>
                {formatarTempo(tempoDoEvento(ev))} {ev.title}
              </option>
            ))}
          </select>
        </label>
      </div>
    </Dialog>
  );
}

function FormTitulo({ onFechar, onTitulo }: Pick<Props, "onFechar" | "onTitulo">) {
  const [modelo, setModelo] = useState<ModeloNovo>("cartela");
  const [titulo, setTitulo] = useState("");
  const [sub, setSub] = useState("");
  return (
    <Dialog
      open
      title="Título"
      onClose={onFechar}
      footer={
        <>
          <Button variant="ghost" onClick={onFechar}>
            Cancelar
          </Button>
          <Button variant="primary" leftIcon={<Plus size={14} />} onClick={() => onTitulo(novoTitulo(modelo, titulo.trim(), sub.trim()))}>
            Adicionar
          </Button>
        </>
      }
    >
      <div className={styles.form}>
        <label className={styles.campo}>
          Modelo
          <select className={styles.input} value={modelo} onChange={(e) => setModelo(e.target.value as ModeloNovo)}>
            {MODELOS.map((m) => (
              <option key={m.valor} value={m.valor}>
                {m.rotulo}
              </option>
            ))}
          </select>
        </label>
        <label className={`${styles.campo} ${styles.larga}`}>
          Título
          <input className={styles.input} value={titulo} onChange={(e) => setTitulo(e.target.value)} autoFocus />
        </label>
        <label className={`${styles.campo} ${styles.larga}`}>
          Subtítulo
          <input className={styles.input} value={sub} onChange={(e) => setSub(e.target.value)} />
        </label>
      </div>
    </Dialog>
  );
}

function FormEventos({ ws, onFechar, onTrechos }: Omit<Props, "qual" | "onTitulo">) {
  const lista = useVideos(ws, true);
  const [id, setId] = useState("");
  const video = lista.find((v) => v.id === id) ?? null;
  const { info, tabela, travado } = useMidia(ws, video);
  const eventos = useEventos(ws, video);
  const [marcados, setMarcados] = useState<Set<string>>(new Set());
  useEffect(() => setMarcados(new Set()), [id]);
  const pares = useMemo(() => paresDeEventos(eventos.filter((e) => marcados.has(e.id)).map(tempoDoEvento)), [eventos, marcados]);
  const adicionar = () => {
    if (!video) return;
    const ref = midiaDoCaso(video, lista, info);
    const itens = pares.map(([a, b]) => pontas(tabela, a, b)).filter((x): x is NonNullable<typeof x> => x !== null);
    // o mesmo retrato da mídia para todos os trechos (um id só no projeto)
    onTrechos(itens.map((p) => ({ midia: ref, ...p })));
  };
  return (
    <Dialog
      open
      title="Dos eventos"
      onClose={onFechar}
      footer={
        <>
          <Button variant="ghost" onClick={onFechar}>
            Cancelar
          </Button>
          <Button variant="primary" leftIcon={<Plus size={14} />} disabled={pares.length === 0 || travado} onClick={adicionar}>
            Adicionar
          </Button>
        </>
      }
    >
      <div className={styles.form}>
        <CampoVideo lista={lista} valor={id} onChange={setId} />
        <ul className={`${styles.eventos} ${styles.larga}`}>
          {eventos.map((ev) => (
            <li key={ev.id}>
              <label className={styles.check}>
                <input
                  type="checkbox"
                  disabled={travado}
                  checked={marcados.has(ev.id)}
                  onChange={(e) =>
                    setMarcados((m) => {
                      const n = new Set(m);
                      if (e.target.checked) n.add(ev.id);
                      else n.delete(ev.id);
                      return n;
                    })
                  }
                />
                <span className={styles.mono}>{formatarTempo(tempoDoEvento(ev))}</span>
                <span>{ev.title}</span>
              </label>
            </li>
          ))}
        </ul>
      </div>
    </Dialog>
  );
}

export function NovoClipe(p: Props) {
  if (p.qual === "trecho") return <FormTrecho ws={p.ws} onFechar={p.onFechar} onTrechos={p.onTrechos} />;
  if (p.qual === "titulo") return <FormTitulo onFechar={p.onFechar} onTitulo={p.onTitulo} />;
  if (p.qual === "eventos") return <FormEventos ws={p.ws} onFechar={p.onFechar} onTrechos={p.onTrechos} />;
  return null;
}
