/**
 * As curvas de um item no Inspetor (D7): uma raia por propriedade — rótulo, "Chave no cursor" / "Tirar chave" (losango
 * com + ou −) e a faixa com as chaves (`EditorCurva`) e a cabeça —, e a interpolação da chave escolhida (Linear, Suave,
 * Degrau). Cada ação é um passo. A raia escolhida é a que o item mostra na linha do tempo. Dono: P4.2.
 */

import { useMemo } from "react";
import { create } from "zustand";
import { DiamondMinus, DiamondPlus } from "lucide-react";
import type { Animavel, Chave, Id, Interp, LinhaResolvida, ProjetoEdicao, Seg } from "@domain/edicao";
import { useEdicaoStore } from "../../store/edicaoStore";
import { avaliar, ehChaves } from "../../modelo/curva";
import { resolver } from "../../modelo/tempo";
import { recursoDe, useRecursos } from "../../palco/recursos";
import { mudar } from "../../inspetor/acoes";
import { Deslizante } from "../../inspetor/campos";
import { acompanhar, chaveEm, chaveNoCursor, gravarEm, instanteNoQuadro, interpDe, tirarChaveEm, trocarInterp } from "./chaves";
import { EditorCurva, INTERPS } from "./EditorCurva";
import {
  FAIXA_DA_PROP,
  baseDoItem,
  comPtsDoSlot,
  cursorNaChave,
  gravarProp,
  instanteNoItem,
  interpNova,
  lerProp,
  propsDe,
  ROTULO_DA_PROP,
  tabelaPor,
  type BaseDoItem,
  type ItemComCurva,
  type Propriedade,
} from "./itens";
import ins from "../../inspetor/Inspetor.module.css";
import st from "./Curvas.module.css";

/** A raia em foco de cada item (a que a linha do tempo mostra) e a chave escolhida nela, pelo instante: chave que entra
 *  antes dela, desfazer e refazer não trocam a escolhida por outra. Só tela. */
interface CurvaUi {
  foco: Record<Id, Propriedade>;
  escolhida: { item: Id; prop: Propriedade; t: Seg } | null;
  focar(item: Id, prop: Propriedade): void;
  escolher(item: Id, prop: Propriedade, t: Seg | null): void;
}
export const useCurvaUi = create<CurvaUi>((set) => ({
  foco: {},
  escolhida: null,
  focar: (item, prop) => set((s) => (s.foco[item] === prop ? s : { foco: { ...s.foco, [item]: prop } })),
  escolher: (item, prop, t) => set((s) => ({ foco: { ...s.foco, [item]: prop }, escolhida: t === null ? null : { item, prop, t } })),
}));

/** Índice da escolhida na curva (-1 = nenhuma ou saiu). */
export const indiceEscolhido = (a: Animavel<number> | undefined, e: { t: Seg } | null): number => (e && a !== undefined ? chaveEm(a, e.t) : -1);

/** "Escolher" a chave `i` da curva como está agora no documento (depois de um `aplicar`, o instante novo). */
export function escolherNoDoc(item: ItemComCurva, prop: Propriedade, i: number | null): void {
  const d = useEdicaoStore.getState().doc;
  const a = d ? lerProp(d, item, prop)?.[0] : undefined;
  const k = i !== null && a !== undefined && ehChaves(a) ? a[i] : undefined;
  useCurvaUi.getState().escolher(item.id, prop, k ? k.t : null);
}


const tabelaDoPalco = (doc: ProjetoEdicao) => tabelaPor(doc, (sha) => recursoDe(sha).tabela);

const linhas = new WeakMap<ProjetoEdicao, { versao: number; linha: LinhaResolvida }>();

/** Linha resolvida com as tabelas que o palco já leu (a mesma conta do Inspetor e da linha). Uma por documento e
 *  versão das tabelas: cada item da linha com curva pedia a sua (200 clipes = 200 resolver a cada mudança). */
export function linhaDoDoc(doc: ProjetoEdicao): LinhaResolvida {
  const versao = useRecursos.getState().versao;
  const guardada = linhas.get(doc);
  if (guardada && guardada.versao === versao) return guardada.linha;
  const linha = resolver(doc, { tabela: tabelaDoPalco(doc) });
  linhas.set(doc, { versao, linha });
  return linha;
}

/** As curvas de uma propriedade com o que mudou numa delas (a posição leva `y` junto de `x`). */
export function comPrimeira(vs: readonly Animavel<number>[], nova: Animavel<number>): Animavel<number>[] {
  const [a, ...resto] = vs;
  return [nova, ...resto.map((o) => acompanhar(a as Animavel<number>, nova, o))];
}

