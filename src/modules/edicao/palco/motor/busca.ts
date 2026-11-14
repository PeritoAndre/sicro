/**
 * Busca de quadro no palco (§7.1): um seek por vez; o pedido que chega durante outro vira o alvo pendente (vale o
 * último). Pinta no `seeked` mais um rAF e de novo no rAF seguinte: no Chromium com decodificação por software a
 * imagem do elemento às vezes ainda é a do quadro anterior no primeiro rAF (medido na bancada do P0.8b-1: 1 em 132).
 * Sem `seeked` em `ESPERA_MS` com o elemento já fora de `seeking` (busca para o mesmo instante em alguns motores) dá
 * o quadro por chegado; busca lenta (GOP longo, 4K) segue esperando: pintar antes mostraria a imagem velha com o
 * número do quadro novo.
 */

import type { ElementoBusca } from "./elemento";

export const ESPERA_MS = 2000;
/** Pinturas depois do `seeked` (uma por rAF); a última é a que vale. */
export const PINTURAS = 2;

export interface Agenda {
  raf(f: () => void): void;
  setTimeout(f: () => void, ms: number): unknown;
  clearTimeout(h: unknown): void;
}

const AGENDA_PADRAO: Agenda = {
  raf: (f) => requestAnimationFrame(() => f()),
  setTimeout: (f, ms) => setTimeout(f, ms),
  clearTimeout: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
};

export class Buscador {
  private emCurso: number | null = null;
  private pendente: number | null = null;
  private relogio: unknown = null;
  private solta: () => void;
  private vivo = true;
  /** Último quadro pintado. */
  ultimo: number | null = null;

  constructor(
    private readonly el: ElementoBusca,
    /** `final` = última pintura deste quadro (a anterior pode ter saído com a imagem velha do elemento). */
    private readonly pintar: (j: number, final: boolean) => void,
    private readonly agenda: Agenda = AGENDA_PADRAO,
  ) {
    this.solta = el.aoBuscado(() => this.chegou());
  }

  /** Quadro pedido e ainda não pintado (o pendente, se houver). */
  get alvo(): number | null {
    return this.pendente ?? this.emCurso;
  }

  ir(j: number): void {
    if (!this.vivo) return;
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
    const j = this.emCurso;
    if (j === null) return;
    if (this.relogio !== null) this.agenda.clearTimeout(this.relogio);
    this.relogio = null;
    let vez = 0;
    const quadro = () => {
      if (!this.vivo || this.emCurso !== j) return;
      vez++;
      const final = vez >= PINTURAS;
      this.ultimo = j;
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
    this.vivo = false;
    if (this.relogio !== null) this.agenda.clearTimeout(this.relogio);
    this.solta();
  }
}
