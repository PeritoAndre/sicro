/**
 * "Rastrear pessoas" de dentro da Edição (contratos-fase4.md D11): o MESMO motor e a mesma tabela da análise de Pessoas
 * (`pessoas_rastrear` na mídia do clipe, com as opções da análise; progresso pelo evento `pessoas-progress` da mesma
 * mídia; "Cancelar" por `pessoas_cancelar`); ao terminar, o "Da análise" do `PainelRastreio` lista as trilhas. Só mídia
 * de vídeo do caso (`MidiaRef.origem = "caso"`); some com "Usar modelos de IA" desligado (§6.6). A tarefa vive fora da
 * tela: o efeito pode fechar e abrir no meio sem perder o andamento.
 * Dono: pacote "Pessoas na Edição e pequenos".
 */

import { useEffect, useRef, useState } from "react";
import { create } from "zustand";
import { Download, Loader2, ScanSearch, Square } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import type { MidiaRef } from "@domain/edicao";
import type { OpcoesRastreio, PessoasProgresso } from "@domain/video_pessoas";
import { commands } from "@core/commands";
import { askConfirm } from "@components/Dialog/ask";
import { pushToast } from "@components/toast/toastStore";
import { useSettingsStore } from "@stores/settingsStore";
import { usePessoasStore } from "@modules/video/editor/pessoas/pessoasStore";
import { separarErro } from "../store/edicaoStore";
import { Mini } from "../inspetor/campos";
import styles from "../inspetor/Inspetor.module.css";

/** O pacote liga ao entregar (regra 3). */
export const PESSOAS_NA_EDICAO_PRONTO = true;

export interface RastrearPessoasProps {
  ws: string;
  midia: MidiaRef;
  /** Terminou (ou foi cancelado): o painel pede a lista do "Da análise" de novo. */
  aoTerminar(): void;
}

/** Evento do Rust (`commands/pessoas_commands.rs`). */
export const EVENTO_PESSOAS = "pessoas-progress";

export interface TarefaPessoas {
  sha: string;
  feitos: number;
  total: number;
  pessoas: number;
}

interface PessoasNaEdicao {
  /** `pessoas_rastrear` pedido daqui (um por vez no backend). */
  tarefa: TarefaPessoas | null;
  /** Quantas vezes a tarefa de cada mídia acabou (bem, cancelada ou com erro): o painel montado pede a lista. */
  fins: Record<string, number>;
}

export const usePessoasNaEdicao = create<PessoasNaEdicao>(() => ({ tarefa: null, fins: {} }));

let ouvindo: Promise<unknown> | null = null;

/** O evento é ouvido uma vez por sessão (o painel pode sair da tela no meio). */
function ouvir(): Promise<unknown> {
  ouvindo ??= listen<PessoasProgresso>(EVENTO_PESSOAS, (e) => receberProgresso(e.payload)).catch((x) => {
    ouvindo = null;
    throw x;
  });
  return ouvindo;
}

/** Andamento da tarefa daqui: só o da mesma mídia e da fase "rastrear" (a análise usa o mesmo evento). */
export function receberProgresso(p: PessoasProgresso): void {
  const t = usePessoasNaEdicao.getState().tarefa;
  if (!t || p.media_hash !== t.sha || p.fase !== "rastrear") return;
  usePessoasNaEdicao.setState({ tarefa: { ...t, feitos: p.feitos, total: p.total, pessoas: p.pessoas } });
}

/** As opções da análise (limiar, passo e classes guardados lá; sem nada guardado, os padrões dela). */
export function opcoesDaAnalise(): OpcoesRastreio {
  const { limiar, passo, classes } = usePessoasStore.getState();
  return { limiar, passo, classes: [...classes] };
}

function acabou(sha: string) {
  usePessoasNaEdicao.setState((s) => ({ tarefa: null, fins: { ...s.fins, [sha]: (s.fins[sha] ?? 0) + 1 } }));
}

/** "Rastrear pessoas": com trilhas detectadas na mídia, confirma antes (a análise troca as detectadas; as marcadas à
 *  mão ficam); depois roda a tarefa da análise. Devolve sem fazer nada se já há uma tarefa daqui. */
