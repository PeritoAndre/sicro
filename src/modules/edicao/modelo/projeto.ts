/**
 * Projeto da Edição: projeto novo, ids, itens conhecidos × de versão futura, busca de clipe e efeito
 * e as mudanças que a store aplica dentro de `alterar` (sobre o rascunho do immer).
 */

import type {
  AudioClipe,
  Caixa,
  Chave,
  ClipePrincipal,
  ClipeSobreposto,
  ClipeTrecho,
  Desconhecido,
  Efeito,
  EfEstabilizar,
  Exportacao,
  Id,
  MidiaRef,
  Mixagem,
  Mudanca,
  Prender,
  ProjetoEdicao,
  Racional,
  Rastreio,
  Saida,
  Seg,
  TrilhaLegendas,
} from "@domain/edicao";
import { ESQUEMA_EDICAO, ESQUEMA_MENOR } from "@domain/edicao";

// ---- itens conhecidos (o resto é de versão futura: preservado e mostrado apagado) ----

/** Todo `tipo` de `X` aparece na lista (erro de tipo se faltar um). */
type Todos<X extends string, L extends readonly string[]> = [Exclude<X, L[number]>] extends [never] ? L : never;

const EFEITOS = [
  "estabilizar", "lente", "desentrelacar", "media_temporal", "ajustes", "nitidez", "girar", "enquadramento",
  "ocultar", "holofote", "destaque", "lupa", "regua", "velocidade_exibida", "texto", "carimbo", "imagem",
] as const satisfies readonly Efeito["tipo"][];
const CLIPES = ["trecho", "congelar", "imagem", "titulo", "tela"] as const satisfies readonly ClipePrincipal["tipo"][];
const SOBREPOSTOS = ["video", "imagem", "titulo"] as const satisfies readonly ClipeSobreposto["tipo"][];

export const TIPOS_EFEITO: Todos<Efeito["tipo"], typeof EFEITOS> = EFEITOS;
export const TIPOS_CLIPE: Todos<ClipePrincipal["tipo"], typeof CLIPES> = CLIPES;
export const TIPOS_SOBREPOSTO: Todos<ClipeSobreposto["tipo"], typeof SOBREPOSTOS> = SOBREPOSTOS;

const conjEfeito = new Set<string>(TIPOS_EFEITO);
const conjClipe = new Set<string>(TIPOS_CLIPE);
const conjSobreposto = new Set<string>(TIPOS_SOBREPOSTO);

type Obj = Record<string, unknown>;
const ehObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);
const ehLista = Array.isArray;

// Conhecido = `tipo` da lista E a forma mínima de que as contas do front dependem. Item de `tipo` conhecido com a
// forma errada é o `item_invalido` do Rust (lido como JSON cru): preservado e mostrado apagado, como o de versão futura.

export function efeitoConhecido(e: Efeito | Desconhecido): e is Efeito {
  const o: unknown = e;
  if (!ehObj(o) || !conjEfeito.has(o.tipo as string)) return false;
  switch (o.tipo as Efeito["tipo"]) {
    case "enquadramento":
      return ehObj(o.regiao) || ehLista(o.regiao);
    case "ocultar":
    case "holofote":
      return ehObj(o.alvo);
    case "destaque":
      return ehObj(o.traco);
    case "texto":
    case "carimbo":
      return ehObj(o.estilo) && ehObj(o.lugar);
    case "imagem":
      return ehObj(o.lugar);
    case "estabilizar":
      return ehObj(o.analise);
    default:
      return true;
  }
}
export function clipeConhecido(c: ClipePrincipal | Desconhecido): c is ClipePrincipal {
  const o: unknown = c;
  if (!ehObj(o) || !conjClipe.has(o.tipo as string) || !ehLista(o.efeitos)) return false;
  switch (o.tipo as ClipePrincipal["tipo"]) {
    case "trecho":
      return ehObj(o.velocidade) && ehObj(o.audio);
    case "tela":
      return ehObj(o.velocidade) && ehObj(o.audio) && ehLista(o.celulas) && o.celulas.every((x) => ehObj(x) && ehLista(x.efeitos));
    case "titulo":
      return ehObj(o.titulo);
    default:
      return true;
  }
}
export function sobrepostoConhecido(c: ClipeSobreposto | Desconhecido): c is ClipeSobreposto {
  const o: unknown = c;
  return ehObj(o) && conjSobreposto.has(o.tipo as string) && ehLista(o.efeitos) && ehObj(o.transformacao) && ehObj(o.pos);
}

