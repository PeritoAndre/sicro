/**
 * Espelho das legendas (P1.6) de `motor/legendas.rs`, conferido pela fixture `legendas-exibidas.json` (mesmos bits):
 * onde cada legenda aparece no programa (de fonte: pelo som do trecho, com velocidade e corte), a quebra em linhas e
 * páginas e o que a tela mostra em cada trecho de slots. E a degravação → `Legenda` (tempo de fonte; `ia` não
 * conferida).
 */

import type { ClipeTrecho, Degravacao, Id, Legenda, LinhaResolvida, ProjetoEdicao } from "@domain/edicao";
import { novoId } from "./projeto";

export const LARGURA = 0.9;
export const AVANCO = 0.6;
export const AVANCO_MONO = 0.62;
export const MAX_POR_LINHA = 10_000;
/** Teto da legenda no eixo de slots do trecho (2^32). */
const MAX_SLOTS = 4_294_967_296;

/** Legenda na tela: slots [de_q, ate_q) do programa e as linhas, como saem na imagem e no arquivo. */
export interface Exibida {
  de_q: number;
  ate_q: number;
  linhas: string[];
}

/** Separa palavras (o espaço sem quebra fica dentro da palavra). */
const ESPACO = /[ \t\r\v\f]/;
const SO_ESPACO = /^[ \t\r\v\f\n]*$/;

/** Texto sem nada além de espaço e quebra: a legenda não aparece. */
export const vazio = (t: string) => SO_ESPACO.test(t);

const arredondar = (x: number) => Math.floor(x + 0.5);

function cortar(xa: number, xb: number, n: number): [number, number] | null {
  if (!Number.isFinite(xa) || !Number.isFinite(xb)) return null;
  const c = (x: number) => Math.min(Math.max(arredondar(x), 0), n);
  const [a, b] = [c(xa), c(xb)];
  return b > a ? [a, b] : null;
}

type TrechoNaLinha = { ini_q: number; n_q: number; ini_s: number; fator: number; t: ClipeTrecho };

function trechos(p: ProjetoEdicao, linha: LinhaResolvida): TrechoNaLinha[] {
  const v: TrechoNaLinha[] = [];
  for (const r of linha.clipes) {
    if (!r.fonte) continue;
    const t = p.principal.find((c) => (c as { tipo?: unknown }).tipo === "trecho" && (c as ClipeTrecho).id === r.id && !(c as ClipeTrecho).desligado) as ClipeTrecho | undefined;
    if (!t || t.velocidade.tipo !== "constante") continue;
    const fator = t.velocidade.fator;
    if (!(Number.isFinite(fator) && fator > 0)) continue;
    v.push({ ini_q: r.inicio_q, n_q: r.n_q, ini_s: t.inicio_s, fator, t });
  }
  return v;
}

/** A legenda inteira no eixo de slots da janela (0 = primeiro slot dela), sem cortar; null = vazia ou não finita. */
function inteira(xa: number, xb: number): [number, number] | null {
  if (!Number.isFinite(xa) || !Number.isFinite(xb)) return null;
  const c = (x: number) => Math.min(Math.max(arredondar(x), -MAX_SLOTS), MAX_SLOTS);
  const [a, b] = [c(xa), c(xb)];
  return b > a ? [a, b] : null;
}

/**
 * Uma aparição: índice em `itens`, os slots [xa, xb) que a legenda INTEIRA ocupa no eixo da janela, o início da janela
 * no programa e o tamanho dela. De fonte a janela é o trecho (pelo som dele); de programa, o programa.
 */
type Aparicao = { i: number; xa: number; xb: number; ini_q: number; n_q: number };

/** Slots [a, b) do programa do pedaço [x, y) do eixo da janela; null = fora dela. */
function visivel(ap: Aparicao, x: number, y: number): [number, number] | null {
  const [a, b] = [Math.max(x, 0), Math.min(y, ap.n_q)];
  return a < b ? [ap.ini_q + a, ap.ini_q + b] : null;
}

