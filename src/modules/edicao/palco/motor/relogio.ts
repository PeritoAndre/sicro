/**
 * Relógio da prévia viva (§7.2, §7.3): o tempo de reprodução vive aqui, FORA do React (`assinar`); segue o
 * `currentTime` do elemento mestre do segmento (nunca o contrário); sem vídeo, relógio de parede. Estados
 * `parado | tocando | guiado | esperando`. O store recebe o cursor ao pausar, ao buscar e a cada `PUBLICAR_MS`.
 * Quem anda o relógio é o laço do motor (`tique`, um por rAF); com `agora` falso ele roda sem tela.
 */

import type { Racional, Seg } from "@domain/edicao";

export type EstadoRelogio = "parado" | "tocando" | "guiado" | "esperando";

/** A cada quanto o store recebe o cursor tocando (ms). */
export const PUBLICAR_MS = 250;
/** Mestre sem andar por tanto tempo (ms): o relógio espera (§7.3). */
export const PARADO_MS = 250;
/** Velocidade que o elemento toca sozinho (§7.3); fora disso, o modo guiado. */
export const TOCA_DE = 0.25;
export const TOCA_ATE = 8;

/** O elemento que manda no tempo do segmento: tempo dele já em tempo de fonte, e se está pronto para tocar. */
export interface Mestre {
  tempo(): Seg;
  pronto(): boolean;
}

export interface Relogio {
  estado(): EstadoRelogio;
  q(): number;
  /** Toca a partir do slot atual; `velocidade` fora de 0,25..8 vai ao modo guiado (§7.3). */
  tocar(velocidade?: number): void;
  pausar(): void;
  irPara(q: number): void;
  /** Mestre do segmento que começa (null = relógio de parede): `T = de_q + (tempo − t0)/velocidade · fps`. */
  mestre(m: Mestre | null, seg: { de_q: number; t0: Seg; velocidade: number } | null): void;
  /** Avisa a cada mudança de slot e de estado; devolve o cancelamento. */
  assinar(fn: (q: number, e: EstadoRelogio) => void): () => void;
  // CONTRATO: acréscimos (P3.3)
  /** Anda o relógio (um por rAF do motor). */
  tique(): void;
  /** Segmento guiado (rampa): relógio de parede no estado "guiado" enquanto `sim`. */
  guiar(sim: boolean): void;
  /** Velocidade pedida (1, ou a do J/L; negativa = ré). */
  velocidade(): number;
  /** Programa mudou de tamanho (documento editado). */
  total(total_q: number): void;
}

export interface OpcoesRelogio {
  fps: Racional;
  total_q: number;
  /** Relógio em ms (padrão `performance.now`). */
  agora?: () => number;
  // CONTRATO: acréscimo (P3.3)
  /** Cursor para o store: ao pausar, ao buscar e a cada `PUBLICAR_MS` tocando. */
  publicar?: (q: number) => void;
}

/** Velocidade que vai ao modo guiado. */
export const foraDaFaixa = (v: number) => !(v >= TOCA_DE && v <= TOCA_ATE);

