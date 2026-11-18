/**
 * Ocultação assistida (§6.4, §6.6, P4.5): "Sugestões" no Inspetor de um trecho. O SICRO propõe alvos (Rosto =
 * cabeça e rosto, Placa, Corpo) no trecho do clipe (`edicao_detectar`); cada proposta entra no projeto como rastreio
 * de origem `deteccao` SEM confirmação, com a ocultação da mídia apontando para ele (o render recusa enquanto não for
 * confirmada: `rastreio_nao_confirmado`). Cada linha: duração, menor tamanho e fração interpolada (dados), a folha de
 * contato, "Trechos fracos", "Confirmar" (só com a folha carregada: o perito viu os recortes) e "Descartar".
 * Confirmar grava quem, quando, o conteúdo (`denso_sha256`) e o intervalo visto. Com "Usar modelos de IA" desligado
 * a seção some. Dono: P4.5.
 */

import { useState, type MouseEvent } from "react";
import { create } from "zustand";
import { Check, ChevronDown, ChevronRight, Circle, Loader2, ScanFace, SkipForward, Square, Trash2 } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import type { ArquivoDenso, ClasseDeteccao, ClipePrincipal, ClipeSobreposto, ContextoClipe, EfOcultar, EventoDeteccao, FimDeteccao, Id, MidiaInfo, ProjetoEdicao, Rastreio, Seg, TabelaQuadros } from "@domain/edicao";
import { ESTADO_AUSENTE, ESTADO_INCERTO, EVENTO_DETECCAO, TIPO_MEDIDO } from "@domain/edicao";
import { commands } from "@core/commands";
import { pushToast } from "@components/toast/toastStore";
import { useContextMenu } from "@components/ContextMenu/ContextMenu";
import { formatarTempo } from "@components/CampoTempo/tempoTexto";
import { useSettingsStore } from "@stores/settingsStore";
import { separarErro, useEdicaoStore } from "../store/edicaoStore";
import { densoEmCache, peritoQueRegistra, useRastreioStore } from "../store/rastreioStore";
import { acharMidia } from "../modelo/projeto";
import { novoOcultar } from "../inspetor/novos";
import { usaRastreio } from "../inspetor/acoes";
import { padraoDoConteudo } from "../inspetor/secoes/ocultarPadrao";
import { Icone, Mini, Secao } from "../inspetor/campos";
import { useMenuAberto } from "../menus";
import { FolhaContato } from "./FolhaContato";
import styles from "../inspetor/Inspetor.module.css";

export const SUGESTOES_PRONTO = true;

/** Alvos que a tela oferece (rótulos do "Conteúdo" do Ocultar). */
export const ALVOS: readonly (readonly [string, ClasseDeteccao])[] = [
  ["Rosto", "rosto"],
  ["Placa", "placa"],
  ["Corpo", "pessoa"],
];

/** Conteúdo da ocultação de cada classe de trilha (o padrão de modo vem dele, §6.6 (1)). */
export function conteudoDaClasse(c: ClasseDeteccao): EfOcultar["conteudo"] {
  return c === "placa" ? "placa" : c === "pessoa" ? "corpo" : c === "veiculo" ? "outro" : "rosto";
}

// ---------------------------------------------------------------- estado da tela

export interface SugestoesState {
  /** Detecção em andamento (`tarefa` vazia: o comando ainda não respondeu). */
  tarefa: { id: string; projetoId: Id; clipe: Id; feitos: number; total: number } | null;
  /** Folhas de contato que apareceram na tela, por SHA-256 do denso: só elas habilitam "Confirmar". */
  vistas: Record<string, true>;
}

export const useSugestoesStore = create<SugestoesState>(() => ({ tarefa: null, vistas: {} }));

const edicao = () => useEdicaoStore.getState();

function avisar(e: unknown) {
  if (typeof e === "string") {
    const m = /^[a-z][a-z0-9_]*: ([\s\S]+)$/.exec(e);
    pushToast("error", m ? (m[1] as string) : e);
    return;
  }
  const r = separarErro(e);
  pushToast("error", r.frase || r.codigo || "");
}

let ouvindo: Promise<unknown> | null = null;

/** O evento `edicao:deteccao` é ouvido uma vez por sessão: a seção pode sair da tela no meio da detecção. */
function ouvir(): Promise<unknown> {
  ouvindo ??= listen<EventoDeteccao>(EVENTO_DETECCAO, (e) => receberDeteccao(e.payload)).catch((x) => {
    ouvindo = null;
    throw x;
  });
  return ouvindo;
}

