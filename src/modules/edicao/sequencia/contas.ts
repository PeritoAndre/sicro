/**
 * Contas da Sequência (§8.2, §8.6): rótulo e duração do cartão, selos, problemas do item, item inválido tratado
 * como desconhecido (contratos (g) 13), destino do arrasto e as mudanças da lista (dividir, congelar aqui,
 * duplicar) feitas sobre o rascunho do `alterar`.
 */

import type {
  Analise,
  ClipeCongelar,
  ClipePrincipal,
  ClipeTrecho,
  Desconhecido,
  EstadoMidia,
  Id,
  LinhaResolvida,
  MidiaInfo,
  MidiaRef,
  Problema,
  ProjetoEdicao,
  Seg,
  TabelaQuadros,
} from "@domain/edicao";
import { clipeConhecido, efeitoConhecido, novoId, sobrepostoConhecido } from "../modelo/projeto";
import { ptsDaTabela, tempoDoQuadro } from "../modelo/tempo";
import { faixaDoClipe, tempoDaFronteira } from "../palco/faixa";

/** Duração padrão de quadro congelado, título e imagem novos (a do v1). */
export const DURACAO_PADRAO = 3;

export type RotuloTipo = "Trecho" | "Título" | "Quadro congelado" | "Imagem" | "Tela dividida";

export function rotuloDoTipo(c: ClipePrincipal): RotuloTipo {
  switch (c.tipo) {
    case "trecho":
      return "Trecho";
    case "titulo":
      return "Título";
    case "congelar":
      return "Quadro congelado";
    case "imagem":
      return "Imagem";
    case "tela":
      return "Tela dividida";
  }
}

const dois = (v: number) => String(v).padStart(2, "0");

/** "0:12" (m:ss) ou "1:02:03" (h:mm:ss), arredondado ao segundo. */
export function textoDuracao(s: Seg): string {
  const t = Math.max(0, Math.round(Number.isFinite(s) ? s : 0));
  const h = Math.floor(t / 3600);
  const m = Math.floor(t / 60) % 60;
  return h > 0 ? `${h}:${dois(m)}:${dois(t % 60)}` : `${m}:${dois(t % 60)}`;
}

/** "0:47,300" (m:ss,mmm) ou "1:02:03,250". */
export function textoDuracaoMs(s: Seg): string {
  const ms = Math.max(0, Math.round((Number.isFinite(s) ? s : 0) * 1000));
  const t = Math.floor(ms / 1000);
  const h = Math.floor(t / 3600);
  const m = Math.floor(t / 60) % 60;
  const base = h > 0 ? `${h}:${dois(m)}:${dois(t % 60)}` : `${m}:${dois(t % 60)}`;
  return `${base},${String(ms % 1000).padStart(3, "0")}`;
}

/** "0,25×", "2×", "1,5×". */
export function textoVelocidade(f: number): string {
  return `${String(Number(f.toFixed(3))).replace(".", ",")}×`;
}

export const textoClipes = (n: number) => `${n} ${n === 1 ? "clipe" : "clipes"}`;

/** "2 avisos" / "1 erro" (erro vence); null sem problema. */
export function textoProblemas(ps: readonly Problema[]): { texto: string; erro: boolean } | null {
  const erros = ps.filter((p) => p.nivel === "erro").length;
  if (erros > 0) return { texto: `${erros} ${erros === 1 ? "erro" : "erros"}`, erro: true };
  const n = ps.length;
  return n > 0 ? { texto: `${n} ${n === 1 ? "aviso" : "avisos"}`, erro: false } : null;
}

/** Problemas da análise e da abertura que citam o item. */
export function problemasDoItem(id: Id, analise: Analise | null, avisos: readonly Problema[]): Problema[] {
  return [...avisos, ...(analise?.problemas ?? [])].filter((p) => p.clipe === id);
}

/** Item citado com `item_invalido`: tratado como desconhecido (cartão apagado, sem editor). */
export function itemInvalido(id: Id | undefined, analise: Analise | null, avisos: readonly Problema[]): boolean {
  if (!id) return false;
  return [...avisos, ...(analise?.problemas ?? [])].some((p) => p.codigo === "item_invalido" && p.clipe === id && p.efeito === null);
}

/** Clipe que a tela trata como conhecido: tipo da lista, forma certa e não acusado de inválido. */
export function clipeNaTela(c: ClipePrincipal | Desconhecido, analise: Analise | null, avisos: readonly Problema[]): c is ClipePrincipal {
  return clipeConhecido(c) && !itemInvalido(c.id, analise, avisos);
}

export type Selo = "Taxa variável" | "Vídeo ausente" | "Danificado" | { velocidade: string };

export function selosDoClipe(c: ClipePrincipal, ref: MidiaRef | null, info: MidiaInfo | null, estado: EstadoMidia | null, problemas: readonly Problema[]): Selo[] {
  const s: Selo[] = [];
  if (estado?.estado === "ausente" || problemas.some((p) => p.codigo === "midia_ausente")) s.push("Vídeo ausente");
  if ((info?.erros ?? 0) > 0 || (estado?.erros ?? 0) > 0 || problemas.some((p) => p.codigo === "fonte_danificada")) s.push("Danificado");
  if (ref && (info?.vfr ?? ref.vfr)) s.push("Taxa variável");
  if ((c.tipo === "trecho" || c.tipo === "tela") && c.velocidade.tipo === "constante" && c.velocidade.fator !== 1) s.push({ velocidade: textoVelocidade(c.velocidade.fator) });
  return s;
}