// ---- projeto novo ----

export function novoId(): Id {
  return globalThis.crypto.randomUUID();
}

/** Título de projeto criado sem nome (o da lâmina da Central). */
export const TITULO_NOVO = "Edição";

export const exportacaoPadrao = (): Exportacao => ({
  predefinicao: "juri",
  codificador: "auto",
  legendas: "nenhuma",
  pacote: null,
  pdf: "a3",
  revisado_por: "",
  personalizada: null,
});

/** `auto`: tamanho e taxa seguem as fontes (`Analise.saida_auto`) até o perito mexer em "Saída". */
export const saidaPadrao = (): Saida => ({
  largura: 1920,
  altura: 1080,
  fps: { num: 30, den: 1 },
  auto: true,
  cor_fundo: "#000000",
  audio: true,
  selos: { velocidade: true, ampliacao: true, realce: true },
  exportacao: exportacaoPadrao(),
});

/** Neutra; `normalizar` nulo na Fase 0 (a normalização é do P1.5). Igual ao `Default` do Rust. */
export const mixagemPadrao = (): Mixagem => ({
  volume_original: 1,
  abaixar: { db: -12, ataque_s: 0.3, soltura_s: 0.5 },
  normalizar: null,
  limitador: false,
});

/** Igual ao `Default` de `TrilhaLegendas` do Rust (M7). */
export const legendasPadrao = (): TrilhaLegendas => ({
  estilo: {
    fonte: "sans",
    negrito: false,
    italico: false,
    tamanho: 0.052,
    cor: "#ffffff",
    alinhamento: "centro",
    caixa: { cor: "#0000008c", margem: 0.013, raio: 0 },
    contorno: null,
  },
  lugar: { x: 0.5, y: 0.94, h: "centro", v: "base" },
  linhas_max: 2,
  origem: null,
  itens: [],
});

export const audioPadrao = (): AudioClipe => ({ volume: 1, mudo: false, manter_tom: true, silencios: [] });

export function novoProjeto(titulo: string = TITULO_NOVO, agora: string = new Date().toISOString()): ProjetoEdicao {
  return {
    esquema: ESQUEMA_EDICAO,
    esquema_menor: ESQUEMA_MENOR,
    id: novoId(),
    titulo,
    created_at: agora,
    updated_at: agora,
    gravado_por: "",
    saida: saidaPadrao(),
    midias: [],
    principal: [],
    fecho: null,
    sobreposicoes: [],
    audios: [],
    legendas: legendasPadrao(),
    rastreios: [],
    marcadores: [],
    sincronias: [],
    mixagem: mixagemPadrao(),
  };
}

// ---- arquivo sem os campos que o Rust lê com `#[serde(default)]` ----

/** `o` com os padrões que faltam; o MESMO objeto se não falta nenhum. */
function comPadroes(o: Obj, padroes: Record<string, () => unknown>): Obj {
  let r = o;
  for (const [k, f] of Object.entries(padroes)) {
    if (o[k] !== undefined) continue;
    if (r === o) r = { ...o };
    r[k] = f();
  }
  return r;
}
const trocar = (o: Obj, k: string, v: unknown): Obj => (v === o[k] ? o : { ...o, [k]: v });
/** `f` em cada item que é objeto; a MESMA lista se nenhum mudou (e o que não é lista volta como veio). */
function cada(v: unknown, f: (o: Obj) => Obj): unknown {
  if (!ehLista(v)) return v;
  let mudou = false;
  const r = v.map((x: unknown) => {
    if (!ehObj(x)) return x;
    const y = f(x);
    if (y !== x) mudou = true;
    return y;
  });
  return mudou ? r : v;
}
const em = (o: Obj, k: string, f: (v: Obj) => Obj): Obj => (ehObj(o[k]) ? trocar(o, k, f(o[k])) : o);
const emCada = (o: Obj, k: string, f: (v: Obj) => Obj): Obj => trocar(o, k, cada(o[k], f));

