/**
 * Onde moram as curvas (D7): propriedades animáveis de cada item (sobreposto: posição, giro, escala, opacidade; clipe de
 * áudio e som do trecho: volume; barra do enquadramento: a região, só na linha — o Inspetor dela é do ZS), a leitura e
 * a gravação delas no documento, e a base de tempo das chaves em slots do programa (local no sobreposto, no clipe de
 * áudio e nos clipes sem fonte; fonte no trecho, pelo mapa de quadros). O Inspetor e a linha usam as mesmas contas.
 * Dono: P4.2.
 */

import { isDraft, type Draft } from "immer";
import type { Animavel, Chave, ClipeTrecho, Cursor, EfEnquadramento, Id, Interp, LinhaResolvida, ProjetoEdicao, Pts, Seg, SobImagem, SobTitulo, TabelaQuadros } from "@domain/edicao";
import { acharEfeito, acharMidia, clipeConhecido, sobrepostoConhecido } from "../../modelo/projeto";
import { ehChaves } from "../../modelo/curva";
import { exibido, instanteDoSlot, mapaTrecho, ptsDaTabela, slotsDoQuadro, tempoDoQuadro, type MapaQuadros } from "../../modelo/tempo";
import { clipeNoSlot, slotDoCursor } from "../../inspetor/secoes/textoLivre";
import { tempoNoPrograma } from "../../palco/motor/cursor";

export type PropSobreposto = "posicao" | "rotacao" | "escala" | "opacidade";
export type Propriedade = PropSobreposto | "volume" | "regiao";

/** Rótulos de contratos-fase4.md (h): "Posição" e "Giro" do texto livre; "Escala", "Opacidade", "Volume" das curvas;
 *  a região do zoom com o nome do efeito. */
export const ROTULO_DA_PROP: Readonly<Record<Propriedade, string>> = {
  posicao: "Posição",
  rotacao: "Giro",
  escala: "Escala",
  opacidade: "Opacidade",
  volume: "Volume",
  regiao: "Enquadramento",
};

export const PROPS_SOBREPOSTO: readonly PropSobreposto[] = ["posicao", "rotacao", "escala", "opacidade"];

/** Escala fixa para desenhar a curva (opacidade 0..1, volume 0..2); as outras vão entre as chaves. */
export const FAIXA_DA_PROP: Readonly<Partial<Record<Propriedade, readonly [number, number]>>> = { opacidade: [0, 1], volume: [0, 2] };

/** Item com curvas: sobreposto (título, texto livre, imagem), clipe de áudio, o som de um trecho da principal, ou o
 *  enquadramento (`id` = o do efeito). */
export type ItemComCurva = { k: "livre"; id: Id } | { k: "audio"; id: Id } | { k: "som"; id: Id } | { k: "efeito"; id: Id };

/** Interpolação da chave nova de cada propriedade (`chaves.ts::chaveNoCursor`): o volume nasce linear sem o campo. */
export const interpNova = (p: Propriedade): Interp | null | undefined => (p === "volume" ? null : undefined);

type Mut = (d: Draft<ProjetoEdicao>) => void;

function acharSobreposto(doc: ProjetoEdicao, id: Id): SobTitulo | SobImagem | null {
  for (const t of doc.sobreposicoes)
    for (const c of t.clipes) if (sobrepostoConhecido(c) && c.id === id && c.tipo !== "video") return c as SobTitulo | SobImagem;
  return null;
}

function acharAudio(doc: ProjetoEdicao, id: Id) {
  for (const t of doc.audios) for (const c of t.clipes) if (c.id === id) return c;
  return null;
}

const porId = new WeakMap<readonly unknown[], Map<Id, ClipeTrecho>>();

function acharTrecho(doc: ProjetoEdicao, id: Id): ClipeTrecho | null {
  // a linha pergunta por cada item a cada mudança: fora de um rascunho do immer, por índice da lista
  if (isDraft(doc.principal)) {
    const c = doc.principal.find((x) => clipeConhecido(x) && x.id === id);
    return c && clipeConhecido(c) && c.tipo === "trecho" ? c : null;
  }
  let m = porId.get(doc.principal);
  if (!m) {
    m = new Map();
    for (const x of doc.principal) if (clipeConhecido(x) && x.tipo === "trecho" && !m.has(x.id)) m.set(x.id, x);
    porId.set(doc.principal, m);
  }
  return m.get(id) ?? null;
}

