/**
 * O que o Inspetor lê do backend sobre as mídias (tabela de quadros, faixa) e o `ContextoClipe` (§8.3) dos editores.
 * Uma leitura por mídia e caso; a resposta fica em memória enquanto o app vive (a da tabela de pacotes, até a do
 * decodificador ficar pronta).
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { commands } from "@core/commands";
import type { ClipeResolvido, ContextoClipe, Id, LivreResolvido, MidiaInfo, PedidoMarcacao, Problema, Seg, TabelaQuadros } from "@domain/edicao";
import { useEdicaoStore } from "../store/edicaoStore";
import { chaveFundo, fundoPorChave, useFundoStore } from "../store/fundoStore";
import { acharMidia } from "../modelo/projeto";
import { aplicarMarcado, tempoNoClipe, type ClipeQualquer } from "./acoes";
import { useInspetorUi, type Marcado } from "./ui";

type Lido<T> = { valor: T | null; promessa: Promise<T | null> | null };
const tabelas = new Map<string, Lido<TabelaQuadros>>();
const infos = new Map<string, Lido<MidiaInfo>>();

function ler<T>(mapa: Map<string, Lido<T>>, chave: string, f: () => Promise<T>): Lido<T> {
  let x = mapa.get(chave);
  if (!x) {
    const novo: Lido<T> = { valor: null, promessa: null };
    novo.promessa = f()
      .then((v) => (novo.valor = v))
      .catch(() => {
        // tabela pendente ou mídia ausente: tenta de novo na próxima montagem
        mapa.delete(chave);
        return null;
      });
    mapa.set(chave, novo);
    x = novo;
  }
  return x;
}

/** `provisoria`: resposta tirada da tabela de pacotes. Quando a do decodificador fica pronta na fila de fundo
 *  (`tabela:<sha256>`), a guardada sai e o Inspetor lê de novo (uma vez por tarefa pronta: sem laço). */
function useLido<T>(mapa: Map<string, Lido<T>>, ws: string | null, sha: string | null, f: (ws: string, sha: string) => Promise<T>, provisoria: (v: T) => boolean): T | null {
  const chave = ws && sha ? `${ws}\n${sha}` : null;
  const [, tique] = useState(0);
  const pronta = useFundoStore((s) => {
    const t = sha ? fundoPorChave(s.tarefas, chaveFundo("tabela", sha)) : null;
    return t?.estado.tipo === "pronta" ? t.id : null;
  });
  const x = chave ? ler(mapa, chave, () => f(ws as string, sha as string)) : null;
  const relida = useRef<string | null>(null);
  useEffect(() => {
    if (!chave || !pronta) return;
    const marca = `${chave}\n${pronta}`;
    const v = mapa.get(chave)?.valor;
    if (relida.current === marca || v == null || !provisoria(v)) return;
    relida.current = marca;
    mapa.delete(chave);
    tique((n) => n + 1);
  }, [mapa, chave, pronta, x?.valor, provisoria]);
  useEffect(() => {
    if (!x || x.valor) return;
    let vivo = true;
    void x.promessa?.then((v) => vivo && v !== null && tique((n) => n + 1));
    return () => {
      vivo = false;
    };
  }, [x]);
  return x?.valor ?? null;
}

const tabelaDePacotes = (t: TabelaQuadros) => t.origem === "pacotes";
const infoDePacotes = (i: MidiaInfo) => i.tabela === "pacotes";
export const useTabela = (ws: string | null, sha: string | null) => useLido(tabelas, ws, sha, (w, s) => commands.edicaoTabelaQuadros(w, s), tabelaDePacotes);
export const useMidiaInfo = (ws: string | null, sha: string | null) => useLido(infos, ws, sha, (w, s) => commands.edicaoMidiaInfo(w, s), infoDePacotes);

/** Só para os testes. */
export function esquecerLeituras(): void {
  tabelas.clear();
  infos.clear();
}

/** SHA-256 da mídia `id` do projeto aberto. */
export function useShaDe(id: Id | null): string | null {
  return useEdicaoStore((s) => (id && s.doc ? (acharMidia(s.doc, id)?.sha256 ?? null) : null));
}

/** Contexto que os editores de efeito recebem. `tempo()` lê o cursor da store (que o relógio do palco atualiza
 *  ao pausar, ao buscar e a cada 250 ms), não uma propriedade React. */
export function useContextoClipe(clipe: ClipeQualquer | null, info: MidiaInfo | null): ContextoClipe | null {
  const ws = useEdicaoStore((s) => s.ws);
  const doc = useEdicaoStore((s) => s.doc);
  const linha = useEdicaoStore((s) => s.analise?.linha ?? null);
  return useMemo(() => {
    if (!clipe || !doc || !ws) return null;
    const achada: ClipeResolvido | LivreResolvido =
      linha?.clipes.find((x) => x.id === clipe.id) ??
      linha?.livres.find((x) => x.id === clipe.id) ?? { id: clipe.id, inicio_q: 0, n_q: 0, sobrepoe_q: 0, fonte: null };
    return {
      ws,
      projeto: doc,
      clipe,
      midias: info ? [info] : [],
      linha: achada,
      tempo: (): Seg => {
        const atual = useEdicaoStore.getState().doc;
        return atual ? tempoNoClipe(clipe) : 0;
      },
      rastreios: doc.rastreios,
      marcar: (p: PedidoMarcacao) => pedirMarcacao(p, (v, t, pts) => aplicarMarcado(p, v, t, pts)),
      irPara: (t: Seg) => useEdicaoStore.getState().posicionar({ modo: "clipe", clipe: clipe.id, t }),
    };
  }, [clipe, doc, ws, linha, info]);
}

/** Liga a ferramenta do palco; o que for marcado chega em `aoMarcar`. */
export function pedirMarcacao(p: PedidoMarcacao, aoMarcar: Marcado, chave?: string): void {
  const ui = useInspetorUi.getState();
  // o mesmo botão de novo desliga
  if (chave && ui.chave === chave) return ui.cancelarMarcacao();
  ui.pedir(p, aoMarcar, chave);
}

/** Ids de clipes e efeitos que o Rust acusa com `item_invalido` (na abertura ou na análise): a tela os trata como
 *  desconhecidos (linha apagada, sem editor; contratos (g) 13). */
export function invalidosDe(avisos: readonly Problema[], problemas: readonly Problema[] | undefined): Set<string> {
  const ids = new Set<string>();
  for (const p of [...avisos, ...(problemas ?? [])]) {
    if (p.codigo !== "item_invalido") continue;
    // o problema do efeito cita também o clipe dele: só o efeito fica apagado
    if (p.efeito) ids.add(p.efeito);
    else if (p.clipe) ids.add(p.clipe);
  }
  return ids;
}

export function useInvalidos(): Set<string> {
  const avisos = useEdicaoStore((s) => s.avisos);
  const problemas = useEdicaoStore((s) => s.analise?.problemas);
  return useMemo(() => invalidosDe(avisos, problemas), [avisos, problemas]);
}
