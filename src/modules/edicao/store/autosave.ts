/**
 * Salvamento automático da Edição (§8.4; defeito B13 da v1): grava 3 s depois da ÚLTIMA mudança, nunca mais
 * de 15 s depois da primeira não gravada; uma gravação por vez; mudança durante a gravação agenda outra;
 * erro vira estado "erro" e tenta de novo. `descarregar()` grava o que faltar e só resolve com tudo gravado.
 */

export const ATRASO_MS = 3000;
export const TETO_MS = 15000;
export const NOVA_TENTATIVA_MS = 3000;

export type EstadoSalvo = "salvo" | "alterado" | "salvando" | "erro";

export interface OpcoesAutosave {
  /** Revisão atual do documento (sobe a cada mudança). */
  revisao(): number;
  /** Grava o documento atual; devolve a revisão gravada. */
  gravar(): Promise<number>;
  /** Revisão que já está no disco ao começar. */
  salva: number;
  estado?(e: EstadoSalvo): void;
  gravou?(revisao: number): void;
  falhou?(erro: unknown): void;
}

export class Autosave {
  private salva: number;
  private timer: ReturnType<typeof setTimeout> | null = null;
  /** Quando surgiram a primeira e a última mudança ainda não gravadas. */
  private primeira: number | null = null;
  private ultima = 0;
  /** Gravação em curso (nunca rejeita: serve para esperar). */
  private emCurso: Promise<void> | null = null;
  private parado = false;
  private estado: EstadoSalvo = "salvo";

  constructor(private readonly o: OpcoesAutosave) {
    this.salva = o.salva;
  }

  get pendente(): boolean {
    return this.o.revisao() > this.salva;
  }
  get revisaoSalva(): number {
    return this.salva;
  }
  get gravando(): boolean {
    return this.emCurso !== null;
  }

  /** Avisar a cada mudança do documento. */
  mudou(): void {
    if (this.parado) return;
    const agora = Date.now();
    // `agora` menor que a primeira: o relógio do sistema andou para trás (o teto conta daqui)
    if (this.primeira === null || agora < this.primeira) this.primeira = agora;
    this.ultima = agora;
    // "erro" fica até uma gravação dar certo
    if (!this.emCurso && this.estado !== "erro") this.marcar("alterado");
    // em erro a nova tentativa já está marcada: mudança não a empurra
    if (this.estado === "erro" && this.timer !== null) return;
    this.agendar(Math.min(ATRASO_MS, this.primeira + TETO_MS - agora));
  }

  /** Grava já o que faltar, esperando a gravação em curso. Rejeita se a gravação falhar.
   *  Cancelado (exclusão em curso) ou parado: só espera; gravar recriaria a pasta. */
  async descarregar(): Promise<void> {
    this.limparTimer();
    for (let i = 0; i < 20; i++) {
      if (this.emCurso) {
        await this.emCurso;
        continue;
      }
      if (this.parado || !this.pendente) return;
      await this.gravarAgora();
    }
    if (this.pendente && !this.parado) throw new Error("salvamento não acompanhou as mudanças");
  }

  /** Para de agendar e espera a gravação em curso (antes de excluir o projeto). */
  async cancelar(): Promise<void> {
    this.parado = true;
    this.limparTimer();
    if (this.emCurso) await this.emCurso;
  }

  /** Volta a agendar depois de `cancelar` (exclusão recusada). */
  retomar(): void {
    this.parado = false;
    if (this.pendente) {
      this.primeira ??= Date.now();
      // o que mudou durante a espera não marcou nada
      if (this.estado !== "erro") this.marcar("alterado");
      this.agendar(ATRASO_MS);
    }
  }

  /** Para de vez, sem esperar. */
  parar(): void {
    this.parado = true;
    this.limparTimer();
  }

  private marcar(e: EstadoSalvo) {
    this.estado = e;
    this.o.estado?.(e);
  }

  private limparTimer() {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }

  private agendar(ms: number) {
    this.limparTimer();
    this.timer = setTimeout(() => {
      this.timer = null;
      if (this.parado || this.emCurso) return; // a gravação em curso reagenda ao terminar
      if (!this.pendente) {
        this.primeira = null;
        return;
      }
      this.gravarAgora().catch(() => {});
    }, Math.max(0, ms));
  }

  private gravarAgora(): Promise<void> {
    this.limparTimer();
    this.marcar("salvando");
    // mudanças feitas daqui em diante contam um prazo novo
    this.primeira = null;
    const p = this.o.gravar().then(
      (rev) => {
        if (rev > this.salva) this.salva = rev;
        this.o.gravou?.(this.salva);
        this.emCurso = null;
        this.marcar(this.pendente ? "alterado" : "salvo");
        if (this.pendente && !this.parado && this.timer === null) {
          const agora = Date.now();
          const prim = this.primeira ?? agora;
          // nunca além de ATRASO_MS: relógio do sistema atrasado no meio não adia a gravação
          this.agendar(Math.min(ATRASO_MS, this.ultima + ATRASO_MS - agora, prim + TETO_MS - agora));
        }
      },
      (e: unknown) => {
        this.emCurso = null;
        this.o.falhou?.(e);
        this.marcar("erro");
        if (!this.parado) this.agendar(NOVA_TENTATIVA_MS);
        throw e;
      },
    );
    this.emCurso = p.catch(() => {});
    return p;
  }
}
