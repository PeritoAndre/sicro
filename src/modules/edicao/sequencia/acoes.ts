/**
 * Ações da Sequência sobre a store (botões, cartões e atalhos do §8.5). Toda mudança do documento passa por
 * `alterar`; seleção e cursor andam juntos (desfazer restaura os dois).
 */

import type { ClipePrincipal, Id, MidiaRef, ProjetoEdicao } from "@domain/edicao";
import { useEdicaoStore } from "../store/edicaoStore";
import { acharMidia, clipeConhecido, garantirMidia, moverClipe, novoTrecho, removerClipe } from "../modelo/projeto";
import { exibido } from "../modelo/tempo";
import { meioDoQuadro, faixaDoClipe } from "../palco/faixa";
import { recursoDe } from "../palco/recursos";
import { congelarAqui, dividirTrecho, duplicarClipe } from "./contas";
import type { TrechoNovo } from "./NovoClipe";

const st = () => useEdicaoStore.getState();

/** Clipe da principal em foco (seleção, senão o do cursor). */
export function clipeEmFoco(): Id | null {
  const s = st();
  if (s.selecao.foco?.k === "clipe") return s.selecao.foco.id;
  return s.cursor.modo === "clipe" ? s.cursor.clipe : null;
}

/** Alvo de Remover, Duplicar e Mover pelos atalhos: o clipe em foco. Com um item preso (ou outro que não é da
 *  principal) em foco, nenhum: a tecla não pode cair no clipe que está por trás. */
function clipeAlvo(): Id | null {
  const f = st().selecao.foco;
  return f && f.k !== "clipe" ? null : clipeEmFoco();
}

const indice = (d: ProjetoEdicao, id: Id | null) => (id ? d.principal.findIndex((c) => (c as { id?: Id }).id === id) : -1);

/** Tempo de partida do cursor no clipe: meio do primeiro quadro (trecho) ou 0 (base local). */
function tempoInicial(d: ProjetoEdicao, c: ClipePrincipal): number {
  if (c.tipo !== "trecho") return 0;
  const tab = recursoDe(acharMidia(d, c.midia)?.sha256).tabela;
  if (!tab) return c.inicio_s;
  try {
    return meioDoQuadro(tab, faixaDoClipe(c, tab)[0]);
  } catch {
    return meioDoQuadro(tab, exibido(tab, c.inicio_s));
  }
}

export function selecionarClipe(id: Id | null): void {
  const s = st();
  if (!s.doc || !id) {
    s.selecionar({ itens: [], foco: null, efeito: null });
    return;
  }
  const c = s.doc.principal.find((x) => (x as { id?: Id }).id === id);
  const item = { k: "clipe" as const, id };
  s.selecionar({ itens: [item], foco: item, efeito: null });
  // já neste clipe: o cursor fica onde está (clicar no cartão não volta ao primeiro quadro)
  if (s.cursor.modo === "clipe" && s.cursor.clipe === id) return;
  if (c && clipeConhecido(c)) s.posicionar({ modo: "clipe", clipe: id, t: tempoInicial(s.doc, c) });
}

/** PgUp / PgDn: clipe anterior / seguinte (conhecidos). */
export function vizinho(delta: -1 | 1): void {
  const d = st().doc;
  if (!d) return;
  const ids = d.principal.filter(clipeConhecido).map((c) => c.id);
  if (ids.length === 0) return;
  const i = ids.indexOf(clipeEmFoco() ?? "");
  const j = i < 0 ? (delta > 0 ? 0 : ids.length - 1) : Math.min(Math.max(i + delta, 0), ids.length - 1);
  selecionarClipe(ids[j] ?? null);
}

/** Alt+PgUp / Alt+PgDn: mover o clipe em foco. */
export function mover(delta: -1 | 1): void {
  const s = st();
  const id = clipeAlvo();
  if (!s.doc || !id) return;
  const i = indice(s.doc, id);
  const j = i + delta;
  if (i < 0 || j < 0 || j >= s.doc.principal.length) return;
  s.alterar(delta < 0 ? "Mover o clipe para antes" : "Mover o clipe para depois", (d) => moverClipe(d, i, j));
}

/** Arrasto na lista: índices da lista inteira (desconhecidos inclusive). */
export function moverPara(de: number, para: number): void {
  if (de === para) return;
  st().alterar("Mover", (d) => moverClipe(d, de, para));
}

/** Tira o item preso (sobreposto ou clipe de áudio) com esse id. */
function removerPreso(d: ProjetoEdicao, id: Id): boolean {
  if (removerClipe(d, id)) return true;
  for (const t of d.audios) {
    const j = t.clipes.findIndex((c) => c.id === id);
    if (j >= 0) {
      t.clipes.splice(j, 1);
      return true;
    }
  }
  return false;
}

