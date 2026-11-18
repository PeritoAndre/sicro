/**
 * Plano da prévia viva (modo "Programa", P3.3; §7.2, §7.4): o que toca em cada slot do programa, montado do documento
 * e da linha resolvida com as MESMAS contas do Rust (`modelo/tempo.ts`, `modelo/sobreposto.ts`); `estadoEm` diz o que
 * pintar num slot. Puro, sem DOM. Transições como na junção do motor (`motor/juncao.rs`): as que sobrepõem misturam
 * a cauda de um com a cabeça do seguinte (progresso k/d); `escurecer` é um `fade` de cada lado, sem sobrepor; a
 * entrada do primeiro e o `fecho` misturam com o fundo dentro do próprio clipe.
 */

import type {
  Animavel,
  ClipePrincipal,
  ClipeSobreposto,
  ClipeTrecho,
  Id,
  LinhaResolvida,
  ProjetoEdicao,
  Racional,
  Seg,
  TabelaQuadros,
  Transicao,
  Velocidade,
} from "@domain/edicao";
import { transformacaoEm, type TransformacaoAvaliada } from "../../modelo/sobreposto";
import { clipeConhecido, sobrepostoConhecido } from "../../modelo/projeto";
import { tabelaDoClipe } from "../../modelo/realce";
import { doPar, exibido, faixaDoTrecho, mapaTrecho, slots, tempoDoQuadro, transicaoSobrepoe, type ContextoTempo } from "../../modelo/tempo";

/** Um clipe da principal em slots `[de_q, ate_q)` do programa. `fonte`: a mídia que toca (trecho, quadro congelado);
 *  `inicio_s` = tempo de fonte do quadro do slot `de_q`. `entrada`: a transição de entrada (os `n_q` primeiros slots
 *  misturam com o segmento anterior; "escurecer" passa pelo fundo). */
export interface SegmentoPrevia {
  clipe: Id;
  de_q: number;
  ate_q: number;
  fonte: null | { midia: Id; sha256: string; inicio_s: Seg; velocidade: Velocidade; congelado: boolean };
  entrada: null | { tipo: string; n_q: number };
  // CONTRATO: acréscimos (P3.3)
  /** A transição do seguinte (ou o `fecho`) vista deste lado: os `n_q` últimos slots (`blocos::escurecimentos`). */
  saida: null | { tipo: string; n_q: number };
  /** Quadro da fonte (índice da tabela da mídia) em cada slot do clipe (o congelado: um só); null = sem tabela. */
  quadros: number[] | null;
  tabela: TabelaQuadros | null;
  /** Elemento parado e quadro por busca (§7.3): rampa e velocidade fora de 0,25..8. */
  guiado: boolean;
}

/** Um sobreposto (título, texto livre, imagem) em slots `[de_q, ate_q)`, na ordem de pintura (trilhas de baixo para cima). */
export interface LivrePrevia {
  id: Id;
  trilha: Id;
  de_q: number;
  ate_q: number;
  // CONTRATO: acréscimo (P3.3): o clipe, para a transformação no slot.
  sobreposto: ClipeSobreposto;
}

/** Som em slots `[de_q, ate_q)`: o do trecho (barra "original") ou um clipe de trilha de áudio ("narracao"). */
export interface SomPrevia {
  id: Id;
  midia: Id;
  sha256: string;
  de_q: number;
  ate_q: number;
  /** Tempo de fonte (trecho) ou do arquivo (clipe de áudio) no slot `de_q`. */
  inicio_s: Seg;
  ganho: number;
  barra: "original" | "narracao";
  // CONTRATO: acréscimos (P3.3)
  /** Volume do item (tempo de fonte no trecho, local no clipe de áudio); multiplica `ganho`. */
  volume: Animavel<number>;
  /** Rampas do clipe de áudio (s, lineares); 0 no trecho (as transições cuidam). */
  entrada_s: Seg;
  saida_s: Seg;
  /** Abaixa o original enquanto toca (`TrilhaAudio.abaixa_original`). */
  abaixa: boolean;
  /** Trechos silenciados (tempo de fonte; "tom" = apito). */
  silencios: { de: Seg; ate: Seg; tom: boolean }[];
  /** Fator da velocidade (trecho; 1 no clipe de áudio). */
  fator: number;
}

