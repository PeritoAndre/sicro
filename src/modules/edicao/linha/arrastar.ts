/**
 * Gestos da linha do tempo sobre o rascunho (um arrasto = um passo, aplicado ao soltar). Regras em contratos-fase4.md
 * D3 (principal), D5 (livre → `pos`) e D6 (faixas de efeito → `ativo`, em `efeitos.ts`). Puro. Dono: P4.1.
 */

import { current, isDraft, original, type Draft } from "immer";
import type { ClipeAudio, ClipePrincipal, ClipeSobreposto, Id, LinhaResolvida, Posicao, ProjetoEdicao, Seg } from "@domain/edicao";
import { clipeConhecido, efeitoConhecido, moverClipe, novoId, sobrepostoConhecido } from "../modelo/projeto";
import { exibido, type ContextoTempo } from "../modelo/tempo";
import { aparar as apararFaixa } from "../palco/faixa";
import { comIdsNovos } from "../sequencia/contas";
import { donoDoClipe, marcadoresNaLinha, quadroNoSlot, tempoDoSlot, trilhasDaLinha, type MarcadorNaLinha, type TrilhaNaLinha } from "./modelo";
import { tabelasDaTela } from "./tabelas";

export type Ferramenta = "selecionar" | "lamina";

/** Raio do ímã em px da tela. */
export const IMA_PX = 8;

/** O slot mais perto de `q` entre os `alvos` (bordas de clipe, cabeça, marcadores, chaves) a até `raio_q`; senão `q`. */
export function imantar(q: number, alvos: readonly number[], raio_q: number): number {
  let melhor = q;
  let dist = Infinity;
  for (const a of alvos) {
    const x = Math.abs(a - q);
    if (x <= raio_q && x < dist) {
      dist = x;
      melhor = a;
    }
  }
  return melhor;
}

/** Alvos do ímã já com as trilhas da tela: bordas de todo item (menos as do `exceto`), losangos, marcadores e cabeça. */
export function alvosDasTrilhas(trilhas: readonly TrilhaNaLinha[], marcadores: readonly MarcadorNaLinha[], cabeca_q: number | null, exceto: Id | null): number[] {
  const s = new Set<number>();
  for (const t of trilhas)
    for (const it of t.itens) {
      if (it.chaves) for (const k of it.chaves) s.add(k);
      if (it.id === exceto) continue;
      s.add(it.de_q);
      s.add(it.ate_q);
    }
  for (const m of marcadores) s.add(m.q);
  if (cabeca_q !== null && Number.isFinite(cabeca_q)) s.add(cabeca_q);
  return [...s].sort((a, b) => a - b);
}

/** Bordas de todos os itens, a cabeça e os marcadores: os alvos do ímã (sem os do próprio item arrastado). */
// CONTRATO: `ctx` opcional (tabelas das mídias; sem ele, as da tela).
export function alvosDoIma(doc: ProjetoEdicao, linha: LinhaResolvida, cabeca_q: number, exceto: Id | null, ctx?: ContextoTempo): number[] {
  const c = ctx ?? tabelasDaTela(doc);
  return alvosDasTrilhas(trilhasDaLinha(doc, linha, c), marcadoresNaLinha(doc, linha, c), cabeca_q, exceto);
}

/** O objeto de verdade por trás de um rascunho do immer (o mapa do clipe fica guardado por ele). */
const base = <T extends object>(x: T): T => (isDraft(x) ? (original(x as Draft<T>) as T) : x);

const indiceNaPrincipal = (d: ProjetoEdicao, id: Id) => d.principal.findIndex((c) => (c as { id?: Id }).id === id);

/** V1: índice da lista inteira (a de `moverPara`) em que o clipe arrastado entra quando solto no slot `q`: antes do
 *  primeiro dos outros clipes da linha cujo meio passa de `q`; depois do último. */
