/**
 * Zoom de câmera que acompanha a pessoa (D8, D9): espelho de `edicao/segue.rs`, pelas fixtures `segue-*.json` que o
 * Rust grava. O palco passa todo `aDe` por `efeitosNoInstante` (cena.ts, Palco.tsx): com o enquadramento que segue,
 * a região do instante entra no lugar da gravada. Também as contas do Inspetor sobre as chaves do zoom.
 */

import type { ArquivoDenso, Caixa, Chave, ClipePrincipal, ClipeSobreposto, Desconhecido, EfEnquadramento, Efeito, Id, Ponto, ProjetoEdicao, Pts, Rastreio, Seg, TabelaQuadros } from "@domain/edicao";
import { avaliar as avaliarCurva, ehChaves } from "./curva";
import { afimGiro, IDENTIDADE, ponto, seguidaDe, type Afim } from "./geometria";
import { acharMidia, acharRastreio, efeitoConhecido } from "./projeto";
import { avaliar as avaliarRastreio } from "./rastreio";
import { ptsDaTabela } from "./tempo";
import { chaveDoZoom, comAjusteDoZoom } from "../palco/alcas";
import { recursoDe } from "../palco/recursos";
import { densoEmCache } from "../store/rastreioStore";

/** Ligada na entrega do pacote (regra 3 de contratos-fase4.md). */
export const SEGUE_PRONTO = true;

/** Suavização que "Acompanhar rastreio" grava (s, na base do rastreio); o `SUAVIZAR_PADRAO_S` do Rust. */
export const SUAVIZAR_PADRAO_S = 1;
/** Maior `suavizar_s` aceito (s); o `SUAVIZAR_MAX_S` do Rust. */
export const SUAVIZAR_MAX_S = 10;

const INTEIRA: Caixa = { x: 0, y: 0, w: 1, h: 1 };
/** Folga (s) nas pontas da janela da média (o `FOLGA_S` do Rust): sem ela o quadro na ponta entrava e saía pelo
 *  arredondamento de `t ± s/2` e o zoom tremia. */
const FOLGA_S = 1e-9;

/** Centro da caixa do rastreio num instante da base dele (espaço girado); null = sem caixa ali. */
export interface AmostraSegue { t: Seg; centro: Ponto | null }

/** O que o motor ainda não faz (espelho de `segue::recusa`); null = renderiza. */
export function recusa(q: Pick<EfEnquadramento, "segue">): string | null {
  void q;
  return null;
}

/** Largura da janela da média (s): 0 sem `segue` ou inválida; limitada a `SUAVIZAR_MAX_S`. */
function suavizarDe(e: Pick<EfEnquadramento, "segue">): number {
  const s = e.segue?.suavizar_s;
  return s !== undefined && Number.isFinite(s) && s > 0 ? Math.min(s, SUAVIZAR_MAX_S) : 0;
}

/** Região gravada em `t`; chaves vazias = a imagem inteira. */
export function regiaoGravada(e: Pick<EfEnquadramento, "regiao">, t: Seg): Caixa {
  return Array.isArray(e.regiao) && e.regiao.length === 0 ? INTEIRA : avaliarCurva(e.regiao, t);
}

function dentro(c: number, lado: number): number {
  if (!(lado < 1)) return 0;
  const v = c - lado / 2;
  return v < 0 ? 0 : v > 1 - lado ? 1 - lado : v;
}

/** Primeiro índice em que `cond` deixa de valer (lista partida, como o `partition_point`). */
function particao<T>(v: readonly T[], cond: (x: T) => boolean): number {
  let a = 0;
  let b = v.length;
  while (a < b) {
    const m = (a + b) >>> 1;
    if (cond(v[m] as T)) a = m + 1;
    else b = m;
  }
  return a;
}