export interface PlanoPrevia {
  fps: Racional;
  total_q: number;
  segmentos: SegmentoPrevia[];
  livres: LivrePrevia[];
  sons: SomPrevia[];
}

/** Uma camada de vídeo no slot: o segmento, o tempo dele (fonte ou local) e o peso na transição (1 = sozinho). */
export interface CamadaPrevia {
  segmento: SegmentoPrevia;
  t: Seg;
  peso: number;
  // CONTRATO: acréscimos (P3.3)
  /** Quadro da fonte que o slot mostra (null sem fonte ou sem tabela). */
  indice: number | null;
  /** Slot local do clipe. */
  k: number;
  /** `escurecer` (fade para o preto, como o `fade` do ffmpeg): 1 = claro. */
  luz: number;
}

/** Transição no slot: `camadas` = [sai, entra] (sem o lado que é o fundo); `progresso` = quanto do que entra (k/d). */
export interface TransicaoPrevia {
  tipo: string;
  progresso: number;
  fundo: null | "sai" | "entra";
}

/** O que pintar no slot `q`: vídeo (um ou dois segmentos na transição) e os sobrepostos com a transformação. */
export interface EstadoPrevia {
  q: number;
  programa_s: Seg;
  camadas: CamadaPrevia[];
  livres: { livre: LivrePrevia; tau: Seg; transformacao: TransformacaoAvaliada }[];
  // CONTRATO: acréscimo (P3.3)
  transicao: TransicaoPrevia | null;
}

/** Faixa de velocidade que o `<video>` toca sozinho (§7.3). */
export const VELOCIDADE_TOCA = [0.25, 8] as const;

const SEM_TABELA: ContextoTempo = { tabela: () => null };

const fatorDe = (v: Velocidade) => (v.tipo === "constante" ? v.fator : 1);

/** Toca guiado: rampa ou velocidade fora do que o elemento toca sozinho. */
export function segmentoGuiado(v: Velocidade): boolean {
  return v.tipo === "rampa" || !(v.fator >= VELOCIDADE_TOCA[0] && v.fator <= VELOCIDADE_TOCA[1]);
}

/** Slots da transição dentro de um clipe de `n` slots (`blocos::slots_da_transicao`). */
const slotsDe = (t: Transicao | null | undefined, n: number, fps: Racional) => (t ? Math.min(slots(t.duracao_s, fps), n) : 0);

/** Quadros (índice da tabela da fonte) dos slots de um trecho, pelo mapa do Rust; null sem tabela ou faixa inválida. */
function quadrosDoTrecho(c: ClipeTrecho, tab: TabelaQuadros | null, fps: Racional, n_q: number): number[] | null {
  if (!tab) return null;
  try {
    const t = tabelaDoClipe(tab, c.efeitos);
    const campos = t === tab ? 1 : 2;
    const [a, b] = faixaDoTrecho(t, c.inicio_s, c.inicio_pts, c.fim_s, c.fim_pts);
    const m = mapaTrecho(t, a, b, c.velocidade, fps);
    if (m.n_q !== n_q) return null;
    return campos === 2 ? m.quadro_do_slot.map((j) => j >> 1) : m.quadro_do_slot;
  } catch {
    return null;
  }
}

