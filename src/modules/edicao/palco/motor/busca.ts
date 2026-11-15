/**
 * Busca de quadro no palco (§7.1): um seek por vez; o pedido que chega durante outro vira o alvo pendente (vale o
 * último). Pinta no `seeked` mais um rAF e de novo no rAF seguinte: no Chromium com decodificação por software a
 * imagem do elemento às vezes ainda é a do quadro anterior no primeiro rAF (medido na bancada do P0.8b-1: 1 em 132).
 * Sem `seeked` em `ESPERA_MS` com o elemento já fora de `seeking` (busca para o mesmo instante em alguns motores) dá
 * o quadro por chegado; busca lenta (GOP longo, 4K) segue esperando: pintar antes mostraria a imagem velha com o
 * número do quadro novo.
 * Medida da busca (§7.6): `medir(alvos)` faz buscas pausadas pelo mesmo elemento e dá o tempo de cada uma até o
 * `seeked`; nada é pintado e qualquer `ir` no meio cancela a medida. No fim volta ao quadro que estava na tela.
 * Da primeira busca da medida até o elemento chegar de volta a um quadro pedido por `ir`, a imagem dele é a de OUTRO
 * quadro (`imagemAlheia`): quem pinta o canvas espera.
 */

import type { ElementoBusca } from "./elemento";

export const ESPERA_MS = 2000;
/** Pinturas depois do `seeked` (uma por rAF); a última é a que vale. */
export const PINTURAS = 2;

/** Busca medida que não chega em tanto tempo (ms) conta como esse tempo, e a medida termina. */
export const MEDIDA_TETO_MS = 3000;

// CONTRATO: acréscimos (contratos-fase1 F5 e F11): `Agenda.agora?`, `Buscador.{medir, cancelarMedida, medindo,
// imagemAlheia}`; `seeked` com o elemento ainda buscando não conta como chegada.
export interface Agenda {
  raf(f: () => void): void;
  setTimeout(f: () => void, ms: number): unknown;
  clearTimeout(h: unknown): void;
  /** Relógio (ms) das medidas; sem ele, `performance.now`. */
  agora?(): number;
}

const AGENDA_PADRAO: Agenda = {
  raf: (f) => requestAnimationFrame(() => f()),
  setTimeout: (f, ms) => setTimeout(f, ms),
  clearTimeout: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
  agora: () => performance.now(),
};

interface Medida {
  alvos: number[];
  tempos: number[];
  desde: number;
  /** Quadro que estava na tela: a medida termina voltando a ele. */
  voltar: number | null;
  fim(tempos: number[] | null): void;
}

export class Buscador {
  private emCurso: number | null = null;
  private pendente: number | null = null;
  private relogio: unknown = null;
  private solta: () => void;
  private vivo = true;
  /** Último quadro pintado. */
  ultimo: number | null = null;
  private medida: Medida | null = null;
  private alheia = false;
  private readonly agora: () => number;

  constructor(
    private readonly el: ElementoBusca,
    /** `final` = última pintura deste quadro (a anterior pode ter saído com a imagem velha do elemento). */
    private readonly pintar: (j: number, final: boolean) => void,
    private readonly agenda: Agenda = AGENDA_PADRAO,
  ) {
    this.agora = agenda.agora ?? (() => performance.now());
    this.solta = el.aoBuscado(() => this.chegou());
  }

  /** Quadro pedido e ainda não pintado (o pendente, se houver). Medindo, o quadro a que a medida volta: enquanto ela
   *  corre a imagem do elemento não é a do quadro pintado. */
  get alvo(): number | null {
    return this.pendente ?? this.emCurso ?? (this.medida ? (this.medida.voltar ?? -1) : null);
  }

  get medindo(): boolean {
    return this.medida !== null;
  }

  /** A imagem do elemento é a de uma busca da medida (em curso, terminada ou cancelada) e o quadro pedido depois dela
   *  ainda não chegou: não é a do último quadro pintado. */
  get imagemAlheia(): boolean {
    return this.alheia;
  }