// os padrões do modelo.rs, item a item (efeito sem `ligado` vale LIGADO); item de versão futura não é tocado
const PTS = { inicio_pts: () => null, fim_pts: () => null };
const efeitoCompleto = (e: Obj): Obj => (conjEfeito.has(e.tipo as string) ? comPadroes(e, { ligado: () => true, nome: () => "", ativo: () => null }) : e);
const audioCompleto = (a: Obj): Obj => comPadroes(a, { silencios: () => [] });
const tituloCompleto = (t: Obj): Obj => comPadroes(t, { blocos: () => [] });
function clipeCompleto(c: Obj): Obj {
  if (!conjClipe.has(c.tipo as string)) return c;
  let r = comPadroes(c, { rotulo: () => "", desligado: () => false, transicao_entrada: () => null, efeitos: () => [] });
  if (c.tipo === "trecho") r = comPadroes(r, { ...PTS, audio: audioPadrao });
  if (c.tipo === "tela") r = emCada(comPadroes(r, PTS), "celulas", (cel) => emCada(cel, "efeitos", efeitoCompleto));
  if (c.tipo === "congelar") r = comPadroes(r, { instante_pts: () => null });
  return em(em(emCada(r, "efeitos", efeitoCompleto), "audio", audioCompleto), "titulo", tituloCompleto);
}
function sobrepostoCompleto(c: Obj): Obj {
  if (!conjSobreposto.has(c.tipo as string)) return c;
  return em(em(emCada(comPadroes(c, { efeitos: () => [] }), "efeitos", efeitoCompleto), "audio", audioCompleto), "titulo", tituloCompleto);
}

/** Arquivo antigo (ou escrito à mão) sem os campos que o Rust lê com `#[serde(default)]`: o `edicao_abrir` devolve o
 *  JSON como está, e o front conta com todos. Põe os mesmos padrões do Rust: nos campos do projeto, em `saida.auto`
 *  (ausente = true), nas opções de `exportacao` e dentro dos itens (clipe, efeito, trilha, rastreio, sincronia,
 *  mídia). Documento completo volta o MESMO objeto; a parte que não mudou é o mesmo objeto de `p`. */
export function completar(p: ProjetoEdicao): ProjetoEdicao {
  const padroes: { [K in keyof ProjetoEdicao]?: () => ProjetoEdicao[K] } = {
    esquema_menor: () => 0,
    gravado_por: () => "",
    midias: () => [],
    principal: () => [],
    fecho: () => null,
    sobreposicoes: () => [],
    audios: () => [],
    legendas: legendasPadrao,
    rastreios: () => [],
    marcadores: () => [],
    sincronias: () => [],
    mixagem: mixagemPadrao,
  };
  let r = comPadroes(p as unknown as Obj, padroes);
  const s = p.saida as (Partial<Saida> & { exportacao?: Partial<Exportacao> }) | undefined;
  const x = s?.exportacao;
  if (s && (s.auto === undefined || (x && (x.pacote === undefined || x.personalizada === undefined)))) {
    r = trocar(r, "saida", { ...s, auto: s.auto ?? true, ...(x ? { exportacao: { ...x, pacote: x.pacote ?? null, personalizada: x.personalizada ?? null } } : {}) });
  }
  r = emCada(r, "principal", clipeCompleto);
  r = emCada(r, "sobreposicoes", (t) => emCada(comPadroes(t, { clipes: () => [] }), "clipes", sobrepostoCompleto));
  r = emCada(r, "audios", (t) => comPadroes(t, { clipes: () => [] }));
  r = em(r, "legendas", (l) => comPadroes(l, { itens: () => [] }));
  r = emCada(r, "rastreios", (x) => comPadroes(x, { ancoras: () => [], ausencias: () => [] }));
  r = emCada(r, "sincronias", (x) => comPadroes(x, { conferencias: () => [] }));
  r = emCada(r, "midias", (m) => emCada(comPadroes(m, { ocultacoes: () => [] }), "ocultacoes", efeitoCompleto));
  return r as unknown as ProjetoEdicao;
}

