/**
 * Ondas dos itens de áudio e do `Som` (`edicao_onda`, P1.9): uma por mídia, pedida uma vez; `null` = pedida à fila de
 * fundo, pede de novo quando `onda:<sha256>` fica pronta. Fora do `palco/recursos` (chegar uma não repinta o editor
 * inteiro). Dono: P4.1.
 */

import { create } from "zustand";
import { commands } from "@core/commands";
import type { Onda, TarefaFundo } from "@domain/edicao";
import { useFundoStore } from "../store/fundoStore";
import { WS_DE_MENTIRA } from "./tiras";

type Estado = { tipo: "pedindo" } | { tipo: "fundo" } | { tipo: "pronta"; onda: Onda } | { tipo: "falhou" };

export const useOndasVersao = create<{ versao: number }>(() => ({ versao: 0 }));
const subir = () => useOndasVersao.setState((s) => ({ versao: s.versao + 1 }));

/** Casos guardados ao mesmo tempo (o aberto e o da medida do Diagnóstico): trocar de um para o outro não pede tudo de novo. */
const CASOS = 2;

export class Ondas {
  private porWs = new Map<string, Map<string, Estado>>();
  private vistas = new Set<string>();

  constructor(
    private readonly pedirOnda: (ws: string, sha: string) => Promise<Onda | null>,
    private readonly aoMudar: () => void,
    private readonly tarefasAgora: () => readonly TarefaFundo[] = () => [],
  ) {}

  /** As do caso; o caso mais antigo além de `CASOS` sai. */
  private doCaso(ws: string): Map<string, Estado> {
    let m = this.porWs.get(ws);
    if (m) {
      this.porWs.delete(ws);
      this.porWs.set(ws, m);
      return m;
    }
    m = new Map();
    this.porWs.set(ws, m);
    while (this.porWs.size > CASOS) this.porWs.delete(this.porWs.keys().next().value as string);
    return m;
  }

  /** A onda da mídia, ou null (pedida agora, se ainda não foi). */
  onda(ws: string, sha: string): Onda | null {
    const e = this.doCaso(ws).get(sha);
    if (e?.tipo === "pronta") return e.onda;
    if (!e) this.pedir(ws, sha);
    return null;
  }

  private pedir(ws: string, sha: string): void {
    const porSha = this.doCaso(ws);
    porSha.set(sha, { tipo: "pedindo" });
    this.pedirOnda(ws, sha).then(
      (o) => {
        if (this.porWs.get(ws) !== porSha) return;
        if (o) {
          porSha.set(sha, { tipo: "pronta", onda: o });
          this.aoMudar();
          return;
        }
        porSha.set(sha, { tipo: "fundo" });
        // pronta antes de a resposta chegar: pede já (o backend agora lê do cache)
        const t = this.tarefasAgora().find((x) => x.chave === `onda:${sha}` && x.estado.tipo === "pronta");
        if (t && !this.vistas.has(`${t.id}:pronta`)) {
          this.vistas.add(`${t.id}:pronta`);
          this.pedir(ws, sha);
        }
      },
      () => {
        if (this.porWs.get(ws) === porSha) porSha.set(sha, { tipo: "falhou" });
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
      for (const [ws, m] of this.porWs) if (m.get(sha)?.tipo === "fundo") this.pedir(ws, sha);
    }
  }
}

/** Medida do Diagnóstico: ondas feitas na hora enquanto ela roda; null = as do app. */
let deMentira: ((sha: string) => Onda) | null = null;
export function ondasDeMentira(f: typeof deMentira): void {
  deMentira = f;
}

export const ondas = new Ondas(
  (ws, sha) => (deMentira && ws === WS_DE_MENTIRA ? Promise.resolve(deMentira(sha)) : Promise.resolve().then(() => commands.edicaoOnda(ws, sha))),
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
