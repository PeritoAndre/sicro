/**
 * Áudio: volume do original e as trilhas de narração (importar, gravar, volume). Cada clipe de narração: "Preso a"
 * (clipe da principal, ou o programa), "Posição", recorte "De"/"Até" no arquivo, "Entrada (s)"/"Saída (s)" e (P4.2)
 * o "Volume" no cursor com as chaves dele (tempo local do clipe).
 * A gravação vai inteira num comando só (`edicao_gravacao_salvar`, bytes crus).
 * É a aba da Fase 0; o `PainelAudio` (P1.5) monta a dele com as mesmas peças (`Trilha`, `Gravador`).
 */

import { useEffect, useRef, useState } from "react";
import { ChevronDown, ChevronRight, Circle, EarOff, Mic, Square, Trash2, Upload, Volume2, VolumeX } from "lucide-react";
import type { ClipeAudio, ClipePrincipal, Id, Posicao as Pos, ProjetoEdicao, TrilhaAudio } from "@domain/edicao";
import { commands } from "@core/commands";
import { pushToast } from "@components/toast/toastStore";
import { formatarTempo } from "@components/CampoTempo/tempoTexto";
import { separarErro, useEdicaoStore } from "../../store/edicaoStore";
import { clipeConhecido } from "../../modelo/projeto";
import { adicionarNarracao, mudar, narracaoNoProjeto } from "../acoes";
import { Deslizante, Grade, Icone, Mini, Numero, Secao, Tempo } from "../campos";
import { escolherMidia } from "./importar";
import { CurvasDoItem, VolumeNoCursor } from "../../linha/curvas/CurvasDoItem";
import { passaDeUm } from "../../linha/curvas/chaves";
import styles from "../Inspetor.module.css";

export const PAPEL: Record<TrilhaAudio["papel"], string> = { narracao: "Narração", musica: "Música", efeitos: "Efeitos" };
const ROTULO_CLIPE: Record<ClipePrincipal["tipo"], string> = { trecho: "Trecho", titulo: "Título", congelar: "Quadro congelado", imagem: "Imagem", tela: "Tela dividida" };

const noClipeAudio = (id: Id, f: (c: ClipeAudio) => void) => (d: { audios: TrilhaAudio[] }) => {
  for (const t of d.audios) {
    const c = t.clipes.find((x) => x.id === id);
    if (c) return f(c);
  }
};
const naTrilha = (id: Id, f: (t: TrilhaAudio) => void) => (d: { audios: TrilhaAudio[] }) => {
  const t = d.audios.find((x) => x.id === id);
  if (t) f(t);
};

export function AudioSecao() {
  const doc = useEdicaoStore((s) => s.doc);
  const foco = useEdicaoStore((s) => s.selecao.foco);
  if (!doc) return null;
  // narração nova fica presa ao clipe da principal em foco
  const presoA = foco?.k === "clipe" && doc.principal.some((c) => clipeConhecido(c) && c.id === foco.id) ? foco.id : null;
  const narracoes = doc.audios.filter((t) => t.papel === "narracao");
  return (
    <>
      <Secao>
        <Grade>
          <Deslizante
            rotulo="Volume do original"
            larga
            item="mixagem"
            valor={doc.mixagem.volume_original}
            min={0}
            max={2}
            passo={0.05}
            mostrar={(v) => `${Math.round(v * 100)} %`}
            aplicar={(v) => (d) => void (d.mixagem.volume_original = v)}
          />
        </Grade>
      </Secao>
      {doc.audios.map((t) => (
        <Trilha key={t.id} t={t} principal={doc.principal} focoAudio={foco?.k === "audio" ? foco.id : null} />
      ))}
      <Secao titulo={narracoes.length === 0 ? "Narração" : undefined}>
        <div className={styles.barra}>
          <Mini data-acao="importar" onClick={() => void escolherMidia("audio").then((m) => m && adicionarNarracao(m, presoA, "Importar…", narracoes[0]?.id))}>
            <Upload size={12} /> Importar…
          </Mini>
          <Gravador presoA={presoA} trilha={narracoes[0]?.id} />
        </div>
      </Secao>
    </>
  );
}

