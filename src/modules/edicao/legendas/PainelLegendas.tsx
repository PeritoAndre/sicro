/**
 * Aba Legendas (P1.6): "Da degravação…" · "Nova legenda" · "Posição" · "Linhas"; cada legenda aberta: "Texto", "De",
 * "Até", "Conferida". Estado por ícone: automática não conferida, fora do programa (pelo espelho do motor).
 * A degravação chega em tempo de fonte (`edicao_degravacao`) e vira legendas de fonte da mídia escolhida.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import type { Draft } from "immer";
import { Bot, ChevronDown, ChevronRight, EyeOff, FileText, Plus, Trash2 } from "lucide-react";
import type { Id, Legenda, MidiaRef, ProjetoEdicao, Problema } from "@domain/edicao";
import { commands } from "@core/commands";
import { pushToast } from "@components/toast/toastStore";
import { formatarTempo } from "@components/CampoTempo/tempoTexto";
import { separarErro, useEdicaoStore } from "../store/edicaoStore";
import { clipeConhecido, novoId } from "../modelo/projeto";
import { legendasDaDegravacao, noPrograma } from "../modelo/legendas";
import { mudar } from "../inspetor/acoes";
import { Grade, Icone, Marca, Mini, Numero, Posicao, Secao, Tempo, Texto } from "../inspetor/campos";
import styles from "../inspetor/Inspetor.module.css";

export const LEGENDAS_PRONTAS = true;

/** Duração (s) da legenda nova. */
export const DURACAO_NOVA = 2;

type Mut = (d: Draft<ProjetoEdicao>) => void;

const naLegenda =
  (id: Id, f: (l: Draft<Legenda>) => void): Mut =>
  (d) => {
    const l = d.legendas.itens.find((x) => x.id === id);
    if (l) f(l);
  };

/** Foco na legenda (abre os campos dela); null fecha. */
function focar(id: Id | null) {
  const it = id ? { k: "legenda" as const, id } : null;
  useEdicaoStore.getState().selecionar({ itens: it ? [it] : [], foco: it, efeito: null });
}

/** Legenda nova de 2 s no cursor: de fonte no trecho em foco, senão de programa. Texto vazio (sem texto de exemplo). */
function novaLegenda(): void {
  const st = useEdicaoStore.getState();
  const doc = st.doc;
  if (!doc) return;
  const c = st.cursor;
  const fps = doc.saida.fps;
  let ref: Legenda["ref"] = { tipo: "programa", de: 0, ate: DURACAO_NOVA };
  if (c.modo === "clipe" && Number.isFinite(c.t)) {
    const clipe = doc.principal.find((x) => clipeConhecido(x) && x.id === c.clipe);
    const r = st.analise?.linha.clipes.find((x) => x.id === c.clipe);
    if (clipe && clipeConhecido(clipe) && clipe.tipo === "trecho") ref = { tipo: "fonte", midia: clipe.midia, de: c.t, ate: c.t + DURACAO_NOVA };
    else if (r) {
      const t = (r.inicio_q * fps.den) / fps.num + c.t;
      ref = { tipo: "programa", de: t, ate: t + DURACAO_NOVA };
    }
  } else if (c.modo === "programa") {
    const t = (c.q * fps.den) / fps.num;
    ref = { tipo: "programa", de: t, ate: t + DURACAO_NOVA };
  }
  const id = novoId();
  mudar("Nova legenda", (d) => void d.legendas.itens.push({ id, texto: "", locutor: null, origem: "perito", conferida: true, locutor_auto: false, ref }));
  focar(id);
}

/** Mesma legenda de fonte (repetir a importação não duplica). */
const mesma = (a: Legenda, b: Legenda) =>
  a.texto === b.texto && a.ref.tipo === "fonte" && b.ref.tipo === "fonte" && a.ref.midia === b.ref.midia && a.ref.de === b.ref.de && a.ref.ate === b.ref.ate;

