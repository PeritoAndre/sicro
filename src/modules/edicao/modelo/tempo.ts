/**
 * Espelho de `src-tauri/src/edicao/tempo.rs` e das contas de tempo de `quadros.rs` (§3.1, D6, D14):
 * mapa de quadros em inteiros (`BigInt` onde o Rust usa i128) e a linha do tempo em slots.
 * Mesmas contas, na mesma ordem; as fixtures `tempo-*.json` (P0.3b) conferem.
 */

import type { AlvoQuadro, Chave, ClipePrincipal, ClipeResolvido, FaixaFonte, Id, LinhaResolvida, LivreResolvido, Posicao, ProjetoEdicao, QuadroEm, Racional, Seg, TabelaQuadros, Velocidade } from "@domain/edicao";
import { clipeConhecido, sobrepostoConhecido } from "./projeto";
import { tabelaDoClipe } from "./realce";

export const TAXA_AUDIO = 48_000;
/** O `setpts` calcula em `double`: inteiro só é exato abaixo de 2^53. */
const LIMITE_DOUBLE = 2n ** 53n;
const U32 = 0xffffffff;
/** Teto de slots de um trecho (`tempo::MAX_SLOTS` do Rust): acima, `mapa_grande` antes de alocar. */
export const MAX_SLOTS = 2 ** 24;

/** Constantes da fórmula e o quadro (índice da tabela) de cada slot do clipe. */
export interface MapaQuadros {
  j_in: number;
  j_out: number;
  c: bigint;
  r: bigint;
  m: bigint;
  kn: bigint;
  kd: bigint;
  e: bigint;
  p0: bigint;
  n_q: number;
  quadro_do_slot: number[];
  repetidos: number;
  descartados: number;
}

/** Erro com o código do catálogo (`intervalo`, `velocidade`, `mapa_grande`, `nao_suportado`); a mensagem é a do Rust. */
export class ErroTempo extends Error {
  constructor(
    readonly codigo: string,
    detalhe?: string,
  ) {
    super(detalhe ? `${codigo}: ${detalhe}` : codigo);
  }
}

// ---- inteiros ----

const abs = (a: bigint) => (a < 0n ? -a : a);
function mdc(a: bigint, b: bigint): bigint {
  a = abs(a);
  b = abs(b);
  while (b !== 0n) [a, b] = [b, a % b];
  return a;
}
/** Teto de a/b com b > 0. */
function teto(a: bigint, b: bigint): bigint {
  const q = a / b;
  return a % b !== 0n && a > 0n ? q + 1n : q;
}
const minB = (a: bigint, b: bigint) => (a < b ? a : b);
const maxB = (a: bigint, b: bigint) => (a > b ? a : b);

// ---- tabela compacta ----

const cachePts = new WeakMap<TabelaQuadros, number[]>();
/** pts de cada quadro: pts0 + soma dos passos (guardado por tabela). */
export function ptsDaTabela(tab: TabelaQuadros): number[] {
  let v = cachePts.get(tab);
  if (!v) {
    v = new Array<number>(tab.passos.length + 1);
    let p = tab.pts0;
    v[0] = p;
    for (let i = 0; i < tab.passos.length; i++) {
      p += tab.passos[i] as number;
      v[i + 1] = p;
    }
    cachePts.set(tab, v);
  }
  return v;
}
export const nQuadros = (tab: TabelaQuadros) => tab.passos.length + 1;
/** Duração do quadro j em ticks; no último, o passo mediano. */
export const passoDe = (tab: TabelaQuadros, j: number) => (j < tab.passos.length ? (tab.passos[j] as number) : tab.passo_mediano);
/** Tempo de fonte (s) do quadro j: (pts·num)/den. */
export const tempoDoQuadro = (tab: TabelaQuadros, j: number) => ((ptsDaTabela(tab)[j] as number) * tab.tb.num) / tab.tb.den;
/** Fim da mídia (s): fim do último quadro. */
export const fimDaMidia = (tab: TabelaQuadros) => (((ptsDaTabela(tab)[nQuadros(tab) - 1] as number) + tab.passo_mediano) * tab.tb.num) / tab.tb.den;