export function Trilha({ t, principal, focoAudio }: { t: TrilhaAudio; principal: ProjetoEdicao["principal"]; focoAudio: Id | null }) {
  const so = useEdicaoStore((s) => s.soLeitura);
  return (
    <Secao
      titulo={PAPEL[t.papel]}
      acoes={
        <>
          <button
            type="button"
            className={`${styles.icone} ${t.mudo ? styles.iconeOn : ""}`}
            aria-pressed={t.mudo}
            aria-label={PAPEL[t.papel]}
            title={PAPEL[t.papel]}
            disabled={so}
            onClick={() => mudar(PAPEL[t.papel], naTrilha(t.id, (x) => void (x.mudo = !x.mudo)))}
          >
            {t.mudo ? <VolumeX size={13} /> : <Volume2 size={13} />}
          </button>
          <Icone rotulo="Remover" onClick={() => mudar("Remover", (d) => void (d.audios = d.audios.filter((x) => x.id !== t.id)))}>
            <Trash2 size={13} />
          </Icone>
        </>
      }
    >
      <Grade>
        <Deslizante
          rotulo={PAPEL[t.papel]}
          semRotulo
          larga
          item={t.id}
          valor={t.volume}
          desligado={t.mudo}
          min={0}
          max={2}
          passo={0.05}
          mostrar={(v) => `${Math.round(v * 100)} %`}
          aplicar={(v) => naTrilha(t.id, (x) => void (x.volume = v))}
        />
      </Grade>
      {t.clipes.length > 0 && (
        <ul className={styles.efeitos}>
          {t.clipes.map((c) => (
            <ClipeNarracao key={c.id} c={c} principal={principal} foco={focoAudio === c.id} />
          ))}
        </ul>
      )}
    </Secao>
  );
}

/** Foco no clipe de narração (abre os campos dele); null fecha. */
function focar(id: Id | null) {
  const st = useEdicaoStore.getState();
  const t = id ? st.doc?.audios.find((x) => x.clipes.some((c) => c.id === id)) : null;
  const it = id && t ? { k: "audio" as const, id, trilha: t.id } : null;
  st.selecionar({ itens: it ? [it] : [], foco: it, efeito: null });
}

function ClipeNarracao({ c, principal, foco }: { c: ClipeAudio; principal: unknown[]; foco: boolean }) {
  const so = useEdicaoStore((s) => s.soLeitura);
  const duracao = useEdicaoStore((s) => s.doc?.midias.find((m) => m.id === c.midia)?.duracao_s ?? null);
  const fps = useEdicaoStore((s) => s.doc?.saida.fps ?? null);
  const clipes = principal.filter((x): x is ClipePrincipal => clipeConhecido(x as ClipePrincipal));
  const pos = c.pos;
  const opcoes = clipes.map((x, i) => ({ v: x.id, r: `${i + 1} ${x.rotulo || ROTULO_CLIPE[x.tipo]}` }));
  const presoA = pos.ref === "clipe" ? pos.clipe : "";
  return (
    <li className={`${styles.efeito} ${foco ? styles.efeitoAberto : ""}`} data-audio={c.id}>
      <div className={styles.efeitoTopo}>
        <button type="button" className={styles.efeitoNome} aria-expanded={foco} data-acao="abrir-narracao" onClick={() => focar(foco ? null : c.id)}>
          {foco ? <ChevronDown size={12} aria-hidden /> : <ChevronRight size={12} aria-hidden />}
          <Mic size={13} aria-hidden />
          <span className={styles.cortaTexto} data-dado>{c.nome}</span>
          {duracao !== null && <span className={styles.dim} data-dado>{formatarTempo(duracao)}</span>}
        </button>
        <span className={styles.acoes}>
          <Icone rotulo="Remover" onClick={() => mudar("Remover", (d) => d.audios.forEach((t) => void (t.clipes = t.clipes.filter((x) => x.id !== c.id))))}>
            <Trash2 size={13} />
          </Icone>
        </span>
      </div>
      {foco && (
        <div className={styles.grade}>
          <label className={`${styles.campo} ${styles.larga}`}>
            <span className={styles.rotulo}>Preso a</span>
            <select
              className={styles.entrada}
              value={presoA}
              disabled={so}
              onChange={(e) => {
                const v = e.target.value;
                const novo: Pos = v ? { ref: "clipe", clipe: v, desloc: 0 } : { ref: "programa", t: 0 };
                mudar("Preso a", noClipeAudio(c.id, (x) => void (x.pos = novo)));
              }}
            >
              <option value="" />
              {opcoes.map((o) => (
                <option key={o.v} value={o.v} data-dado>
                  {o.r}
                </option>
              ))}
            </select>
          </label>
          <Tempo
            rotulo="Posição"
            item={c.id}
            base="programa"
            fps={fps}
            valor={pos.ref === "clipe" ? pos.desloc : pos.ref === "programa" ? pos.t : null}
            valido={(t) => pos.ref === "clipe" || t >= 0}
            aplicar={(t) => noClipeAudio(c.id, (x) => void (x.pos = x.pos.ref === "clipe" ? { ...x.pos, desloc: t } : { ref: "programa", t }))}
          />
          <Tempo
            rotulo="De"
            item={c.id}
            base="fonte"
            valor={c.de_s}
            valido={(t) => t >= 0 && (c.ate_s === null || t < c.ate_s)}
            aplicar={(t) => noClipeAudio(c.id, (x) => void (x.de_s = t))}
          />
          <Tempo
            rotulo="Até"
            item={c.id}
            base="fonte"
            valor={c.ate_s ?? duracao}
            valido={(t) => t > c.de_s && (duracao === null || t <= duracao + 1e-9)}
            aplicar={(t) => noClipeAudio(c.id, (x) => void (x.ate_s = duracao !== null && Math.abs(t - duracao) < 1e-9 ? null : t))}
          />
          <Numero rotulo="Entrada (s)" item={c.id} valor={c.entrada_s} min={0} max={30} passo={0.1} casas={1} valido={(v) => v >= 0} aplicar={(v) => noClipeAudio(c.id, (x) => void (x.entrada_s = v))} />
          <Numero rotulo="Saída (s)" item={c.id} valor={c.saida_s} min={0} max={30} passo={0.1} casas={1} valido={(v) => v >= 0} aplicar={(v) => noClipeAudio(c.id, (x) => void (x.saida_s = v))} />
          <VolumeNoCursor item={{ k: "audio", id: c.id }} />
          {/* acima de 100 % a prévia toca em 100 %: só no render */}
          {passaDeUm(c.volume) && (
            <span className={styles.dim} data-estado="so-no-render" data-de="volume" aria-hidden>
              <EarOff size={12} />
            </span>
          )}
          <CurvasDoItem item={{ k: "audio", id: c.id }} />
        </div>
      )}
    </li>
  );
}