export function criarRelogio(op: OpcoesRelogio): Relogio {
  const agora = op.agora ?? (() => performance.now());
  const fps = op.fps;
  let total = Math.max(0, op.total_q);
  let q = 0;
  let estado: EstadoRelogio = "parado";
  let taxa = 1;
  let guiadoSeg = false;
  // âncora do relógio de parede
  let q0 = 0;
  let parede0 = 0;
  // mestre
  let m: Mestre | null = null;
  let seg: { de_q: number; t0: Seg; velocidade: number } | null = null;
  let ultimoTempo = NaN;
  let ultimoAndou = 0;
  let ultimoPost = -Infinity;
  const ouvintes = new Set<(q: number, e: EstadoRelogio) => void>();

  const limitar = (n: number) => Math.min(Math.max(Math.floor(n), 0), Math.max(total - 1, 0));
  const avisar = () => {
    for (const f of [...ouvintes]) f(q, estado);
  };
  const publicar = () => {
    ultimoPost = agora();
    op.publicar?.(q);
  };
  const ancorar = () => {
    q0 = q;
    parede0 = agora();
  };
  const tocando = () => estado === "tocando" || estado === "guiado" || estado === "esperando";
  /** Estado de quem anda pelo relógio de parede. */
  const deParede = (): EstadoRelogio => (guiadoSeg || foraDaFaixa(taxa) ? "guiado" : "tocando");

  const parar = (fim: boolean) => {
    const antes = estado;
    estado = "parado";
    m = null;
    seg = null;
    if (fim) q = limitar(q);
    publicar();
    if (antes !== "parado" || fim) avisar();
  };

  return {
    estado: () => estado,
    q: () => q,
    velocidade: () => taxa,
    total: (n) => {
      total = Math.max(0, n);
      if (q > limitar(q)) {
        q = limitar(q);
        avisar();
      }
    },
    tocar: (velocidade = 1) => {
      if (total === 0 || !Number.isFinite(velocidade) || velocidade === 0) return;
      taxa = velocidade;
      // no fim (ou no começo, de ré): recomeça da ponta
      if (velocidade > 0 && q >= total - 1) q = 0;
      if (velocidade < 0 && q <= 0) return;
      estado = m && !foraDaFaixa(taxa) && !guiadoSeg ? "tocando" : deParede();
      ultimoTempo = NaN;
      ultimoAndou = agora();
      ancorar();
      avisar();
    },
    pausar: () => {
      if (!tocando()) return;
      parar(false);
    },
    irPara: (n) => {
      const novo = limitar(n);
      const mudou = novo !== q;
      q = novo;
      if (tocando()) ancorar();
      publicar();
      if (mudou) avisar();
    },
    mestre: (mm, s) => {
      m = mm;
      seg = mm ? s : null;
      ultimoTempo = NaN;
      ultimoAndou = agora();
      ancorar();
      if (tocando()) {
        const novo: EstadoRelogio = m && !foraDaFaixa(taxa) && !guiadoSeg ? "tocando" : deParede();
        if (novo !== estado && estado !== "esperando") {
          estado = novo;
          avisar();
        }
      }
    },
    guiar: (sim) => {
      if (guiadoSeg === sim) return;
      guiadoSeg = sim;
      ancorar();
      if (tocando() && !m) {
        estado = deParede();
        avisar();
      }
    },
    assinar: (fn) => {
      ouvintes.add(fn);
      return () => ouvintes.delete(fn);
    },
    tique: () => {
      if (!tocando()) return;
      const antes = q;
      const antesEstado = estado;
      const t = agora();
      if (m && seg && !foraDaFaixa(taxa) && !guiadoSeg) {
        const mt = m.tempo();
        if (mt !== ultimoTempo) {
          ultimoTempo = mt;
          ultimoAndou = t;
        }
        // mestre buscando, sem dados ou parado há PARADO_MS: espera (o relógio não anda)
        if (!m.pronto() || t - ultimoAndou > PARADO_MS) estado = "esperando";
        else {
          estado = "tocando";
          const k = Math.floor(((mt - seg.t0) / seg.velocidade) * (fps.num / fps.den) + 1e-6);
          // o elemento parte 30 a 90 ms atrasado: o relógio não volta
          q = Math.max(q, Math.min(seg.de_q + Math.max(k, 0), total));
        }
      } else {
        estado = deParede();
        q = q0 + Math.floor((((t - parede0) / 1000) * taxa * fps.num) / fps.den + 1e-6);
      }
      if (q >= total || q < 0) {
        q = q < 0 ? 0 : total - 1;
        parar(true);
        return;
      }
      if (t - ultimoPost >= PUBLICAR_MS) publicar();
      if (q !== antes || estado !== antesEstado) avisar();
    },
  };
}
