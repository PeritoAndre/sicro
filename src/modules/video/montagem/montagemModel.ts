/** Regras puras da Montagem: criação de itens, duração e reordenação. */

import type {
  FormaDestaque,
  Montagem,
  MontagemDesfoque,
  MontagemDestaque,
  MontagemItem,
  MontagemNarracao,
  MontagemQuadro,
  MontagemTexto,
  MontagemTrecho,
  Regiao,
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
    narracoes: [],
    volume_original: 1,
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

/** Duração do trecho no tempo do original (referência dos quadros-chave). */
export const duracaoOriginal = (t: MontagemTrecho): number => Math.max(0, t.fim_s - t.inicio_s);

export const regiaoInteira = (): Regiao => ({ x: 0, y: 0, w: 1, h: 1 });

export const novoDestaque = (forma: FormaDestaque, cor = "#ff2d2d"): MontagemDestaque => ({
  id: uid(),
  forma,
  cor,
  quadros: [],
});

export const novoDesfoque = (): MontagemDesfoque => ({ id: uid(), quadros: [] });

export const novaNarracao = (arquivo_relativo: string, nome: string, inicio_s = 0): MontagemNarracao => ({
  id: uid(),
  arquivo_relativo,
  inicio_s,
  volume: 1,
  nome,
});

/** Insere (ou substitui, se já há quadro no mesmo `t`) mantendo a ordem por tempo. */
export function inserirQuadro(quadros: MontagemQuadro[], q: MontagemQuadro): MontagemQuadro[] {
  const r = quadros.filter((k) => Math.abs(k.t - q.t) > 1e-3);
  r.push(q);
  return r.sort((a, b) => a.t - b.t);
}

/** Posição no instante `t` (segundos do trecho): interpola; antes do primeiro quadro não há nada, depois fica parado. */
export function posicaoEm(quadros: MontagemQuadro[], t: number): MontagemQuadro | null {
  if (quadros.length === 0) return null;
  const q = quadros.slice().sort((a, b) => a.t - b.t);
  const primeiro = q[0] as MontagemQuadro;
  if (t < primeiro.t) return null;
  for (let i = 0; i < q.length - 1; i++) {
    const a = q[i] as MontagemQuadro;
    const b = q[i + 1] as MontagemQuadro;
    if (t < b.t) {
      const f = b.t - a.t < 1e-6 ? 1 : (t - a.t) / (b.t - a.t);
      const l = (u: number | undefined, v: number | undefined) => (u == null || v == null ? u ?? v : u + (v - u) * f);
      return { t, x: a.x + (b.x - a.x) * f, y: a.y + (b.y - a.y) * f, w: l(a.w, b.w), h: l(a.h, b.h) };
    }
  }
  return { ...(q[q.length - 1] as MontagemQuadro), t };
}

const entre01 = (v: number) => Number.isFinite(v) && v >= 0 && v <= 1;

export function validarRegiao(r: Regiao): string | null {
  if (![r.x, r.y, r.w, r.h].every(Number.isFinite)) return "região inválida";
  if (r.w < 0.1 || r.w > 1 || r.h <= 0) return "região de zoom: largura de 10% a 100%";
  if (!entre01(r.x) || !entre01(r.y) || r.x + r.w > 1.0001 || r.y + r.h > 1.0001) return "região de zoom fora da imagem";
  return null;
}

export function validarQuadros(q: MontagemQuadro[], dur: number, exigeTamanho: boolean, rotulo: string): string | null {
  if (q.length === 0) return `${rotulo} sem quadro-chave`;
  for (const k of q) {
    if (![k.t, k.x, k.y].every(Number.isFinite) || k.t < 0 || k.t > dur + 1e-6) return `${rotulo}: quadro fora do trecho`;
    if (!entre01(k.x) || !entre01(k.y)) return `${rotulo}: posição fora da imagem`;
    if (exigeTamanho && !(k.w != null && k.h != null && k.w > 0 && k.h > 0 && k.w <= 1 && k.h <= 1)) {
      return `${rotulo}: falta largura e altura`;
    }
  }
  return null;
}

export function validarNarracao(n: MontagemNarracao): string | null {
  if (!n.arquivo_relativo) return "narração sem arquivo";
  if (!(n.inicio_s >= 0) || !Number.isFinite(n.inicio_s)) return "início da narração inválido";
  if (!(n.volume >= 0 && n.volume <= 2)) return "volume da narração fora de 0 a 2";
  return null;
}

/** Problema que impede de renderizar, ou null. */
export function validarItem(it: MontagemItem): string | null {
  if (it.tipo === "trecho") {
    if (!(it.inicio_s >= 0) || !(it.fim_s - it.inicio_s >= 0.04)) return "intervalo inválido";
    if (!(it.velocidade >= 0.1 && it.velocidade <= 8)) return "velocidade fora de 0,1 a 8";
    const dur = duracaoOriginal(it);
    if (it.zoom) {
      const p = validarRegiao(it.zoom.inicio) ?? validarRegiao(it.zoom.fim);
      if (p) return p;
    }
    for (const d of it.destaques ?? []) {
      const p = validarQuadros(d.quadros, dur, d.forma === "retangulo", "destaque");
      if (p) return p;
    }
    for (const d of it.desfoques ?? []) {
      const p = validarQuadros(d.quadros, dur, true, "desfoque");
      if (p) return p;
    }
  } else if (it.tipo === "cartela") {
    if (!(it.duracao_s >= 0.1)) return "duração inválida";
  } else {
    if (!(it.instante_s >= 0) || !(it.duracao_s >= 0.1)) return "instante ou duração inválidos";
  }
  return null;
}

/** Problema da montagem como um todo (itens, narrações, volume), ou null. */
export function validarMontagem(m: Montagem): string | null {
  for (const it of m.itens) {
    const p = validarItem(it);
    if (p) return p;
  }
  const v = m.volume_original ?? 1;
  if (!(v >= 0 && v <= 2)) return "volume original fora de 0 a 2";
  for (const n of m.narracoes ?? []) {
    const p = validarNarracao(n);
    if (p) return p;
  }
  return null;
}