/** Quantos pts satisfazem `cond` (lista crescente). */
function contar(v: number[], cond: (p: number) => boolean): number {
  let lo = 0;
  let hi = v.length;
  while (lo < hi) {
    const m = (lo + hi) >>> 1;
    if (cond(v[m] as number)) lo = m + 1;
    else hi = m;
  }
  return lo;
}

/** Em ticks: chão(t/tb + 1/2). */
const alvoEmTicks = (tab: TabelaQuadros, t: Seg) => Math.floor((t * tab.tb.den) / tab.tb.num + 0.5);

/** Quadro exibido em `t`: maior j com pts_j ≤ chão(t/tb + 1/2) (antes do primeiro, o primeiro). */
export function exibido(tab: TabelaQuadros, t: Seg): number {
  const alvo = alvoEmTicks(tab, t);
  return Math.max(0, contar(ptsDaTabela(tab), (p) => p <= alvo) - 1);
}

/** Índice do quadro com esse pts exato (o primeiro, se o pts se repete). */
export function indiceDoPts(tab: TabelaQuadros, pts: number): number | null {
  const v = ptsDaTabela(tab);
  const j = contar(v, (p) => p < pts);
  return v[j] === pts ? j : null;
}

/** Primeiro quadro com o pts do quadro j (o MPEG-PS repete pts; o `trim` por pts pega todos). */
const primeiroDoPts = (tab: TabelaQuadros, j: number) => {
  const p = ptsDaTabela(tab)[j];
  return (p !== undefined ? indiceDoPts(tab, p) : null) ?? j;
};

/** Quadro de um tempo preso (s, pts): o do pts quando está na tabela; senão o exibido em `t` (o primeiro desse pts). */
export const doPar = (tab: TabelaQuadros, t: Seg, pts: number | null) => (pts != null ? indiceDoPts(tab, pts) : null) ?? primeiroDoPts(tab, exibido(tab, t));

/** Fim de trecho (índice exclusivo): `n` quando `t` alcança o fim da mídia. */
export function fimDoPar(tab: TabelaQuadros, t: Seg, pts: number | null): number {
  const j = pts != null ? indiceDoPts(tab, pts) : null;
  if (j != null) return j;
  const v = ptsDaTabela(tab);
  return alvoEmTicks(tab, t) >= (v[v.length - 1] as number) + tab.passo_mediano ? v.length : primeiroDoPts(tab, exibido(tab, t));
}

/** Duração (s) do quadro j; no último, o passo mediano. */
export const duracaoDoQuadro = (tab: TabelaQuadros, j: number) => (passoDe(tab, j) * tab.tb.num) / tab.tb.den;

/** `edicao_quadro_em` sem ida ao Rust: `{ t }` = quadro exibido; `{ pts }` fora da tabela = o anterior. */
export function quadroEm(tab: TabelaQuadros, alvo: AlvoQuadro): QuadroEm {
  const v = ptsDaTabela(tab);
  let j: number;
  if ("t" in alvo) j = exibido(tab, alvo.t);
  else if ("indice" in alvo) {
    if (!(alvo.indice >= 0 && alvo.indice < v.length)) throw new ErroTempo("intervalo");
    j = alvo.indice;
  } else j = indiceDoPts(tab, alvo.pts) ?? Math.max(0, contar(v, (p) => p <= alvo.pts) - 1);
  return { indice: j, pts: v[j] as number, pts_s: tempoDoQuadro(tab, j), dur_s: duracaoDoQuadro(tab, j) };
}

/** [j_in, j_out) de um trecho pelos pares de cada ponta. */
export function faixaDoTrecho(tab: TabelaQuadros, inicio_s: Seg, inicio_pts: number | null, fim_s: Seg, fim_pts: number | null): [number, number] {
  const jIn = doPar(tab, inicio_s, inicio_pts);
  const jOut = fimDoPar(tab, fim_s, fim_pts);
  if (jOut <= jIn) throw new ErroTempo("intervalo");
  return [jIn, jOut];
}

// ---- taxas e slots ----

/** Fator decimal (até 3 casas) como fração exata por milésimos; fora de 0,05..16 = `velocidade`. */
export function fatorRacional(fator: number): Racional {
  const mil = Math.round(fator * 1000);
  if (!Number.isFinite(fator) || Math.abs(fator * 1000 - mil) > 1e-6 || !(mil >= 50 && mil <= 16000)) throw new ErroTempo("velocidade");
  const g = mdc(BigInt(mil), 1000n);
  return { num: Number(BigInt(mil) / g), den: Number(1000n / g) };
}

