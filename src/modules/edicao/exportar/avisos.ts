/**
 * Regras do diálogo Exportar (puras, testáveis): erros bloqueiam; avisos e avisos periciais contam no botão;
 * aviso pericial pede "Ciente" (a ciência vai por código para `edicao_enfileirar`). E o salto até o item.
 */

import { create } from "zustand";
import type { ClipeCongelar, ClipeTrecho, ItemSel, Problema, ProjetoEdicao } from "@domain/edicao";
import { useEdicaoStore } from "../store/edicaoStore";
import { acharEfeito, clipeConhecido } from "../modelo/projeto";
import { useInspetorUi } from "../inspetor/ui";

export interface Classificados {
  erros: Problema[];
  avisos: Problema[];
  /** Agrupados por código: uma ciência por código. */
  periciais: { codigo: string; mensagem: string; itens: Problema[] }[];
}

export function classificar(ps: readonly Problema[]): Classificados {
  const r: Classificados = { erros: [], avisos: [], periciais: [] };
  for (const p of ps) {
    if (p.nivel === "erro") r.erros.push(p);
    else if (p.nivel === "aviso") r.avisos.push(p);
    else {
      const g = r.periciais.find((x) => x.codigo === p.codigo);
      if (g) g.itens.push(p);
      else r.periciais.push({ codigo: p.codigo, mensagem: p.mensagem, itens: [p] });
    }
  }
  return r;
}

/** Avisos e avisos periciais (cada problema conta). */
export const contarAvisos = (c: Classificados) => c.avisos.length + c.periciais.reduce((n, g) => n + g.itens.length, 0);

/** "Exportar" / "Exportar com 1 aviso" / "Exportar com 3 avisos". */
export const rotuloExportar = (n: number) => (n === 0 ? "Exportar" : `Exportar com ${n} ${n === 1 ? "aviso" : "avisos"}`);

export const faltaCiencia = (c: Classificados, ciencias: ReadonlySet<string>) => c.periciais.filter((g) => !ciencias.has(g.codigo)).map((g) => g.codigo);

export const podeExportar = (c: Classificados, ciencias: ReadonlySet<string>) => c.erros.length === 0 && faltaCiencia(c, ciencias).length === 0;

// ---- o diálogo e o painel da fila: abertos de qualquer lugar da tela (botões do topo, Ctrl+E) ----

interface ExportarUi {
  dialogo: boolean;
  fila: boolean;
  abrirExportar(): void;
  fecharExportar(): void;
  mostrarFila(v: boolean): void;
}

export const useExportarUi = create<ExportarUi>((set) => ({
  dialogo: false,
  fila: false,
  abrirExportar: () => set({ dialogo: true, fila: false }),
  fecharExportar: () => set({ dialogo: false }),
  mostrarFila: (fila) => set({ fila }),
}));

/** "Exportar…" e Ctrl+E (edicao.exportar). */
export const abrirExportar = () => useExportarUi.getState().abrirExportar();

// ---- salto para o item do problema ----

/** Item da linha do tempo citado pelo problema (R7: `clipe` = id do item; `trilha` = a dele). */
export function itemDoProblema(doc: ProjetoEdicao, p: Problema): { sel: ItemSel; aba: "clipe" | "audio" | "legendas" | "saida" } | null {
  if (p.codigo === "saida" || p.campo?.startsWith("saida")) return null;
  let id = p.clipe;
  const ef = p.efeito ? acharEfeito(doc, p.efeito) : null;
  if (!id) id = ef?.clipe ?? null;
  // ocultação da mídia: o trecho que a usa (de preferência um cuja faixa o `ativo` cruza, onde ela aparece aberta)
  const midia = p.midia ?? ef?.midia ?? null;
  if (!id && midia) {
    const usam = doc.principal.filter((x) => clipeConhecido(x) && (x.tipo === "trecho" || x.tipo === "congelar") && x.midia === midia) as (ClipeTrecho | ClipeCongelar)[];
    const at = ef?.midia ? ef.efeito.ativo : null;
    const cruza = (c: ClipeTrecho | ClipeCongelar) => !at || c.tipo !== "trecho" || ((at.de ?? -Infinity) < c.fim_s && (at.ate ?? Infinity) > c.inicio_s);
    id = (usam.find((c) => c.tipo === "trecho" && cruza(c)) ?? usam[0])?.id ?? null;
  }
  if (!id) return null;
  const tem = (x: unknown) => typeof x === "object" && x !== null && (x as { id?: unknown }).id === id;
  if (doc.principal.some(tem)) return { sel: { k: "clipe", id }, aba: "clipe" };
  for (const t of doc.sobreposicoes) if (t.clipes.some(tem)) return { sel: { k: "livre", id, trilha: t.id }, aba: "clipe" };
  for (const t of doc.audios) if (t.clipes.some(tem)) return { sel: { k: "audio", id, trilha: t.id }, aba: "audio" };
  if (doc.legendas.itens.some(tem)) return { sel: { k: "legenda", id }, aba: "legendas" };
  if (doc.marcadores.some(tem)) return { sel: { k: "marcador", id }, aba: "clipe" };
  return null;
}

/** Seleciona o item (e o efeito) do problema e abre a aba dele; problema da saída abre "Saída". */
export function irParaProblema(p: Problema): boolean {
  const st = useEdicaoStore.getState();
  if (!st.doc) return false;
  const ui = useInspetorUi.getState();
  if (p.codigo === "saida" || p.campo?.startsWith("saida")) {
    ui.abrirAba("saida");
    return true;
  }
  const r = itemDoProblema(st.doc, p);
  if (!r) return false;
  st.selecionar({ itens: [r.sel], foco: r.sel, efeito: p.efeito });
  ui.abrirAba(r.aba);
  return true;
}
