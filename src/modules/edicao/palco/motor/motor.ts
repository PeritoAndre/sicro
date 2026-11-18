/**
 * Laço da prévia viva (§7.2, §7.3): rAF → `relogio.q()` → `estadoEm` → `pinta`; prepara o segmento seguinte quando o
 * atual começa, troca o mestre na fronteira, corrige seguidores por seek acima de 100 ms. O palco usa o modo
 * "Programa" só com `PREVIA_VIVA_PRONTA` (regra 3 de contratos-fase4.md).
 * Regras do §7.3: o relógio segue o elemento mestre; `playbackRate` só muda com o elemento parado; rampa e velocidade
 * fora de 0,25..8 tocam guiadas (elemento parado, quadro por busca, sem som); clipes contíguos do mesmo arquivo usam
 * o mesmo elemento; corte para outro ponto do mesmo arquivo, um segundo elemento.
 */

import type { Animavel, Id, ProjetoEdicao, Seg, Transicao } from "@domain/edicao";
import { avaliar } from "../../modelo/curva";
import { acharClipe } from "../../modelo/projeto";
import type { RelogioMidia } from "./relogioMidia";
import { porNoTempo, tempoDoElemento } from "./elemento";
import { criarMistura, type MisturaAudio } from "./audio";
import { pinta, type FontesPintor } from "./pintor";
import { estadoEm, segmentoDoSlot, segmentosEm, tempoNoSegmento, type EstadoPrevia, type PlanoPrevia, type SegmentoPrevia, type SomPrevia } from "./plano";
import { criarPool, tamanhoDoPool, type ElementoPool, type Pool } from "./pool";
import { criarRelogio, foraDaFaixa, type Mestre, type Relogio } from "./relogio";

/** O pacote liga ao entregar. */
export const PREVIA_VIVA_PRONTA = true;

/** Seguidor fora do relógio por mais que isto (s) é corrigido por busca (§7.3). */
export const DESVIO_MAX_S = 0.1;
/** Segmentos que começam até tanto à frente (s) são preparados (preparar custa até 275 ms com GOP de 2 s). */
export const PREPARAR_S = 2;
/** Quadro tocando acima disto (ms) por 1 s: o canvas cai para 960×540 (§7.2). */
export const QUADRO_LENTO_MS = 25;
/** Pinturas depois do `seeked` (no Chromium a primeira às vezes ainda tem a imagem velha; `busca.ts`). */
const PINTURAS = 2;
/** Elementos de som de trilha de áudio (uma narração, uma música e um efeito ao mesmo tempo). */
const SONS = 3;
/** Nível do apito do "Silenciar trecho" (o `TOM` do `motor/audio.rs`). */
const APITO = 0.125;

/** O que o motor pede de fora (o palco liga aos recursos; a bancada, a arquivos). */
export interface FontesMotor {
  /** O que o elemento toca para a mídia do segmento: endereço e o relógio do arquivo; null = ainda não. */
  midia(seg: SegmentoPrevia): { src: string; relogio: RelogioMidia } | null;
  /** Arquivo de um som de trilha de áudio (o tempo do elemento é o do arquivo). */
  som(s: SomPrevia): string | null;
  /** Sprites, imagens, sobrepostos e camada global (o pintor). */
  pintor: Omit<FontesPintor, "video" | "apoio">;
}

export interface OpcoesMotor {
  fontes: FontesMotor;
  /** Cursor para o store (ao pausar, buscar e a cada 250 ms). */
  publicar?(q: number): void;
  /** Depois de cada pintura: slot, tempo gasto e o quadro de cada camada (alças e transporte do palco; bancada). */
  pintou?(p: Pintada): void;
  /** Canvas lento tocando: o palco reduz para 960×540. */
  lento?(): void;
  agora?(): number;
  /** Mistura própria (bancada); padrão: `criarMistura()` ao ligar. */
  mistura?: MisturaAudio | null;
}