export const transicaoSobrepoe = (tipo: string) => tipo !== "escurecer";

/** round((d·num)/den); não finito ou negativo = 0. `round` do Rust: meio para longe do zero. */
function arredondar(duracao_s: Seg, fps: Racional): number {
  const v = (duracao_s * fps.num) / fps.den;
  const x = v < 0 ? -Math.round(-v) : Math.round(v);
  return Number.isFinite(x) && x > 0 ? x : 0;
}
/** round(duracao_s·fps). */
export const slots = (duracao_s: Seg, fps: Racional) => arredondar(duracao_s, fps);
/** Clipe sem fonte: max(1, round(duracao_s·fps)). */
export const nQSemFonte = (duracao_s: Seg, fps: Racional) => Math.min(Math.max(arredondar(duracao_s, fps), 1), U32);
/** Instante do slot q: q·den/num. */
export const instanteDoSlot = (q: number, fps: Racional) => (q * fps.den) / fps.num;
/** Fronteira de amostra do slot q a 48 kHz: round(q·48000·den/num), em inteiros. */
export function amostraDoSlot(q: number, fps: Racional): number {
  const num = BigInt(Math.max(fps.num, 1));
  const den = BigInt(Math.max(fps.den, 1));
  return Number((2n * BigInt(q) * BigInt(TAXA_AUDIO) * den + num) / (2n * num));
}

// ---- mapa de um trecho ----

// ---- rampa (P3.4): mesmas contas de `tempo::Rampa`, na mesma ordem (só + − × ÷: os mesmos bits) ----

/** `ln` com redução por potências de 2 e a série de atanh (o `Math.log` muda de um motor para outro no último bit). */
function ln(x: number): number {
  let m = x;
  let e = 0;
  while (m > Math.SQRT2) {
    m *= 0.5;
    e += 1;
  }
  while (m < Math.SQRT1_2) {
    m *= 2;
    e -= 1;
  }
  const z = (m - 1) / (m + 1);
  return e * Math.LN2 + 2 * z * atanhSobreZ(z * z);
}

/** atanh(z)/z = Σ z²ᵏ/(2k + 1), |z| ≤ 0,18. */
function atanhSobreZ(z2: number): number {
  let s = 0;
  for (let k = 13; k >= 0; k--) s = s * z2 + 1 / (2 * k + 1);
  return s;
}

/** ln(b/a)/(b − a): ∫ dw/v por unidade de w com v linear de a a b. */
function lnSobreDif(a: number, b: number): number {
  if (a === b) return 1 / a;
  const z = (b - a) / (b + a);
  return Math.abs(z) < 0.125 ? (2 * atanhSobreZ(z * z)) / (a + b) : ln(b / a) / (b - a);
}

/** Teto de pontos de uma rampa (`tempo::MAX_PONTOS_RAMPA`). */
export const MAX_PONTOS_RAMPA = 1000;

/** Pontos aceitos (`tempo::pontos_da_rampa_ok`): erro `velocidade` sem ponto, fora de ordem, fator inválido ou suave. */
export function pontosDaRampaOk(pontos: Chave<number>[]): void {
  if (pontos.length === 0 || pontos.length > MAX_PONTOS_RAMPA) throw new ErroTempo("velocidade");
  let antes = -Infinity;
  for (const k of pontos) {
    fatorRacional(k.v);
    if (!Number.isFinite(k.t) || k.t < antes || k.interp === "suave") throw new ErroTempo("velocidade");
    antes = k.t;
  }
}