export async function rastrearPessoas(ws: string, sha: string): Promise<void> {
  if (usePessoasNaEdicao.getState().tarefa) return;
  let auto: { estado: string }[];
  try {
    auto = (await commands.pessoasListar(ws, sha)).pessoas.filter((p) => p.origem === "auto");
  } catch (e) {
    avisar(e);
    return;
  }
  if (auto.length > 0) {
    const ok = await askConfirm({
      title: "Rastrear pessoas",
      message: auto.length === 1 ? "1 pessoa" : `${auto.length} pessoas`,
      confirmLabel: "Rastrear pessoas",
      danger: auto.some((p) => p.estado === "editado"),
    });
    if (!ok) return;
  }
  if (usePessoasNaEdicao.getState().tarefa) return;
  usePessoasNaEdicao.setState({ tarefa: { sha, feitos: 0, total: 0, pessoas: 0 } });
  try {
    await ouvir().catch(() => {});
    await commands.pessoasRastrear(ws, sha, opcoesDaAnalise());
    // a aba Pessoas da análise com a mesma mídia aberta mostra o resultado
    const analise = usePessoasStore.getState();
    if (analise.mediaHash === sha) void analise.carregar(ws, sha);
  } catch (e) {
    if (separarErro(e).codigo !== "cancelado") avisar(e);
  } finally {
    acabou(sha);
  }
}

/** "Cancelar": a tarefa de pessoas em andamento (a daqui; o backend roda uma por vez). */
export function cancelarPessoas(): void {
  void Promise.resolve()
    .then(() => commands.pessoasCancelar())
    .catch(avisar);
}

function avisar(e: unknown) {
  const r = separarErro(e);
  pushToast("error", r.frase || r.codigo || "");
}

/** Só para os testes. */
export function esquecerPessoasNaEdicao(): void {
  ouvindo = null;
  usePessoasNaEdicao.setState({ tarefa: null, fins: {} });
}

export function RastrearPessoas({ ws, midia, aoTerminar }: RastrearPessoasProps) {
  const ai = useSettingsStore((s) => s.settings.ai);
  const tarefa = usePessoasNaEdicao((s) => s.tarefa);
  const fins = usePessoasNaEdicao((s) => s.fins[midia.sha256] ?? 0);
  // tarefa longa da própria análise (rastrear, mancha, exportar): o backend roda uma por vez
  const daAnalise = usePessoasStore((s) => s.tarefa != null);
  const [pedindo, setPedindo] = useState(false);
  // só os fins que acontecem com o painel na tela
  const vistos = useRef(fins);
  const terminar = useRef(aoTerminar);
  terminar.current = aoTerminar;
  useEffect(() => {
    if (fins > vistos.current) terminar.current();
    vistos.current = fins;
  }, [fins]);

  if (!PESSOAS_NA_EDICAO_PRONTO || ai.modelos_desligados || midia.origem !== "caso" || midia.tipo !== "video") return null;
  const instalado = !!ai.ort_lib_path && !!ai.pessoas_model_path;
  const minha = tarefa?.sha === midia.sha256;

  if (minha) {
    const fracao = tarefa.total > 0 ? Math.min(1, tarefa.feitos / tarefa.total) : null;
    return (
      // linha própria dentro da barra do "Da análise"
      <div className={styles.som} style={{ flex: "1 0 100%" }} data-rastrear-pessoas="rodando">
        <div className={styles.barra}>
          <Mini onClick={cancelarPessoas} data-acao="cancelar-pessoas">
            <Square size={12} /> Cancelar
          </Mini>
          <Loader2 size={13} className={styles.dim} aria-hidden />
          {tarefa.total > 0 && (
            <span className={styles.tempoMarca} data-dado="">
              {tarefa.feitos}/{tarefa.total}
            </span>
          )}
        </div>
        <div style={{ height: 3, borderRadius: 2, background: "var(--sicro-surface-2)", overflow: "hidden" }} aria-hidden>
          {fracao != null && <div data-andamento={fracao} style={{ height: "100%", width: `${fracao * 100}%`, background: "var(--sicro-accent)" }} />}
        </div>
      </div>
    );
  }
  return (
    <Mini
      disabled={!instalado || pedindo || !!tarefa || daAnalise}
      onClick={() => {
        setPedindo(true);
        void rastrearPessoas(ws, midia.sha256).finally(() => setPedindo(false));
      }}
      data-acao="rastrear-pessoas"
      data-estado={instalado ? undefined : "ausente"}
    >
      {!instalado ? <Download size={12} /> : pedindo ? <Loader2 size={12} /> : <ScanSearch size={12} />} Rastrear pessoas
    </Mini>
  );
}