export interface Pintada {
  q: number;
  ms: number;
  estado: EstadoPrevia;
  /** Todas as camadas no quadro pedido (parado: o quadro exato do slot). */
  exato: boolean;
}

export interface MotorPrevia {
  readonly relogio: Relogio;
  /** Canvas de saída e o elemento que hospeda o pool. */
  ligar(canvas: HTMLCanvasElement, host: HTMLElement): void;
  desligar(): void;
  /** Plano novo (documento mudou): o relógio fica no mesmo slot. */
  plano(p: PlanoPrevia, doc: ProjetoEdicao): void;
  // CONTRATO: acréscimos (P3.3): o palco anda pelo motor (prepara antes de tocar, busca e retoma).
  tocar(velocidade?: number): void;
  pausar(): void;
  irPara(q: number): void;
  /** Repinta no próximo rAF (recurso chegou, vista mudou). */
  repintar(): void;
  /** Parado com todas as camadas no quadro exato do slot. */
  exato(): boolean;
  readonly mistura: MisturaAudio | null;
}

/** Estado de busca de um elemento (um seek por vez, o último pedido vale). */
interface Busca {
  emCurso: number | null;
  pendente: number | null;
  mostrado: number | null;
  /** rAFs que faltam até a imagem do `seeked` valer. */
  pinturas: number;
  rel: RelogioMidia | null;
}

const fatorDe = (s: SegmentoPrevia) => (s.fonte && s.fonte.velocidade.tipo === "constante" ? s.fonte.velocidade.fator : 1);
const temVideo = (s: SegmentoPrevia) => !!s.fonte;
const toca = (s: SegmentoPrevia) => !!s.fonte && !s.fonte.congelado && !s.guiado;