/** O enquadramento e o clipe da principal dono dele. */
function acharZoom(doc: ProjetoEdicao, id: Id): { e: EfEnquadramento; clipe: Id } | null {
  const a = acharEfeito(doc, id);
  return a && a.clipe && a.efeito.tipo === "enquadramento" ? { e: a.efeito, clipe: a.clipe } : null;
}

/** Propriedades com curva do item (vazio = item não achado). */
export function propsDe(doc: ProjetoEdicao, item: ItemComCurva): Propriedade[] {
  if (item.k === "livre") return acharSobreposto(doc, item.id) ? [...PROPS_SOBREPOSTO] : [];
  if (item.k === "audio") return acharAudio(doc, item.id) ? ["volume"] : [];
  if (item.k === "efeito") return acharZoom(doc, item.id) ? ["regiao"] : [];
  return acharTrecho(doc, item.id) ? ["volume"] : [];
}

/** As curvas da propriedade (posição: `x` e `y`, nessa ordem); null = o item não tem. */
export function lerProp(doc: ProjetoEdicao, item: ItemComCurva, p: Propriedade): Animavel<number>[] | null {
  if (item.k === "livre") {
    const c = acharSobreposto(doc, item.id);
    if (!c || p === "volume" || p === "regiao") return null;
    const t = c.transformacao;
    return p === "posicao" ? [t.x, t.y] : [t[p]];
  }
  if (item.k === "efeito") {
    const z = acharZoom(doc, item.id);
    // a caixa da região vai como está: o editor só mexe no tempo e na interpolação das chaves dela
    return z && p === "regiao" ? [z.e.regiao as unknown as Animavel<number>] : null;
  }
  if (p !== "volume") return null;
  if (item.k === "audio") {
    const c = acharAudio(doc, item.id);
    return c ? [c.volume] : null;
  }
  const c = acharTrecho(doc, item.id);
  return c ? [c.audio.volume] : null;
}

/** Grava as curvas da propriedade (a mesma ordem de `lerProp`). */
export function gravarProp(item: ItemComCurva, p: Propriedade, vs: readonly Animavel<number>[]): Mut {
  return (d) => {
    const doc = d as unknown as ProjetoEdicao;
    if (item.k === "livre") {
      const c = acharSobreposto(doc, item.id);
      if (!c || p === "volume" || p === "regiao") return;
      if (p === "posicao") {
        c.transformacao.x = vs[0] as Animavel<number>;
        c.transformacao.y = vs[1] as Animavel<number>;
      } else c.transformacao[p] = vs[0] as Animavel<number>;
      return;
    }
    if (item.k === "efeito") {
      const a = acharEfeito(doc, item.id);
      if (a && p === "regiao" && a.efeito.tipo === "enquadramento") (a.lista[a.indice] as EfEnquadramento).regiao = vs[0] as unknown as EfEnquadramento["regiao"];
      return;
    }
    if (p !== "volume") return;
    if (item.k === "audio") {
      const c = acharAudio(doc, item.id);
      if (c) c.volume = vs[0] as Animavel<number>;
      return;
    }
    const c = acharTrecho(doc, item.id);
    if (c) c.audio.volume = vs[0] as Animavel<number>;
  };
}

// ---------------------------------------------------------------- base de tempo das chaves

/** As chaves do item em slots do programa: `[de_q, ate_q)` e as duas mãos da conversão. */
export interface BaseDoItem {
  de_q: number;
  ate_q: number;
  deSlot(q: number): Seg;
  paraSlot(t: Seg): number;
  /** Trecho com tabela: o pts do quadro mostrado no slot (a chave presa a quadro leva o dela). */
  ptsDoSlot?(q: number): Pts;
  /** Quadro na tela de um instante (sem limite): o mesmo número = a mesma chave do cursor (`instanteNoQuadro`). */
  quadro(t: Seg): number;
}

const limitar = (v: number, a: number, b: number) => Math.min(Math.max(v, a), b);

/** Quadro de um instante local (o slot dele desde o começo do item). */
export const quadroLocal = (fps: LinhaResolvida["fps"]) => (t: Seg) => Math.round((t * fps.num) / fps.den);

