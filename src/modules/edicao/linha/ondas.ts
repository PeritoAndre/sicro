/**
 * Ondas dos itens de áudio e do `Som` (`edicao_onda`, P1.9): uma por mídia, pedida uma vez; `null` = pedida à fila de
 * fundo, pede de novo quando `onda:<sha256>` fica pronta. Fora do `palco/recursos` (chegar uma não repinta o editor
 * inteiro). Dono: P4.1.
 */

import { create } from "zustand";
import { commands } from "@core/commands";
import type { Onda, TarefaFundo } from "@domain/edicao";
import { useFundoStore } from "../store/fundoStore";

type Estado = { tipo: "pedindo" } | { tipo: "fundo" } | { tipo: "pronta"; onda: Onda } | { tipo: "falhou" };

export const useOndasVersao = create<{ versao: number }>(() => ({ versao: 0 }));
const subir = () => useOndasVersao.setState((s) => ({ versao: s.versao + 1 }));

export class Ondas {
  private porSha = new Map<string, Estado>();
  private ws: string | null = null;
  private vistas = new Set<string>();

  constructor(
    private readonly pedirOnda: (ws: string, sha: string) => Promise<Onda | null>,
    private readonly aoMudar: () => void,
    private readonly tarefasAgora: () => readonly TarefaFundo[] = () => [],
  ) {}

  /** A onda da mídia, ou null (pedida agora, se ainda não foi). */
  onda(ws: string, sha: string): Onda | null {
    if (this.ws !== ws) {
      this.ws = ws;
      this.porSha.clear();
    }
    const e = this.porSha.get(sha);
    if (e?.tipo === "pronta") return e.onda;
    if (!e) this.pedir(ws, sha);
    return null;
  }

  private pedir(ws: string, sha: string): void {
    this.porSha.set(sha, { tipo: "pedindo" });
    this.pedirOnda(ws, sha).then(
      (o) => {
        if (this.ws !== ws) return;
        if (o) {
          this.porSha.set(sha, { tipo: "pronta", onda: o });
          this.aoMudar();
          return;
        }
        this.porSha.set(sha, { tipo: "fundo" });
        // pronta antes de a resposta chegar: pede já (o backend agora lê do cache)
        const t = this.tarefasAgora().find((x) => x.chave === `onda:${sha}` && x.estado.tipo === "pronta");
        if (t && !this.vistas.has(`${t.id}:pronta`)) {
          this.vistas.add(`${t.id}:pronta`);
          this.pedir(ws, sha);
        }
      },
      () => {
        if (this.ws === ws) this.porSha.set(sha, { tipo: "falhou" });
      },
    );
  }

  receberFundo(tarefas: readonly TarefaFundo[]): void {
    for (const t of tarefas) {
      if (t.tipo !== "onda" || t.estado.tipo !== "pronta") continue;
      const v = `${t.id}:pronta`;
      if (this.vistas.has(v)) continue;
      this.vistas.add(v);
      const sha = t.chave.slice(t.chave.indexOf(":") + 1);
      if (this.ws && this.porSha.get(sha)?.tipo === "fundo") this.pedir(this.ws, sha);
    }
  }
}

export const ondas = new Ondas(
  (ws, sha) => Promise.resolve().then(() => commands.edicaoOnda(ws, sha)),
  subir,
  () => useFundoStore.getState().tarefas,
);
let ouvindo = false;
/** Assina a fila de fundo uma vez. */
export function ouvirOndas(): void {
  if (ouvindo) return;
  ouvindo = true;
  useFundoStore.subscribe((s) => ondas.receberFundo(s.tarefas));
}

const picos = new WeakMap<Onda, number>();
/** Maior valor absoluto da onda (escala do desenho: som baixo não some). */
export function picoDe(o: Onda): number {
  let p = picos.get(o);
  if (p === undefined) {
    p = 1;
    for (let i = 0; i < o.min.length; i++) p = Math.max(p, Math.abs(o.min[i] as number), Math.abs(o.max[i] as number));
    picos.set(o, p);
  }
  return p;
}