/** Rampa pronta: pontos [s a partir de t0, fator, degrau]; v linear entre eles, a ponta fora deles. */
export class Rampa {
  private readonly p: [number, number, boolean][];
  constructor(pontos: Chave<number>[], t0: number) {
    pontosDaRampaOk(pontos);
    this.p = pontos.map((k) => [k.t - t0, k.v, k.interp === "degrau"]);
  }
  /** 0 = até o primeiro ponto; r = entre r−1 e r; n = depois do último. */
  private regiao(x: number): number {
    return contar(
      this.p.map((q) => q[0]),
      (t) => t < x,
    );
  }
  private pedaco(r: number, x0: number, x1: number): number {
    const p = this.p;
    const n = p.length;
    if (r === 0) return (x1 - x0) / p[0]![1];
    if (r === n) return (x1 - x0) / p[n - 1]![1];
    const [t0, v0, degrau] = p[r - 1]!;
    const [t1, v1] = p[r]!;
    if (degrau || v0 === v1 || !(t1 > t0)) return (x1 - x0) / v0;
    const va = v0 + (v1 - v0) * ((x0 - t0) / (t1 - t0));
    const vb = v0 + (v1 - v0) * ((x1 - t0) / (t1 - t0));
    return (x1 - x0) * lnSobreDif(va, vb);
  }
  /** ∫ dw/v de x a y (x ≤ y) a partir da região r[0], que anda. */
  private integrar(x: number, y: number, r: [number]): number {
    const p = this.p;
    const n = p.length;
    let total = 0;
    for (;;) {
      const fim = r[0] < n ? p[r[0]]![0] : Infinity;
      const ate = y < fim ? y : fim;
      if (ate > x) {
        total += this.pedaco(r[0], x, ate);
        x = ate;
      }
      if (y <= fim || r[0] === n) return total;
      r[0] += 1;
    }
  }
  /** ∫ dw/v de a a b (a ≤ b). */
  entre(a: number, b: number): number {
    const r: [number] = [this.regiao(a)];
    return this.integrar(a, b, r);
  }
  /** Duração na saída (s) da fonte [0, d]. */
  duracao(d: number): number {
    const r: [number] = [this.regiao(0)];
    return d > 0 ? this.integrar(0, d, r) : 0;
  }
  /** τ acumulado em cada instante de `us` (crescentes), a partir do primeiro. */
  taus(us: number[]): number[] {
    const v: number[] = [];
    if (us.length === 0) return v;
    const r: [number] = [this.regiao(us[0]!)];
    let x = us[0]!;
    let tau = 0;
    for (const u of us) {
      if (u > x) {
        tau += this.integrar(x, u, r);
        x = u;
      }
      v.push(tau);
    }
    return v;
  }
}

/** Constantes da conta de 1× (c, R, M, P_jin) e as conferências do trecho, como `Conta::nova` do Rust. */
function contaDe1x(tab: TabelaQuadros, jIn: number, jOut: number, fps: Racional) {
  const pts = ptsDaTabela(tab);
  const n = pts.length;
  const [tn, td, fnum, fden] = [tab.tb.num, tab.tb.den, fps.num, fps.den].map((x) => BigInt(x)) as [bigint, bigint, bigint, bigint];
  if (jIn >= jOut || jOut > n || tn <= 0n || td <= 0n || fnum <= 0n || fden <= 0n) throw new ErroTempo("intervalo");
  const repete = (j: number) => j > 0 && j < n && pts[j - 1] === pts[j];
  if (repete(jIn) || repete(jOut)) throw new ErroTempo("intervalo");
  const c = (td / mdc(td, fnum)) * fnum;
  const r = (tn * c) / td;
  const m = (c * fden) / fnum;
  let kn = fnum;
  let kd = fden * c;
  const g = mdc(kn, kd);
  kn /= g;
  kd /= g;
  const P = (j: number) => BigInt(pts[j] as number) * r;
  const p0 = P(jIn);
  const pfim = jOut < n ? P(jOut) : P(n - 1) + BigInt(Math.max(tab.passo_mediano, 1)) * r;
  if (pfim <= p0) throw new ErroTempo("intervalo");
  if (abs(p0) >= LIMITE_DOUBLE || abs(pfim) >= LIMITE_DOUBLE || (pfim - p0) * kn >= LIMITE_DOUBLE) throw new ErroTempo("mapa_grande");
  for (let j = jIn; j < jOut; j++) if ((j + 1 < jOut ? P(j + 1) : pfim) - P(j) < 0n) throw new ErroTempo("intervalo");
  return { c, r, m, p0 };
}