/** Para cada amostra, a última com centro até ela e a primeira a partir dela (o `Vizinhos` do Rust). */
function vizinhos(amostras: readonly AmostraSegue[]): { antes: (number | null)[]; depois: (number | null)[] } {
  const n = amostras.length;
  const antes = new Array<number | null>(n).fill(null);
  const depois = new Array<number | null>(n).fill(null);
  let u: number | null = null;
  for (let i = 0; i < n; i++) {
    if ((amostras[i] as AmostraSegue).centro) u = i;
    antes[i] = u;
  }
  u = null;
  for (let i = n - 1; i >= 0; i--) {
    if ((amostras[i] as AmostraSegue).centro) u = i;
    depois[i] = u;
  }
  return { antes, depois };
}

function maisProxima(amostras: readonly AmostraSegue[], a: number | null, d: number | null, t: Seg): Ponto | null {
  if (a !== null && d !== null) return (amostras[t - (amostras[a] as AmostraSegue).t <= (amostras[d] as AmostraSegue).t - t ? a : d] as AmostraSegue).centro;
  const i = a ?? d;
  return i === null ? null : (amostras[i] as AmostraSegue).centro;
}

/** Média de TODAS as amostras da janela; a sem centro vale a com centro mais próxima no tempo (empate: a anterior):
 *  a lacuna passa em `s` segundos em vez de saltar (espelho de `segue::centro_em`). */
function centroEm(amostras: readonly AmostraSegue[], viz: ReturnType<typeof vizinhos>, t: Seg, s: number): Ponto | null {
  const de = t - s / 2 - FOLGA_S;
  const ate = t + s / 2 + FOLGA_S;
  const lo = particao(amostras, (a) => a.t < de);
  const hi = Math.max(particao(amostras, (a) => a.t <= ate), lo);
  if (hi === lo) return maisProxima(amostras, lo > 0 ? (viz.antes[lo - 1] ?? null) : null, viz.depois[hi] ?? null, t);
  let sx = 0;
  let sy = 0;
  for (let i = lo; i < hi; i++) {
    const a = amostras[i] as AmostraSegue;
    const c = a.centro ?? maisProxima(amostras, viz.antes[i] ?? null, viz.depois[i] ?? null, a.t);
    if (!c) return null;
    sx += c.x;
    sy += c.y;
  }
  const n = hi - lo;
  return { x: sx / n, y: sy / n };
}

/** Região (0..1 da origem girada) em cada instante de `em` (espelho de `segue::regiao_seguida`). `amostras` em
 *  ordem de `t`. */
export function regiaoSeguida(e: EfEnquadramento, amostras: readonly AmostraSegue[], em: readonly Seg[]): Caixa[] {
  const s = suavizarDe(e);
  const viz = vizinhos(amostras);
  return em.map((t) => {
    const r = regiaoGravada(e, t);
    const c = centroEm(amostras, viz, t, s) ?? { x: r.x + r.w / 2, y: r.y + r.h / 2 };
    return { x: dentro(c.x, r.w), y: dentro(c.y, r.h), w: r.w, h: r.h };
  });
}

/** Giro e espelhos dos `girar` ligados, na ordem. */
function giroDe(efeitos: readonly (Efeito | Desconhecido)[]): Afim {
  let g = IDENTIDADE;
  for (const e of efeitos) if (efeitoConhecido(e) && e.tipo === "girar" && e.ligado) g = seguidaDe(g, afimGiro(e.graus, e.espelhar_h, e.espelhar_v));
  return g;
}

/** Centro da caixa no espaço girado (espelho de `segue::centro_no_girado`). */
export function centroNoGirado(c: Caixa, efeitos: readonly (Efeito | Desconhecido)[]): Ponto {
  return ponto(giroDe(efeitos), { x: c.x + c.w / 2, y: c.y + c.h / 2 });
}

// ---- amostras pela tabela da mídia (o palco: um instante por pintura) ----