function aparicoes(p: ProjetoEdicao, linha: LinhaResolvida): Aparicao[] {
  const fps = linha.fps;
  if (fps.num <= 0 || fps.den <= 0) return [];
  const ts = trechos(p, linha);
  const v: Aparicao[] = [];
  p.legendas.itens.forEach((l, i) => {
    if (vazio(l.texto)) return;
    const r = l.ref;
    if (r.tipo === "programa") {
      const x = (t: number) => (t * fps.num) / fps.den;
      const s = cortar(x(r.de), x(r.ate), linha.total_q);
      if (s) v.push({ i, xa: s[0], xb: s[1], ini_q: 0, n_q: linha.total_q });
      return;
    }
    for (const t of ts) {
      if (t.t.midia !== r.midia) continue;
      const x = (s: number) => ((s - t.ini_s) * fps.num) / fps.den / t.fator;
      const s = inteira(x(r.de), x(r.ate));
      if (!s) continue;
      const ap = { i, xa: s[0], xb: s[1], ini_q: t.ini_q, n_q: t.n_q };
      if (visivel(ap, ap.xa, ap.xb)) v.push(ap);
    }
  });
  return v;
}

/** Onde cada legenda aparece: [índice em `itens`, slots [a, b) do programa]. */
export function noPrograma(p: ProjetoEdicao, linha: LinhaResolvida): [number, number, number][] {
  return aparicoes(p, linha).map((ap) => {
    const s = visivel(ap, ap.xa, ap.xb)!;
    return [ap.i, s[0], s[1]];
  });
}

/** Ids das legendas que mostrariam o que um silêncio cala: o silêncio age só na janela do trecho; sem arredondar. */
export function sobSilencio(p: ProjetoEdicao, linha: LinhaResolvida): Id[] {
  const fps = linha.fps;
  if (fps.num <= 0 || fps.den <= 0) return [];
  const agem: [Id, number, number, number, number][] = [];
  for (const x of trechos(p, linha)) {
    const fim = x.ini_s + ((x.n_q * fps.den) / fps.num) * x.fator;
    const t0 = (x.ini_q * fps.den) / fps.num;
    for (const s of x.t.audio.silencios) {
      if (!Number.isFinite(s.de) || !Number.isFinite(s.ate)) continue;
      const [a, b] = [Math.max(s.de, x.ini_s), Math.min(s.ate, fim)];
      if (a < b) agem.push([x.t.midia, a, b, t0 + (a - x.ini_s) / x.fator, t0 + (b - x.ini_s) / x.fator]);
    }
  }
  const ids: Id[] = [];
  const vistos = new Set<Id>();
  for (const l of p.legendas.itens) {
    if (vazio(l.texto)) continue;
    const r = l.ref;
    const cruza = agem.some(([m, fa, fb, pa, pb]) =>
      r.tipo === "fonte" ? r.midia === m && r.de < r.ate && r.de < fb && fa < r.ate : r.de < r.ate && r.de < pb && pa < r.ate,
    );
    if (cruza && !vistos.has(l.id)) {
      vistos.add(l.id);
      ids.push(l.id);
    }
  }
  return ids;
}

/** Caracteres por linha que cabem em `LARGURA` no estilo da trilha. */
export function porLinha(p: ProjetoEdicao): number {
  const e = p.legendas.estilo;
  const avanco = e.fonte === "mono" ? AVANCO_MONO : AVANCO;
  const x = (LARGURA * p.saida.largura) / (avanco * e.tamanho * p.saida.altura);
  if (!(x >= 1)) return 1;
  if (x > MAX_POR_LINHA) return MAX_POR_LINHA;
  return Math.floor(x);
}

/** Linhas do texto: quebra do perito respeitada, palavras juntas até `n` caracteres, palavra maior partida. */
export function quebrar(texto: string, n: number): string[] {
  n = Math.max(n, 1);
  const linhas: string[] = [];
  for (const paragrafo of texto.split("\n")) {
    let atual: string[] = [];
    for (const palavra of paragrafo.split(ESPACO).filter((w) => w !== "")) {
      const cs = Array.from(palavra);
      if (atual.length > 0 && atual.length + 1 + cs.length <= n) {
        atual.push(" ", ...cs);
        continue;
      }
      if (atual.length > 0) {
        linhas.push(atual.join(""));
        atual = [];
      }
      for (let i = 0; i < cs.length; i += n) {
        const c = cs.slice(i, i + n);
        if (i + n < cs.length) linhas.push(c.join(""));
        else atual.push(...c);
      }
    }
    if (atual.length > 0) linhas.push(atual.join(""));
  }
  return linhas;
}

