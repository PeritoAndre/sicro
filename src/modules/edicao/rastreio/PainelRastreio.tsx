/**
 * Rastreio do efeito aberto no Inspetor (§8.6 "Rastreio"): "Seguir", "Seguir para trás", "Parar", "Retomar",
 * "Aceitar ausência" (Oculto, Fora de cena, Perdido), "Próximo trecho fraco", "Apagar daqui em diante",
 * "Simplificar" e "Da análise" (trilhas de Pessoas da mídia), com "Atualizar" quando a trilha mudou. Estado por ícone;
 * "Perdido em 00:12,345" quando parou. Só em trecho com mídia: é a mídia que se segue.
 * Fase 4 (contratos-fase4.md D11, D12): "Motor" antes do "Seguir", o contador de marcações à mão (ícone de mão +
 * número) e "Rastrear pessoas" ao lado do "Da análise".
 * Dono: P2.4; na Fase 4, o pacote "Pessoas na Edição e pequenos". Montado em `MarcacoesSecao` (com a bandeira
 * `RASTREIO_PRONTO`).
 */

import { useId, useState, type MouseEvent } from "react";
import { ChevronsLeft, ChevronsRight, CircleDashed, CircleOff, Download, Eraser, EyeOff, Hand, Loader2, Play, RefreshCw, SkipForward, Spline, Square, TriangleAlert, Users } from "lucide-react";
import type { ClipePrincipal, ClipeSobreposto, ContextoClipe, Efeito, Id, Rastreio, TabelaQuadros } from "@domain/edicao";
import { ESTADO_AUSENTE, ESTADO_INCERTO } from "@domain/edicao";
import { commands } from "@core/commands";
import { pushToast } from "@components/toast/toastStore";
import { useContextMenu, type MenuItem } from "@components/ContextMenu/ContextMenu";
import { formatarTempo } from "@components/CampoTempo/tempoTexto";
import { useSettingsStore } from "@stores/settingsStore";
import { separarErro, useEdicaoStore } from "../store/edicaoStore";
import { acharMidia } from "../modelo/projeto";
import { exibido, ptsDaTabela } from "../modelo/tempo";
import { useMenuAberto } from "../menus";
import { Mini } from "../inspetor/campos";
import {
  aceitarAusencia,
  alvoDoEfeito,
  apagarDaquiEmDiante,
  ausenciaNoCursor,
  desatualizado,
  editarRastreio,
  estadoDoMotor,
  irAoProximoFraco,
  marcasDoAlvo,
  MOTORES,
  motorDoPedido,
  paradaDe,
  pedidoDe,
  RASTREIO_PRONTO,
  seguirEfeito,
  temFraco,
  opSimplificar,
  podeSimplificar,
  trazerDaAnalise,
  useRastreioStore,
} from "../store/rastreioStore";
import { PESSOAS_NA_EDICAO_PRONTO, RastrearPessoas, usePessoasNaEdicao } from "./RastrearPessoas";
import { marcadasAMao } from "./contagem";
import styles from "../inspetor/Inspetor.module.css";

export interface PainelRastreioProps {
  efeito: Efeito;
  rastreio: Id;
  clipe: ClipePrincipal | ClipeSobreposto;
  ctx: ContextoClipe;
  /** Marcação parada (intermediário de origem em preparo): "Seguir" e "Retomar" desligados. */
  parada: boolean;
  /** Tabela da mídia, só quando confiável (a mesma do "De" e "Até"): pts do denso ↔ tempo de fonte. */
  tabela: TabelaQuadros | null;
}

const MOTIVOS = [
  ["Oculto", "oculto"],
  ["Fora de cena", "fora_de_cena"],
  ["Perdido", "perdido"],
] as const;

/** Estado do último quadro seguido: incerto ou sem o alvo, por ícone (o texto fica no `title`). */
export function IconeEstado({ estado, tamanho = 13 }: { estado: number; tamanho?: number }) {
  if (estado === ESTADO_INCERTO)
    return (
      <span title="Incerto" aria-label="Incerto" data-estado-rastreio="incerto" style={{ display: "inline-flex", color: "var(--sicro-warning)" }}>
        <TriangleAlert size={tamanho} aria-hidden />
      </span>
    );
  if (estado === ESTADO_AUSENTE)
    return (
      <span title="Fora de cena" aria-label="Fora de cena" data-estado-rastreio="fora" style={{ display: "inline-flex", color: "var(--sicro-danger)" }}>
        <EyeOff size={tamanho} aria-hidden />
      </span>
    );
  return null;
}

