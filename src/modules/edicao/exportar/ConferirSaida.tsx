/**
 * "Conferir ocultações" (§5.10): percorre no ARQUIVO exportado os intervalos ocultados (`edicao_conferencia_trechos`),
 * os trechos fracos (incerto, interpolado, ausência aceita) primeiro; "Próximo trecho fraco", "Quadro anterior", "Próximo quadro";
 * termina em "Conferido" ou "Concluir sem conferir" (`conferirSaida`). Dono: P2.6.
 * Montado na Fila e nas Saídas ("A conferir"); só chama comando quando o diálogo abre.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { AlertTriangle, CheckCircle2, CircleDashed, EyeOff, Loader2, ScanEye, SkipForward, Spline, StepBack, StepForward } from "lucide-react";
import { Dialog } from "@components/Dialog/Dialog";
import { formatarTempo } from "@components/CampoTempo/tempoTexto";
import { caminhoDoCaso } from "@core/caminhoDoCaso";
import { commands } from "@core/commands";
import { mediaSrc } from "@core/mediaSrc";
import type { FaixaFraca, PreviaMidia, SaidaRegistrada, TabelaQuadros, TrechoConferencia } from "@domain/edicao";
import { elementoDe, type ElementoBusca } from "../palco/motor/elemento";
import { relogioMidia } from "../palco/motor/relogioMidia";
import { separarErro } from "../store/edicaoStore";
import { conferirSaida } from "./Saidas";
import styles from "./Exportar.module.css";

export const CONFERIR_PRONTO = true;

export interface ConferirSaidaProps {
  ws: string;
  saidaId: string;
}

/** Base de tempo da tabela do arquivo: 1/360000 s dá passo inteiro de 23,976 a 120 qps (e em 29,97 e 59,94). */
const BASE = 360000;

/** Tabela de quadros do arquivo exportado: taxa fixa a partir de 0 (§5.8), `n_quadros` quadros em `duracao_s`. */
export function tabelaDaSaida(s: Pick<SaidaRegistrada, "duracao_s" | "n_quadros">): TabelaQuadros {
  const n = Math.max(1, s.n_quadros);
  const passo = s.duracao_s > 0 ? Math.max(1, Math.round((BASE * s.duracao_s) / n)) : BASE / 30;
  return { tb: { num: 1, den: BASE }, pts0: 0, passos: new Array<number>(n - 1).fill(passo), passo_mediano: passo, chaves: [0], vfr: false, origem: "decodificador" };
}

/** Duração de um quadro do arquivo (s). */
export const passoDe = (s: Pick<SaidaRegistrada, "duracao_s" | "n_quadros"> | null): number => (s ? tabelaDaSaida(s).passo_mediano / BASE : 1 / 30);

/** Começo da próxima faixa fraca depois do quadro em `t` (no fim, volta à primeira); null = não há. */
export function proximoFraco(trechos: readonly TrechoConferencia[], t: number, passo: number): number | null {
  const comecos = trechos.flatMap((x) => x.fracos.map((f) => f.de_s)).sort((a, b) => a - b);
  if (comecos.length === 0) return null;
  return comecos.find((c) => c > t + passo / 2) ?? comecos[0]!;
}

/** Botão "Conferir ocultações" e o diálogo dele. */
export function ConferirSaida({ ws, saidaId }: ConferirSaidaProps) {
  const [aberto, setAberto] = useState(false);
  if (!CONFERIR_PRONTO) return null;
  return (
    <>
      <button type="button" className={styles.mini} data-acao="conferir-ocultacoes" onClick={() => setAberto(true)}>
        <ScanEye size={12} /> Conferir ocultações
      </button>
      {aberto && <Conferencia ws={ws} saidaId={saidaId} fechar={() => setAberto(false)} />}
    </>
  );
}

/** Incerto, interpolado, ou sem região numa ausência aceita (o alvo, se estiver ali, está à vista). */
const IconeFraco = ({ como }: { como: FaixaFraca["como"] }) =>
  como === "incerto" ? (
    <AlertTriangle size={12} className={styles.alerta} aria-hidden data-fraco="incerto" />
  ) : como === "ausencia" ? (
    <EyeOff size={12} className={styles.alerta} aria-hidden data-fraco="ausencia" />
  ) : (
    <Spline size={12} className={styles.alerta} aria-hidden data-fraco="interpolado" />
  );

const faixa = (de: number, ate: number) => `${formatarTempo(de)} – ${formatarTempo(ate)}`;

