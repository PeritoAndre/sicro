/**
 * Intermediário de origem nos cartões (§7.5 "progresso no cartão"): a situação de cada trecho com efeito de origem
 * ligado que ninguém está pedindo agora (o palco e a seção Realce pedem só o clipe deles). Consulta SEM pedir
 * (`edicao_origem_estado`): nada entra na fila por causa do cartão.
 * CONTRATO: módulo novo (contratos-fase1 F4).
 */

import { useEffect, useMemo } from "react";
import type { Id } from "@domain/edicao";
import { useEdicaoStore } from "../store/edicaoStore";
import { useFundoStore } from "../store/fundoStore";
import { useOrigemStore } from "../store/origemStore";
import { clipeConhecido } from "../modelo/projeto";
import { assinaturaDaOrigem, guardarOrigem, origemTemDono } from "../inspetor/origemDoClipe";
import { estadoDaOrigem } from "../palco/origem";

/** Espera (ms) depois da última mudança do que muda o intermediário, antes de consultar. */
export const ESPERA_DA_CONSULTA = 600;

export function useOrigensDosClipes(): void {
  const ws = useEdicaoStore((s) => s.ws);
  const principal = useEdicaoStore((s) => s.doc?.principal);
  // [clipe, o que muda o intermediário dele]: só os trechos com efeito de origem ligado
  const clipes = useMemo(() => {
    const r: [Id, string][] = [];
    for (const c of principal ?? []) {
      const a = clipeConhecido(c) ? assinaturaDaOrigem(c) : null;
      if (a && clipeConhecido(c)) r.push([c.id, a]);
    }
    return r;
  }, [principal]);
  const assinatura = JSON.stringify(clipes);
  // a fila de fundo começou ou terminou um intermediário: a situação pode ter mudado
  const naFila = useFundoStore((s) => s.tarefas.filter((t) => t.tipo === "origem").map((t) => `${t.id}:${t.estado.tipo}`).join("|"));
  useEffect(() => {
    if (!ws || clipes.length === 0) return;
    let vivo = true;
    const relogio = setTimeout(async () => {
      for (const [id, a] of clipes) {
        const doc = useEdicaoStore.getState().doc;
        if (!vivo || !doc) return;
        if (origemTemDono(id)) continue;
        const e = await estadoDaOrigem(ws, doc, id);
        if (vivo && !origemTemDono(id)) guardarOrigem(id, e, a);
      }
    }, ESPERA_DA_CONSULTA);
    return () => {
      vivo = false;
      clearTimeout(relogio);
    };
  }, [ws, assinatura, naFila]); // eslint-disable-line react-hooks/exhaustive-deps

  // progresso dos que estão sendo feitos sem dono: o da tarefa na fila de fundo
  useEffect(
    () =>
      useFundoStore.subscribe((f) => {
        const { porClipe, guardar } = useOrigemStore.getState();
        for (const [id, e] of Object.entries(porClipe)) {
          if (e?.tipo !== "fazendo" || origemTemDono(id)) continue;
          const t = f.tarefas.find((x) => x.id === e.tarefa);
          if (t && (t.estado.tipo === "na_fila" || t.estado.tipo === "rodando") && t.feito !== e.feito) guardar(id, { ...e, feito: t.feito });
        }
      }),
    [],
  );
}