export function destinoNaPrincipal(doc: ProjetoEdicao, linha: LinhaResolvida, clipe: Id, q: number): number {
  const i = indiceNaPrincipal(doc, clipe);
  const outros = linha.clipes.filter((c) => c.id !== clipe);
  if (i < 0 || outros.length === 0) return i;
  const k = outros.filter((c) => c.inicio_q + c.n_q / 2 <= q).length;
  const semMim = (j: number) => j - (i < j ? 1 : 0);
  if (k < outros.length) return semMim(indiceNaPrincipal(doc, outros[k]!.id));
  return semMim(indiceNaPrincipal(doc, outros[outros.length - 1]!.id)) + 1;
}

/** Slot da fronteira onde o clipe entraria (a linha de inserção na tela). */
export function fronteiraDoDestino(linha: LinhaResolvida, clipe: Id, q: number): number {
  const outros = linha.clipes.filter((c) => c.id !== clipe);
  const k = outros.filter((c) => c.inicio_q + c.n_q / 2 <= q).length;
  if (outros.length === 0) return 0;
  const alvo = outros[Math.min(k, outros.length - 1)]!;
  return k < outros.length ? alvo.inicio_q : alvo.inicio_q + alvo.n_q;
}

/** V1: arrastar = inserir o clipe na posição `para` da principal (a mesma operação de `sequencia/acoes.moverPara`). */
export function moverNaPrincipal(d: Draft<ProjetoEdicao>, clipe: Id, para: number): void {
  const i = indiceNaPrincipal(d as ProjetoEdicao, clipe);
  if (i < 0) return;
  moverClipe(d as ProjetoEdicao, i, Math.min(Math.max(para, 0), d.principal.length - 1));
}

/** V1: aparar a borda `lado` até o slot `q` (início/fim do trecho pelo quadro exibido; duração dos sem fonte).
 *  Trecho e tela: fora do clipe a conta segue pela velocidade dele (alargar); nunca menos de um quadro. */
// CONTRATO: `ctx` opcional (tabelas das mídias; sem ele, as da tela).
export function aparar(d: Draft<ProjetoEdicao>, linha: LinhaResolvida, clipe: Id, lado: "inicio" | "fim", q: number, ctx?: ContextoTempo): void {
  const rc = linha.clipes.find((x) => x.id === clipe);
  const i = indiceNaPrincipal(d as ProjetoEdicao, clipe);
  const c = d.principal[i] as ClipePrincipal | undefined;
  if (!rc || !c || !clipeConhecido(c)) return;
  const fps = linha.fps;
  const passo = fps.den / fps.num;
  if (c.tipo === "trecho" || c.tipo === "tela") {
    const ct = ctx ?? tabelasDaTela(base(d as ProjetoEdicao));
    const dono = donoDoClipe(base(c), rc, ct, fps);
    const midia = c.tipo === "trecho" ? c.midia : c.celulas[c.referencia]?.midia;
    const tab = midia ? ct.tabela(midia) : null;
    let s = Math.round(q) - rc.inicio_q;
    if (lado === "inicio") {
      s = Math.min(s, rc.n_q - 1);
      if (tab && dono.mapa) {
        const j = s >= 0 ? quadroNoSlot(dono.mapa, s) : exibido(tab, tempoDoSlot(dono, s));
        const p = apararFaixa(c, "inicio", j, tab);
        if (p) Object.assign(c, p);
      } else {
        const t = c.inicio_s + s * passo * dono.fator;
        if (t < c.fim_s) {
          c.inicio_s = t;
          c.inicio_pts = null;
        }
      }
      return;
    }
    s = Math.max(s, 1);
    if (tab && dono.mapa) {
      const j = s <= rc.n_q ? quadroNoSlot(dono.mapa, s - 1) + 1 : exibido(tab, tempoDoSlot(dono, s - 1)) + 1;
      const p = apararFaixa(c, "fim", j, tab);
      if (p) Object.assign(c, p);
    } else {
      const t = c.inicio_s + s * passo * dono.fator;
      if (t > c.inicio_s) {
        c.fim_s = t;
        c.fim_pts = null;
      }
    }
    return;
  }
  // congelado, imagem, título: a duração (o começo fica no lugar: a principal é contígua)
  const n = lado === "fim" ? Math.round(q) - rc.inicio_q : rc.inicio_q + rc.n_q - Math.round(q);
  c.duracao_s = Math.max(1, n) * passo;
}

