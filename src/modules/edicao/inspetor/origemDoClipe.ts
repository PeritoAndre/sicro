/**
 * Intermediário de origem do clipe (§7.5; P1.4): o pedido (`edicaoOrigemPedir`) sempre que o que muda a imagem do
 * trecho muda, o acompanhamento pela fila de fundo e a gravação do `.trf` e do zoom que ele devolve (`alterar()`).
 * A situação fica em `store/origemStore.ts`. Montado mais de uma vez para o mesmo clipe (palco e seção Realce), só
 * um pede. Fica fora de `secoes/RealceSecao.tsx` para o palco e os cartões não importarem a seção (o registro a
 * importa, e ela o registro).
 * CONTRATO: módulo novo (contratos-fase1 F3); `RealceSecao.tsx` reexporta os nomes de antes.
 */

import { useEffect, useRef, useState } from "react";
import type { Desconhecido, EfEstabilizar, Efeito, Id, TrfNovo } from "@domain/edicao";
import { commands } from "@core/commands";
import { separarErro, useEdicaoStore } from "../store/edicaoStore";
import { useOrigemStore, type OrigemDoClipe } from "../store/origemStore";
import { useFundoStore } from "../store/fundoStore";
import { acharClipe, efeitoConhecido } from "../modelo/projeto";
import { noEfeito, type ClipeQualquer } from "./acoes";

/** Os efeitos de origem (os da seção Realce): entram no intermediário. */
export const TIPOS_DE_ORIGEM: ReadonlySet<string> = new Set(["desentrelacar", "estabilizar", "lente", "ajustes", "nitidez", "media_temporal"]);
const ehDeOrigem = (e: Efeito | Desconhecido): boolean => efeitoConhecido(e) && TIPOS_DE_ORIGEM.has(e.tipo);

/** `.trf` já gravados nesta sessão: desfeito pelo perito, não volta sozinho (o Desfazer ficaria preso). */
const gravados = new Set<string>();
const chaveDoTrf = (t: TrfNovo) => `${t.efeito}:${t.sha256}:${t.zoom_pct}`;
/** O perito pediu análise ou zoom novos para o efeito: o que voltar é gravado, mesmo igual ao de antes. */
export function pedirDeNovo(efeito: Id): void {
  for (const k of [...gravados]) if (k.startsWith(`${efeito}:`)) gravados.delete(k);
}

// projeto fechado ou outro aberto: o que foi gravado valia para o documento de antes (o mesmo projeto, aberto de novo
// sem o `.trf`, recebe-o outra vez)
useEdicaoStore.subscribe((s, antes) => {
  if (s.aberta !== antes.aberta) gravados.clear();
});

/** Só para testes: esquece o que foi gravado e o estado dos intermediários. */
export function esquecerOrigens(): void {
  gravados.clear();
  donos.clear();
  validoPara.clear();
  useOrigemStore.setState({ porClipe: {} });
}

/** Grava no efeito o `.trf` e o zoom que o intermediário devolveu (um passo, um escritor só: D2). */
function gravarTrf(t: TrfNovo): void {
  const st = useEdicaoStore.getState();
  const k = chaveDoTrf(t);
  if (!st.doc || st.soLeitura || gravados.has(k)) return;
  gravados.add(k);
  st.alterar(
    "Estabilizar",
    noEfeito<EfEstabilizar>(t.efeito, (e) => {
      if (e.tipo !== "estabilizar") return;
      e.trf_sha256 = t.sha256;
      e.zoom_pct = t.zoom_pct;
    }),
  );
}

/** Esperas (ms): depois da última mudança; consulta de reserva enquanto faz (quem avisa é a fila de fundo);
 *  enquanto a tabela do decodificador não chega. */
export const ESPERA = { mudanca: 400, fazendo: 5000, tabela: 1500 };

/** Ponteiro apertado (arrasto de um controle em curso): o intermediário só é refeito ao soltar (§7.5). */
let apertado = false;
if (typeof window !== "undefined") {
  window.addEventListener("pointerdown", () => void (apertado = true), true);
  // `blur`: soltou fora da janela
  for (const fim of ["pointerup", "pointercancel", "blur"]) window.addEventListener(fim, () => void (apertado = false), true);
}

/** Assinatura (`assinaturaDaOrigem`) para a qual a situação guardada de cada clipe vale. */
const validoPara = new Map<Id, string>();

/** Guarda a situação do intermediário do clipe, com a assinatura do que foi perguntado (null = clipe sem
 *  intermediário). */
export function guardarOrigem(clipe: Id, e: OrigemDoClipe | undefined, assinatura: string | null): void {
  if (assinatura === null || e === undefined) validoPara.delete(clipe);
  else validoPara.set(clipe, assinatura);
  useOrigemStore.getState().guardar(clipe, e);
}

/** Quem montou `useOrigem` para cada clipe, na ordem: só o primeiro pede (o palco e a seção Realce montam o mesmo
 *  gancho para o mesmo clipe); saindo ele, o seguinte assume. */