  /** Buscas pausadas nos quadros `alvos`, uma por vez: o tempo (ms) de cada uma até o `seeked`. null = não mediu
   *  (busca em curso, cancelada por um `ir`, ou `seeked` que não veio com o elemento parado). Busca que passa de
   *  `MEDIDA_TETO_MS` conta como o teto e encerra a medida com o que há. */
  medir(alvos: readonly number[]): Promise<number[] | null> {
    if (!this.vivo || this.emCurso !== null || this.medida || alvos.length === 0) return Promise.resolve(null);
    return new Promise((fim) => {
      this.medida = { alvos: [...alvos], tempos: [], desde: 0, voltar: this.ultimo, fim };
      this.proximaMedida();
    });
  }

  /** Para a medida sem resultado (o perito tocou, buscou ou trocou de clipe). */
  cancelarMedida(): void {
    const m = this.medida;
    if (!m) return;
    this.medida = null;
    if (this.relogio !== null) this.agenda.clearTimeout(this.relogio);
    this.relogio = null;
    m.fim(null);
  }

  private proximaMedida(): void {
    const m = this.medida;
    if (!m) return;
    const j = m.alvos[m.tempos.length];
    if (j === undefined) return this.fimDaMedida(m.tempos);
    m.desde = this.agora();
    this.alheia = true;
    this.el.buscar(j);
    this.relogio = this.agenda.setTimeout(() => {
      this.relogio = null;
      if (!this.vivo || this.medida !== m) return;
      // ainda buscando: lenta de verdade; parado e sem `seeked`: não há o que medir
      if (this.el.buscando()) this.fimDaMedida([...m.tempos, MEDIDA_TETO_MS]);
      else this.fimDaMedida(null);
    }, MEDIDA_TETO_MS);
  }

  private fimDaMedida(tempos: number[] | null): void {
    const m = this.medida;
    if (!m) return;
    this.medida = null;
    if (this.relogio !== null) this.agenda.clearTimeout(this.relogio);
    this.relogio = null;
    m.fim(tempos);
    // de volta ao quadro que estava na tela (se ninguém pediu outro no `fim`)
    if (this.vivo && this.emCurso === null && m.voltar !== null) this.ir(m.voltar);
    // sem quadro a que voltar e sem pedido: não há imagem a esperar
    else if (this.emCurso === null) this.alheia = false;
  }

  ir(j: number): void {
    if (!this.vivo) return;
    this.cancelarMedida();
    if (this.emCurso !== null) {
      this.pendente = j;
      return;
    }
    this.emCurso = j;
    this.el.buscar(j);
    this.vigiar();
  }

  private vigiar(): void {
    this.relogio = this.agenda.setTimeout(() => {
      this.relogio = null;
      if (!this.vivo) return;
      // ainda buscando: o quadro não chegou
      if (this.el.buscando()) return this.vigiar();
      this.chegou();
    }, ESPERA_MS);
  }

  private chegou(): void {
    const m = this.medida;
    if (m) {
      if (this.relogio !== null) this.agenda.clearTimeout(this.relogio);
      this.relogio = null;
      m.tempos.push(this.agora() - m.desde);
      return this.proximaMedida();
    }
    const j = this.emCurso;
    if (j === null) return;
    // `seeked` de uma busca que outra substituiu (a da medida cancelada por este pedido): o elemento ainda busca o
    // quadro pedido; o `seeked` dele vem depois
    if (this.el.buscando()) return;
    if (this.relogio !== null) this.agenda.clearTimeout(this.relogio);
    this.relogio = null;
    let vez = 0;
    const quadro = () => {
      if (!this.vivo || this.emCurso !== j) return;
      vez++;
      const final = vez >= PINTURAS;
      this.ultimo = j;
      this.alheia = false;
      this.pintar(j, final);
      if (!final) return this.agenda.raf(quadro);
      this.emCurso = null;
      const p = this.pendente;
      this.pendente = null;
      if (p !== null && p !== j) this.ir(p);
    };
    this.agenda.raf(quadro);
  }

  fechar(): void {
    this.cancelarMedida();
    this.vivo = false;
    if (this.relogio !== null) this.agenda.clearTimeout(this.relogio);
    this.solta();
  }
}