/** Quadro de um instante de fonte do trecho: o mostrado pela tabela; sem ela, pela velocidade constante. */
export function quadroDoTrecho(c: ClipeTrecho, tab: TabelaQuadros | null, fps: LinhaResolvida["fps"]): (t: Seg) => number {
  if (tab) return (t) => exibido(tab, t);
  const v = c.velocidade.tipo === "constante" && c.velocidade.fator > 0 ? c.velocidade.fator : 1;
  return (t) => Math.round(((t - c.inicio_s) / v) * (fps.num / fps.den));
}

/** Base local: τ = (q − início)·den/num. */
function baseLocal(de_q: number, n_q: number, fps: LinhaResolvida["fps"]): BaseDoItem {
  return {
    de_q,
    ate_q: de_q + n_q,
    deSlot: (q) => instanteDoSlot(q - de_q, fps),
    paraSlot: (t) => de_q + Math.round((t * fps.num) / fps.den),
    quadro: quadroLocal(fps),
  };
}

/** Base de fonte do trecho: o quadro mostrado no slot (o mapa do motor); sem tabela, pela velocidade constante. */
function baseDoTrecho(c: ClipeTrecho, de_q: number, n_q: number, linha: LinhaResolvida, tab: TabelaQuadros | null, fonte: { j_in: number; j_out: number } | null): BaseDoItem {
  const fps = linha.fps;
  let mapa: MapaQuadros | null = null;
  if (tab && fonte)
    try {
      mapa = mapaTrecho(tab, fonte.j_in, fonte.j_out, c.velocidade, fps);
    } catch {
      mapa = null;
    }
  if (tab && mapa) {
    const m = mapa;
    const j = (q: number) => m.quadro_do_slot[limitar(q - de_q, 0, m.n_q - 1)] as number;
    return {
      de_q,
      ate_q: de_q + n_q,
      quadro: quadroDoTrecho(c, tab, fps),
      ptsDoSlot: (q) => ptsDaTabela(tab)[j(q)] as Pts,
      deSlot: (q) => tempoDoQuadro(tab, j(q)),
      paraSlot: (t) => {
        const j = exibido(tab, t);
        const s = slotsDoQuadro(m, j);
        if (s) return de_q + s[0];
        // quadro descartado (aceleração) ou fora do trecho: a ponta mais perto
        return j < m.j_in ? de_q : j >= m.j_out ? de_q + n_q - 1 : de_q + m.quadro_do_slot.findIndex((x) => x >= j);
      },
    };
  }
  const v = c.velocidade.tipo === "constante" && c.velocidade.fator > 0 ? c.velocidade.fator : 1;
  return {
    de_q,
    ate_q: de_q + n_q,
    quadro: quadroDoTrecho(c, null, fps),
    deSlot: (q) => c.inicio_s + instanteDoSlot(q - de_q, fps) * v,
    paraSlot: (t) => de_q + Math.round(((t - c.inicio_s) / v) * (fps.num / fps.den)),
  };
}

/** Base das chaves do item na linha resolvida; null = fora dela. `tabela` = a da mídia (`MidiaRef.id`). */
export function baseDoItem(doc: ProjetoEdicao, linha: LinhaResolvida, item: ItemComCurva, tabela: (midia: Id) => TabelaQuadros | null): BaseDoItem | null {
  const clipe = clipeDoItem(doc, item);
  if (clipe) {
    // som do trecho e enquadramento: a base do clipe (fonte no trecho; local nos clipes sem fonte)
    const r = linha.clipes.find((x) => x.id === clipe);
    const c = doc.principal.find((x) => clipeConhecido(x) && x.id === clipe);
    if (!r || !c || !clipeConhecido(c) || r.n_q <= 0) return null;
    if (c.tipo === "trecho") return baseDoTrecho(c, r.inicio_q, r.n_q, linha, tabela(c.midia), r.fonte);
    // congelado: a base é o instante da fonte (todas as chaves no mesmo quadro); tela: a célula de referência
    if (c.tipo === "congelar" || c.tipo === "tela") return null;
    return baseLocal(r.inicio_q, r.n_q, linha.fps);
  }
  const l = linha.livres.find((x) => x.id === item.id);
  if (!l || l.n_q <= 0) return null;
  return baseLocal(l.inicio_q, l.n_q, linha.fps);
}

/** Clipe da principal em cuja base as chaves do item estão (som do trecho, enquadramento); null = base local própria. */
export function clipeDoItem(doc: ProjetoEdicao, item: ItemComCurva): Id | null {
  if (item.k === "som") return item.id;
  if (item.k === "efeito") return acharZoom(doc, item.id)?.clipe ?? null;
  return null;
}

