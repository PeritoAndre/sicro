/** Regras puras da Montagem: criação de itens, duração e reordenação. */

import type {
  Montagem,
  MontagemItem,
  MontagemTexto,
} from "@domain/montagem";

export const uid = (): string => crypto.randomUUID();

export function novaMontagem(titulo = "Montagem"): Montagem {
  const agora = new Date().toISOString();
  return {
    id: uid(),
    titulo,
    created_at: agora,
    updated_at: agora,
    itens: [],
    saida: { largura: 1920, altura: 1080, fps: 30, incluir_audio: true },
  };
}

export const textoPadrao = (): MontagemTexto => ({
  conteudo: "",
  posicao: "rodape",
  tamanho: "m",
  carimbo_tempo: false,
});

export function novoTrecho(media_id: string, inicio_s: number, fim_s: number): MontagemItem {
  return { tipo: "trecho", id: uid(), media_id, inicio_s, fim_s, velocidade: 1, texto: null, transicao_entrada: "corte" };
}

export function novaCartela(texto: string, duracao_s: number): MontagemItem {
  return {
    tipo: "cartela",
    id: uid(),
    duracao_s,
    texto,
    cor_fundo: "#000000",
    cor_texto: "#ffffff",
    tamanho: "m",
    transicao_entrada: "corte",
  };
}

export function novoCongelar(media_id: string, instante_s: number, duracao_s: number): MontagemItem {
  return { tipo: "congelar", id: uid(), media_id, instante_s, duracao_s, texto: null };
}

/** Duração do item na saída (já com a velocidade). */
export function duracaoItem(it: MontagemItem): number {
  if (it.tipo === "trecho") return Math.max(0, (it.fim_s - it.inicio_s) / (it.velocidade || 1));
  return Math.max(0, it.duracao_s);
}

export const duracaoTotal = (itens: MontagemItem[]): number =>
  itens.reduce((a, it) => a + duracaoItem(it), 0);

export function mover<T>(lista: T[], de: number, para: number): T[] {
  if (de === para || de < 0 || para < 0 || de >= lista.length || para >= lista.length) return lista;
  const r = lista.slice();
  const [x] = r.splice(de, 1);
  r.splice(para, 0, x as T);
  return r;
}

/** Problema que impede de renderizar, ou null. */
export function validarItem(it: MontagemItem): string | null {
  if (it.tipo === "trecho") {
    if (!(it.inicio_s >= 0) || !(it.fim_s - it.inicio_s >= 0.04)) return "intervalo inválido";
    if (!(it.velocidade >= 0.1 && it.velocidade <= 8)) return "velocidade fora de 0,1 a 8";
  } else if (it.tipo === "cartela") {
    if (!(it.duracao_s >= 0.1)) return "duração inválida";
  } else {
    if (!(it.instante_s >= 0) || !(it.duracao_s >= 0.1)) return "instante ou duração inválidos";
  }
  return null;
}