/** Mapa de um trecho em rampa (`tempo::mapa_rampa`): slot(j) = teto(T_j − ε), ε = min(1/2, menor passo de T / 2). */
function mapaRampa(tab: TabelaQuadros, jIn: number, jOut: number, pontos: Chave<number>[], fps: Racional): MapaQuadros {
  pontosDaRampaOk(pontos);
  const k = contaDe1x(tab, jIn, jOut, fps);
  const pts = ptsDaTabela(tab);
  const n = pts.length;
  const [tn, td, fnum, fden] = [tab.tb.num, tab.tb.den, fps.num, fps.den];
  const pin = pts[jIn] as number;
  const t0 = (pin * tn) / td;
  const rampa = new Rampa(pontos, t0);
  const fim = jOut < n ? (pts[jOut] as number) : (pts[n - 1] as number) + Math.max(tab.passo_mediano, 1);
  const us: number[] = [];
  for (let j = jIn; j < jOut; j++) us.push((((pts[j] as number) - pin) * tn) / td);
  us.push(((fim - pin) * tn) / td);
  const ts = rampa.taus(us).map((tau) => (tau * fnum) / fden);
  const tFim = ts[ts.length - 1] as number;
  if (!Number.isFinite(tFim) || tFim > MAX_SLOTS) throw new ErroTempo("mapa_grande");
  let menor = Infinity;
  for (let i = 0; i + 1 < ts.length; i++) {
    const d = (ts[i + 1] as number) - (ts[i] as number);
    if (d > 0 && d < menor) menor = d;
  }
  const eps = menor / 2 < 0.5 ? menor / 2 : 0.5;
  const nQ = Math.max(Math.ceil(tFim - eps), 1);
  if (nQ > MAX_SLOTS || jOut > U32 || BigInt(nQ) * k.m >= LIMITE_DOUBLE) throw new ErroTempo("mapa_grande");
  const slot = (i: number) => Math.min(Math.max(Math.ceil((ts[i] as number) - eps), 0), nQ);
  const quadro = new Array<number>(nQ).fill(0);
  let distintos = 0;
  let s = 0;
  for (let j = jIn; j < jOut; j++) {
    const depois = j + 1 < jOut ? slot(j + 1 - jIn) : nQ;
    if (depois > s) {
      quadro.fill(j, s, depois);
      distintos++;
      s = depois;
    }
  }
  return { j_in: jIn, j_out: jOut, c: k.c, r: k.r, m: k.m, kn: 0n, kd: 0n, e: 0n, p0: k.p0, n_q: nQ, quadro_do_slot: quadro, repetidos: nQ - distintos, descartados: jOut - jIn - distintos };
}

/** Segundos de programa, desde o início do clipe, em que o trecho mostra o instante de fonte `t` (`tempo::programa_da_fonte`);
 *  null = velocidade inválida. */
export function programaDaFonte(v: Velocidade, inicioS: Seg, t: Seg): number | null {
  if (v.tipo === "constante") return Number.isFinite(v.fator) && v.fator > 0 ? (t - inicioS) / v.fator : null;
  let r: Rampa;
  try {
    r = new Rampa(v.pontos, inicioS);
  } catch {
    return null;
  }
  const d = t - inicioS;
  if (!Number.isFinite(d)) return null;
  const x = d >= 0 ? r.entre(0, d) : r.entre(d, 0);
  return d >= 0 ? x : -x;
}