/** Instante do cursor na base do item: `t` dentro do item (a ponta mais perto, se o cursor está fora) e se o cursor
 *  está nele. A chave nova só nasce com o cursor no item; o campo grava no instante mais perto. */
export function instanteNoItem(doc: ProjetoEdicao, linha: LinhaResolvida, item: ItemComCurva, base: BaseDoItem, cursor: Cursor): { t: Seg; dentro: boolean } {
  const clipe = clipeDoItem(doc, item);
  const c = clipe ? acharTrecho(doc, clipe) : null;
  if (c && cursor.modo === "clipe") {
    if (cursor.clipe !== c.id) return { t: c.inicio_s, dentro: false };
    const t = limitar(cursor.t, c.inicio_s, c.fim_s);
    return { t, dentro: cursor.t >= c.inicio_s - 1e-9 && cursor.t <= c.fim_s + 1e-9 };
  }
  const q = cursor.modo === "programa" ? Math.round(cursor.q) : slotDoCursor(doc, linha, cursor);
  const dentro = q >= base.de_q && q < base.ate_q;
  // som do trecho com o cursor de programa: a conta do palco e do Inspetor (P3.3, `tempoNoClipe`)
  const noPrograma = c && cursor.modo === "programa" && dentro ? tempoNoPrograma(doc, c.id, q) : null;
  return { t: noPrograma ?? base.deSlot(limitar(q, base.de_q, base.ate_q - 1)), dentro };
}

/** Cursor no slot `q` do programa, no modo em que o cursor já está: no programa, o slot; no clipe, o clipe da
 *  principal sob `q` no quadro mostrado (trecho: tempo de fonte pelo mapa; congelado: o instante; outros: local). */
export function cursorNoSlot(doc: ProjetoEdicao, linha: LinhaResolvida, q: number, atual: Cursor, tabela: (midia: Id) => TabelaQuadros | null): Cursor {
  if (atual.modo === "programa") return { modo: "programa", q };
  const r = clipeNoSlot(linha, q);
  const c = r ? doc.principal.find((x) => clipeConhecido(x) && x.id === r.id) : undefined;
  if (!r || !c || !clipeConhecido(c)) return { modo: "programa", q };
  const local = limitar(q - r.inicio_q, 0, Math.max(r.n_q - 1, 0));
  if (c.tipo === "trecho") return { modo: "clipe", clipe: c.id, t: baseDoTrecho(c, r.inicio_q, r.n_q, linha, tabela(c.midia), r.fonte).deSlot(r.inicio_q + local) };
  if (c.tipo === "congelar") return { modo: "clipe", clipe: c.id, t: c.instante_s };
  return { modo: "clipe", clipe: c.id, t: instanteDoSlot(local, linha.fps) };
}

/** Cursor na chave `t` do item: o som do trecho, no próprio trecho (tempo de fonte exato); os outros, pelo slot. */
export function cursorNaChave(doc: ProjetoEdicao, linha: LinhaResolvida, item: ItemComCurva, base: BaseDoItem, t: Seg, atual: Cursor, tabela: (midia: Id) => TabelaQuadros | null): Cursor {
  const clipe = clipeDoItem(doc, item);
  if (clipe && atual.modo === "clipe") return { modo: "clipe", clipe, t };
  return cursorNoSlot(doc, linha, base.paraSlot(t), atual, tabela);
}

/** Chave presa a quadro que mudou de instante leva o pts do quadro novo (o do slot dela); sem pts antes, sem pts. */
export function comPtsDoSlot<T>(antes: Animavel<T>, depois: Animavel<T>, base: BaseDoItem): Animavel<T> {
  if (!ehChaves(antes) || !ehChaves(depois) || antes.length !== depois.length || !base.ptsDoSlot) return depois;
  const pts = base.ptsDoSlot;
  return depois.map((k, i) => {
    const a = antes[i] as Chave<T>;
    if (a.pts == null || k.t === a.t) return k;
    return { ...k, pts: pts(base.paraSlot(k.t)) };
  });
}

/** Tabela de quadros de uma mídia pelo id dela (a que o palco já leu). */
export const tabelaPor = (doc: ProjetoEdicao, ler: (sha: string | undefined) => TabelaQuadros | null) => (midia: Id) => ler(acharMidia(doc, midia)?.sha256);
