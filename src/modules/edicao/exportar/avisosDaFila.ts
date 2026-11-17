/**
 * Aviso discreto no fim de cada exportação (P4.4): "Pronto" ou "A conferir" com "Abrir pasta"; "Falhou" sem ação.
 * Só a passagem de "em curso" para o fim avisa: a lista lida ao abrir (app reaberto, tela recarregada) não repete aviso.
 */

import { commands } from "@core/commands";
import { revealExported } from "@core/reveal";
import { pushToast } from "@components/toast/toastStore";
import { useWorkspaceStore } from "@stores/workspaceStore";
import type { Id, Tarefa } from "@domain/edicao";
import { useEdicaoStore } from "../store/edicaoStore";
import { useRenderStore, wsDaTarefa } from "../store/renderStore";
import { NOMES } from "./DialogoExportar";

const EM_CURSO = new Set<Tarefa["estado"]["tipo"]>(["na_fila", "preparando", "rodando"]);
/** Com botão: tempo de alcançar. */
const DURACAO_MS = 8000;

/** Pasta do caso da tarefa: a de quando foi pedida; senão a do caso aberto (se for o dela). */
function wsDe(t: Tarefa): string | null {
  const w = wsDaTarefa.get(t.id);
  if (w) return w;
  const ws = useWorkspaceStore.getState();
  if (ws.activeOccurrence?.id === t.caso && ws.activeWorkspacePath) return ws.activeWorkspacePath;
  return useEdicaoStore.getState().ws;
}

/** "Abrir pasta" da saída: o arquivo selecionado na pasta dele (como em Saídas). */
export async function abrirPastaDaSaida(ws: string, projetoId: Id, saidaId: Id): Promise<void> {
  try {
    const s = (await commands.edicaoSaidas(ws, projetoId)).find((x) => x.id === saidaId);
    if (s) revealExported(ws, s.arquivo);
  } catch {
    // saída excluída ou caso fechado: nada a abrir
  }
}

/** Avisa as exportações (e refeitas) que estavam em curso em `antes` e terminaram em `agora`. */
export function avisarFins(antes: readonly Tarefa[], agora: readonly Tarefa[]): void {
  const emCurso = new Set(antes.filter((t) => EM_CURSO.has(t.estado.tipo)).map((t) => t.id));
  for (const t of agora) {
    if (!emCurso.has(t.id) || (t.tipo !== "exportar" && t.tipo !== "refazer")) continue;
    const e = t.estado;
    const nome = t.predefinicao ? `${NOMES[t.predefinicao]} · ` : "";
    if (e.tipo === "concluida" || e.tipo === "a_conferir") {
      const ws = wsDe(t);
      const saida = e.saida_id;
      pushToast(e.tipo === "concluida" ? "success" : "warn", `${nome}${e.tipo === "concluida" ? "Pronto" : "A conferir"}`, {
        title: t.titulo,
        durationMs: DURACAO_MS,
        ...(ws ? { acao: { rotulo: "Abrir pasta", fazer: () => void abrirPastaDaSaida(ws, t.projeto_id, saida) } } : {}),
      });
    } else if (e.tipo === "falhou") {
      pushToast("error", `${nome}Falhou`, { title: t.titulo, durationMs: DURACAO_MS });
    }
  }
}

let ligado = false;

/** Liga os avisos uma vez (ficam ligados com a Edição fechada: a fila segue rodando). */
export function ligarAvisosDaFila(): void {
  if (ligado) return;
  ligado = true;
  useRenderStore.subscribe((s, a) => {
    if (s.tarefas !== a.tarefas) avisarFins(a.tarefas, s.tarefas);
  });
}