const tamanho = (s: string) => Array.from(s).length;

function paginas(linhas: string[], linhasMax: number, a: number, b: number): [number, number, string[]][] {
  const n = b - a;
  const total = linhas.length;
  if (total === 0 || b <= a) return [];
  let por = Math.min(Math.max(linhasMax, 1), total);
  if (Math.ceil(total / por) > n) por = Math.ceil(total / n);
  const grupos: string[][] = [];
  for (let i = 0; i < total; i += por) grupos.push(linhas.slice(i, i + por));
  const k = grupos.length;
  const pesos = grupos.map((g) => Math.max(g.reduce((s, l) => s + tamanho(l), 0), 1));
  const soma = pesos.reduce((s, x) => s + x, 0);
  // inteiros exatos (a legenda de fonte inteira pode ter muito mais slots que o trecho)
  const parte = (acc: number) => Number((BigInt(n - k) * BigInt(acc)) / BigInt(soma));
  let acc = 0;
  return grupos.map((g, i) => {
    const ini = a + i + parte(acc);
    acc += pesos[i]!;
    const fim = a + i + 1 + parte(acc);
    return [ini, fim, g];
  });
}

const aparar = (s: string) => s.replace(/^[ \t\r\v\f\n]+|[ \t\r\v\f\n]+$/g, "");

/** O que a tela mostra, em ordem de slot (a imagem e os arquivos saem daqui). */
export function exibidas(p: ProjetoEdicao, linha: LinhaResolvida): Exibida[] {
  const n = porLinha(p);
  const pags: [number, number, string[]][] = [];
  for (const ap of aparicoes(p, linha)) {
    const l = p.legendas.itens[ap.i]!;
    const loc = l.locutor === null ? "" : aparar(l.locutor);
    const texto = loc ? `${loc}: ${l.texto}` : l.texto;
    // páginas da legenda inteira: cada trecho mostra só as da fala que toca nele
    for (const [x, y, g] of paginas(quebrar(texto, n), p.legendas.linhas_max, ap.xa, ap.xb)) {
      const s = visivel(ap, x, y);
      if (s) pags.push([s[0], s[1], g]);
    }
  }
  const ordem = pags.map((_, k) => k).sort((x, y) => pags[x]![0] - pags[y]![0] || x - y);
  const pontos = [...new Set(pags.flatMap((x) => [x[0], x[1]]))].sort((x, y) => x - y);
  let ativas: number[] = [];
  let prox = 0;
  const v: Exibida[] = [];
  for (let w = 0; w + 1 < pontos.length; w++) {
    const [x, y] = [pontos[w]!, pontos[w + 1]!];
    ativas = ativas.filter((k) => pags[k]![1] > x);
    while (prox < ordem.length && pags[ordem[prox]!]![0] <= x) ativas.push(ordem[prox++]!);
    if (ativas.length === 0) continue;
    const linhas = ativas.flatMap((k) => pags[k]![2]);
    const ultima = v.at(-1);
    if (ultima && ultima.ate_q === x && ultima.linhas.length === linhas.length && ultima.linhas.every((s, i) => s === linhas[i])) ultima.ate_q = y;
    else v.push({ de_q: x, ate_q: y, linhas });
  }
  return v;
}

/** Trechos da degravação viram legendas de fonte da mídia; os de IA chegam não conferidos (barram a exportação). */
export function legendasDaDegravacao(d: Degravacao, midia: Id): Legenda[] {
  return d.trechos
    .filter((t) => !vazio(t.texto))
    .map((t) => ({
      id: novoId(),
      texto: t.texto,
      locutor: t.locutor,
      origem: t.ia ? "ia" : "perito",
      conferida: !t.ia,
      // o contrato não traz o locutor automático à parte: com IA e locutor, conta como automático
      locutor_auto: t.ia && t.locutor !== null,
      ref: { tipo: "fonte", midia, de: t.de, ate: t.ate },
    }));
}