/** Só para os testes. */
export function esquecerSugestoes(): void {
  ouvindo = null;
  useSugestoesStore.setState({ tarefa: null, vistas: {} });
}

/** "Sugestões" → alvo: detecção no trecho do clipe. */
export async function iniciarDeteccao(clipeId: Id, classes: ClasseDeteccao[]): Promise<void> {
  const e = edicao();
  if (!e.doc || !e.ws || !e.aberta || e.soLeitura || useSugestoesStore.getState().tarefa) return;
  await ouvir().catch(() => {});
  useSugestoesStore.setState({ tarefa: { id: "", projetoId: e.aberta.projetoId, clipe: clipeId, feitos: 0, total: 0 } });
  try {
    const id = await commands.edicaoDetectar(e.ws, e.doc, { clipe_id: clipeId, classes });
    const t = useSugestoesStore.getState().tarefa;
    if (t && !t.id) useSugestoesStore.setState({ tarefa: { ...t, id } });
  } catch (x) {
    useSugestoesStore.setState({ tarefa: null });
    avisar(x);
  }
}

/** "Parar". */
export function pararDeteccao(): void {
  const t = useSugestoesStore.getState().tarefa;
  if (t?.id) void commands.edicaoDetectarCancelar(t.id).catch(() => {});
}

export function receberDeteccao(ev: EventoDeteccao): void {
  const t = useSugestoesStore.getState().tarefa;
  // evento de outra tarefa (ou que chegou antes da resposta do comando, com a tarefa ainda sem id)
  if (!t || (t.id && t.id !== ev.tarefa_id)) return;
  if (ev.tipo === "avanco") {
    useSugestoesStore.setState({ tarefa: { ...t, id: ev.tarefa_id, feitos: ev.feitos, total: ev.total } });
    return;
  }
  useSugestoesStore.setState({ tarefa: null });
  if (ev.tipo === "falhou") {
    avisar(ev.mensagem);
    return;
  }
  // o projeto pode ter sido trocado no meio
  if (edicao().aberta?.projetoId === t.projetoId) aplicarSugestoes(ev);
}

/** O fim da detecção entra no projeto (um passo): os rastreios propostos, sem confirmação, e uma ocultação da mídia
 *  para cada um (conteúdo e modo pela classe, no intervalo do denso, sem margem a mais: a trilha já tem a dela). */
export function aplicarSugestoes(fim: FimDeteccao): void {
  if (fim.rastreios.length === 0) return;
  edicao().alterar("Sugestões", (d) => {
    const m = d.midias.find((x) => x.id === fim.midia_id);
    if (!m) return;
    for (const r of fim.rastreios) {
      if (!r.denso || r.origem.tipo !== "deteccao" || d.rastreios.some((x) => x.id === r.id)) continue;
      d.rastreios.push({ ...r, confirmacao: null });
      const conteudo = conteudoDaClasse(r.origem.classe);
      const o = novoOcultar(r.id, { de: r.denso.de_s, ate: r.denso.ate_s });
      o.conteudo = conteudo;
      Object.assign(o, padraoDoConteudo(conteudo));
      o.alvo.margem = 0;
      m.ocultacoes.push(o);
    }
  });
}

// ---------------------------------------------------------------- sugestões do clipe

/** As propostas da mídia do clipe que cruzam o trecho dele, na ordem do início. Proposta = rastreio de detecção com a
 *  ocultação da mídia apontando para ele: descartada, sai da lista mesmo que outro efeito siga o rastreio. */
export function sugestoesDoClipe(doc: ProjetoEdicao, clipe: ClipePrincipal | ClipeSobreposto): Rastreio[] {
  if (clipe.tipo !== "trecho") return [];
  const ocultados = new Set(acharMidia(doc, clipe.midia)?.ocultacoes.map((o) => ("alvo" in o ? (o as EfOcultar).alvo?.rastreio : null)));
  return doc.rastreios
    .filter((r) => r.origem.tipo === "deteccao" && r.midia === clipe.midia && ocultados.has(r.id) && r.denso && r.denso.de_s <= clipe.fim_s && r.denso.ate_s >= clipe.inicio_s)
    .sort((a, b) => (a.denso?.de_s ?? 0) - (b.denso?.de_s ?? 0));
}

/** Confirmação válida: do mesmo conteúdo e cobrindo o intervalo dele (a regra da validação, `rastreio_nao_confirmado`). */
export function confirmada(r: Rastreio): boolean {
  const c = r.confirmacao;
  return !!c && !!r.denso && c.denso_sha256 === r.denso.sha256 && c.de_s <= r.denso.de_s && c.ate_s >= r.denso.ate_s;
}