interface Guardado {
  tab: TabelaQuadros;
  denso: ArquivoDenso | null;
  giro: string;
  centros: Map<number, Ponto | null>;
  /** Quadro com centro mais próximo antes de `lo` e a partir de `hi` (o que a janela vazia procura). */
  antes: Map<number, number | null>;
  depois: Map<number, number | null>;
}
const guardados = new WeakMap<Rastreio, Guardado>();

function guardadoDe(r: Rastreio, tab: TabelaQuadros, denso: ArquivoDenso | null, giro: Afim): Guardado {
  const chave = giro.join(",");
  const g = guardados.get(r);
  if (g && g.tab === tab && g.denso === denso && g.giro === chave) return g;
  const novo: Guardado = { tab, denso, giro: chave, centros: new Map(), antes: new Map(), depois: new Map() };
  guardados.set(r, novo);
  return novo;
}

/** As amostras que dão, em cada instante de `instantes` (crescentes), o mesmo que todos os quadros da mídia (como
 *  `segue::amostras`): os quadros da janela e, com quadro sem centro nela, o mais próximo com centro de cada lado. */
export function amostrasDoSegue(r: Rastreio, tab: TabelaQuadros, denso: ArquivoDenso | null, giro: Afim, instantes: readonly Seg[], s: number): AmostraSegue[] {
  const g = guardadoDe(r, tab, denso, giro);
  const pts = ptsDaTabela(tab);
  const tempo = (p: number) => (p * tab.tb.num) / tab.tb.den;
  const centro = (j: number): Ponto | null => {
    const ja = g.centros.get(j);
    if (ja !== undefined) return ja;
    const a = avaliarRastreio(r, { t: tempo(pts[j] as Pts), pts: pts[j] as Pts }, denso);
    const c = a ? ponto(giro, { x: a.caixa.x + a.caixa.w / 2, y: a.caixa.y + a.caixa.h / 2 }) : null;
    g.centros.set(j, c);
    return c;
  };
  const usados = new Set<number>();
  for (const t of instantes) {
    const lo = particao(pts, (p) => tempo(p) < t - s / 2 - FOLGA_S);
    const hi = Math.max(particao(pts, (p) => tempo(p) <= t + s / 2 + FOLGA_S), lo);
    // janela com quadro sem centro (ou vazia): entra o com centro mais próximo de cada lado dela
    let falta = hi === lo;
    for (let j = lo; j < hi; j++) {
      usados.add(j);
      if (!centro(j)) falta = true;
    }
    if (!falta) continue;
    let a = g.antes.get(lo);
    if (a === undefined) {
      a = null;
      for (let j = lo - 1; j >= 0; j--) if (centro(j)) {
        a = j;
        break;
      }
      g.antes.set(lo, a);
    }
    let d = g.depois.get(hi);
    if (d === undefined) {
      d = null;
      for (let j = hi; j < pts.length; j++) if (centro(j)) {
        d = j;
        break;
      }
      g.depois.set(hi, d);
    }
    if (a !== null) usados.add(a);
    if (d !== null) usados.add(d);
  }
  return [...usados].sort((x, y) => x - y).map((j) => ({ t: tempo(pts[j] as Pts), centro: centro(j) }));
}

/** Primeiro enquadramento ligado (o que vale). */
function enquadramentoDe(efeitos: readonly (Efeito | Desconhecido)[]): [number, EfEnquadramento] | null {
  const i = efeitos.findIndex((e) => efeitoConhecido(e) && e.tipo === "enquadramento" && e.ligado);
  return i < 0 ? null : [i, efeitos[i] as EfEnquadramento];
}

/** Região que o enquadramento que segue mostra no instante `t` (base do clipe); null sem `segue`, sem o rastreio
 *  de fonte ou sem a tabela da mídia lida. O palco desenha a caixa da vista de origem com ela. */