/** Plano do programa. `ctx` dá a tabela de cada mídia (a mesma do `resolver`); sem ela o tempo sai da velocidade. */
// CONTRATO: `ctx` acrescentado (P3.3): o quadro de cada slot precisa da tabela, como no `resolver`.
export function planoDe(doc: ProjetoEdicao, linha: LinhaResolvida, ctx: ContextoTempo = SEM_TABELA): PlanoPrevia {
  const fps = linha.fps;
  const porId = new Map<Id, ClipePrincipal>();
  for (const c of doc.principal) if (clipeConhecido(c)) porId.set(c.id, c);
  const sha = (midia: Id) => doc.midias.find((m) => m.id === midia)?.sha256 ?? "";
  const midiaRef = (midia: Id) => doc.midias.find((m) => m.id === midia);
  const segmentos: SegmentoPrevia[] = [];
  const sons: SomPrevia[] = [];
  linha.clipes.forEach((r, i) => {
    const c = porId.get(r.id);
    if (!c) return;
    const proximo = linha.clipes[i + 1];
    const tSaida = proximo ? (porId.get(proximo.id)?.transicao_entrada ?? null) : doc.fecho;
    const tEntrada = c.transicao_entrada;
    // entre clipes que se sobrepõem a cabeça é a sobreposição da linha; no primeiro e no escurecer, os slots da transição
    const nEntrada = tEntrada ? (i > 0 && transicaoSobrepoe(tEntrada.tipo) ? r.sobrepoe_q : slotsDe(tEntrada, r.n_q, fps)) : 0;
    const nSaida = tSaida ? (proximo && transicaoSobrepoe(tSaida.tipo) ? proximo.sobrepoe_q : slotsDe(tSaida, r.n_q, fps)) : 0;
    const seg: SegmentoPrevia = {
      clipe: c.id,
      de_q: r.inicio_q,
      ate_q: r.inicio_q + r.n_q,
      fonte: null,
      entrada: tEntrada && nEntrada > 0 ? { tipo: tEntrada.tipo, n_q: nEntrada } : null,
      saida: tSaida && nSaida > 0 ? { tipo: tSaida.tipo, n_q: nSaida } : null,
      quadros: null,
      tabela: null,
      guiado: false,
    };
    if (c.tipo === "trecho") {
      const tab = ctx.tabela(c.midia);
      const quadros = quadrosDoTrecho(c, tab, fps, r.n_q);
      seg.quadros = quadros;
      seg.tabela = quadros ? tab : null;
      seg.guiado = segmentoGuiado(c.velocidade);
      const inicio = quadros && tab ? tempoDoQuadro(tab, quadros[0] as number) : c.inicio_s;
      seg.fonte = { midia: c.midia, sha256: sha(c.midia), inicio_s: inicio, velocidade: c.velocidade, congelado: false };
      const m = midiaRef(c.midia);
      if (doc.saida.audio && m?.tem_audio && !c.audio.mudo && !seg.guiado)
        sons.push({
          id: c.id,
          midia: c.midia,
          sha256: sha(c.midia),
          de_q: seg.de_q,
          ate_q: seg.ate_q,
          inicio_s: inicio,
          ganho: doc.mixagem.volume_original,
          barra: "original",
          volume: c.audio.volume,
          entrada_s: 0,
          saida_s: 0,
          abaixa: false,
          silencios: c.audio.silencios.map((s) => ({ de: s.de, ate: s.ate, tom: s.modo === "tom" })),
          fator: fatorDe(c.velocidade),
        });
    } else if (c.tipo === "congelar") {
      const tab = ctx.tabela(c.midia);
      const j = tab ? doPar(tab, c.instante_s, c.instante_pts) : null;
      seg.quadros = j !== null ? [j] : null;
      seg.tabela = tab;
      const t = tab && j !== null ? tempoDoQuadro(tab, j) : c.instante_s;
      seg.fonte = { midia: c.midia, sha256: sha(c.midia), inicio_s: t, velocidade: { tipo: "constante", fator: 1 }, congelado: true };
    }
    segmentos.push(seg);
  });
  const livres: LivrePrevia[] = [];
  for (const t of doc.sobreposicoes) {
    if (t.oculta) continue;
    for (const c of t.clipes) {
      if (!sobrepostoConhecido(c)) continue;
      const l = linha.livres.find((x) => x.id === c.id && x.trilha === t.id);
      if (l && l.n_q > 0) livres.push({ id: c.id, trilha: t.id, de_q: l.inicio_q, ate_q: l.inicio_q + l.n_q, sobreposto: c });
    }
  }
  if (doc.saida.audio)
    for (const t of doc.audios) {
      if (t.mudo) continue;
      for (const c of t.clipes) {
        const l = linha.livres.find((x) => x.id === c.id && x.trilha === t.id);
        if (!l || l.n_q <= 0) continue;
        sons.push({
          id: c.id,
          midia: c.midia,
          sha256: sha(c.midia),
          de_q: l.inicio_q,
          ate_q: l.inicio_q + l.n_q,
          inicio_s: c.de_s,
          ganho: t.volume,
          barra: "narracao",
          volume: c.volume,
          entrada_s: c.entrada_s,
          saida_s: c.saida_s,
          abaixa: t.abaixa_original,
          silencios: [],
          fator: 1,
        });
      }
    }
  return { fps, total_q: linha.total_q, segmentos, livres, sons };
}