/** Mapa dos quadros `jIn..jOut` (exclusivo; `jOut = n` = fim da mídia) para slots na taxa `fps`. Rampa: kn = kd = e = 0. */
export function mapaTrecho(tab: TabelaQuadros, jIn: number, jOut: number, v: Velocidade, fps: Racional): MapaQuadros {
  if (v.tipo === "rampa") return mapaRampa(tab, jIn, jOut, v.pontos, fps);
  const vel = fatorRacional(v.fator);
  const pts = ptsDaTabela(tab);
  const n = pts.length;
  const [tn, td, fnum, fden, vn, vd] = [tab.tb.num, tab.tb.den, fps.num, fps.den, vel.num, vel.den].map((x) => BigInt(x)) as bigint[] as [bigint, bigint, bigint, bigint, bigint, bigint];
  if (jIn >= jOut || jOut > n || tn <= 0n || td <= 0n || fnum <= 0n || fden <= 0n) throw new ErroTempo("intervalo");
  // o `trim` corta por pts: a ponta do trecho tem de ser o primeiro quadro do pts dela
  const repete = (j: number) => j > 0 && j < n && pts[j - 1] === pts[j];
  if (repete(jIn) || repete(jOut)) throw new ErroTempo("intervalo");
  const c = (td / mdc(td, fnum)) * fnum;
  const r = (tn * c) / td;
  const m = (c * fden) / fnum;
  let kn = fnum * vd;
  let kd = fden * vn * c;
  const g = mdc(kn, kd);
  kn /= g;
  kd /= g;
  const P = (j: number) => BigInt(pts[j] as number) * r;
  const p0 = P(jIn);
  // fim da mídia: o último quadro dura o passo mediano
  const pfim = jOut < n ? P(jOut) : P(n - 1) + BigInt(Math.max(tab.passo_mediano, 1)) * r;
  // mesma ordem do Rust: vazio, depois 2^53, depois passo negativo
  if (pfim <= p0) throw new ErroTempo("intervalo");
  if (abs(p0) >= LIMITE_DOUBLE || abs(pfim) >= LIMITE_DOUBLE || (pfim - p0) * kn >= LIMITE_DOUBLE) throw new ErroTempo("mapa_grande");
  // menor passo positivo do trecho
  let pmin: bigint | null = null;
  for (let j = jIn; j < jOut; j++) {
    const d = (j + 1 < jOut ? P(j + 1) : pfim) - P(j);
    if (d < 0n) throw new ErroTempo("intervalo");
    if (d > 0n) pmin = pmin === null ? d : minB(pmin, d);
  }
  const e = pmin !== null ? minB(kd, pmin * kn) / 2n : kd / 2n;
  const slot = (p: number) => teto((BigInt(p) * r - p0) * kn - e, kd);
  const nqB = maxB(teto((pfim - p0) * kn - e, kd), 1n);
  if (nqB > BigInt(MAX_SLOTS) || jOut > U32) throw new ErroTempo("mapa_grande");
  const nQ = Number(nqB);
  const quadro = new Array<number>(nQ).fill(0);
  let distintos = 0;
  let s = 0;
  for (let j = jIn; j < jOut; j++) {
    // o quadro j fica do slot dele até o slot do seguinte
    const depois = j + 1 < jOut ? Math.min(Number(maxB(slot(pts[j + 1] as number), 0n)), nQ) : nQ;
    if (depois > s) {
      quadro.fill(j, s, depois);
      distintos++;
      s = depois;
    }
  }
  return { j_in: jIn, j_out: jOut, c, r, m, kn, kd, e, p0, n_q: nQ, quadro_do_slot: quadro, repetidos: nQ - distintos, descartados: jOut - jIn - distintos };
}

/** Slots do clipe em que o quadro j aparece: [primeiro slot local, quantos]; null = descartado ou fora. */
export function slotsDoQuadro(mapa: MapaQuadros, j: number): [number, number] | null {
  const a = contar(mapa.quadro_do_slot, (x) => x < j);
  const b = contar(mapa.quadro_do_slot, (x) => x <= j);
  return b > a ? [a, b - a] : null;
}

// ---- linha do tempo ----

/** A tabela de cada mídia (por `MidiaRef.id`); null = sem tabela ainda. */
export interface ContextoTempo {
  tabela(midia: Id): TabelaQuadros | null;
}

