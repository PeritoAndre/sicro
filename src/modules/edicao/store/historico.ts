/**
 * Histórico de desfazer/refazer (§8.4): cada passo guarda documento, seleção e cursor de ANTES da mudança.
 * Documentos imutáveis (immer): guardar o passo é guardar a referência. Agrupamento por tempo só quando
 * quem chama pede (tecla repetida, campo numérico); arrasto é um passo pelo `iniciarGesto` da store.
 */

import type { Cursor, ProjetoEdicao, Selecao } from "@domain/edicao";

export const LIMITE_PASSOS = 200;
/** Mesma chave de agrupamento dentro desta janela = um passo só. */
export const JANELA_GRUPO_MS = 1000;

export interface Retrato {
  doc: ProjetoEdicao;
  selecao: Selecao;
  cursor: Cursor;
}
/** `rotulo` = nome da mudança que sai deste retrato (no `futuro`, a que o refazer reaplica). */
export interface Passo extends Retrato {
  rotulo: string;
  quando: number;
  grupo: string | null;
}
export interface Historico {
  passado: Passo[];
  futuro: Passo[];
}

export const historicoVazio = (): Historico => ({ passado: [], futuro: [] });

/** Registra a mudança `rotulo` feita a partir de `antes`. */
export function registrar(h: Historico, rotulo: string, antes: Retrato, quando: number, grupo: string | null = null): Historico {
  const topo = h.passado[h.passado.length - 1];
  if (grupo != null && topo && topo.grupo === grupo && h.futuro.length === 0 && quando - topo.quando < JANELA_GRUPO_MS) {
    // continua o mesmo passo; a janela anda com a última mudança
    return { passado: [...h.passado.slice(0, -1), { ...topo, quando }], futuro: [] };
  }
  const passado = [...h.passado, { ...antes, rotulo, quando, grupo }];
  return { passado: passado.length > LIMITE_PASSOS ? passado.slice(passado.length - LIMITE_PASSOS) : passado, futuro: [] };
}

/** Volta um passo: devolve o retrato a restaurar e o histórico novo; null = nada a desfazer. */
export function desfazer(h: Historico, atual: Retrato, quando: number): { h: Historico; alvo: Passo } | null {
  const alvo = h.passado[h.passado.length - 1];
  if (!alvo) return null;
  return {
    alvo,
    h: { passado: h.passado.slice(0, -1), futuro: [...h.futuro, { ...atual, rotulo: alvo.rotulo, quando, grupo: null }] },
  };
}

/** Reaplica o último passo desfeito. */
export function refazer(h: Historico, atual: Retrato, quando: number): { h: Historico; alvo: Passo } | null {
  const alvo = h.futuro[h.futuro.length - 1];
  if (!alvo) return null;
  return {
    alvo,
    h: { passado: [...h.passado, { ...atual, rotulo: alvo.rotulo, quando, grupo: null }], futuro: h.futuro.slice(0, -1) },
  };
}