export function regiaoNoInstante(doc: ProjetoEdicao, efeitos: readonly (Efeito | Desconhecido)[], t: Seg): Caixa | null {
  if (!SEGUE_PRONTO) return null;
  const q = enquadramentoDe(efeitos)?.[1];
  if (!q?.segue) return null;
  const r = acharRastreio(doc, q.segue.rastreio);
  if (!r || r.base !== "fonte" || !r.midia) return null;
  const tab = recursoDe(acharMidia(doc, r.midia)?.sha256).tabela;
  if (!tab) return null;
  const giro = giroDe(efeitos);
  const s = suavizarDe(q);
  return regiaoSeguida(q, amostrasDoSegue(r, tab, densoEmCache(r), giro, [t], s), [t])[0] ?? null;
}

/** Os efeitos com o enquadramento que segue trocado pela região do instante `t` (base do clipe), valor fixo.
 *  Sem enquadramento que segue: a MESMA lista, sem cópia. */
export function efeitosNoInstante<L extends readonly (Efeito | Desconhecido)[]>(doc: ProjetoEdicao, efeitos: L, t: Seg): L {
  const achado = enquadramentoDe(efeitos);
  if (!achado || !achado[1].segue) return efeitos;
  const r = regiaoNoInstante(doc, efeitos, t);
  if (!r) return efeitos;
  const copia = efeitos.slice();
  copia[achado[0]] = { ...achado[1], regiao: r };
  return copia as unknown as L;
}

// ---- chaves do zoom (Inspetor) ----

/** Ampliação da região (dado na lista): 1 = a imagem inteira na largura ou na altura. */
export const fatorDe = (c: Caixa): number => Math.min(1 / c.w, 1 / c.h);

const QUASE = 1e-6;
export type RotuloChave = "Cena inteira" | "Aproximar" | "Segurar" | "Afastar";

/** A região mostra a fonte inteira: cobre a imagem (0..1 nos dois eixos) e, em "Preencher", tem a proporção da saída
 *  (`proporcao`, a da origem girada; senão o corte tira as bordas). Em "Inteiro" basta cobrir. */
export function mostraTudo(c: Caixa, ajuste: EfEnquadramento["ajuste"], proporcao: number | null): boolean {
  const cobre = c.x <= QUASE && c.y <= QUASE && c.x + c.w >= 1 - QUASE && c.y + c.h >= 1 - QUASE;
  if (!cobre || ajuste === "conter") return cobre;
  return proporcao !== null && proporcao > 0 && Math.abs(c.w / c.h - proporcao) <= QUASE * Math.max(1, proporcao);
}

/** Rótulo de cada chave pelo fator da anterior (a primeira, pela cena inteira): maior aproxima, menor afasta,
 *  igual segura; igual e mostrando a fonte inteira de verdade (`mostraTudo`), "Cena inteira". */
export function rotulosDasChaves(ks: readonly Chave<Caixa>[], ajuste: EfEnquadramento["ajuste"] = "cobrir", proporcao: number | null = 1): { rotulo: RotuloChave; fator: number }[] {
  let antes = 1;
  return ks.map((k) => {
    const f = fatorDe(k.v);
    const rotulo: RotuloChave = f > antes + QUASE ? "Aproximar" : f < antes - QUASE ? "Afastar" : mostraTudo(k.v, ajuste, proporcao) ? "Cena inteira" : "Segurar";
    antes = f;
    return { rotulo, fator: f };
  });
}

/** "Chave no cursor": `v` (a região do instante, sem dividir o trecho) vira chave em (t, pts); o valor fixo vira a
 *  primeira chave; a mesma chave (mesmo quadro) é trocada. Animado sai em "Preencher". */
export function comChaveNoCursor(e: EfEnquadramento, t: Seg, pts: Pts | null, v: Caixa): EfEnquadramento {
  return comAjusteDoZoom({ ...e, regiao: chaveDoZoom(ehChaves(e.regiao) ? e.regiao : [], t, pts, v) });
}