/** "Gravar" / "Parar": microfone pelo `MediaRecorder`; o arquivo inteiro vai ao backend ao parar. */
export function Gravador({ presoA, trilha }: { presoA: Id | null; trilha?: Id }) {
  const [gravando, setGravando] = useState<{ mr: MediaRecorder; inicio: number } | null>(null);
  const [agora, setAgora] = useState(0);
  const partes = useRef<Blob[]>([]);
  const destino = useRef({ presoA, trilha });
  destino.current = { presoA, trilha };
  // o Inspetor troca de aba sozinho (foco num clipe): ao desmontar, a gravação para e é guardada, e o microfone solta
  const atual = useRef<MediaRecorder | null>(null);
  const montado = useRef(true);
  useEffect(() => {
    montado.current = true;
    return () => {
      montado.current = false;
      const mr = atual.current;
      atual.current = null;
      mr?.stop();
    };
  }, []);
  useEffect(() => {
    if (!gravando) return;
    const t = setInterval(() => setAgora(Date.now()), 250);
    return () => clearInterval(t);
  }, [gravando]);
  const tem = typeof MediaRecorder !== "undefined" && typeof navigator !== "undefined" && !!navigator.mediaDevices?.getUserMedia;
  const gravar = async () => {
    const st = useEdicaoStore.getState();
    if (!st.ws || !st.aberta) return;
    // a gravação é do projeto em que começou, mesmo que a tela troque de edição ou de caso antes de parar
    const dono = { ws: st.ws, id: st.aberta.projetoId };
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      if (!montado.current) {
        stream.getTracks().forEach((x) => x.stop());
        return;
      }
      const mr = new MediaRecorder(stream);
      atual.current = mr;
      partes.current = [];
      mr.ondataavailable = (e) => e.data.size > 0 && partes.current.push(e.data);
      mr.onstop = () => {
        stream.getTracks().forEach((x) => x.stop());
        void salvar(new Blob(partes.current, { type: mr.mimeType }), mr.mimeType || "audio/webm", dono);
      };
      mr.start();
      setGravando({ mr, inicio: Date.now() });
      setAgora(Date.now());
    } catch (e) {
      pushToast("error", separarErro(e).frase);
    }
  };
  const salvar = async (b: Blob, mime: string, dono: { ws: string; id: Id }) => {
    const st = useEdicaoStore.getState();
    try {
      // o projeto no disco antes do arquivo; se essa gravação falhar (ela tenta de novo), a narração não se perde
      if (st.ws === dono.ws && st.aberta?.projetoId === dono.id) await st.descarregar().catch(() => {});
      const m = await commands.edicaoGravacaoSalvar(dono.ws, dono.id, mime, new Uint8Array(await b.arrayBuffer()));
      await narracaoNoProjeto(dono.ws, dono.id, m, destino.current.presoA, "Gravar", destino.current.trilha);
    } catch (e) {
      pushToast("error", separarErro(e).frase);
    }
  };
  if (gravando)
    return (
      <span className={styles.gravando}>
        <Circle size={10} className={styles.rec} aria-hidden />
        <span className={styles.mono} data-dado>{formatarTempo((agora - gravando.inicio) / 1000).slice(3, 8)}</span>
        <Mini
          data-acao="parar"
          onClick={() => {
            atual.current = null;
            gravando.mr.stop();
            setGravando(null);
          }}
        >
          <Square size={11} /> Parar
        </Mini>
      </span>
    );
  return (
    <Mini data-acao="gravar" disabled={!tem} onClick={() => void gravar()}>
      <Mic size={12} /> Gravar
    </Mini>
  );
}