/** Para gravar: do que `completar` pôs, só vai o que o perito mexeu; campo do projeto intocado volta como chegou
 *  (D2) e o que nem existia não é criado. `completado` = o que `completar(original)` devolveu; o immer mantém a
 *  referência do que não mudou. */
export function comoChegou(doc: ProjetoEdicao, completado: ProjetoEdicao, original: ProjetoEdicao): ProjetoEdicao {
  if (completado === original) return doc;
  const [d, c, o] = [doc, completado, original] as unknown as [Obj, Obj, Obj];
  const saida = { ...d };
  for (const k of Object.keys(c)) {
    if (d[k] !== c[k]) continue;
    if (k in o) saida[k] = o[k];
    else delete saida[k];
  }
  return saida as unknown as ProjetoEdicao;
}

/** Trecho de `midia` em [inicio_s, fim_s); os `*_pts` ficam para quem prende (Rust). */
export function novoTrecho(midia: Id, inicio_s: Seg, fim_s: Seg, rotulo = ""): ClipeTrecho {
  return {
    tipo: "trecho",
    id: novoId(),
    rotulo,
    desligado: false,
    transicao_entrada: null,
    efeitos: [],
    midia,
    inicio_s,
    fim_s,
    inicio_pts: null,
    fim_pts: null,
    velocidade: { tipo: "constante", fator: 1 },
    audio: audioPadrao(),
  };
}

// ---- busca ----

export type ClipeAchado =
  | { onde: "principal"; indice: number; clipe: ClipePrincipal }
  | { onde: "sobreposicao"; trilha: Id; indice: number; clipe: ClipeSobreposto };

/** Clipe conhecido com esse id, na principal ou numa trilha de sobreposição. */
export function acharClipe(p: ProjetoEdicao, id: Id): ClipeAchado | null {
  const i = p.principal.findIndex((c) => ehObj(c) && c.id === id);
  const c = p.principal[i];
  if (c && clipeConhecido(c)) return { onde: "principal", indice: i, clipe: c };
  for (const t of p.sobreposicoes) {
    const j = t.clipes.findIndex((c) => ehObj(c) && c.id === id);
    const s = t.clipes[j];
    if (s && sobrepostoConhecido(s)) return { onde: "sobreposicao", trilha: t.id, indice: j, clipe: s };
  }
  return null;
}

/** Lista que contém o efeito e a posição dele: efeitos de clipe, de célula, de sobreposto, ou ocultações da mídia. */
export interface EfeitoAchado {
  lista: (Efeito | Desconhecido)[];
  indice: number;
  efeito: Efeito;
  /** Clipe dono; null = ocultação da mídia. */
  clipe: Id | null;
  /** Mídia dona (ocultação da mídia). */
  midia: Id | null;
}

export function acharEfeito(p: ProjetoEdicao, efeito: Id): EfeitoAchado | null {
  const procurar = (lista: (Efeito | Desconhecido)[], clipe: Id | null, midia: Id | null): EfeitoAchado | null => {
    const i = lista.findIndex((e) => ehObj(e) && e.id === efeito);
    const e = lista[i];
    return e && efeitoConhecido(e) ? { lista, indice: i, efeito: e, clipe, midia } : null;
  };
  for (const c of p.principal) {
    if (!clipeConhecido(c)) continue;
    const r = procurar(c.efeitos, c.id, null);
    if (r) return r;
    if (c.tipo === "tela") for (const cel of c.celulas) {
      const rc = procurar(cel.efeitos, c.id, null);
      if (rc) return rc;
    }
  }
  for (const t of p.sobreposicoes) for (const c of t.clipes) {
    if (!sobrepostoConhecido(c)) continue;
    const r = procurar(c.efeitos, c.id, null);
    if (r) return r;
  }
  for (const m of p.midias) {
    const r = procurar(m.ocultacoes, null, m.id);
    if (r) return r;
  }
  return null;
}