/** "Chave no cursor" (valor do instante em todas as curvas da propriedade) ou "Tirar chave" (a do instante). */
export function alternarChave(vs: readonly Animavel<number>[], t: number, prop: Propriedade): { rotulo: string; vs: Animavel<number>[] } {
  if (chaveEm(vs[0] as Animavel<number>, t) >= 0) return { rotulo: "Tirar chave", vs: vs.map((a) => tirarChaveEm(a, t)) };
  return { rotulo: "Chave no cursor", vs: vs.map((a) => chaveNoCursor(a, t, avaliar(a, t), interpNova(prop))) };
}

interface RaiaProps {
  doc: ProjetoEdicao;
  linha: LinhaResolvida;
  item: ItemComCurva;
  prop: Propriedade;
  base: BaseDoItem;
}

function Raia({ doc, linha, item, prop, base }: RaiaProps) {
  const cursor = useEdicaoStore((s) => s.cursor);
  const so = useEdicaoStore((s) => s.soLeitura);
  const foco = useCurvaUi((s) => s.foco[item.id]);
  const e = useCurvaUi((s) => (s.escolhida?.item === item.id && s.escolhida.prop === prop ? s.escolhida : null));
  const vs = lerProp(doc, item, prop);
  if (!vs) return null;
  const escolhida = indiceEscolhido(vs[0], e);
  const rotulo = ROTULO_DA_PROP[prop];
  const inst = instanteNoItem(doc, linha, item, base, cursor);
  // a chave que já está no quadro do cursor é a dele (o instante exato varia com o modo do cursor)
  const tChave = instanteNoQuadro(vs[0] as Animavel<number>, inst.t, base.quadro);
  const naChave = inst.dentro && chaveEm(vs[0] as Animavel<number>, tChave) >= 0;
  const cursorQ = cursor.modo === "programa" ? Math.round(cursor.q) : inst.dentro ? base.paraSlot(inst.t) : null;
  const ui = useCurvaUi.getState();
  const alternar = () => {
    const d = useEdicaoStore.getState().doc;
    const atuais = d ? lerProp(d, item, prop) : null;
    if (!atuais || !inst.dentro) return;
    const t = instanteNoQuadro(atuais[0] as Animavel<number>, inst.t, base.quadro);
    const r = alternarChave(atuais, t, prop);
    mudar(r.rotulo, gravarProp(item, prop, r.vs));
    ui.escolher(item.id, prop, r.rotulo === "Tirar chave" ? null : t);
  };
  return (
    <div className={`${st.raia} ${foco === prop ? st.emFoco : ""}`} data-raia={prop}>
      <span className={st.nome}>{rotulo}</span>
      <button
        type="button"
        className={`${st.chaveBtn} ${naChave ? st.naChave : ""}`}
        title={naChave ? "Tirar chave" : "Chave no cursor"}
        aria-label={naChave ? "Tirar chave" : "Chave no cursor"}
        aria-pressed={naChave}
        disabled={so || !inst.dentro}
        data-acao={item.k === "som" ? "marcar-volume" : "chave-no-cursor"}
        onClick={alternar}
      >
        {naChave ? <DiamondMinus size={13} /> : <DiamondPlus size={13} />}
      </button>
      <EditorCurva
        rotulo={rotulo}
        valor={vs[0] as Animavel<number>}
        de_q={base.de_q}
        ate_q={base.ate_q}
        deSlot={base.deSlot}
        paraSlot={base.paraSlot}
        cursor_q={cursorQ}
        faixa={FAIXA_DA_PROP[prop] ?? (prop === "posicao" ? undefined : "auto")}
        escolhida={escolhida >= 0 ? escolhida : null}
        desligado={so}
        onEscolher={(i) => escolherNoDoc(item, prop, i)}
        onIr={(t) => {
          const s = useEdicaoStore.getState();
          if (s.doc) s.posicionar(cursorNaChave(s.doc, linhaDoDoc(s.doc), item, base, t, s.cursor, tabelaDoPalco(s.doc)));
        }}
        aplicar={(v) => {
          const d = useEdicaoStore.getState().doc;
          const atuais = d ? lerProp(d, item, prop) : null;
          if (atuais) mudar(rotulo, gravarProp(item, prop, comPrimeira(atuais, comPtsDoSlot(atuais[0] as Animavel<number>, v, base))));
        }}
      />
    </div>
  );
}