/** Duração, menor lado da caixa em pixels e a fração de quadros interpolados ou estendidos. */
export function resumoDaSugestao(r: Rastreio, d: ArquivoDenso | null): { duracao: Seg; menor: number | null; interpolada: number | null } {
  const duracao = r.denso ? r.denso.ate_s - r.denso.de_s : 0;
  if (!d || d.pts.length === 0) return { duracao, menor: null, interpolada: null };
  let menor = Infinity;
  let interp = 0;
  for (let i = 0; i < d.pts.length; i++) {
    if (d.estado[i] === ESTADO_AUSENTE) continue;
    menor = Math.min(menor, d.w[i]! * d.pixel.w, d.h[i]! * d.pixel.h);
    if (d.tipo[i] !== TIPO_MEDIDO) interp++;
  }
  return { duracao, menor: Number.isFinite(menor) ? Math.round(menor) : null, interpolada: interp / d.pts.length };
}

/** Intervalos de baixa confiança (interpolados, estendidos, incertos) em tempo de fonte, pela base da mídia. */
export function trechosFracos(d: ArquivoDenso, tb: { num: number; den: number }): [Seg, Seg][] {
  const fraco = (i: number) => d.estado[i] !== ESTADO_AUSENTE && (d.tipo[i] !== TIPO_MEDIDO || d.estado[i] === ESTADO_INCERTO);
  const t = (i: number) => ((d.pts[i] as number) * tb.num) / tb.den;
  const out: [Seg, Seg][] = [];
  for (let i = 0; i < d.pts.length; i++) {
    if (!fraco(i)) continue;
    let j = i;
    while (j + 1 < d.pts.length && fraco(j + 1)) j++;
    out.push([t(i), t(j)]);
    i = j;
  }
  return out;
}

/** O próximo trecho fraco depois de `agora` (volta ao primeiro no fim). */
export function proximoFraco(trechos: [Seg, Seg][], agora: Seg): Seg | null {
  if (trechos.length === 0) return null;
  return (trechos.find(([de]) => de > agora + 1e-6) ?? trechos[0]!)[0];
}

/** "Confirmar": o perito das Configurações, agora, o conteúdo e o intervalo que a folha mostrou. */
export function confirmarSugestao(rid: Id): void {
  const r = edicao().doc?.rastreios.find((x) => x.id === rid);
  if (!r?.denso || !useSugestoesStore.getState().vistas[r.denso.sha256]) return;
  const por = peritoQueRegistra();
  if (!por) return;
  const { sha256, de_s, ate_s } = r.denso;
  const em = new Date().toISOString();
  edicao().alterar("Confirmar", (d) => {
    const x = d.rastreios.find((y) => y.id === rid);
    if (x?.denso?.sha256 === sha256) x.confirmacao = { por, em, denso_sha256: sha256, de_s, ate_s };
  });
}

/** "Descartar": sai a ocultação da mídia que aponta para o rastreio e, se nenhum outro efeito o usa, o rastreio
 *  (o denso vira órfão e sai ao fechar). */
export function descartarSugestao(rid: Id): void {
  edicao().alterar("Descartar", (d) => {
    for (const m of d.midias) m.ocultacoes = m.ocultacoes.filter((o) => !("alvo" in o && (o as EfOcultar).alvo?.rastreio === rid));
    if (!usaRastreio(d as ProjetoEdicao, rid)) d.rastreios = d.rastreios.filter((r) => r.id !== rid);
  });
}

// ---------------------------------------------------------------- tela

export interface SugestoesSecaoProps {
  clipe: ClipePrincipal | ClipeSobreposto;
  ctx: ContextoClipe | null;
  info: MidiaInfo | null;
  tabela: TabelaQuadros | null;
}