/** Remove o clipe da principal e o que está preso a ele (sem o clipe, o preso ficaria sem âncora e fora da tela). */
export function removerComPresos(d: ProjetoEdicao, id: Id): void {
  if (indice(d, id) < 0) return;
  removerClipe(d, id);
  const solto = (pos: { ref: string; clipe?: Id }) => !(pos.ref === "clipe" && pos.clipe === id);
  for (const t of d.sobreposicoes) t.clipes = t.clipes.filter((c) => !("pos" in c) || solto(c.pos as { ref: string; clipe?: Id }));
  for (const t of d.audios) t.clipes = t.clipes.filter((c) => solto(c.pos));
  d.marcadores = d.marcadores.filter((m) => solto(m.pos));
}

/** Sem argumento (Del): o que está em foco (o item preso selecionado, senão o clipe). `null` = nada. */
export function remover(id?: Id | null): void {
  const s = st();
  if (!s.doc || id === null) return;
  const foco = s.selecao.foco;
  if (id === undefined && foco && (foco.k === "livre" || foco.k === "audio")) {
    const dono = s.cursor.modo === "clipe" ? s.cursor.clipe : null;
    s.alterar("Remover", (d) => {
      removerPreso(d, foco.id);
    });
    const item = dono ? { k: "clipe" as const, id: dono } : null;
    st().selecionar({ itens: item ? [item] : [], foco: item, efeito: null });
    return;
  }
  const alvo = id ?? clipeAlvo();
  if (!alvo) return;
  const lista = s.doc.principal;
  const i = indice(s.doc, alvo);
  // seleção vai para o vizinho
  const prox = (lista[i + 1] ?? lista[i - 1]) as { id?: Id } | undefined;
  s.alterar("Remover", (d) => {
    removerComPresos(d, alvo);
  });
  selecionarClipe(prox?.id ?? null);
}

export function duplicar(id: Id | null = clipeAlvo()): void {
  const s = st();
  if (!s.doc || !id) return;
  let novo: Id | null = null;
  s.alterar("Duplicar", (d) => {
    novo = duplicarClipe(d, id);
  });
  if (novo) selecionarClipe(novo);
}

/** Quadro exibido no cursor dentro do trecho em foco, com a tabela dele. */
function noCursor(): { id: Id; j: number; tab: NonNullable<ReturnType<typeof recursoDe>["tabela"]> } | null {
  const s = st();
  if (!s.doc || s.cursor.modo !== "clipe") return null;
  const id = s.cursor.clipe;
  const c = s.doc.principal.find((x) => (x as { id?: Id }).id === id);
  if (!c || !clipeConhecido(c) || c.tipo !== "trecho") return null;
  const tab = recursoDe(acharMidia(s.doc, c.midia)?.sha256).tabela;
  if (!tab) return null;
  return { id, j: exibido(tab, s.cursor.t), tab };
}

/** "Congelar aqui" é possível agora? (cursor dentro de um trecho com tabela) */
export function podeCongelar(): boolean {
  const s = st();
  const n = noCursor();
  if (!n || !s.doc) return false;
  const c = s.doc.principal.find((x) => (x as { id?: Id }).id === n.id) as ClipePrincipal;
  if (c.tipo !== "trecho") return false;
  const [a, b] = faixaDoClipe(c, n.tab);
  return n.j >= a && n.j < b;
}

export function congelar(): void {
  const n = noCursor();
  if (!n) return;
  let id: Id | null = null;
  st().alterar("Congelar aqui", (d) => {
    id = congelarAqui(d, n.id, n.j, n.tab);
  });
  if (id) selecionarClipe(id);
}

/** Ctrl+B: divide o trecho no quadro exibido. */
export function dividir(): void {
  const n = noCursor();
  if (!n) return;
  let id: Id | null = null;
  st().alterar("Dividir no cursor", (d) => {
    id = dividirTrecho(d, n.id, n.j, n.tab);
  });
  if (id) selecionarClipe(id);
}

/** Insere depois do clipe em foco (ou no fim) e seleciona o primeiro inserido. */
export function inserir(rotulo: string, novos: (d: ProjetoEdicao) => ClipePrincipal[]): void {
  const s = st();
  if (!s.doc) return;
  const foco = clipeEmFoco();
  let primeiro: Id | null = null;
  s.alterar(rotulo, (d) => {
    const lista = novos(d);
    const i = indice(d, foco);
    d.principal.splice(i < 0 ? d.principal.length : i + 1, 0, ...lista);
    primeiro = lista[0]?.id ?? null;
  });
  if (primeiro) selecionarClipe(primeiro);
}

export function inserirTrechos(itens: TrechoNovo[]): void {
  if (itens.length === 0) return;
  inserir("Trecho", (d) =>
    itens.map((it) => {
      const id = garantirMidia(d, it.midia as MidiaRef);
      const c = novoTrecho(id, it.inicio_s, it.fim_s);
      c.inicio_pts = it.inicio_pts;
      c.fim_pts = it.fim_pts;
      return c;
    }),
  );
}