/** "Motor" (D12): a escolha fica no `rastreioStore` (e no navegador); item não instalado desligado com o ícone de
 *  baixar; com "Usar modelos de IA" desligado os neurais somem (§6.6). Marcada = o que vai no pedido. */
function EscolhaDoMotor({ desligado }: { desligado: boolean }) {
  const escolhido = useRastreioStore((s) => s.motor);
  const ai = useSettingsStore((s) => s.settings.ai);
  const soLeitura = useEdicaoStore((s) => s.soLeitura);
  const id = useId();
  const vai = motorDoPedido(escolhido, ai);
  return (
    <div className={styles.campo} data-campo="Motor">
      <span className={styles.rotulo} id={id}>
        Motor
      </span>
      <div className={styles.seg} role="group" aria-labelledby={id} style={{ flexWrap: "wrap" }}>
        {MOTORES.map(([m, rotulo]) => {
          const estado = estadoDoMotor(m, ai);
          if (estado === "desligada") return null;
          return (
            <button
              key={m}
              type="button"
              className={`${styles.segBtn} ${m === vai ? styles.segOn : ""}`}
              style={{ flex: "1 0 auto" }}
              aria-pressed={m === vai}
              data-valor={m}
              data-estado={estado === "ausente" ? "ausente" : undefined}
              disabled={soLeitura || desligado || estado !== "pronto"}
              onClick={() => m !== escolhido && useRastreioStore.getState().escolherMotor(m)}
            >
              {estado === "ausente" && <Download size={11} aria-hidden />}
              {rotulo}
            </button>
          );
        })}
      </div>
    </div>
  );
}

/** Marcações do perito (D12): ícone de mão + número (o mesmo do "marcações do perito" do PDF). */
function MarcadasAMao({ r }: { r: Rastreio }) {
  const n = marcadasAMao(r);
  return (
    <div
      data-marcadas-a-mao={n}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "4px 8px",
        borderRadius: "var(--radius-sm)",
        background: "var(--sicro-surface-2)",
        border: "1px solid var(--sicro-border)",
      }}
    >
      <Hand size={15} className={styles.dim} aria-hidden />
      <b data-dado="" style={{ fontFamily: "var(--font-mono)", fontSize: 14, fontWeight: 600, fontVariantNumeric: "tabular-nums" }}>
        {n}
      </b>
    </div>
  );
}