/** Duração do clipe na linha resolvida (s); null se ele não está na linha. */
export function duracaoNaLinha(id: Id, linha: LinhaResolvida | null): Seg | null {
  const c = linha?.clipes.find((x) => x.id === id);
  if (!c || !linha) return null;
  return (c.n_q * linha.fps.den) / linha.fps.num;
}

export const duracaoDoPrograma = (linha: LinhaResolvida | null): Seg => (linha ? (linha.total_q * linha.fps.den) / linha.fps.num : 0);

/** Posição final de um item arrastado: quantos dos OUTROS cartões têm o centro acima do ponteiro. */
export function destinoDoArrasto(y: number, centros: readonly number[], de: number): number {
  let n = 0;
  centros.forEach((c, i) => {
    if (i !== de && c < y) n++;
  });
  return n;
}

/** Itens presos ao clipe (`pos.ref = "clipe"`): sobrepostos e clipes de áudio. */
export interface Preso {
  k: "livre" | "audio";
  id: Id;
  trilha: Id;
  tipo: "titulo" | "imagem" | "video" | "narracao" | "musica" | "efeitos";
  rotulo: string;
}

export function presosAoClipe(doc: ProjetoEdicao, id: Id): Preso[] {
  const r: Preso[] = [];
  for (const t of doc.sobreposicoes)
    for (const c of t.clipes) if (sobrepostoConhecido(c) && c.pos.ref === "clipe" && c.pos.clipe === id) r.push({ k: "livre", id: c.id, trilha: t.id, tipo: c.tipo, rotulo: c.rotulo });
  for (const t of doc.audios) for (const c of t.clipes) if (c.pos.ref === "clipe" && c.pos.clipe === id) r.push({ k: "audio", id: c.id, trilha: t.id, tipo: t.papel, rotulo: c.nome });
  return r;
}

// ---- mudanças sobre o rascunho ----

/** Cópia profunda com ids novos (clipe, efeitos, silêncios): duas cópias nunca dividem um id. */
export function comIdsNovos<C extends ClipePrincipal>(c: C): C {
  const x = JSON.parse(JSON.stringify(c)) as C;
  x.id = novoId();
  for (const e of x.efeitos) if (efeitoConhecido(e)) e.id = novoId();
  if (x.tipo === "trecho") for (const s of x.audio.silencios) s.id = novoId();
  return x;
}

const indiceDe = (d: ProjetoEdicao, id: Id) => d.principal.findIndex((c) => (c as { id?: Id }).id === id);

export function duplicarClipe(d: ProjetoEdicao, id: Id): Id | null {
  const i = indiceDe(d, id);
  const c = d.principal[i];
  if (!c || !clipeConhecido(c)) return null;
  const novo = comIdsNovos(c);
  novo.transicao_entrada = null;
  d.principal.splice(i + 1, 0, novo);
  return novo.id;
}

/**
 * Divide o trecho na fronteira do quadro `j` (o exibido no cursor): [j_in, j) e [j, j_out).
 * Devolve o id da segunda parte; null se `j` não fica DENTRO do trecho.
 */
export function dividirTrecho(d: ProjetoEdicao, id: Id, j: number, tab: TabelaQuadros): Id | null {
  const i = indiceDe(d, id);
  const c = d.principal[i];
  if (!c || !clipeConhecido(c) || c.tipo !== "trecho") return null;
  const [a, b] = faixaDoClipe(c, tab);
  if (j <= a || j >= b) return null;
  const f = tempoDaFronteira(tab, j);
  const segunda = comIdsNovos(c) as ClipeTrecho;
  segunda.inicio_s = f.t;
  segunda.inicio_pts = f.pts;
  segunda.transicao_entrada = null;
  c.fim_s = f.t;
  c.fim_pts = f.pts;
  d.principal.splice(i + 1, 0, segunda);
  return segunda.id;
}

/** Congelar aqui (§8.1): divide no quadro exibido `j` e põe o quadro congelado entre as partes (antes do trecho
 *  quando `j` é o primeiro). Devolve o id do congelado; null fora do trecho. */
export function congelarAqui(d: ProjetoEdicao, id: Id, j: number, tab: TabelaQuadros, duracao_s: Seg = DURACAO_PADRAO): Id | null {
  const i = indiceDe(d, id);
  const c = d.principal[i];
  if (!c || !clipeConhecido(c) || c.tipo !== "trecho") return null;
  const [a, b] = faixaDoClipe(c, tab);
  if (j < a || j >= b) return null;
  const cong: ClipeCongelar = {
    tipo: "congelar",
    id: novoId(),
    rotulo: "",
    desligado: false,
    transicao_entrada: null,
    efeitos: [],
    midia: c.midia,
    instante_s: tempoDoQuadro(tab, j),
    instante_pts: ptsDaTabela(tab)[j] as number,
    duracao_s,
  };
  if (j === a) {
    d.principal.splice(i, 0, cong);
    return cong.id;
  }
  dividirTrecho(d, id, j, tab);
  d.principal.splice(i + 1, 0, cong);
  return cong.id;
}