/** Lâmina num clipe sem fonte (congelado, imagem, título): duas cópias com a duração dividida no slot `q`. A segunda
 *  fica sem transição de entrada e com os `ativo` dos efeitos trazidos para o tempo local dela. Devolve o id dela. */
export function dividirSemFonte(d: Draft<ProjetoEdicao>, linha: LinhaResolvida, clipe: Id, q: number): Id | null {
  const rc = linha.clipes.find((x) => x.id === clipe);
  const i = indiceNaPrincipal(d as ProjetoEdicao, clipe);
  const c = d.principal[i] as ClipePrincipal | undefined;
  if (!rc || !c || !clipeConhecido(c) || (c.tipo !== "congelar" && c.tipo !== "imagem" && c.tipo !== "titulo")) return null;
  const s = Math.round(q) - rc.inicio_q;
  if (s <= 0 || s >= rc.n_q) return null;
  const passo = linha.fps.den / linha.fps.num;
  const d1 = s * passo;
  const copia = comIdsNovos((isDraft(c) ? current(c) : c) as ClipePrincipal & { duracao_s: Seg });
  copia.duracao_s = (rc.n_q - s) * passo;
  copia.transicao_entrada = null;
  for (const e of copia.efeitos) {
    if (!efeitoConhecido(e) || !e.ativo) continue;
    e.ativo = { de: e.ativo.de == null ? null : e.ativo.de - d1, ate: e.ativo.ate == null ? null : e.ativo.ate - d1 };
  }
  (c as { duracao_s: Seg }).duracao_s = d1;
  d.principal.splice(i + 1, 0, copia as Draft<ClipePrincipal>);
  return copia.id;
}

// ---- livres (sobrepostos e clipes de áudio, D5) ----

type Livre = { onde: "sobreposicao"; trilha: number; indice: number; c: ClipeSobreposto } | { onde: "audio"; trilha: number; indice: number; c: ClipeAudio };

function acharLivre(d: ProjetoEdicao, id: Id): Livre | null {
  for (let t = 0; t < d.sobreposicoes.length; t++) {
    const j = d.sobreposicoes[t]!.clipes.findIndex((c) => (c as { id?: Id }).id === id);
    const c = d.sobreposicoes[t]!.clipes[j];
    if (c && sobrepostoConhecido(c)) return { onde: "sobreposicao", trilha: t, indice: j, c };
  }
  for (let t = 0; t < d.audios.length; t++) {
    const j = d.audios[t]!.clipes.findIndex((c) => c.id === id);
    if (j >= 0) return { onde: "audio", trilha: t, indice: j, c: d.audios[t]!.clipes[j]! };
  }
  return null;
}

/** Clipe da principal que contém `q`: o último que começa até `q` (na transição, o que entra); antes do começo, o
 *  primeiro; depois do fim, o último. */
function ancoraEm(linha: LinhaResolvida, q: number) {
  let a = linha.clipes[0] ?? null;
  for (const c of linha.clipes) if (c.inicio_q <= q) a = c;
  return a;
}

/** `pos` que começa no slot `q` (D5): preso a clipe passa ao clipe sob `q` (com `manter`, fica no mesmo); do programa,
 *  `t = q·den/num`. Sem nenhum clipe na linha, o preso vira do programa. */
function posEm(pos: Posicao, linha: LinhaResolvida, q: number, manter: boolean): Posicao {
  const passo = linha.fps.den / linha.fps.num;
  if (pos.ref === "programa") return { ref: "programa", t: Math.max(0, q) * passo };
  const a = (manter ? linha.clipes.find((c) => c.id === pos.clipe) : null) ?? ancoraEm(linha, q);
  if (!a) return { ref: "programa", t: Math.max(0, q) * passo };
  return { ref: "clipe", clipe: a.id, desloc: (q - a.inicio_q) * passo };
}

/** Livre (sobreposição ou áudio): começa no slot `de_q` (D5), opcionalmente em outra trilha (`trilha` nova: criada
 *  acima da de cima). `manterAncora` (Alt): fica preso ao mesmo clipe, só muda o deslocamento. */