export function PainelRastreio({ efeito, rastreio, clipe, ctx, parada, tabela }: PainelRastreioProps) {
  const doc = useEdicaoStore((s) => s.doc);
  const ws = useEdicaoStore((s) => s.ws);
  const soLeitura = useEdicaoStore((s) => s.soLeitura);
  const problemas = useEdicaoStore((s) => s.analise?.problemas);
  // o cursor muda o que "Aceitar ausência" e "Apagar daqui em diante" alcançam
  useEdicaoStore((s) => s.cursor);
  const corrida = useRastreioStore((s) => s.corrida);
  const paradas = useRastreioStore((s) => s.paradas);
  // o denso que chega refaz as marcas (guardadas no store: o arrasto de uma alça não as refaz)
  useRastreioStore((s) => s.densos);
  const menu = useContextMenu();
  useMenuAberto(menu.isOpen);
  const [ocupado, setOcupado] = useState(false);
  const [trazendo, setTrazendo] = useState(false);
  /** Mídias sem trilha na análise ("Da análise" desliga depois de perguntar). */
  const [semTrilha, setSemTrilha] = useState<string | null>(null);
  /** Mídia com trilhas novas do "Rastrear pessoas" ainda não abertas: "Da análise" em destaque. */
  const [novas, setNovas] = useState<string | null>(null);
  const pessoasAqui = usePessoasNaEdicao((s) => !!s.tarefa);

  const x = doc ? alvoDoEfeito(doc, efeito.id, clipe.id, tabela) : null;
  const r: Rastreio | null = x?.rastreio ?? null;
  const mm = x ? marcasDoAlvo(x) : null;
  if (!RASTREIO_PRONTO || !x || !r || !doc || r.id !== rastreio) return null;

  const midiaRef = acharMidia(doc, x.clipe.midia);
  const sha = midiaRef?.sha256 ?? null;
  const minha = corrida?.rastreio === r.id;
  const livre = !soLeitura && !parada && !corrida && !ocupado;
  const frente = pedidoDe(x, "frente", "seguir", paradas);
  const tras = pedidoDe(x, "tras", "seguir", paradas);
  const p = paradaDe(r, paradas);
  const tParou = p?.pts != null && x.tabela ? (p.pts * x.tabela.tb.num) / x.tabela.tb.den : null;
  const retomar = p ? pedidoDe(x, p.sentido, "retomar", paradas) : null;
  // o cursor anda sem refazer as marcas
  const tCursor = ctx.tempo();
  const ausencia = mm ? ausenciaNoCursor(x, mm) : null;
  const haFraco = !!mm && temFraco(mm);
  const ptsCursor = x.tabela ? (ptsDaTabela(x.tabela)[exibido(x.tabela, tCursor)] as number) : null;
  const algoDepois = ptsCursor != null && (r.ancoras.some((k) => (k.pts != null ? k.pts >= ptsCursor : k.t >= tCursor)) || (!!r.denso && r.denso.ate_s >= tCursor));
  const velho = desatualizado(r, efeito.id, problemas ?? []);

  const comEspera = (f: () => Promise<void>) => {
    setOcupado(true);
    void f().finally(() => setOcupado(false));
  };
  const trazer = (pessoa: string, rotulo: string) => {
    if (!sha) return;
    setTrazendo(true);
    comEspera(() => trazerDaAnalise(efeito.id, x.clipe.midia, sha, pessoa, rotulo).finally(() => setTrazendo(false)));
  };
  const abrirMenu = (el: HTMLElement, itens: MenuItem[]) => {
    const b = el.getBoundingClientRect();
    menu.open({ clientX: b.left, clientY: b.bottom + 2, preventDefault() {}, stopPropagation() {} }, itens);
  };
  const daAnalise = (ev: MouseEvent<HTMLButtonElement>) => {
    if (!ws || !sha) return;
    const alvo = ev.currentTarget;
    setNovas(null);
    setOcupado(true);
    void commands
      .pessoasListar(ws, sha)
      .then((e) => {
        if (e.pessoas.length === 0) {
          setSemTrilha(sha);
          return;
        }
        abrirMenu(
          alvo,
          e.pessoas.map((pe) => ({
            label: pe.nome || `#${pe.numero}`,
            onSelect: () => trazer(pe.id, "Da análise"),
          })),
        );
      })
      .catch((e) => {
        const er = separarErro(e);
        pushToast("error", er.frase || er.codigo || "");
      })
      .finally(() => setOcupado(false));
  };
  /** "Rastrear pessoas" acabou: a lista da análise de novo (vazia desliga o "Da análise"; com trilhas, destaque). */
  const depoisDePessoas = () => {
    if (!ws || !sha) return;
    const m = sha;
    setSemTrilha(null);
    void commands
      .pessoasListar(ws, m)
      .then((e) => {
        if (e.pessoas.length === 0) setSemTrilha(m);
        else setNovas(m);
      })
      .catch(() => {});
  };

  return (
    <div className={styles.som} data-rastreio={r.id}>
      {menu.element}
      {PESSOAS_NA_EDICAO_PRONTO && (
        <>
          <EscolhaDoMotor desligado={!!corrida} />
          <MarcadasAMao r={r} />
        </>
      )}
      <div className={styles.barra}>
        {minha ? (
          <>
            <Mini onClick={() => void useRastreioStore.getState().parar()} data-acao="parar-rastreio">
              <Square size={12} /> Parar
            </Mini>
            {corrida?.ultimo ? (
              <>
                <IconeEstado estado={corrida.ultimo.estado} />
                {x.tabela && (
                  <span className={styles.tempoMarca} data-dado="">
                    {formatarTempo((corrida.ultimo.pts * x.tabela.tb.num) / x.tabela.tb.den)}
                  </span>
                )}
              </>
            ) : (
              <Loader2 size={13} className={styles.dim} aria-hidden />
            )}
          </>
        ) : trazendo ? (
          <>
            <Mini onClick={() => void commands.pessoasCancelar(true)} data-acao="parar-da-analise">
              <Square size={12} /> Parar
            </Mini>
            <Loader2 size={13} className={styles.dim} aria-hidden />
          </>
        ) : (
          <>
            <Mini disabled={!livre || !tras} onClick={() => void seguirEfeito(efeito.id, clipe.id, "tras", "seguir", tabela)} data-acao="seguir-tras">
              <ChevronsLeft size={12} /> Seguir para trás
            </Mini>
            <Mini disabled={!livre || !frente} onClick={() => void seguirEfeito(efeito.id, clipe.id, "frente", "seguir", tabela)} data-acao="seguir">
              Seguir <ChevronsRight size={12} />
            </Mini>
          </>
        )}
      </div>
      {p && tParou != null && !minha && (
        <div className={styles.barra}>
          {p.motivo === "perdido" ? (
            <Mini onClick={() => ctx.irPara(tParou)} data-acao="onde-parou">
              <CircleOff size={12} style={{ color: "var(--sicro-danger)" }} /> {`Perdido em ${formatarTempo(tParou)}`}
            </Mini>
          ) : (
            <Mini onClick={() => ctx.irPara(tParou)} data-acao="onde-parou" title="Fora de cena">
              <EyeOff size={12} style={{ color: "var(--sicro-danger)" }} /> {formatarTempo(tParou)}
            </Mini>
          )}
          <Mini disabled={!livre || !retomar} onClick={() => void seguirEfeito(efeito.id, clipe.id, p.sentido, "retomar", tabela)} data-acao="retomar">
            <Play size={12} /> Retomar
          </Mini>
        </div>
      )}
      <div className={styles.barra}>
        <Mini disabled={!x.tabela || minha || !haFraco} onClick={() => irAoProximoFraco(x, mm)} data-acao="trecho-fraco">
          <SkipForward size={12} /> Próximo trecho fraco
        </Mini>
        <Mini
          disabled={soLeitura || ocupado || minha || !ausencia}
          onClick={(ev) =>
            ausencia &&
            abrirMenu(
              ev.currentTarget,
              MOTIVOS.map(([rotulo, motivo]) => ({ label: rotulo, onSelect: () => aceitarAusencia(r.id, ausencia.de_s, ausencia.ate_s, motivo) })),
            )
          }
          data-acao="aceitar-ausencia"
        >
          <CircleDashed size={12} /> Aceitar ausência
        </Mini>
      </div>
      <div className={styles.barra}>
        <Mini disabled={!livre || !algoDepois} onClick={() => comEspera(() => apagarDaquiEmDiante(x))} data-acao="apagar-daqui">
          <Eraser size={12} /> Apagar daqui em diante
        </Mini>
        <Mini disabled={!livre || !podeSimplificar(doc, r)} onClick={() => comEspera(() => editarRastreio(r.id, opSimplificar(doc, r.id), "Simplificar"))} data-acao="simplificar">
          <Spline size={12} /> Simplificar
        </Mini>
      </div>
      <div className={styles.barra}>
        {PESSOAS_NA_EDICAO_PRONTO && ws && midiaRef && <RastrearPessoas ws={ws} midia={midiaRef} aoTerminar={depoisDePessoas} />}
        <Mini disabled={!livre || !sha || semTrilha === sha || pessoasAqui} ativo={!!sha && novas === sha} onClick={daAnalise} data-acao="da-analise">
          <Users size={12} /> Da análise
        </Mini>
        {velho && r.origem.tipo === "pessoas" && sha && (
          <Mini
            disabled={!livre}
            onClick={() => {
              const o = r.origem;
              if (o.tipo === "pessoas") trazer(o.pessoa_id, "Atualizar");
            }}
            data-acao="atualizar-rastreio"
          >
            <RefreshCw size={12} /> Atualizar
          </Mini>
        )}
      </div>
    </div>
  );
}