const donos = new Map<Id, (() => void)[]>();
/** Há um `useOrigem` montado pedindo e acompanhando o intermediário desse clipe? */
export const origemTemDono = (clipe: Id): boolean => (donos.get(clipe)?.length ?? 0) > 0;

/** Entra na fila de donos do clipe; devolve se é o dono agora. `avisar` roda quando a vez chega. */
function useDono(clipe: Id): boolean {
  const [, tique] = useState(0);
  const eu = useRef<() => void>();
  eu.current ??= () => tique((n) => n + 1);
  useEffect(() => {
    const f = eu.current!;
    const fila = donos.get(clipe) ?? [];
    donos.set(clipe, fila);
    fila.push(f);
    // a ordem só fica certa depois de entrar: confere de novo
    f();
    return () => {
      const i = fila.indexOf(f);
      if (i >= 0) fila.splice(i, 1);
      if (fila.length === 0) donos.delete(clipe);
      else if (i === 0) fila[0]!();
    };
  }, [clipe]);
  return donos.get(clipe)?.[0] === eu.current;
}

/** Assinatura do que muda o intermediário do clipe (mídia, faixa e efeitos de origem ligados); null = clipe sem
 *  intermediário (não é trecho, ou sem efeito de origem ligado). */
export function assinaturaDaOrigem(c: ClipeQualquer): string | null {
  const ligados = c.efeitos.filter((e) => ehDeOrigem(e) && e.ligado);
  return c.tipo === "trecho" && ligados.length > 0 ? JSON.stringify([c.midia, c.inicio_s, c.inicio_pts, c.fim_s, c.fim_pts, ligados]) : null;
}

/** Pede o intermediário do trecho sempre que o que muda a imagem dele muda, e acompanha até ficar pronto.
 *  O estado fica em `useOrigemStore` (o palco lê de lá). Montado mais de uma vez para o mesmo clipe, só um pede. */
export function useOrigem(c: ClipeQualquer): OrigemDoClipe | undefined {
  const ws = useEdicaoStore((s) => s.ws);
  const assinatura = assinaturaDaOrigem(c);
  const id = c.id;
  const dono = useDono(id);
  useEffect(() => {
    if (!dono) return;
    if (!assinatura || !ws) {
      guardarOrigem(id, undefined, null);
      return;
    }
    const guardar = (c: Id, e: OrigemDoClipe | undefined) => guardarOrigem(c, e, assinatura);
    // o que muda a imagem do clipe mudou: o intermediário guardado é de OUTRA imagem; o palco volta ao original (e
    // a anotação para) já, sem esperar o pedido
    if (useOrigemStore.getState().porClipe[id]?.tipo === "pronto" && validoPara.get(id) !== assinatura) guardar(id, undefined);
    let vivo = true;
    let relogio: ReturnType<typeof setTimeout> | undefined;
    let tarefa: string | null = null;
    const pedir = async () => {
      const doc = useEdicaoStore.getState().doc;
      if (!vivo || !doc) return;
      if (apertado) {
        relogio = setTimeout(() => void pedir(), ESPERA.mudanca);
        return;
      }
      tarefa = null;
      try {
        const e = await commands.edicaoOrigemPedir(ws, doc, id);
        if (!vivo) return;
        guardar(id, e);
        if (e.tipo === "fazendo") {
          tarefa = e.tarefa;
          relogio = setTimeout(() => void pedir(), ESPERA.fazendo);
        } else if (e.trf) {
          gravarTrf(e.trf);
          // gravar o `.trf` muda a assinatura do clipe sem mudar o arquivo: o pronto vale também para ela
          const d = useEdicaoStore.getState().doc;
          const c = d ? acharClipe(d, id)?.clipe : null;
          const nova = c ? assinaturaDaOrigem(c) : null;
          if (nova) validoPara.set(id, nova);
        }
      } catch (x) {
        if (!vivo) return;
        const erro = separarErro(x);
        // a tabela do decodificador ainda está na fila de fundo
        if (erro.codigo === "tabela_pendente") relogio = setTimeout(() => void pedir(), ESPERA.tabela);
        else guardar(id, { tipo: "falhou", erro });
      }
    };
    relogio = setTimeout(() => void pedir(), ESPERA.mudanca);
    // a fila de fundo avisa: progresso enquanto roda, nova consulta assim que a tarefa termina
    const largar = useFundoStore.subscribe((f) => {
      const t = tarefa ? f.tarefas.find((x) => x.id === tarefa) : undefined;
      if (!t || !tarefa) return;
      if (t.estado.tipo === "na_fila" || t.estado.tipo === "rodando") guardar(id, { tipo: "fazendo", tarefa, feito: t.feito });
      else {
        clearTimeout(relogio);
        void pedir();
      }
    });
    return () => {
      vivo = false;
      clearTimeout(relogio);
      largar();
    };
  }, [assinatura, ws, id, dono]);
  return useOrigemStore((s) => s.porClipe[id]);
}