/** A chave `i` sai; a última volta a valor fixo (nunca lista vazia). */
export function semChave(e: EfEnquadramento, i: number): EfEnquadramento {
  if (!ehChaves(e.regiao)) return e;
  const ks = e.regiao;
  return { ...e, regiao: ks.length <= 1 ? ((ks[0] as Chave<Caixa>)?.v ?? INTEIRA) : ks.filter((_, j) => j !== i) };
}

/** Rastreio que um efeito segue (destaque, lupa; ocultar e holofote pelo alvo; velocidade que acompanha). */
function rastreioDoEfeito(e: Efeito): Id | null {
  if (e.tipo === "destaque" || e.tipo === "lupa") return e.rastreio ?? null;
  if (e.tipo === "ocultar" || e.tipo === "holofote") return e.alvo?.rastreio ?? null;
  if (e.tipo === "velocidade_exibida" && e.onde?.modo === "segue") return e.onde.rastreio ?? null;
  return null;
}

/** "Acompanhar rastreio": os rastreios de fonte da mídia do trecho usados pelos efeitos dele, o do primeiro destaque
 *  primeiro (e o que o zoom já segue). Fora do trecho, nenhum. */
export function candidatosDoSegue(doc: ProjetoEdicao, c: ClipePrincipal | ClipeSobreposto): Rastreio[] {
  if (c.tipo !== "trecho") return [];
  const efs = c.efeitos.filter(efeitoConhecido);
  const ids: Id[] = [];
  const por = (id: Id | null | undefined) => id && !ids.includes(id) && ids.push(id);
  for (const e of efs) if (e.tipo === "destaque") por(e.rastreio);
  for (const e of efs) por(rastreioDoEfeito(e));
  por(enquadramentoDe(c.efeitos)?.[1].segue?.rastreio);
  return ids.map((id) => acharRastreio(doc, id)).filter((r): r is Rastreio => !!r && r.base === "fonte" && r.midia === c.midia);
}

const ROTULO_DO_USO: Partial<Record<Efeito["tipo"], string>> = { destaque: "Destaque", lupa: "Lupa", ocultar: "Ocultar", holofote: "Holofote", velocidade_exibida: "Velocidade" };

/** Nome do rastreio na lista: o dele; sem nome, o rótulo do efeito do trecho que o usa e a ordem na lista. */
export function nomeDoCandidato(c: ClipePrincipal | ClipeSobreposto, r: Rastreio, i: number): string {
  if (r.nome.trim()) return r.nome;
  const uso = c.efeitos.filter(efeitoConhecido).find((e) => rastreioDoEfeito(e) === r.id);
  return `${(uso && ROTULO_DO_USO[uso.tipo]) || "Rastreio"} ${i + 1}`;
}

/** Região fixa nova do zoom que acompanha: metade da altura, na proporção da saída (`proporcao` da origem girada). */
export function regiaoDoSegue(proporcao: number | null): Caixa {
  let h = 0.5;
  let w = proporcao && proporcao > 0 ? h * proporcao : 0.5;
  if (w > 1) {
    h /= w;
    w = 1;
  }
  return { x: (1 - w) / 2, y: (1 - h) / 2, w, h };
}

/** Liga o acompanhamento: `segue` com a suavização padrão (a que já havia fica), "Preencher" e o tamanho da região;
 *  região fixa da cena inteira (nada a acompanhar) vira a de `regiaoDoSegue`. */
export function comSegue(e: EfEnquadramento, rastreio: Id, proporcao: number | null): EfEnquadramento {
  const inteira = !ehChaves(e.regiao) && Math.abs(fatorDe(e.regiao) - 1) <= QUASE;
  const regiao = inteira || (ehChaves(e.regiao) && e.regiao.length === 0) ? regiaoDoSegue(proporcao) : e.regiao;
  return comAjusteDoZoom({ ...e, regiao, segue: { rastreio, suavizar_s: e.segue?.suavizar_s ?? SUAVIZAR_PADRAO_S } });
}

export function semSegue(e: EfEnquadramento): EfEnquadramento {
  const { segue, ...resto } = e;
  void segue;
  return resto;
}