export function SugestoesSecao({ clipe, ctx, tabela }: SugestoesSecaoProps) {
  const doc = useEdicaoStore((s) => s.doc);
  const soLeitura = useEdicaoStore((s) => s.soLeitura);
  const iaLigada = !useSettingsStore((s) => s.settings.ai.modelos_desligados);
  const tarefa = useSugestoesStore((s) => s.tarefa);
  useRastreioStore((s) => s.densos);
  const menu = useContextMenu();
  useMenuAberto(menu.isOpen);
  if (!SUGESTOES_PRONTO || !iaLigada || !doc || clipe.tipo !== "trecho") return null;
  const midia = acharMidia(doc, clipe.midia);
  if (!midia || midia.tipo !== "video") return null;
  const lista = sugestoesDoClipe(doc, clipe);
  const minha = tarefa?.clipe === clipe.id;
  const pedir = (ev: MouseEvent<HTMLButtonElement>) => {
    const b = ev.currentTarget.getBoundingClientRect();
    menu.open(
      { clientX: b.left, clientY: b.bottom + 2, preventDefault() {}, stopPropagation() {} },
      ALVOS.map(([rotulo, classe]) => ({ label: rotulo, onSelect: () => void iniciarDeteccao(clipe.id, [classe]) })),
    );
  };
  return (
    <Secao
      titulo="Sugestões"
      acoes={
        minha ? (
          <>
            <Loader2 size={12} className={styles.dim} aria-hidden />
            {tarefa.total > 0 && (
              <span className={styles.tempoMarca} data-dado="">
                {tarefa.feitos}/{tarefa.total}
              </span>
            )}
            <Mini onClick={pararDeteccao} disabled={!tarefa.id} data-acao="parar-sugestoes">
              <Square size={12} /> Parar
            </Mini>
          </>
        ) : (
          <Mini onClick={pedir} disabled={soLeitura || !!tarefa} data-acao="sugestoes">
            <ScanFace size={12} /> Sugestões
          </Mini>
        )
      }
    >
      {menu.element}
      {lista.length > 0 && (
        <ul className={styles.efeitos} data-sugestoes={clipe.id}>
          {lista.map((r) => (
            <LinhaSugestao key={r.id} r={r} midiaSha={midia.sha256} ctx={ctx} tb={tabela?.tb ?? null} />
          ))}
        </ul>
      )}
    </Secao>
  );
}

function LinhaSugestao({ r, midiaSha, ctx, tb }: { r: Rastreio; midiaSha: string; ctx: ContextoClipe | null; tb: { num: number; den: number } | null }) {
  const soLeitura = useEdicaoStore((s) => s.soLeitura);
  const vista = useSugestoesStore((s) => !!r.denso && !!s.vistas[r.denso.sha256]);
  const [aberta, setAberta] = useState(false);
  const d = densoEmCache(r);
  const ok = confirmada(r);
  const { duracao, menor, interpolada } = resumoDaSugestao(r, d);
  const fracos = d && tb ? trechosFracos(d, tb) : [];
  const ver = (sha: string) => useSugestoesStore.setState((s) => ({ vistas: { ...s.vistas, [sha]: true } }));
  return (
    <li className={`${styles.efeito} ${aberta ? styles.efeitoAberto : ""}`} data-sugestao={r.id} data-confirmada={ok || undefined}>
      <div className={styles.efeitoTopo}>
        <button type="button" className={styles.efeitoNome} onClick={() => setAberta((x) => !x)} aria-expanded={aberta}>
          {aberta ? <ChevronDown size={12} aria-hidden /> : <ChevronRight size={12} aria-hidden />}
          {ok ? <Check size={13} aria-hidden data-estado-sugestao="confirmada" style={{ color: "var(--sicro-success)" }} /> : <Circle size={13} aria-hidden data-estado-sugestao="pendente" />}
          <span data-dado="">{r.nome}</span>
        </button>
        <span className={styles.acoes}>
          <span className={styles.tempoMarca} data-dado="duracao">
            {formatarTempo(duracao)}
          </span>
          {menor != null && (
            <span className={styles.tempoMarca} data-dado="menor">
              {menor} px
            </span>
          )}
          {interpolada != null && (
            <span className={styles.tempoMarca} data-dado="interpolada">
              {Math.round(interpolada * 100)} %
            </span>
          )}
          <Icone rotulo="Descartar" disabled={soLeitura} onClick={() => descartarSugestao(r.id)} data-acao="descartar">
            <Trash2 size={13} />
          </Icone>
        </span>
      </div>
      {aberta && (
        <div className={styles.efeitoCorpo}>
          <FolhaContato rastreio={r} midiaSha={midiaSha} onCarregada={ver} onIrPara={(t) => ctx?.irPara(t)} />
          <div className={styles.barra}>
            <Mini
              disabled={fracos.length === 0 || !ctx}
              onClick={() => {
                const t = ctx ? proximoFraco(fracos, ctx.tempo()) : null;
                if (t != null) ctx?.irPara(t);
              }}
              data-acao="trechos-fracos"
            >
              <SkipForward size={12} /> Trechos fracos
            </Mini>
            {!ok && (
              <Mini disabled={soLeitura || !vista} onClick={() => confirmarSugestao(r.id)} data-acao="confirmar">
                <Check size={12} /> Confirmar
              </Mini>
            )}
          </div>
        </div>
      )}
    </li>
  );
}