export function acharMidia(p: ProjetoEdicao, id: Id): MidiaRef | null {
  return p.midias.find((m) => m.id === id) ?? null;
}

export function acharRastreio(p: ProjetoEdicao, id: Id): Rastreio | null {
  return p.rastreios.find((r) => r.id === id) ?? null;
}

// ---- chaves: o front nunca grava lista vazia ----

/** Põe a chave em ordem de `t`; substitui a do mesmo quadro (mesmo pts) ou do mesmo instante. */
export function porChave<T>(ks: readonly Chave<T>[], k: Chave<T>): Chave<T>[] {
  const mesma = (o: Chave<T>) => (o.pts != null && k.pts != null ? o.pts === k.pts : o.t === k.t);
  const resto = ks.filter((o) => !mesma(o));
  const i = resto.findIndex((o) => o.t > k.t);
  return i < 0 ? [...resto, k] : [...resto.slice(0, i), k, ...resto.slice(i)];
}

/** Tira a chave `i`; tirar a última vira valor fixo (o dela). */
export function tirarChave<T>(ks: readonly Chave<T>[], i: number): Chave<T>[] | T {
  if (ks.length === 1) return (ks[0] as Chave<T>).v;
  return ks.filter((_, j) => j !== i);
}

// ---- mudanças sobre o rascunho (dentro de `alterar`) ----

/** Mudança vinda de uma alça do palco. Devolve false quando o alvo não existe. */
export function aplicarMudanca(d: ProjetoEdicao, m: Mudanca): boolean {
  if ("efeito" in m) {
    const a = acharEfeito(d, m.efeito.id);
    if (!a) return false;
    a.lista[a.indice] = m.efeito;
    return true;
  }
  if ("rastreio" in m) {
    const r = acharRastreio(d, m.rastreio);
    if (!r) return false;
    r.ancoras = porChave<Caixa>(r.ancoras, m.ancora);
    return true;
  }
  const c = acharClipe(d, m.clipe);
  if (!c) return false;
  Object.assign(c.clipe, m.campos);
  return true;
}

export function removerClipe(d: ProjetoEdicao, id: Id): boolean {
  const i = d.principal.findIndex((c) => ehObj(c) && c.id === id);
  if (i >= 0) {
    d.principal.splice(i, 1);
    return true;
  }
  for (const t of d.sobreposicoes) {
    const j = t.clipes.findIndex((c) => ehObj(c) && c.id === id);
    if (j >= 0) {
      t.clipes.splice(j, 1);
      return true;
    }
  }
  return false;
}

/** Move na principal (índices da lista inteira, desconhecidos inclusive). */
export function moverClipe(d: ProjetoEdicao, de: number, para: number): void {
  const n = d.principal.length;
  if (de < 0 || de >= n || para < 0 || para >= n || de === para) return;
  const [c] = d.principal.splice(de, 1);
  if (c) d.principal.splice(para, 0, c);
}

/** Mídia do caso já no projeto (mesmo SHA-256), ou a nova acrescentada. Devolve o id local. */
export function garantirMidia(d: ProjetoEdicao, m: MidiaRef): Id {
  const ja = d.midias.find((x) => x.sha256 === m.sha256 && x.origem === m.origem);
  if (ja) return ja.id;
  d.midias.push(m);
  return m.id;
}