export function moverLivre(d: Draft<ProjetoEdicao>, linha: LinhaResolvida, livre: Id, de_q: number, op: { trilha?: Id | "nova"; manterAncora?: boolean }): void {
  const l = acharLivre(d as ProjetoEdicao, livre);
  if (!l) return;
  l.c.pos = posEm(l.c.pos, linha, Math.round(de_q), op.manterAncora === true);
  if (!op.trilha) return;
  if (l.onde === "sobreposicao") {
    const origem = d.sobreposicoes[l.trilha]!;
    let destino = op.trilha === "nova" ? null : d.sobreposicoes.find((t) => t.id === op.trilha);
    if (op.trilha === "nova") {
      destino = { id: novoId(), nome: "", oculta: false, clipes: [] } as Draft<ProjetoEdicao["sobreposicoes"][number]>;
      d.sobreposicoes.push(destino);
    }
    if (!destino || destino === origem) return;
    const [c] = origem.clipes.splice(l.indice, 1);
    if (c) destino.clipes.push(c);
    return;
  }
  const origem = d.audios[l.trilha]!;
  const destino = op.trilha === "nova" ? null : d.audios.find((t) => t.id === op.trilha);
  if (!destino || destino === origem) return;
  const [c] = origem.clipes.splice(l.indice, 1);
  if (c) destino.clipes.push(c);
}

/** Livre: aparar `lado` até `q` (`duracao_s` passa a valer; "até o fim do clipe" vira duração explícita). O início
 *  anda e o fim fica (o preso continua no mesmo clipe). Vídeo sobreposto e clipe de áudio: o recorte da mídia anda
 *  junto (aparar o começo tira o começo do som ou do vídeo). */
export function apararLivre(d: Draft<ProjetoEdicao>, linha: LinhaResolvida, livre: Id, lado: "inicio" | "fim", q: number): void {
  const r = linha.livres.find((x) => x.id === livre);
  const l = acharLivre(d as ProjetoEdicao, livre);
  if (!r || !l) return;
  const passo = linha.fps.den / linha.fps.num;
  const fim = r.inicio_q + r.n_q;
  const qq = Math.round(q);
  if (l.onde === "audio") {
    const c = l.c;
    const dur = d.midias.find((m) => m.id === c.midia)?.duracao_s ?? null;
    if (lado === "fim") {
      const ate = c.de_s + Math.max(1, qq - r.inicio_q) * passo;
      c.ate_s = dur !== null ? Math.min(ate, dur) : ate;
      return;
    }
    // o começo não passa do começo do arquivo nem do fim do clipe
    const delta = Math.max(Math.min(qq, fim - 1) - r.inicio_q, Math.ceil(-c.de_s / passo - 1e-9));
    c.de_s = Math.max(0, c.de_s + delta * passo);
    c.pos = deslocar(c.pos, delta * passo);
    return;
  }
  const c = l.c;
  if (lado === "fim") {
    c.duracao_s = Math.max(1, qq - r.inicio_q) * passo;
    return;
  }
  let delta = Math.min(qq, fim - 1) - r.inicio_q;
  if (c.tipo === "video") {
    const fator = c.velocidade.tipo === "constante" && c.velocidade.fator > 0 ? c.velocidade.fator : 1;
    const m = d.midias.find((x) => x.id === c.midia);
    const ini = m?.inicio_s ?? 0;
    // não começa antes do começo da mídia
    delta = Math.max(delta, Math.ceil((ini - c.inicio_s) / (passo * fator) - 1e-9));
    c.inicio_s = c.inicio_s + delta * passo * fator;
    c.inicio_pts = null;
  }
  c.pos = deslocar(c.pos, delta * passo);
  if (c.duracao_s !== null) c.duracao_s = Math.max(1, fim - (r.inicio_q + delta)) * passo;
}

/** Mesmo lugar de referência, `s` segundos adiante (preso: no mesmo clipe). */
function deslocar(pos: Posicao, s: Seg): Posicao {
  return pos.ref === "programa" ? { ref: "programa", t: Math.max(0, pos.t + s) } : { ref: "clipe", clipe: pos.clipe, desloc: pos.desloc + s };
}