function Conferencia({ ws, saidaId, fechar }: { ws: string; saidaId: string; fechar(): void }) {
  const [saida, setSaida] = useState<SaidaRegistrada | null>(null);
  const [trechos, setTrechos] = useState<TrechoConferencia[] | null>(null);
  const [erro, setErro] = useState<string | null>(null);
  const [q, setQ] = useState(0);
  const [concluindo, setConcluindo] = useState(false);
  const video = useRef<HTMLVideoElement | null>(null);
  const tabela = useMemo(() => (saida ? tabelaDaSaida(saida) : null), [saida]);

  useEffect(() => {
    let vivo = true;
    Promise.all([commands.edicaoSaidas(ws), commands.edicaoConferenciaTrechos(ws, saidaId)])
      .then(([lista, tr]) => {
        if (!vivo) return;
        const s = lista.find((x) => x.id === saidaId) ?? null;
        setSaida(s);
        setTrechos(tr);
        if (!s) setErro("");
      })
      .catch((e) => vivo && setErro(separarErro(e).frase));
    return () => {
      vivo = false;
    };
  }, [ws, saidaId]);

  const passo = passoDe(saida);
  const n = saida ? Math.max(1, saida.n_quadros) : 1;
  /** O `<video>` só pela porta do palco (§7.1): busca no meio do quadro, quadro exibido pela tabela. */
  const porta = (): ElementoBusca | null => {
    if (!video.current || !saida || !tabela) return null;
    const previa: PreviaMidia = { relativo: saida.arquivo, tipo: "original", desloc_s: 0, de_s: 0, ate_s: saida.duracao_s, largura: 0, altura: 0, pronta: true };
    return elementoDe(video.current, relogioMidia(previa, tabela));
  };
  /** Para no quadro `j` do arquivo. */
  const irAo = (j: number) => {
    const k = Math.min(Math.max(Math.round(j), 0), n - 1);
    setQ(k);
    const e = porta();
    e?.pausar();
    e?.buscar(k);
  };
  /** Para no quadro que começa em `s` segundos do arquivo. */
  const ir = (s: number) => irAo(s / passo);
  const sincronizar = () => {
    const e = porta();
    if (e) setQ(e.quadro());
  };
  const proximo = trechos ? proximoFraco(trechos, q * passo, passo) : null;

  const concluir = async (conferida: boolean) => {
    setConcluindo(true);
    try {
      await conferirSaida(ws, saidaId, conferida);
      // fecha só se concluiu ("Conferido" pede confirmação)
      const s = (await commands.edicaoSaidas(ws)).find((x) => x.id === saidaId);
      if (!s || s.estado !== "a_conferir") fechar();
    } catch (e) {
      setErro(separarErro(e).frase);
    } finally {
      setConcluindo(false);
    }
  };

  const pronto = saida !== null && trechos !== null;
  const rodape = (
    <>
      <button type="button" className={styles.mini} data-acao="proximo-fraco" disabled={proximo === null} onClick={() => proximo !== null && ir(proximo)}>
        <SkipForward size={12} /> Próximo trecho fraco
      </button>
      <button type="button" className={styles.mini} data-acao="conferido" disabled={!pronto || concluindo} onClick={() => void concluir(true)}>
        {concluindo ? <Loader2 size={12} className={styles.gira} /> : <CheckCircle2 size={12} />} Conferido
      </button>
      <button type="button" className={styles.mini} data-acao="sem-conferir" disabled={!pronto || concluindo} onClick={() => void concluir(false)}>
        <CircleDashed size={12} /> Concluir sem conferir
      </button>
      <button type="button" className={styles.mini} data-acao="fechar" onClick={fechar}>
        Fechar
      </button>
    </>
  );

  return (
    <Dialog open title="Conferir ocultações" onClose={fechar} footer={rodape}>
      <div style={{ display: "flex", flexDirection: "column", gap: 8, minWidth: 0 }} data-conferencia>
        {erro !== null && (
          <span className={styles.erroLinha} data-erro data-dado>
            <AlertTriangle size={12} aria-hidden /> {erro}
          </span>
        )}
        {!pronto && erro === null && <Loader2 size={16} className={styles.gira} aria-hidden data-carregando />}
        {saida && (
          <video
            ref={video}
            src={mediaSrc(caminhoDoCaso(ws, saida.arquivo))}
            controls
            preload="auto"
            style={{ width: "100%", maxHeight: "55vh", background: "#000" }}
            onTimeUpdate={sincronizar}
            onSeeked={sincronizar}
          />
        )}
        {saida && (
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <button type="button" className={styles.icone} title="Quadro anterior" aria-label="Quadro anterior" onClick={() => irAo(q - 1)}>
              <StepBack size={13} />
            </button>
            <button type="button" className={styles.icone} title="Próximo quadro" aria-label="Próximo quadro" onClick={() => irAo(q + 1)}>
              <StepForward size={13} />
            </button>
            <span className={styles.mono} data-dado data-tempo>
              {formatarTempo(q * passo)}
            </span>
          </div>
        )}
        {trechos && (
          <ul className={styles.saidas} data-trechos style={{ maxHeight: "24vh", overflowY: "auto" }}>
            {trechos.map((x, i) => {
              const aqui = q * passo >= x.de_s - passo / 2 && q * passo < x.ate_s - passo / 2;
              return (
                <li key={`${x.efeito}:${i}`} className={styles.saida} data-trecho={x.efeito} data-fraco={x.fracos.length > 0 || undefined} aria-current={aqui || undefined}>
                  <button type="button" className={styles.tarefaTopo} style={{ background: "none", border: 0, color: "inherit", cursor: "pointer", padding: 0 }} onClick={() => ir(x.fracos[0]?.de_s ?? x.de_s)}>
                    {x.fracos.length > 0 ? <IconeFraco como={x.fracos[0]!.como} /> : <CheckCircle2 size={12} className={styles.dim} aria-hidden />}
                    <span className={styles.mono} data-dado>
                      {faixa(x.de_s, x.ate_s)}
                    </span>
                  </button>
                  {x.fracos.map((f, k) => (
                    <button key={k} type="button" className={styles.tarefaLinha} data-faixa-fraca={f.como} style={{ background: "none", border: 0, color: "inherit", cursor: "pointer", paddingLeft: 16 }} onClick={() => ir(f.de_s)}>
                      <IconeFraco como={f.como} />
                      <span className={styles.mono} data-dado>
                        {faixa(f.de_s, f.ate_s)}
                      </span>
                    </button>
                  ))}
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </Dialog>
  );
}