/** Grava um tempo preso a quadro (`Analise.prender`). Devolve true se mudou algo. */
export function aplicarPrender(d: ProjetoEdicao, pr: Prender): boolean {
  const o = pr.onde;
  const por = <K extends string>(alvo: Record<K, unknown>, campoT: K, campoPts: K, pts: number | null): boolean => {
    if (alvo[campoT] === pr.t && (alvo[campoPts] ?? null) === pts) return false;
    alvo[campoT] = pr.t;
    alvo[campoPts] = pts;
    return true;
  };
  if (o.tipo === "ancora") {
    const k = acharRastreio(d, o.rastreio)?.ancoras[o.indice];
    if (!k) return false;
    // chave sem `pts` = nulo (M2)
    if (k.t === pr.t && (k.pts ?? null) === pr.pts) return false;
    k.t = pr.t;
    k.pts = pr.pts;
    return true;
  }
  const c = acharClipe(d, o.clipe)?.clipe;
  if (!c) return false;
  switch (o.tipo) {
    case "inicio":
      if (c.tipo !== "trecho" && c.tipo !== "tela" && c.tipo !== "video") return false;
      return por(c as unknown as Record<string, unknown>, "inicio_s", "inicio_pts", pr.pts);
    case "fim":
      if (c.tipo !== "trecho" && c.tipo !== "tela") return false;
      return por(c as unknown as Record<string, unknown>, "fim_s", "fim_pts", pr.pts);
    case "instante":
      if (c.tipo !== "congelar") return false;
      return por(c as unknown as Record<string, unknown>, "instante_s", "instante_pts", pr.pts);
    case "analise_de":
    case "analise_ate": {
      const e = c.efeitos.find((x) => efeitoConhecido(x) && x.id === o.efeito);
      if (!e || e.tipo !== "estabilizar" || pr.pts == null) return false;
      const a = (e as EfEstabilizar).analise as unknown as Record<string, unknown>;
      return o.tipo === "analise_de" ? por(a, "de_s", "de_pts", pr.pts) : por(a, "ate_s", "ate_pts", pr.pts);
    }
  }
  return false;
}

/** Saída automática (`Analise.saida_auto`), só enquanto `saida.auto`. Devolve true se mudou. */
export function aplicarSaidaAuto(d: ProjetoEdicao, sa: [number, number, Racional] | null): boolean {
  if (!sa || !d.saida.auto) return false;
  const [w, h, fps] = sa;
  const s = d.saida;
  if (s.largura === w && s.altura === h && s.fps.num === fps.num && s.fps.den === fps.den) return false;
  s.largura = w;
  s.altura = h;
  s.fps = { num: fps.num, den: fps.den };
  return true;
}

/** Caminhos de `Animavel` com lista de chaves vazia (inválido: contratos (f) 7). Só os campos animáveis do modelo. */
export function animaveisVazios(p: ProjetoEdicao): string[] {
  const vazios: string[] = [];
  const ver = (v: unknown, caminho: string) => {
    if (Array.isArray(v) && v.length === 0) vazios.push(caminho);
  };
  const efeitos = (lista: (Efeito | Desconhecido)[], caminho: string) =>
    lista.forEach((e, i) => efeitoConhecido(e) && e.tipo === "enquadramento" && ver(e.regiao, `${caminho}.efeitos[${i}].regiao`));
  p.principal.forEach((c, i) => {
    if (!clipeConhecido(c)) return;
    const k = `principal[${i}]`;
    efeitos(c.efeitos, k);
    if (c.tipo === "trecho" || c.tipo === "tela") ver(c.audio.volume, `${k}.audio.volume`);
    if (c.tipo === "tela") c.celulas.forEach((cel, j) => efeitos(cel.efeitos, `${k}.celulas[${j}]`));
  });
  p.sobreposicoes.forEach((t, i) =>
    t.clipes.forEach((c, j) => {
      if (!sobrepostoConhecido(c)) return;
      const k = `sobreposicoes[${i}].clipes[${j}]`;
      efeitos(c.efeitos, k);
      for (const campo of ["x", "y", "escala", "rotacao", "opacidade"] as const) ver(c.transformacao[campo], `${k}.transformacao.${campo}`);
      if (c.tipo === "video" && c.audio) ver(c.audio.volume, `${k}.audio.volume`);
    }),
  );
  p.audios.forEach((t, i) => t.clipes.forEach((c, j) => ver(c.volume, `audios[${i}].clipes[${j}].volume`)));
  return vazios;
}