export function criarMotor(op?: OpcoesMotor): MotorPrevia {
  const fontes = op?.fontes;
  const agora = op?.agora ?? (() => performance.now());
  let plano: PlanoPrevia = { fps: { num: 30, den: 1 }, total_q: 0, segmentos: [], livres: [], sons: [] };
  let doc: ProjetoEdicao | null = null;
  let relogio = criarRelogio({ fps: plano.fps, total_q: 0, agora, publicar: op?.publicar });
  let canvas: HTMLCanvasElement | null = null;
  let pool: Pool | null = null;
  let poolSom: Pool | null = null;
  let mistura: MisturaAudio | null = null;
  let laco: number | null = null;
  let sujo = true;
  /** Faixa (0..2) de cada segmento: clipes contíguos do mesmo arquivo ficam na mesma (o mesmo elemento); as outras
   *  giram, para o que vem adiante nunca tomar o elemento do que ainda toca. */
  let faixas = new Map<SegmentoPrevia, number>();
  const buscas = new Map<HTMLVideoElement, Busca>();
  const apoios = new Map<string, HTMLCanvasElement>();
  let mestreDe: SegmentoPrevia | null = null;
  let tocava = false;
  /** O mestre terminou antes do fim do segmento: o relógio de parede completa. */
  let paredeNoFim = false;
  let taxa = 1;
  let ultimaPintura = "";
  let lentoDesde = -1;
  let lentoSoma = 0;
  let lentoN = 0;

  const fps = () => plano.fps;

  /** Contíguo: o mesmo arquivo, o quadro seguinte, mesma velocidade, sem mistura entre os dois. */
  function contiguo(a: SegmentoPrevia, b: SegmentoPrevia): boolean {
    if (!a.fonte || !b.fonte || a.fonte.congelado || b.fonte.congelado || a.guiado || b.guiado) return false;
    if (a.fonte.midia !== b.fonte.midia || a.ate_q !== b.de_q || fatorDe(a) !== fatorDe(b)) return false;
    if (b.entrada && b.entrada.tipo !== "escurecer") return false;
    const ja = a.quadros?.[a.quadros.length - 1];
    const jb = b.quadros?.[0];
    return ja !== undefined && jb !== undefined && jb > ja && jb - ja <= Math.max(1, Math.ceil(fatorDe(a)));
  }

  function calcularFaixas() {
    faixas = new Map();
    let ant: SegmentoPrevia | null = null;
    let f = 0;
    for (const s of plano.segmentos) {
      if (!temVideo(s)) continue;
      if (ant && !contiguo(ant, s)) f = (f + 1) % 3;
      faixas.set(s, f);
      ant = s;
    }
  }

  const midiaDe = (s: SegmentoPrevia) => (s.fonte && fontes ? fontes.midia(s) : null);
  const chaveDe = (s: SegmentoPrevia, src: string) => `${src}#${faixas.get(s) ?? 0}`;

  function elementoDe(s: SegmentoPrevia): { e: ElementoPool; rel: RelogioMidia } | null {
    const m = midiaDe(s);
    if (!m || !pool) return null;
    const e = pool.pedir(chaveDe(s, m.src), m.src);
    let b = buscas.get(e.el);
    if (!b) {
      b = { emCurso: null, pendente: null, mostrado: null, pinturas: 0, rel: m.relogio };
      buscas.set(e.el, b);
      const busca = b;
      e.el.addEventListener("seeked", () => {
        if (busca.emCurso === null || e.el.seeking) return;
        busca.mostrado = busca.emCurso;
        busca.emCurso = null;
        busca.pinturas = PINTURAS;
        sujo = true;
        // o pendente sai em `encadear`, depois de a imagem ir à tela (encadear aqui deixava o guiado sem pintar nada)
      });
      e.el.addEventListener("loadeddata", () => void (sujo = true));
    }
    if (b.rel !== m.relogio) {
      // outro arquivo no mesmo elemento (proxy pronto): o quadro mostrado não vale mais
      b.rel = m.relogio;
      b.mostrado = null;
      b.emCurso = null;
      b.pendente = null;
      b.pinturas = 0;
    }
    return { e, rel: m.relogio };
  }

  function buscar(el: HTMLVideoElement, b: Busca, j: number) {
    if (!b.rel) return;
    // um seek por vez, e o seguinte só depois das pinturas do anterior (como o `Buscador` do modo clipe)
    if (b.emCurso !== null || b.pinturas > 0) {
      b.pendente = j;
      return;
    }
    if (b.mostrado === j && !el.seeking) return;
    b.emCurso = j;
    b.mostrado = null;
    if (el.readyState >= 1) porNoTempo(el, b.rel.alvo(j));
    else {
      const ir = () => {
        el.removeEventListener("loadedmetadata", ir);
        if (b.emCurso === j && b.rel) porNoTempo(el, b.rel.alvo(j));
      };
      el.addEventListener("loadedmetadata", ir);
    }
  }

  /** Quadro que o segmento mostra no slot `q` (entrada, para os que ainda vão começar). */
  const quadroEm = (s: SegmentoPrevia, q: number) => tempoNoSegmento(s, Math.min(Math.max(q - s.de_q, 0), s.ate_q - s.de_q - 1), fps()).indice;

  /** Elemento parado no quadro do slot `q` do segmento (com a velocidade já posta, parado). */
  function posicionar(s: SegmentoPrevia, q: number): { e: ElementoPool; rel: RelogioMidia } | null {
    const x = elementoDe(s);
    if (!x) return null;
    const j = quadroEm(s, q);
    if (j === null) return x;
    const el = x.e.el;
    if (!el.paused) el.pause();
    const r = Math.min(Math.max(fatorDe(s) * taxa, 0.0625), 16);
    if (toca(s) && el.playbackRate !== r) el.playbackRate = r;
    buscar(el, buscas.get(el)!, j);
    return x;
  }

  function tocarEl(el: HTMLMediaElement) {
    if (el.ended) return;
    // tocando, o quadro da última busca deixa de ser o da imagem: sem isso, pedir de novo esse quadro (repetir o
    // programa) não buscava, e o elemento tocava de onde tinha parado (o relógio pulava o clipe)
    const b = buscas.get(el as HTMLVideoElement);
    if (b) b.mostrado = null;
    recemTocados.add(el);
    void el.play().catch(() => {});
  }
  /** Elementos que acabaram de tocar: o volume deles vai ao fluxo mesmo igual (o fluxo novo herda o guardado). */
  const recemTocados = new Set<HTMLMediaElement>();
  const volume = (el: HTMLMediaElement, v: number) => {
    if (!mistura) return;
    mistura.volume(el, v, recemTocados.delete(el));
  };
  const buscarTocando = (el: HTMLMediaElement, t: number) => porNoTempo(el, t);
  /** O som sai pelo próprio elemento (`audio.ts`): o `currentTime` dele é o da imagem e do som. */
  const tempoReal = (el: HTMLMediaElement): number => tempoDoElemento(el);

  /** O elemento manda no relógio. */
  const mestreDoElemento = (el: HTMLVideoElement, rel: RelogioMidia): Mestre => ({
    tempo: () => rel.paraFonte(tempoReal(el)),
    pronto: () => !el.seeking && el.readyState >= 3 && !el.paused,
  });

  /** O segmento dono do slot manda no relógio (o elemento dele, ou o relógio de parede). */
  function trocarMestre(s: SegmentoPrevia | null) {
    mestreDe = s;
    paredeNoFim = false;
    if (!s) {
      relogio.guiar(false);
      relogio.mestre(null, null);
      return;
    }
    if (s.guiado || foraDaFaixa(fatorDe(s) * taxa) || taxa < 0) {
      relogio.mestre(null, null);
      relogio.guiar(true);
      return;
    }
    relogio.guiar(false);
    if (!toca(s)) {
      relogio.mestre(null, null);
      return;
    }
    const x = elementoDe(s);
    if (!x) {
      relogio.mestre(null, null);
      return;
    }
    relogio.mestre(mestreDoElemento(x.e.el, x.rel), { de_q: s.de_q, t0: s.fonte!.inicio_s, velocidade: fatorDe(s) });
  }

  const tocandoAgora = () => {
    const e = relogio.estado();
    return e === "tocando" || e === "esperando" || e === "guiado";
  };

  /** Elementos em uso agora e logo adiante (os outros são soltos). */
  function emUso(q: number): Set<string> {
    const v = new Set<string>();
    const ate = q + Math.ceil(PREPARAR_S * (fps().num / fps().den));
    for (const s of plano.segmentos) {
      if (s.ate_q <= q || s.de_q > ate || !temVideo(s)) continue;
      const m = midiaDe(s);
      if (m) v.add(chaveDe(s, m.src));
    }
    return v;
  }

  function soltarOsOutros(q: number) {
    if (!pool) return;
    const usados = emUso(q);
    for (const e of pool.elementos) if (e.chave && e.preso && !usados.has(e.chave)) pool.soltar(e.chave);
  }

  /** Prepara (parado no quadro de entrada) quem começa logo adiante e ainda não toca. */
  function prepararAdiante(q: number) {
    const ate = q + Math.ceil(PREPARAR_S * (fps().num / fps().den));
    const atuais = new Set(segmentosEm(plano, q));
    const ocupados = new Set<string>();
    for (const s of atuais) {
      const m = midiaDe(s);
      if (m) ocupados.add(chaveDe(s, m.src));
    }
    for (const s of plano.segmentos) {
      if (s.de_q <= q || s.de_q > ate || !temVideo(s) || atuais.has(s)) continue;
      // o elemento do que toca agora (o contíguo segue nele) não é tomado
      const m = midiaDe(s);
      if (!m || ocupados.has(chaveDe(s, m.src))) continue;
      posicionar(s, s.de_q);
    }
  }

  // ---- som ----

  function ganhoDoSom(s: SomPrevia, q: number, t: Seg): number {
    const f = fps();
    const tau = ((q - s.de_q) * f.den) / f.num;
    const dur = ((s.ate_q - s.de_q) * f.den) / f.num;
    const vol = avaliar(s.volume as Animavel<number>, s.barra === "original" ? t : tau);
    let g = s.ganho * (Number.isFinite(vol) ? vol : 1);
    if (s.entrada_s > 0) g *= Math.min(1, tau / s.entrada_s);
    if (s.saida_s > 0) g *= Math.min(1, (dur - tau) / s.saida_s);
    for (const x of s.silencios) if (t >= x.de && t < x.ate) return 0;
    return g;
  }

  function transicaoDe(clipe: Id): Transicao | null {
    const a = doc ? acharClipe(doc, clipe) : null;
    return a?.onde === "principal" ? a.clipe.transicao_entrada : null;
  }

  /** Ganho da barra do original (abaixar sob a narração), andado a cada volta pela constante do ataque ou da soltura. */
  let barraOriginal = 1;
  let ultimoSom = -1;

  /** Volume de cada elemento (o som sai por ele): o do item, a transição em potência constante, o escurecer e o
   *  abaixar; o apito do "Silenciar trecho" à parte. */
  function atualizarSom(estado: EstadoPrevia) {
    if (!mistura) return;
    const q = estado.q;
    const tocando = tocandoAgora() && relogio.estado() !== "guiado";
    const t = agora();
    const dt = ultimoSom < 0 ? 0 : Math.max(0, (t - ultimoSom) / 1000);
    ultimoSom = t;
    // trilhas de áudio (e se alguma abaixa o original)
    let abaixa = false;
    const f = fps();
    const ativos = new Set<string>();
    for (const s of plano.sons) {
      if (s.barra !== "narracao") continue;
      const comeca = s.de_q - Math.ceil(PREPARAR_S * (f.num / f.den));
      if (q < comeca || q >= s.ate_q) continue;
      const src = fontes?.som(s);
      if (!src || !poolSom) continue;
      const chave = `som:${s.id}`;
      ativos.add(chave);
      const e = poolSom.pedir(chave, src);
      const dentro = q >= s.de_q;
      const alvo = s.inicio_s + Math.max(0, ((q - s.de_q) * f.den) / f.num);
      if (dentro && tocando) {
        if (e.el.paused) tocarEl(e.el);
        else if (!e.el.seeking && Math.abs(tempoReal(e.el) - alvo) > DESVIO_MAX_S) buscarTocando(e.el, alvo);
        if (s.abaixa) abaixa = true;
      } else {
        if (!e.el.paused) e.el.pause();
        if (Math.abs(tempoDoElemento(e.el) - alvo) > 1e-3 && !e.el.seeking && e.el.readyState >= 1) porNoTempo(e.el, alvo);
      }
      if (dentro && tocando) volume(e.el, ganhoDoSom(s, q, alvo));
      else mistura.repouso(e.el);
    }
    for (const e of poolSom?.elementos ?? []) if (e.chave && !ativos.has(e.chave)) poolSom!.soltar(e.chave);
    const ab = doc?.mixagem.abaixar;
    const alvoBarra = ab && abaixa ? Math.pow(10, ab.db / 20) : 1;
    const tau = ab ? Math.max((abaixa ? ab.ataque_s : ab.soltura_s) / 3, 1e-3) : 1e-3;
    barraOriginal = tocando ? barraOriginal + (alvoBarra - barraOriginal) * (1 - Math.exp(-dt / tau)) : alvoBarra;
    // o som do trecho mora no elemento do vídeo
    let apito = false;
    const vivos = new Set<HTMLMediaElement>();
    for (const c of estado.camadas) {
      const s = c.segmento;
      if (!toca(s)) continue;
      const m = midiaDe(s);
      const e = m && pool ? pool.achar(chaveDe(s, m.src)) : null;
      if (!e) continue;
      const som = plano.sons.find((x) => x.id === s.clipe && x.barra === "original");
      let g = som && tocando ? ganhoDoSom(som, q, c.t) : 0;
      if (som && tocando) for (const x of som.silencios) if (x.tom && c.t >= x.de && c.t < x.ate) apito = true;
      // transição: potência constante ("acompanhar") ou troca no meio ("cortar")
      if (estado.transicao) {
        const p = estado.transicao.progresso;
        const entra = estado.transicao.fundo === "sai" || (estado.transicao.fundo === null && c === estado.camadas[1]);
        const cortar = transicaoDe(estado.transicao.fundo === "entra" ? s.clipe : (estado.camadas[1]?.segmento.clipe ?? s.clipe))?.audio === "cortar";
        g *= cortar ? ((entra ? p >= 0.5 : p < 0.5) ? 1 : 0) : entra ? Math.sin((p * Math.PI) / 2) : Math.cos((p * Math.PI) / 2);
      }
      // parado (ainda não tocou, ou o relógio parou): o silêncio é a pausa e o volume fica em repouso
      if (e.el.paused) mistura.repouso(e.el);
      else volume(e.el, g * c.luz * barraOriginal);
      vivos.add(e.el);
    }
    // fora das camadas: tocando ainda (vai parar), calado; parado, em repouso
    for (const e of pool?.elementos ?? []) if (!vivos.has(e.el)) (e.el.paused ? mistura.repouso(e.el) : volume(e.el, 0));
    mistura.apito(apito ? APITO : 0);
  }

  // ---- pintura ----

  function apoio(nome: string): HTMLCanvasElement | null {
    let c = apoios.get(nome);
    if (!c && typeof document !== "undefined") {
      c = document.createElement("canvas");
      apoios.set(nome, c);
    }
    return c ?? null;
  }

  function videoDe(clipe: Id): HTMLVideoElement | null {
    const s = plano.segmentos.find((x) => x.clipe === clipe);
    if (!s) return null;
    const m = midiaDe(s);
    const e = m && pool ? pool.achar(chaveDe(s, m.src)) : null;
    if (!e || e.el.readyState < 2) return null;
    const b = buscas.get(e.el);
    // imagem de um quadro que não foi o pedido (busca em curso sem nada mostrado): não pinta
    if (b && b.mostrado === null && b.emCurso !== null && e.el.paused) return null;
    return e.el;
  }

  /** Parado: todas as camadas com o quadro do slot já na imagem. */
  function camadasExatas(estado: EstadoPrevia): boolean {
    for (const c of estado.camadas) {
      if (!c.segmento.fonte || c.indice === null) continue;
      const m = midiaDe(c.segmento);
      const e = m && pool ? pool.achar(chaveDe(c.segmento, m.src)) : null;
      const b = e ? buscas.get(e.el) : null;
      if (!e || !b || b.mostrado !== c.indice || b.emCurso !== null || b.pinturas > 0 || e.el.readyState < 2) return false;
    }
    return true;
  }

  /** Alguma camada tem elemento mas ainda sem o quadro (busca em curso, sem dados): fica o quadro anterior. */
  function esperandoImagem(estado: EstadoPrevia): boolean {
    for (const c of estado.camadas) {
      if (!c.segmento.fonte || !midiaDe(c.segmento)) continue;
      if (!videoDe(c.segmento.clipe)) return true;
    }
    return false;
  }

  function pintar(estado: EstadoPrevia) {
    if (!canvas || !doc || !fontes) return;
    const ctx = canvas.getContext("2d", { alpha: false }) as CanvasRenderingContext2D | null;
    if (!ctx) return;
    const t0 = agora();
    pinta(ctx, estado, doc, { ...fontes.pintor, video: videoDe, apoio }, { w: canvas.width, h: canvas.height });
    const ms = agora() - t0;
    const exato = relogio.estado() === "parado" && camadasExatas(estado);
    op?.pintou?.({ q: estado.q, ms, estado, exato });
    // canvas lento tocando por 1 s: o palco reduz
    if (relogio.estado() === "tocando") {
      if (lentoDesde < 0) lentoDesde = t0;
      lentoSoma += ms;
      lentoN++;
      if (t0 - lentoDesde >= 1000) {
        if (lentoSoma / Math.max(lentoN, 1) > QUADRO_LENTO_MS) op?.lento?.();
        lentoDesde = t0;
        lentoSoma = 0;
        lentoN = 0;
      }
    } else lentoDesde = -1;
  }

  // ---- laço ----

  function passo() {
    laco = null;
    if (!canvas) return;
    relogio.tique();
    const q = relogio.q();
    const estado = estadoEm(plano, q);
    const tocando = tocandoAgora();
    // o relógio parou sozinho (fim do programa): os elementos param junto
    if (!tocando && tocava) {
      pararElementos();
      mestreDe = null;
      sujo = true;
    }
    tocava = tocando;
    if (tocando) {
      const dono = segmentoDoSlot(plano, q);
      if (dono !== mestreDe) trocarMestre(dono);
      for (const c of estado.camadas) {
        const s = c.segmento;
        if (!temVideo(s)) continue;
        if (toca(s) && relogio.estado() !== "guiado") {
          const x = elementoDe(s);
          if (!x) continue;
          const el = x.e.el;
          // terminou (o som chega ao fim do arquivo antes, no WebKitGTK): fica a última imagem e o relógio de parede
          if (el.ended) {
            if (s === mestreDe && !paredeNoFim) {
              paredeNoFim = true;
              relogio.mestre(null, null);
            }
            continue;
          }
          if (el.paused && !el.seeking) {
            const b = buscas.get(el);
            // ainda não está no quadro de entrada (pulo, preparo atrasado): busca e toca quando chegar
            const j = c.indice;
            if (j !== null && b && b.mostrado !== j && Math.abs(x.rel.paraFonte(tempoReal(el)) - c.t) > DESVIO_MAX_S * fatorDe(s)) posicionar(s, q);
            else {
              const r = Math.min(Math.max(fatorDe(s) * taxa, 0.0625), 16);
              if (el.playbackRate !== r) el.playbackRate = r;
              tocarEl(el);
            }
          } else if (s !== mestreDe && !el.paused && !el.seeking) {
            // seguidor (o que entra na transição): corrige por busca acima de 100 ms
            if (Math.abs(x.rel.paraFonte(tempoReal(el)) - c.t) > DESVIO_MAX_S * Math.max(1, fatorDe(s))) buscarTocando(el, x.rel.alvo(c.indice ?? 0));
          }
        } else if (c.indice !== null) posicionar(s, q);
      }
      prepararAdiante(q);
      soltarOsOutros(q);
    } else {
      for (const c of estado.camadas) if (c.segmento.fonte && c.indice !== null) posicionar(c.segmento, q);
    }
    atualizarSom(estado);
    // pinta quando o slot, o quadro de algum elemento ou o documento mudou
    let chave = `${q}|${canvas.width}x${canvas.height}`;
    for (const c of estado.camadas) {
      const v = videoDe(c.segmento.clipe);
      const b = v ? buscas.get(v) : null;
      if (b && b.pinturas > 0) {
        b.pinturas--;
        sujo = true;
      }
      chave += `|${v && b?.rel ? b.rel.quadro(tempoReal(v)) : "-"}`;
    }
    // tocando, toda volta pinta: o quadro do elemento troca sem aviso (o tempo dele anda aos pulos)
    if ((sujo || chave !== ultimaPintura || (tocando && relogio.estado() === "tocando")) && !esperandoImagem(estado)) {
      sujo = false;
      ultimaPintura = chave;
      pintar(estado);
    }
    encadear(estado);
    laco = requestAnimationFrame(passo);
  }

  /** Busca pendente sai depois das pinturas da anterior; elemento que não está na tela não espera pintura. */
  function encadear(estado: EstadoPrevia) {
    const naTela = new Set<HTMLVideoElement>();
    for (const c of estado.camadas) {
      const v = videoDe(c.segmento.clipe);
      if (v) naTela.add(v);
    }
    for (const [el, b] of buscas) {
      if (b.pendente === null || b.emCurso !== null) continue;
      if (b.pinturas > 0) {
        if (naTela.has(el)) continue;
        b.pinturas = 0;
      }
      const p = b.pendente;
      b.pendente = null;
      buscar(el, b, p);
    }
  }

  const agendar = () => {
    if (laco === null && canvas) laco = requestAnimationFrame(passo);
  };

  function pararElementos() {
    for (const e of [...(pool?.elementos ?? []), ...(poolSom?.elementos ?? [])]) {
      if (!e.el.paused) e.el.pause();
      mistura?.repouso(e.el);
    }
  }

  return {
    get relogio() {
      return relogio;
    },
    get mistura() {
      return mistura;
    },
    ligar: (cv, host) => {
      canvas = cv;
      mistura = op?.mistura !== undefined ? op.mistura : criarMistura();
      pool = criarPool({ tamanho: tamanhoDoPool(1), host });
      poolSom = criarPool({ tamanho: SONS, host });
      mistura?.mestre(1);
      sujo = true;
      agendar();
    },
    desligar: () => {
      if (laco !== null) cancelAnimationFrame(laco);
      laco = null;
      relogio.pausar();
      pararElementos();
      pool?.destruir();
      poolSom?.destruir();
      pool = null;
      poolSom = null;
      buscas.clear();
      mistura?.fechar();
      mistura = null;
      canvas = null;
      mestreDe = null;
    },
    plano: (p, d) => {
      const novoFps = p.fps.num !== plano.fps.num || p.fps.den !== plano.fps.den;
      plano = p;
      doc = d;
      calcularFaixas();
      if (novoFps) {
        const q = relogio.q();
        relogio = criarRelogio({ fps: p.fps, total_q: p.total_q, agora, publicar: op?.publicar });
        relogio.irPara(q);
      } else relogio.total(p.total_q);
      mestreDe = null;
      if (tocandoAgora()) trocarMestre(segmentoDoSlot(plano, relogio.q()));
      sujo = true;
      agendar();
    },
    tocar: (velocidade = 1) => {
      if (plano.total_q === 0 || !Number.isFinite(velocidade) || velocidade === 0) return;
      taxa = velocidade;
      void mistura?.retomar();
      // de ré no começo, nada; no fim, recomeça (o relógio decide)
      const q0 = velocidade > 0 && relogio.q() >= plano.total_q - 1 ? 0 : relogio.q();
      if (q0 !== relogio.q()) relogio.irPara(q0);
      // o mestre antes de tocar: o relógio não anda até o elemento andar
      for (const c of estadoEm(plano, q0).camadas) if (c.segmento.fonte) posicionar(c.segmento, q0);
      trocarMestre(segmentoDoSlot(plano, q0));
      relogio.tocar(velocidade);
      agendar();
    },
    pausar: () => {
      relogio.pausar();
      pararElementos();
      mestreDe = null;
      sujo = true;
      agendar();
    },
    irPara: (q) => {
      const tocava = tocandoAgora();
      if (tocava) {
        relogio.pausar();
        pararElementos();
      }
      relogio.irPara(q);
      mestreDe = null;
      sujo = true;
      if (tocava) {
        const q0 = relogio.q();
        for (const c of estadoEm(plano, q0).camadas) if (c.segmento.fonte) posicionar(c.segmento, q0);
        trocarMestre(segmentoDoSlot(plano, q0));
        relogio.tocar(taxa);
      }
      agendar();
    },
    repintar: () => {
      sujo = true;
      agendar();
    },
    exato: () => relogio.estado() === "parado" && camadasExatas(estadoEm(plano, relogio.q())),
  };
}
