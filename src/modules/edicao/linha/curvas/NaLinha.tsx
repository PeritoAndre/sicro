/**
 * Ponto de montagem na linha do tempo (P4.1, fecho do bloco 6.4): as chaves da raia em foco do item (a escolhida no
 * Inspetor; sem escolha, a primeira com chaves) por cima dele, arrastáveis, com o menu da interpolação; no volume e na
 * opacidade, a curva desenhada. Na barra do enquadramento, as chaves da região (o zoom do ZS), presas ao quadro. O
 * `ClipeNaLinha` do P4.1 põe `<CurvasNaLinha item={it} />` dentro do item ou da barra. Dono: P4.2.
 */

import { useMemo } from "react";
import type { Animavel, Id } from "@domain/edicao";
import { useEdicaoStore } from "../../store/edicaoStore";
import { ehChaves } from "../../modelo/curva";
import { useRecursos, recursoDe } from "../../palco/recursos";
import { mudar } from "../../inspetor/acoes";
import { EditorCurva } from "./EditorCurva";
import { comPrimeira, escolherNoDoc, indiceEscolhido, linhaDoDoc, useCurvaUi } from "./CurvasDoItem";
import { FAIXA_DA_PROP, baseDoItem, comPtsDoSlot, cursorNaChave, gravarProp, lerProp, propsDe, ROTULO_DA_PROP, tabelaPor, type ItemComCurva, type Propriedade } from "./itens";

/** Trilha do som dos trechos na linha (`linha/modelo.ts::TRILHA_SOM` do P4.1). */
const TRILHA_SOM = "som";

/** O que o P4.1 sabe do item (`ItemNaLinha`): tipo, id, trilha, slots e o tipo no documento. */
export interface ItemDaLinha {
  k: string;
  id: Id;
  trilha: string;
  de_q?: number;
  ate_q?: number;
  tipo?: string;
}

/** O item da linha que tem curvas: sobreposto, clipe de áudio, o som de um trecho ou a barra do enquadramento. */
export function itemComCurva(it: ItemDaLinha): ItemComCurva | null {
  if (it.k === "livre") return { k: "livre", id: it.id };
  if (it.k === "audio") return { k: "audio", id: it.id };
  if (it.k === "clipe" && it.trilha === TRILHA_SOM) return { k: "som", id: it.id };
  if (it.k === "efeito" && it.tipo === "enquadramento") return { k: "efeito", id: it.id };
  return null;
}

export function CurvasNaLinha({ item: it }: { item: ItemDaLinha }) {
  const doc = useEdicaoStore((s) => s.doc);
  const so = useEdicaoStore((s) => s.soLeitura);
  const versao = useRecursos((s) => s.versao);
  const foco = useCurvaUi((s) => s.foco[it.id]);
  const escolhida = useCurvaUi((s) => (s.escolhida?.item === it.id ? s.escolhida : null));
  const item = itemComCurva(it);
  const animada = (p: Propriedade) => !!doc && !!item && !!lerProp(doc, item, p)?.some((a) => ehChaves(a));
  const props = doc && item ? propsDe(doc, item) : [];
  // só o item com chaves paga a linha e a base (o mapa do trecho): os sem curva não fazem conta a cada mudança
  const comChaves = props.some(animada);
  const linha = useMemo(() => (doc && comChaves ? linhaDoDoc(doc) : null), [doc, versao, comChaves]);
  const base = useMemo(() => (doc && linha && item ? baseDoItem(doc, linha, item, tabelaPor(doc, (sha) => recursoDe(sha).tabela)) : null), [doc, linha, item?.k, item?.id]);
  if (!doc || !linha || !item) return null;
  const prop = foco && props.includes(foco) ? foco : props.find(animada);
  const vs = prop ? lerProp(doc, item, prop) : null;
  const tabela = tabelaPor(doc, (sha) => recursoDe(sha).tabela);
  if (!prop || !vs || !base || !ehChaves(vs[0] as Animavel<number>)) return null;
  const rotulo = ROTULO_DA_PROP[prop];
  // a faixa ocupa o item (ou a barra do efeito, que é só o `ativo` dele)
  const de_q = it.de_q ?? base.de_q;
  const ate_q = it.ate_q ?? base.ate_q;
  return (
    <EditorCurva
      modo="linha"
      rotulo={rotulo}
      valor={vs[0] as Animavel<number>}
      de_q={de_q}
      ate_q={ate_q}
      deSlot={base.deSlot}
      paraSlot={base.paraSlot}
      faixa={FAIXA_DA_PROP[prop]}
      escolhida={escolhida?.prop === prop && indiceEscolhido(vs[0], escolhida) >= 0 ? indiceEscolhido(vs[0], escolhida) : null}
      desligado={so}
      onEscolher={(i) => escolherNoDoc(item, prop, i)}
      onIr={(t) => {
        const s = useEdicaoStore.getState();
        if (s.doc) s.posicionar(cursorNaChave(s.doc, linhaDoDoc(s.doc), item, base, t, s.cursor, tabela));
      }}
      aplicar={(v) => {
        const d = useEdicaoStore.getState().doc;
        const atuais = d ? lerProp(d, item, prop) : null;
        if (atuais) mudar(rotulo, gravarProp(item, prop, comPrimeira(atuais, comPtsDoSlot(atuais[0] as Animavel<number>, v, base))));
      }}
    />
  );
}