/** Segmentos com o slot `q` (no máximo dois, na sobreposição), na ordem do programa. */
export function segmentosEm(plano: PlanoPrevia, q: number): SegmentoPrevia[] {
  const s = plano.segmentos;
  // último com de_q ≤ q
  let lo = 0;
  let hi = s.length;
  while (lo < hi) {
    const m = (lo + hi) >>> 1;
    if ((s[m] as SegmentoPrevia).de_q <= q) lo = m + 1;
    else hi = m;
  }
  const v: SegmentoPrevia[] = [];
  for (let i = Math.max(0, lo - 2); i < lo; i++) {
    const x = s[i] as SegmentoPrevia;
    if (q >= x.de_q && q < x.ate_q) v.push(x);
  }
  return v;
}

/** Segmento "dono" do slot (o que entra, na sobreposição): o que o seletor "Clipe" abre. */
export function segmentoDoSlot(plano: PlanoPrevia, q: number): SegmentoPrevia | null {
  const v = segmentosEm(plano, q);
  return v[v.length - 1] ?? null;
}

/** Tempo do clipe no slot local `k` (fonte no trecho; local nos demais) e o quadro da fonte. */
export function tempoNoSegmento(s: SegmentoPrevia, k: number, fps: Racional): { t: Seg; indice: number | null } {
  const local = (k * fps.den) / fps.num;
  const f = s.fonte;
  if (!f) return { t: local, indice: null };
  if (f.congelado) return { t: local, indice: s.quadros?.[0] ?? null };
  const j = s.quadros ? (s.quadros[Math.min(Math.max(k, 0), s.quadros.length - 1)] as number) : null;
  if (j !== null && s.tabela) return { t: tempoDoQuadro(s.tabela, j), indice: j };
  return { t: f.inicio_s + local * fatorDe(f.velocidade), indice: null };
}

/** `fade` do ffmpeg por tempo: entrada k/d nos d primeiros slots; saída 1 − (k − (n − d))/d nos d últimos. */
function luzDe(s: SegmentoPrevia, k: number): number {
  const n = s.ate_q - s.de_q;
  let luz = 1;
  if (s.entrada?.tipo === "escurecer" && k < s.entrada.n_q) luz *= k / s.entrada.n_q;
  if (s.saida?.tipo === "escurecer") {
    const d = s.saida.n_q;
    if (k >= n - d) luz *= 1 - (k - (n - d)) / d;
  }
  return Math.min(Math.max(luz, 0), 1);
}