/** Degravação da mídia → legendas, num passo só, no projeto em que o pedido começou. */
async function importar(m: MidiaRef): Promise<void> {
  const st = useEdicaoStore.getState();
  if (!st.ws || !st.aberta) return;
  const dono = { ws: st.ws, id: st.aberta.projetoId };
  try {
    const d = await commands.edicaoDegravacao(dono.ws, m.sha256);
    const agora = useEdicaoStore.getState();
    if (agora.ws !== dono.ws || agora.aberta?.projetoId !== dono.id) return;
    const novas = legendasDaDegravacao(d, m.id);
    mudar("Da degravação…", (doc) => {
      for (const x of novas) if (!doc.legendas.itens.some((y) => mesma(y as Legenda, x))) doc.legendas.itens.push(x);
      doc.legendas.origem = d.origem;
    });
  } catch (e) {
    pushToast("error", separarErro(e).frase);
  }
}

function DaDegravacao({ videos }: { videos: MidiaRef[] }) {
  const [escolher, setEscolher] = useState(false);
  return (
    <>
      <Mini
        data-acao="da-degravacao"
        disabled={videos.length === 0}
        aria-expanded={videos.length > 1 ? escolher : undefined}
        ativo={escolher}
        onClick={() => (videos.length === 1 ? void importar(videos[0]!) : setEscolher(!escolher))}
      >
        <FileText size={12} /> Da degravação…
      </Mini>
      {escolher && (
        <ul className={styles.efeitos} data-escolha="degravacao">
          {videos.map((m) => (
            <li key={m.id} className={styles.efeito}>
              <button
                type="button"
                className={styles.efeitoNome}
                data-midia={m.id}
                onClick={() => {
                  setEscolher(false);
                  void importar(m);
                }}
              >
                <span className={styles.cortaTexto} data-dado>
                  {m.nome}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

export function PainelLegendas() {
  const doc = useEdicaoStore((s) => s.doc);
  const linha = useEdicaoStore((s) => s.analise?.linha ?? null);
  const problemas = useEdicaoStore((s) => s.analise?.problemas);
  const foco = useEdicaoStore((s) => s.selecao.foco);
  // primeiro slot de cada legenda no programa; sem a linha resolvida ainda, nada se afirma
  const inicio = useMemo(() => {
    if (!doc || !linha) return null;
    const m = new Map<Id, number>();
    for (const [i, a] of noPrograma(doc, linha)) {
      const id = doc.legendas.itens[i]!.id;
      if (!m.has(id)) m.set(id, a);
    }
    return m;
  }, [doc, linha]);
  if (!doc) return null;
  const l = doc.legendas;
  const videos = doc.midias.filter((m) => m.tipo === "video");
  // em ordem de aparição no programa; as que não aparecem, no fim, na ordem do documento
  const ordem = l.itens
    .map((g, i) => ({ g, i, q: inicio?.get(g.id) ?? Infinity }))
    .sort((a, b) => a.q - b.q || a.i - b.i)
    .map((x) => x.g);
  return (
    <>
      <Secao>
        <div className={styles.barra}>
          <DaDegravacao videos={videos} />
          <Mini data-acao="nova-legenda" onClick={novaLegenda}>
            <Plus size={12} /> Nova legenda
          </Mini>
        </div>
        <Grade>
          <Posicao rotulo="Posição" valor={l.lugar} aplicar={(v) => (d) => void (d.legendas.lugar = v)} />
          <Numero
            rotulo="Linhas"
            item="legendas"
            valor={l.linhas_max}
            min={1}
            max={4}
            passo={1}
            casas={0}
            valido={(v) => Number.isInteger(v) && v >= 1 && v <= 4}
            aplicar={(v) => (d) => void (d.legendas.linhas_max = v)}
          />
        </Grade>
      </Secao>
      {ordem.length > 0 && (
        <Secao>
          <ul className={styles.efeitos}>
            {ordem.map((g) => (
              <ItemLegenda
                key={g.id}
                g={g}
                aberta={foco?.k === "legenda" && foco.id === g.id}
                fora={!!inicio && !inicio.has(g.id)}
                problema={primeiro(problemas, g.id)}
              />
            ))}
          </ul>
        </Secao>
      )}
    </>
  );
}

/** Erro primeiro; senão o primeiro aviso da legenda (`Problema.clipe` = id do item). */
function primeiro(problemas: Problema[] | undefined, id: Id): Problema | null {
  const meus = problemas?.filter((p) => p.clipe === id) ?? [];
  return meus.find((p) => p.nivel === "erro") ?? meus[0] ?? null;
}

function ItemLegenda({ g, aberta, fora, problema }: { g: Legenda; aberta: boolean; fora: boolean; problema: Problema | null }) {
  const fps = useEdicaoStore((s) => s.doc?.saida.fps ?? null);
  const li = useRef<HTMLLIElement>(null);
  // aberta de fora (diálogo de exportar): rola até ela
  useEffect(() => {
    if (aberta) li.current?.scrollIntoView?.({ block: "nearest" });
  }, [aberta]);
  const base = g.ref.tipo === "fonte" ? "fonte" : "programa";
  return (
    <li ref={li} className={`${styles.efeito} ${aberta ? styles.efeitoAberto : ""}`} data-legenda={g.id}>
      <div className={styles.efeitoTopo}>
        <button type="button" className={styles.efeitoNome} aria-expanded={aberta} data-acao="abrir-legenda" onClick={() => focar(aberta ? null : g.id)}>
          {aberta ? <ChevronDown size={12} aria-hidden /> : <ChevronRight size={12} aria-hidden />}
          <span className={styles.mono} data-dado>
            {formatarTempo(g.ref.de)}
          </span>
          <span className={styles.cortaTexto} data-dado>
            {g.locutor ? `${g.locutor}: ${g.texto}` : g.texto}
          </span>
        </button>
        <span className={styles.acoes}>
          {g.origem === "ia" && !g.conferida && <Bot size={13} aria-hidden data-estado="ia" />}
          {fora && <EyeOff size={13} aria-hidden data-estado="fora" />}
          <Icone rotulo="Remover" onClick={() => mudar("Remover", (d) => void (d.legendas.itens = d.legendas.itens.filter((x) => x.id !== g.id)))}>
            <Trash2 size={13} />
          </Icone>
        </span>
      </div>
      {problema && (
        <p
          data-problema={problema.codigo}
          style={{ margin: 0, fontSize: "var(--text-xs)", color: problema.nivel === "erro" ? "var(--sicro-danger)" : "var(--sicro-warning)" }}
        >
          {problema.mensagem}
        </p>
      )}
      {aberta && (
        <div className={styles.grade}>
          <Texto rotulo="Texto" larga linhas={3} valor={g.texto} aplicar={(v) => naLegenda(g.id, (x) => void (x.texto = v))} />
          <Tempo
            rotulo="De"
            item={g.id}
            base={base}
            fps={base === "programa" ? fps : null}
            valor={g.ref.de}
            valido={(t) => t < g.ref.ate && (base === "fonte" || t >= 0)}
            aplicar={(t) => naLegenda(g.id, (x) => void (x.ref.de = t))}
          />
          <Tempo
            rotulo="Até"
            item={g.id}
            base={base}
            fps={base === "programa" ? fps : null}
            valor={g.ref.ate}
            valido={(t) => t > g.ref.de}
            aplicar={(t) => naLegenda(g.id, (x) => void (x.ref.ate = t))}
          />
          <Marca rotulo="Conferida" valor={g.conferida} aplicar={(v) => naLegenda(g.id, (x) => void (x.conferida = v))} />
        </div>
      )}
    </li>
  );
}
