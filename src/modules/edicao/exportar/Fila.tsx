/**
 * Fila (botão do topo e o painel dele): as exportações do caso com fase, progresso e as ações de cada estado;
 * embaixo, as saídas registradas do projeto. Estado pelos eventos `edicao:fila` e `edicao:progresso` (renderStore).
 */

import { useEffect, useRef } from "react";
import { Ban, CheckCircle2, CircleDashed, FileText, Hourglass, ListVideo, Loader2, ShieldAlert, X, XCircle } from "lucide-react";
import { Button } from "@components/Button/Button";
import { revealExported } from "@core/reveal";
import { selectActiveOccurrence, useWorkspaceStore } from "@stores/workspaceStore";
import { formatarTempo } from "@components/CampoTempo/tempoTexto";
import type { EstadoTarefa, Fase, Tarefa } from "@domain/edicao";
import { useEdicaoStore } from "../store/edicaoStore";
import { useRenderStore } from "../store/renderStore";
import { useExportarUi } from "./avisos";
import { Saidas, conferirSaida } from "./Saidas";
import styles from "./Exportar.module.css";

const EM_CURSO = new Set<EstadoTarefa["tipo"]>(["na_fila", "preparando", "rodando"]);

/** Rótulo do §8.6 para a fase do render. */
export function rotuloFase(f: Fase): string {
  switch (f.tipo) {
    case "previas":
      return "Preparando";
    case "camadas":
      return "Camadas";
    case "blocos":
      return `Vídeo ${f.feitos}/${f.total}`;
    case "audio":
      return "Áudio";
    case "juncao":
      return "Juntando";
    case "conferindo":
      return "Conferindo";
    case "relatorio":
      return "Relatório";
    case "pacote":
      return "Pacote do júri";
  }
}

/** Rótulo do §8.6 para o estado da tarefa. */
export function rotuloEstado(e: EstadoTarefa): string {
  switch (e.tipo) {
    case "na_fila":
      return "Na fila";
    case "preparando":
      return "Preparando";
    case "rodando":
      return rotuloFase(e.fase);
    case "a_conferir":
      return "A conferir";
    case "concluida":
    case "pronta":
      return "Pronto";
    case "falhou":
      return "Falhou";
    case "cancelada":
      return "Cancelado";
  }
}

/** Tarefas que a fila mostra: exportações (e "refazer") do caso aberto. */
export const tarefasDaFila = (ts: readonly Tarefa[], caso: string | null) => ts.filter((t) => (t.tipo === "exportar" || t.tipo === "refazer") && (!caso || t.caso === caso));

export function Fila() {
  const aberta = useExportarUi((s) => s.fila);
  const mostrar = useExportarUi((s) => s.mostrarFila);
  const tarefas = useRenderStore((s) => s.tarefas);
  const caso = useWorkspaceStore(selectActiveOccurrence)?.id ?? null;
  const raiz = useRef<HTMLDivElement>(null);
  useEffect(() => {
    void useRenderStore
      .getState()
      .ouvir()
      .catch(() => {});
  }, []);
  // fecha com Esc e com clique fora
  useEffect(() => {
    if (!aberta) return;
    const fora = (e: PointerEvent) => raiz.current && !raiz.current.contains(e.target as Node) && mostrar(false);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && mostrar(false);
    window.addEventListener("pointerdown", fora);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("pointerdown", fora);
      window.removeEventListener("keydown", esc);
    };
  }, [aberta, mostrar]);
  const lista = tarefasDaFila(tarefas, caso);
  const rodando = lista.filter((t) => EM_CURSO.has(t.estado.tipo)).length;
  return (
    <div className={styles.filaRaiz} ref={raiz}>
      <Button
        size="sm"
        variant="secondary"
        aria-expanded={aberta}
        data-acao="fila"
        leftIcon={rodando > 0 ? <Loader2 size={14} className={styles.gira} /> : <ListVideo size={14} />}
        onClick={() => mostrar(!aberta)}
      >
        Fila
        {rodando > 0 && <span className={styles.badge}>{rodando}</span>}
      </Button>
      {aberta && (
        <div className={styles.painel} role="dialog" aria-label="Fila">
          {lista.length > 0 && (
            <ul className={styles.tarefas}>
              {lista
                .slice()
                .reverse()
                .map((t) => (
                  <LinhaTarefa key={t.id} t={t} />
                ))}
            </ul>
          )}
          <Saidas />
        </div>
      )}
    </div>
  );
}

function IconeEstado({ e }: { e: EstadoTarefa }) {
  switch (e.tipo) {
    case "na_fila":
      return <Hourglass size={14} className={styles.dim} aria-hidden />;
    case "preparando":
    case "rodando":
      return <Loader2 size={14} className={`${styles.gira} ${styles.acento}`} aria-hidden />;
    case "a_conferir":
      return <ShieldAlert size={14} className={styles.alerta} aria-hidden />;
    case "concluida":
    case "pronta":
      return <CheckCircle2 size={14} className={styles.ok} aria-hidden />;
    case "falhou":
      return <XCircle size={14} className={styles.perigo} aria-hidden />;
    case "cancelada":
      return <Ban size={14} className={styles.dim} aria-hidden />;
  }
}

function LinhaTarefa({ t }: { t: Tarefa }) {
  const e = t.estado;
  const ws = useEdicaoStore((s) => s.ws);
  const raizCache = useEdicaoStore((s) => s.raizCache);
  const cancelar = () => void useRenderStore.getState().cancelar(t.id).catch(() => {});
  const fracao = e.tipo === "rodando" && e.total_q > 0 ? Math.min(1, e.feito_q / e.total_q) : null;
  return (
    <li className={styles.tarefa} data-estado={e.tipo}>
      <div className={styles.tarefaTopo}>
        <IconeEstado e={e} />
        <span className={styles.tarefaTitulo} title={t.titulo} data-dado>
          {t.titulo}
        </span>
        <span className={styles.tarefaEstado}>{rotuloEstado(e)}</span>
        {EM_CURSO.has(e.tipo) && (
          <button type="button" className={styles.icone} title="Cancelar" aria-label="Cancelar" onClick={cancelar}>
            <X size={13} />
          </button>
        )}
      </div>
      {e.tipo === "rodando" && (
        <div className={styles.progresso}>
          <span className={styles.barraFundo}>
            <span className={styles.barra} style={{ width: `${(fracao ?? 0) * 100}%` }} />
          </span>
          <span className={styles.mono} data-dado>{e.restante_s > 0 ? formatarTempo(e.restante_s).slice(0, 8) : ""}</span>
        </div>
      )}
      {e.tipo === "falhou" && (
        <div className={styles.tarefaLinha}>
          <span className={styles.erroLinha} data-dado>{e.mensagem}</span>
          {raizCache && e.log_rel && (
            <button type="button" className={styles.mini} onClick={() => ws && revealExported(ws, `${raizCache}/${e.log_rel}`)}>
              <FileText size={12} /> Ver registro
            </button>
          )}
        </div>
      )}
      {e.tipo === "a_conferir" && ws && (
        <div className={styles.tarefaLinha}>
          <button type="button" className={styles.mini} onClick={() => void conferirSaida(ws, e.saida_id, true)}>
            <CheckCircle2 size={12} /> Conferido
          </button>
          <button type="button" className={styles.mini} onClick={() => void conferirSaida(ws, e.saida_id, false)}>
            <CircleDashed size={12} /> Concluir sem conferir
          </button>
        </div>
      )}
    </li>
  );
}