const camada = (s: SegmentoPrevia, q: number, fps: Racional, peso: number): CamadaPrevia => {
  const k = q - s.de_q;
  const { t, indice } = tempoNoSegmento(s, k, fps);
  return { segmento: s, t, peso, indice, k, luz: luzDe(s, k) };
};

export function estadoEm(plano: PlanoPrevia, q: number): EstadoPrevia {
  const fps = plano.fps;
  const programa_s = (q * fps.den) / fps.num;
  const em = segmentosEm(plano, q);
  let camadas: CamadaPrevia[] = [];
  let transicao: TransicaoPrevia | null = null;
  if (em.length === 2) {
    const [a, b] = em as [SegmentoPrevia, SegmentoPrevia];
    const d = Math.max(1, a.ate_q - b.de_q);
    const p = (q - b.de_q) / d;
    transicao = { tipo: b.entrada?.tipo ?? "fundir", progresso: p, fundo: null };
    camadas = [camada(a, q, fps, 1 - p), camada(b, q, fps, p)];
  } else if (em.length === 1) {
    const s = em[0] as SegmentoPrevia;
    const k = q - s.de_q;
    const n = s.ate_q - s.de_q;
    const primeiro = s === plano.segmentos[0];
    const ultimo = s === plano.segmentos[plano.segmentos.length - 1];
    if (primeiro && s.entrada && s.entrada.tipo !== "escurecer" && k < s.entrada.n_q) {
      const p = k / s.entrada.n_q;
      transicao = { tipo: s.entrada.tipo, progresso: p, fundo: "sai" };
      camadas = [camada(s, q, fps, p)];
    } else if (ultimo && s.saida && s.saida.tipo !== "escurecer" && k >= n - s.saida.n_q) {
      const p = (k - (n - s.saida.n_q)) / s.saida.n_q;
      transicao = { tipo: s.saida.tipo, progresso: p, fundo: "entra" };
      camadas = [camada(s, q, fps, 1 - p)];
    } else camadas = [camada(s, q, fps, 1)];
  }
  const livres: EstadoPrevia["livres"] = [];
  for (const l of plano.livres) {
    if (q < l.de_q || q >= l.ate_q) continue;
    const tau = ((q - l.de_q) * fps.den) / fps.num;
    const dur = ((l.ate_q - l.de_q) * fps.den) / fps.num;
    livres.push({ livre: l, tau, transformacao: transformacaoEm(l.sobreposto, tau, dur) });
  }
  return { q, programa_s, camadas, livres, transicao };
}

/** Slot do programa em que o clipe mostra o tempo `t` da base dele (o seletor "Programa"); fora do clipe, a ponta. */
export function slotDoTempo(plano: PlanoPrevia, clipe: Id, t: Seg): number | null {
  const s = plano.segmentos.find((x) => x.clipe === clipe);
  if (!s) return null;
  const n = s.ate_q - s.de_q;
  const fps = plano.fps;
  if (s.fonte && !s.fonte.congelado && s.quadros && s.tabela) {
    const q = s.quadros;
    const j = exibido(s.tabela, t);
    // primeiro slot do quadro exibido; descartado (aceleração): o slot do quadro que o cobre
    let lo = 0;
    let hi = q.length;
    while (lo < hi) {
      const m = (lo + hi) >>> 1;
      if ((q[m] as number) < j) lo = m + 1;
      else hi = m;
    }
    let k = Math.min(lo, n - 1);
    if (lo < q.length && (q[lo] as number) > j && lo > 0) k = lo - 1;
    return s.de_q + Math.max(0, k);
  }
  if (s.fonte && !s.fonte.congelado) {
    const k = Math.floor(((t - s.fonte.inicio_s) / fatorDe(s.fonte.velocidade)) * (fps.num / fps.den) + 1e-9);
    return s.de_q + Math.min(Math.max(k, 0), n - 1);
  }
  const k = Math.floor((t * fps.num) / fps.den + 1e-9);
  return s.de_q + Math.min(Math.max(k, 0), n - 1);
}