/** Interpolação da chave escolhida até a seguinte; a última não tem trecho depois dela. */
function Interpolacao({ doc, item }: { doc: ProjetoEdicao; item: ItemComCurva }) {
  const so = useEdicaoStore((s) => s.soLeitura);
  const e = useCurvaUi((s) => (s.escolhida?.item === item.id ? s.escolhida : null));
  const vs = e ? lerProp(doc, item, e.prop) : null;
  const ks = vs && ehChaves(vs[0] as Animavel<number>) ? (vs[0] as Chave<number>[]) : null;
  const i = ks ? indiceEscolhido(ks, e) : -1;
  const k = ks ? ks[i] : undefined;
  if (!e || !ks || !k) return null;
  const ultima = i === ks.length - 1;
  const valor = interpDe(k);
  const trocar = (v: Interp) => {
    if (v === valor) return;
    const d = useEdicaoStore.getState().doc;
    const atuais = d ? lerProp(d, item, e.prop) : null;
    const a = atuais?.[0];
    if (!atuais || !a || !ehChaves(a)) return;
    mudar(INTERPS.find((o) => o.v === v)!.r, gravarProp(item, e.prop, comPrimeira(atuais, trocarInterp(a, chaveEm(a, e.t), v))));
  };
  return (
    <div className={`${ins.seg} ${st.interp}`} role="group" aria-label={ROTULO_DA_PROP[e.prop]} data-interp-de={e.prop}>
      {INTERPS.map((o) => (
        <button
          key={o.v}
          type="button"
          className={`${ins.segBtn} ${o.v === valor ? ins.segOn : ""}`}
          aria-pressed={o.v === valor}
          data-valor={o.v}
          disabled={so || ultima}
          onClick={() => trocar(o.v)}
        >
          {o.r}
        </button>
      ))}
    </div>
  );
}

/** Bloco das curvas do item no Inspetor (todas as propriedades dele, ou só `props`). */
export function CurvasDoItem({ item, props }: { item: ItemComCurva; props?: readonly Propriedade[] }) {
  const doc = useEdicaoStore((s) => s.doc);
  // a tabela que chega depois muda os slots do trecho
  const versao = useRecursos((s) => s.versao);
  const linha = useMemo(() => (doc ? linhaDoDoc(doc) : null), [doc, versao]);
  // o mapa do trecho custa: refeito só com o documento
  const base = useMemo(() => (doc && linha ? baseDoItem(doc, linha, item, tabelaDoPalco(doc)) : null), [doc, linha, item.k, item.id]);
  if (!doc || !linha) return null;
  if (!base) return null;
  const lista = (props ?? propsDe(doc, item)).filter((p) => propsDe(doc, item).includes(p));
  if (lista.length === 0) return null;
  return (
    <div className={`${st.bloco} ${ins.larga}`} data-curvas={item.id}>
      {lista.map((p) => (
        <Raia key={p} doc={doc} linha={linha} item={item} prop={p} base={base} />
      ))}
      <Interpolacao doc={doc} item={item} />
    </div>
  );
}

/** Instante da chave do cursor no item, na conta da raia (a chave que já está no quadro do cursor é a dele); null = o
 *  item fora da linha. É onde o campo do valor grava. */
export function useInstanteDaChave(item: ItemComCurva, a: Animavel<number> | undefined): Seg | null {
  const doc = useEdicaoStore((s) => s.doc);
  const cursor = useEdicaoStore((s) => s.cursor);
  const versao = useRecursos((s) => s.versao);
  const linha = useMemo(() => (doc ? linhaDoDoc(doc) : null), [doc, versao]);
  const base = useMemo(() => (doc && linha ? baseDoItem(doc, linha, item, tabelaDoPalco(doc)) : null), [doc, linha, item.k, item.id]);
  if (!doc || !linha || !base || a === undefined) return null;
  return instanteNoQuadro(a, instanteNoItem(doc, linha, item, base, cursor).t, base.quadro);
}

/** "Volume" do item no cursor (0..2): fixo troca o valor; com chaves, grava a chave do instante (D7). */
export function VolumeNoCursor({ item }: { item: ItemComCurva }) {
  const doc = useEdicaoStore((s) => s.doc);
  const a = doc ? lerProp(doc, item, "volume")?.[0] : undefined;
  const t = useInstanteDaChave(item, a);
  if (a === undefined || t === null) return null;
  return (
    <Deslizante
      rotulo="Volume"
      larga
      item={item.id}
      valor={avaliar(a, t)}
      min={0}
      max={2}
      passo={0.05}
      mostrar={(v) => `${Math.round(v * 100)} %`}
      aplicar={(v) => gravarProp(item, "volume", [gravarEm(a, t, v, interpNova("volume"))])}
    />
  );
}
