/**
 * Saídas registradas do projeto aberto (`edicao_saidas`): estado por ícone (a conferir, conferida, não conferida,
 * pronta), "Abrir pasta", "Conferido" / "Concluir sem conferir" e "Excluir" (confirmação padrão do app).
 */

import { useCallback, useEffect, useState } from "react";
import { CheckCircle2, CircleDashed, FolderOpen, ShieldAlert, ShieldCheck, ShieldX, Trash2 } from "lucide-react";
import { commands } from "@core/commands";
import { revealExported } from "@core/reveal";
import { askConfirm } from "@components/Dialog/ask";
import { pushToast } from "@components/toast/toastStore";
import { formatarTempo } from "@components/CampoTempo/tempoTexto";
import type { SaidaRegistrada } from "@domain/edicao";
import { separarErro, useEdicaoStore } from "../store/edicaoStore";
import { useRenderStore } from "../store/renderStore";
import styles from "./Exportar.module.css";

/** Estado mostrado: a conferir; concluída e conferida; concluída sem a conferência exigida; concluída. */
export type EstadoMostrado = "a_conferir" | "conferida" | "nao_conferida" | "pronta";

export function estadoDaSaida(s: SaidaRegistrada): EstadoMostrado {
  if (s.estado === "a_conferir") return "a_conferir";
  if (!s.exige_conferencia) return "pronta";
  return s.conferida_por ? "conferida" : "nao_conferida";
}

const ouvintes = new Set<() => void>();
const avisar = () => ouvintes.forEach((f) => f());

/** `edicao_conferir`: conclui a saída "A conferir" (conferida ou não). "Conferido" registra o perito: pede
 *  confirmação antes (contratos (h) 24). */
export async function conferirSaida(ws: string, saidaId: string, conferida: boolean): Promise<void> {
  if (conferida && !(await askConfirm({ title: "Conferido", confirmLabel: "Conferido" }))) return;
  try {
    await commands.edicaoConferir(ws, saidaId, conferida);
  } catch (e) {
    pushToast("error", separarErro(e).frase);
  }
  avisar();
}

export function Saidas() {
  const ws = useEdicaoStore((s) => s.ws);
  const projeto = useEdicaoStore((s) => s.aberta?.projetoId ?? null);
  // fim de exportação: a lista muda
  const fins = useRenderStore((s) => s.tarefas.filter((t) => t.estado.tipo === "concluida" || t.estado.tipo === "a_conferir").length);
  const [lista, setLista] = useState<SaidaRegistrada[]>([]);
  const ler = useCallback(() => {
    if (!ws || !projeto) return;
    let vivo = true;
    commands
      .edicaoSaidas(ws, projeto)
      .then((l) => vivo && setLista(l))
      .catch(() => {});
    return () => {
      vivo = false;
    };
  }, [ws, projeto]);
  useEffect(() => ler(), [ler, fins]);
  useEffect(() => {
    const f = () => void ler();
    ouvintes.add(f);
    return () => void ouvintes.delete(f);
  }, [ler]);
  if (!ws || lista.length === 0) return null;
  const excluir = async (s: SaidaRegistrada) => {
    if (!(await askConfirm({ title: "Excluir", confirmLabel: "Excluir", danger: true }))) return;
    try {
      await commands.edicaoSaidaExcluir(ws, s.id);
    } catch (e) {
      pushToast("error", separarErro(e).frase);
    }
    avisar();
  };
  return (
    <ul className={styles.saidas} data-saidas>
      {lista.map((s) => {
        const est = estadoDaSaida(s);
        const nome = s.arquivo.split(/[\\/]/).pop() ?? s.arquivo;
        return (
          <li key={s.id} className={styles.saida} data-estado={est}>
            <div className={styles.tarefaTopo}>
              <IconeSaida e={est} />
              <span className={styles.tarefaTitulo} title={s.arquivo} data-dado>
                {nome}
              </span>
              <span className={styles.mono} data-dado>{formatarTempo(s.duracao_s).slice(0, 8)}</span>
              <button type="button" className={styles.icone} title="Abrir pasta" aria-label="Abrir pasta" onClick={() => revealExported(ws, s.arquivo)}>
                <FolderOpen size={13} />
              </button>
              <button type="button" className={styles.icone} title="Excluir" aria-label="Excluir" onClick={() => void excluir(s)}>
                <Trash2 size={13} />
              </button>
            </div>
            {est === "a_conferir" && (
              <div className={styles.tarefaLinha}>
                <button type="button" className={styles.mini} onClick={() => void conferirSaida(ws, s.id, true)}>
                  <CheckCircle2 size={12} /> Conferido
                </button>
                <button type="button" className={styles.mini} onClick={() => void conferirSaida(ws, s.id, false)}>
                  <CircleDashed size={12} /> Concluir sem conferir
                </button>
              </div>
            )}
          </li>
        );
      })}
    </ul>
  );
}

function IconeSaida({ e }: { e: EstadoMostrado }) {
  switch (e) {
    case "a_conferir":
      return <ShieldAlert size={14} className={styles.alerta} aria-hidden />;
    case "conferida":
      return <ShieldCheck size={14} className={styles.ok} aria-hidden />;
    case "nao_conferida":
      return <ShieldX size={14} className={styles.alerta} aria-hidden />;
    case "pronta":
      return <CheckCircle2 size={14} className={styles.ok} aria-hidden />;
  }
}
