/**
 * Quadro final (§7.2, D22): o motor real renderiza o slot pausado e o palco mostra a imagem por cima da viva.
 * `fiel`: "viva" | "pedindo" | { q, src }. Pedido automático 300 ms depois de pausar, nunca com passo de quadro nos
 * últimos 600 ms; a imagem fica até tocar ou mudar de slot; editar repete o pedido sem voltar à viva.
 * O botão "Quadro final" pede na hora. Resposta de um pedido velho não vale.
 */

export type Fiel = "viva" | "pedindo" | { q: number; src: string };

export const ESPERA_PAUSA_MS = 300;
export const SEM_PASSO_MS = 600;

export interface RelogioFiel {
  agora(): number;
  setTimeout(f: () => void, ms: number): unknown;
  clearTimeout(h: unknown): void;
}

const RELOGIO_PADRAO: RelogioFiel = {
  agora: () => performance.now(),
  setTimeout: (f, ms) => setTimeout(f, ms),
  clearTimeout: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
};

export class QuadroFiel {
  estado: Fiel = "viva";
  private q: number | null = null;
  private timer: unknown = null;
  private ultimoPasso = -Infinity;
  private seq = 0;

  constructor(
    /** Devolve o endereço da imagem do slot `q`. */
    private readonly pedir: (q: number) => Promise<string>,
    private readonly aoMudar: (f: Fiel) => void,
    private readonly relogio: RelogioFiel = RELOGIO_PADRAO,
  ) {}

  private definir(f: Fiel) {
    this.estado = f;
    this.aoMudar(f);
  }

  private cancelarTimer() {
    if (this.timer !== null) this.relogio.clearTimeout(this.timer);
    this.timer = null;
  }

  /** Esquece a imagem e o pedido em curso. */
  private viva() {
    this.cancelarTimer();
    this.seq++;
    if (this.estado !== "viva") this.definir("viva");
  }

  /** Parado num slot (`null` = fora do programa: nada a pedir). */
  pausou(q: number | null): void {
    if (q !== this.q) this.viva();
    this.q = q;
    if (q === null) return;
    this.agendar();
  }

  private agendar() {
    this.cancelarTimer();
    const desdePasso = this.relogio.agora() - this.ultimoPasso;
    const espera = Math.max(ESPERA_PAUSA_MS, SEM_PASSO_MS - desdePasso);
    // um passo novo cancela este e agenda de novo (`passo`)
    this.timer = this.relogio.setTimeout(() => {
      this.timer = null;
      if (this.q !== null) void this.buscar(this.q);
    }, espera);
  }

  /** Passo de quadro (←/→): volta à viva e adia o pedido. */
  passo(q: number | null): void {
    this.ultimoPasso = this.relogio.agora();
    this.viva();
    this.q = q;
    if (q !== null) this.agendar();
  }

  tocou(): void {
    this.q = null;
    this.viva();
  }

  /** Documento mudou: repete o pedido do mesmo slot, sem voltar à viva. */
  editou(): void {
    if (this.q === null || this.estado === "viva") return;
    this.seq++;
    this.cancelarTimer();
    this.timer = this.relogio.setTimeout(() => {
      this.timer = null;
      if (this.q !== null) void this.buscar(this.q, false);
    }, ESPERA_PAUSA_MS);
  }

  /** Botão "Quadro final". */
  pedirJa(q: number): void {
    this.q = q;
    this.cancelarTimer();
    void this.buscar(q);
  }

  private async buscar(q: number, mostrarPedindo = true): Promise<void> {
    const meu = ++this.seq;
    if (mostrarPedindo || this.estado === "viva") this.definir("pedindo");
    try {
      const src = await this.pedir(q);
      if (meu === this.seq && this.q === q) this.definir({ q, src });
    } catch {
      if (meu === this.seq) this.definir("viva");
    }
  }

  fechar(): void {
    this.cancelarTimer();
    this.seq++;
  }
}