/** Slots e faixa de fonte de um clipe; sem tabela ou faixa inválida: slots estimados pelos segundos, sem `fonte`. */
function medir(c: ClipePrincipal, ctx: ContextoTempo, fps: Racional): [number, FaixaFonte | null] {
  if (c.tipo === "trecho" || c.tipo === "tela") {
    const midia = c.tipo === "trecho" ? c.midia : c.celulas[c.referencia]?.midia;
    if (midia !== undefined) {
      const v = c.velocidade;
      const estimado = () => {
        const d = c.fim_s - c.inicio_s;
        let saida = d;
        if (v.tipo === "constante") saida = Number.isFinite(v.fator) && v.fator > 0 ? d / v.fator : d;
        else
          try {
            saida = new Rampa(v.pontos, c.inicio_s).duracao(d);
          } catch {
            saida = d;
          }
        return nQSemFonte(saida, fps);
      };
      // a tabela do mapa: a da fonte, ou a de campos no desentrelace por campo (P1.4)
      const daFonte = ctx.tabela(midia);
      const tab = daFonte ? tabelaDoClipe(daFonte, c.efeitos) : null;
      if (!tab) return [estimado(), null];
      try {
        const [a, b] = faixaDoTrecho(tab, c.inicio_s, c.inicio_pts, c.fim_s, c.fim_pts);
        const m = mapaTrecho(tab, a, b, v, fps);
        const pts = ptsDaTabela(tab);
        return [
          m.n_q,
          {
            midia,
            j_in: m.j_in,
            j_out: m.j_out,
            pts_in: pts[m.j_in] as number,
            pts_out: m.j_out < pts.length ? (pts[m.j_out] as number) : null,
            lidos: m.j_out - m.j_in,
            repetidos: m.repetidos,
            descartados: m.descartados,
            tabela: tab.origem,
          },
        ];
      } catch {
        return [estimado(), null];
      }
    }
    return [1, null];
  }
  if (c.tipo === "congelar") {
    const nQ = nQSemFonte(c.duracao_s, fps);
    const tab = ctx.tabela(c.midia);
    if (!tab) return [nQ, null];
    const pts = ptsDaTabela(tab);
    const j = doPar(tab, c.instante_s, c.instante_pts);
    return [
      nQ,
      { midia: c.midia, j_in: j, j_out: j + 1, pts_in: pts[j] as number, pts_out: j + 1 < pts.length ? (pts[j + 1] as number) : null, lidos: 1, repetidos: nQ - 1, descartados: 0, tabela: tab.origem },
    ];
  }
  return [nQSemFonte(c.duracao_s, fps), null];
}

/** Linha do tempo em slots (§3.1). Só clipes ligados e conhecidos; livre preso a clipe fora da linha fica de fora. */
export function resolver(p: ProjetoEdicao, ctx: ContextoTempo): LinhaResolvida {
  const fps = p.saida.fps;
  const linha: LinhaResolvida = { fps, total_q: 0, clipes: [], livres: [] };
  if (fps.num <= 0 || fps.den <= 0) return linha;
  let fim = 0;
  let nAnterior = 0;
  for (const c of p.principal) {
    if (!clipeConhecido(c) || c.desligado) continue;
    const [nQ, fonte] = medir(c, ctx, fps);
    const t = c.transicao_entrada;
    // a transição nunca passa do clipe dela nem do anterior
    const sobrepoe = t && linha.clipes.length > 0 && transicaoSobrepoe(t.tipo) ? Math.min(slots(t.duracao_s, fps), nQ, nAnterior) : 0;
    const inicio = fim - sobrepoe;
    const r: ClipeResolvido = { id: c.id, inicio_q: inicio, n_q: nQ, sobrepoe_q: sobrepoe, fonte };
    linha.clipes.push(r);
    fim = inicio + nQ;
    nAnterior = nQ;
  }
  linha.total_q = fim;
  const porId = new Map(linha.clipes.map((c) => [c.id, c] as const));
  // (início, fim natural): do programa ou do clipe-âncora
  const lugar = (pos: Posicao): [number, number] | null => {
    if (pos.ref === "programa") return [slots(pos.t, fps), linha.total_q];
    const a = porId.get(pos.clipe);
    if (!a) return null;
    const v = (pos.desloc * fps.num) / fps.den;
    const d = v < 0 ? -Math.round(-v) : Math.round(v);
    const inicio = Number.isFinite(d) ? Math.floor(Math.max(a.inicio_q + d, 0)) : a.inicio_q;
    return [inicio, a.inicio_q + a.n_q];
  };
  const livre = (id: Id, trilha: Id, pos: Posicao, duracao: Seg | null) => {
    const l = lugar(pos);
    if (!l) return;
    const [inicio, f] = l;
    const nQ = duracao !== null ? Math.min(Math.max(slots(duracao, fps), 1), U32) : Math.min(Math.max(f - inicio, 0), U32);
    linha.livres.push({ id, trilha, inicio_q: inicio, n_q: nQ } satisfies LivreResolvido);
  };
  for (const t of p.sobreposicoes) for (const c of t.clipes) if (sobrepostoConhecido(c)) livre(c.id, t.id, c.pos, c.duracao_s ?? null);
  for (const t of p.audios)
    for (const c of t.clipes) {
      // sem `ate_s`: até o fim do arquivo (duração do retrato da mídia)
      const ate = c.ate_s ?? p.midias.find((m) => m.id === c.midia)?.duracao_s ?? null;
      livre(c.id, t.id, c.pos, ate !== null ? ate - c.de_s : null);
    }
  return linha;
}
