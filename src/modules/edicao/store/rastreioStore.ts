/**
 * Corrida do rastreio na Edição (§6.3, §8.4): pede "Seguir", acompanha o evento `edicao:rastreio`, aplica o fim
 * (denso e silhuetas) por `alterar()` e guarda os densos lidos para o palco e a faixa. Também as ações da linha do
 * efeito (Retomar, Aceitar ausência, Próximo trecho fraco, Apagar daqui em diante, Simplificar, Da análise), que o
 * painel, o palco e os atalhos chamam igual. Dono: P2.4 (contratos-fase2.md).
 */

import { useEffect } from "react";
import { create } from "zustand";
import type { Draft } from "immer";
import { listen } from "@tauri-apps/api/event";
import { commands } from "@core/commands";
import { pushToast } from "@components/toast/toastStore";
import { useSettingsStore } from "@stores/settingsStore";
import { ESTADO_AUSENTE, ESTADO_INCERTO, EVENTO_RASTREIO, TIPO_MEDIDO } from "@domain/edicao";
import type {
  ArquivoDenso,
  AvancoRastreio,
  Caixa,
  Chave,
  ClipeTrecho,
  Efeito,
  Editado,
  EventoRastreio,
  FimRastreio,
  Id,
  OpRastreio,
  ParouSentido,
  PedidoSeguir,
  ProjetoEdicao,
  Pts,
  Rastreio,
  Seg,
  Sentido,
  TabelaQuadros,
} from "@domain/edicao";
import { separarErro, useEdicaoStore } from "./edicaoStore";
import { acharClipe, acharEfeito, acharMidia, acharRastreio, efeitoConhecido } from "../modelo/projeto";
import { ancorasDoSimplificar, avaliar, emAusencia } from "../modelo/rastreio";
import { exibido, nQuadros, ptsDaTabela, tempoDoQuadro } from "../modelo/tempo";
import { recursoDe, useRecursos } from "../palco/recursos";
import { marcarAncora, rastreioDe } from "../inspetor/acoes";
import { pedirMarcacao } from "../inspetor/contexto";
import { useInspetorUi } from "../inspetor/ui";
import { marcacaoParada } from "../palco/origem";
import { useOrigemStore } from "./origemStore";

/** Ligado pelo P2.4 ao entregar: painel, ferramenta, faixa, atalhos e o denso no palco. */
export const RASTREIO_PRONTO = true;

/** Tolerância do "Simplificar": fração do lado médio (`modelos-rastreio.md`, quadros-chave: 4 %, mínimo 2 px no Rust). */
export const TOLERANCIA_SIMPLIFICAR = 0.04;
/** Rastreio só de anotação (seta, lupa, destaque…): janela da média móvel antes de simplificar (o tremido do motor
 *  viraria uma marcação por quadro). */
export const SUAVIZAR_ANOTACAO_S = 0.5;
/** Densos guardados em memória (o resto é relido do disco). */
export const MAX_DENSOS = 32;

/** Corrida em andamento (uma por vez, §6.3); `ultimo` = último andamento recebido (miniatura, caixa, estado).
 *  `tarefa` vazia: o comando ainda não respondeu (os eventos podem chegar antes). */
export interface CorridaNaTela {
  tarefa: string;
  rastreio: Id;
  pedido: PedidoSeguir;
  ultimo: AvancoRastreio | null;
}

export interface RastreioState {
  corrida: CorridaNaTela | null;
  /** Densos lidos (`edicao_rastreio_ler`), por SHA-256. */
  densos: Record<string, ArquivoDenso>;
  // CONTRATO: acréscimo do P2.4. Onde parou a corrida que gravou cada denso (por SHA-256 do denso): "Perdido em…" e
  // "Retomar" valem enquanto o rastreio aponta para ele (Ctrl+Z devolve o ponteiro e a parada junto).
  paradas: Record<string, ParouSentido[]>;
  /** "Seguir" / "Seguir para trás" / "Retomar". */
  seguir(p: PedidoSeguir): Promise<void>;
  /** "Parar". */
  parar(): Promise<void>;
  /** Evento `edicao:rastreio`: andamento na tela; fim aplicado por `alterar()`; falha no aviso do app. */
  receber(ev: EventoRastreio): void;
}

/** Tarefas que já acabaram: evento atrasado e resposta do comando depois do fim não ressuscitam a corrida. */
const acabadas = new Set<string>();
/** Tarefas acabadas cuja falha (depois do fim) já foi para o aviso do app. */
const avisadas = new Set<string>();
/** "Parar" antes de o comando responder: cancela assim que o id chegar. */
let pararAoChegar = false;

/** Cancela a tarefa largada; nada a mostrar (a tela já soltou). */
function cancelarCalado(tarefa: string) {
  void Promise.resolve()
    .then(() => commands.edicaoRastreioCancelar(tarefa))
    .catch(() => {});
}

function acabou(tarefa: string) {
  acabadas.add(tarefa);
  if (acabadas.size > 64) {
    const velha = acabadas.values().next().value as string;
    acabadas.delete(velha);
    avisadas.delete(velha);
  }
}

/** "<código>: <frase>" (do catálogo) ou o erro de um comando: a frase no aviso do app. */
function avisar(e: unknown) {
  if (typeof e === "string") {
    const m = /^[a-z][a-z0-9_]*: ([\s\S]+)$/.exec(e);
    pushToast("error", m ? (m[1] as string) : e);
    return;
  }
  const r = separarErro(e);
  pushToast("error", r.frase || r.codigo || "");
}

const edicao = () => useEdicaoStore.getState();

export const useRastreioStore = create<RastreioState>((set, get) => ({
  corrida: null,
  densos: {},
  paradas: {},

  async seguir(p) {
    const e = edicao();
    if (get().corrida || !e.doc || !e.ws || e.soLeitura) return;
    pararAoChegar = false;
    set({ corrida: { tarefa: "", rastreio: p.rastreio_id, pedido: p, ultimo: null } });
    let tarefa: string;
    try {
      tarefa = await commands.edicaoRastreioSeguir(e.ws, e.doc, p.rastreio_id, p.clipe_id, p.ancora, p.sentido, p.ate_s, p.motor);
    } catch (x) {
      const c = get().corrida;
      if (c && c.pedido === p && !c.tarefa) set({ corrida: null });
      avisar(x);
      return;
    }
    if (acabadas.has(tarefa)) return;
    const c = get().corrida;
    if (!c || c.pedido !== p) {
      // largada antes da resposta (editor desmontado): ninguém ouviria o fim
      acabou(tarefa);
      cancelarCalado(tarefa);
      return;
    }
    if (!c.tarefa) set({ corrida: { ...c, tarefa } });
    if (pararAoChegar) {
      pararAoChegar = false;
      await get().parar();
    }
  },

  async parar() {
    const c = get().corrida;
    if (!c) return;
    if (!c.tarefa) {
      pararAoChegar = true;
      return;
    }
    try {
      await commands.edicaoRastreioCancelar(c.tarefa);
    } catch (x) {
      // já tinha acabado: o fim pode ter se perdido (editor desmontado); a tela não fica presa
      if (separarErro(x).codigo === "tarefa_ausente") {
        if (get().corrida?.tarefa === c.tarefa) set({ corrida: null });
      } else avisar(x);
    }
  },

  receber(ev) {
    const c = get().corrida;
    // "ambos" com o segundo lado em falha: o fim (o que foi seguido) e depois a falha da mesma tarefa, só a frase
    if (ev.tipo === "falhou" && acabadas.has(ev.tarefa_id) && !avisadas.has(ev.tarefa_id)) {
      avisadas.add(ev.tarefa_id);
      avisar(ev.mensagem);
      return;
    }
    if (!c || acabadas.has(ev.tarefa_id)) return;
    if (c.tarefa ? c.tarefa !== ev.tarefa_id : c.rastreio !== ev.rastreio_id) return;
    if (ev.tipo === "avanco") {
      set({ corrida: { ...c, tarefa: ev.tarefa_id, ultimo: ev } });
      return;
    }
    acabou(ev.tarefa_id);
    set({ corrida: null });
    if (ev.tipo === "falhou") avisar(ev.mensagem);
    else aplicarFim(c, ev);
  },
}));

/** Fim da corrida: um passo do histórico (Ctrl+Z devolve o ponteiro anterior); cursor onde parou; perdeu o alvo
 *  com o efeito aberto: a ferramenta de marcar fica ligada para recolocar a caixa. */
function aplicarFim(c: CorridaNaTela, ev: FimRastreio) {
  if (ev.denso) {
    const denso = ev.denso;
    const rotulo = c.pedido.sentido === "tras" ? "Seguir para trás" : "Seguir";
    edicao().alterar(rotulo, (d) => {
      const r = d.rastreios.find((x) => x.id === ev.rastreio_id);
      if (!r) return;
      r.denso = denso;
      // um arquivo de silhuetas por denso: o anterior não vale para o novo
      r.silhuetas = ev.silhuetas;
      if (r.origem.tipo === "manual") r.origem = { tipo: "assistido" };
    });
    useRastreioStore.setState((s) => ({ paradas: { ...s.paradas, [denso.sha256]: ev.parou } }));
    void lerDenso(ev.rastreio_id, denso.sha256);
  }
  const sentido: Sentido = c.pedido.sentido === "tras" ? "tras" : "frente";
  const p = ev.parou.find((x) => x.sentido === sentido) ?? ev.parou[0] ?? null;
  const pts = p?.pts ?? c.ultimo?.pts ?? null;
  const doc = edicao().doc;
  const r = doc ? acharRastreio(doc, ev.rastreio_id) : null;
  const tab = doc && r?.midia ? recursoDe(acharMidia(doc, r.midia)?.sha256).tabela : null;
  if (pts != null && tab) edicao().posicionar({ modo: "clipe", clipe: c.pedido.clipe_id, t: (pts * tab.tb.num) / tab.tb.den });
  if (!p || (p.motivo !== "perdido" && p.motivo !== "saiu_do_quadro")) return;
  const ef = edicao().selecao.efeito;
  const aberto = doc && ef ? acharEfeito(doc, ef)?.efeito : null;
  if (!aberto || rastreioDe(aberto) !== ev.rastreio_id || useInspetorUi.getState().ferramenta) return;
  const rid = ev.rastreio_id;
  pedirMarcacao({ tipo: "caixa", rastreio: rid, efeito: aberto.id }, (v, t, q) => "w" in v && marcarAncora(rid, { t, pts: q, v }, "Marcar"), `${aberto.id}:caixa`);
}

// ---- densos lidos ----

const lendo = new Set<string>();
/** Leitura que falhou (arquivo ausente ou adulterado): não repete a cada quadro pintado. */
const naoLidos = new Set<string>();

function guardar(sha: string, d: ArquivoDenso) {
  useRastreioStore.setState((s) => {
    const densos = { ...s.densos, [sha]: d };
    const chaves = Object.keys(densos);
    if (chaves.length > MAX_DENSOS) {
      const doc = edicao().doc;
      const usados = new Set((doc?.rastreios ?? []).map((r) => r.denso?.sha256));
      for (const k of chaves) {
        if (Object.keys(densos).length <= MAX_DENSOS) break;
        if (k !== sha && !usados.has(k)) delete densos[k];
      }
    }
    return { densos };
  });
  // o palco repinta com o denso
  useRecursos.setState((s) => ({ versao: s.versao + 1 }));
}

async function lerDenso(rid: Id, sha: string): Promise<void> {
  const e = edicao();
  if (lendo.has(sha) || naoLidos.has(sha) || useRastreioStore.getState().densos[sha] || !e.ws || !e.aberta) return;
  lendo.add(sha);
  try {
    guardar(sha, await commands.edicaoRastreioLer(e.ws, e.aberta.projetoId, rid, sha));
  } catch {
    naoLidos.add(sha);
  } finally {
    lendo.delete(sha);
  }
}

/** Denso do rastreio já lido (null = sem denso ou ainda não chegou: pede ao backend uma vez).
 *  Quem chama: `palco/cena.ts` (`avaliar(r, em, densoEmCache(r))`) e a faixa. */
export function densoEmCache(r: Rastreio): ArquivoDenso | null {
  if (!RASTREIO_PRONTO || !r.denso) return null;
  const d = useRastreioStore.getState().densos[r.denso.sha256];
  if (d) return d;
  void lerDenso(r.id, r.denso.sha256);
  return null;
}

/** Só para os testes. */
export function esquecerRastreios(): void {
  lendo.clear();
  naoLidos.clear();
  acabadas.clear();
  avisadas.clear();
  pararAoChegar = false;
  useRastreioStore.setState({ corrida: null, densos: {}, paradas: {} });
}

/** Editor desmontado (projeto fechado ou trocado) no meio da corrida: o fim não seria ouvido e a tela ficaria presa
 *  em "Parar". Cancela e solta. */
function largarCorrida(): void {
  const c = useRastreioStore.getState().corrida;
  if (!c) return;
  pararAoChegar = false;
  if (c.tarefa) {
    acabou(c.tarefa);
    cancelarCalado(c.tarefa);
  }
  useRastreioStore.setState({ corrida: null });
}

/** Escuta `edicao:rastreio` enquanto o editor está montado (`EditorEdicao`). */
export function useOuvirRastreio(): void {
  useEffect(() => {
    if (!RASTREIO_PRONTO) return;
    // leitura que falhou (pasta de rede fora do ar) volta a ser tentada quando o editor abre de novo
    naoLidos.clear();
    let vivo = true;
    let soltar: (() => void) | null = null;
    void listen<EventoRastreio>(EVENTO_RASTREIO, (e) => useRastreioStore.getState().receber(e.payload))
      .then((f) => {
        if (vivo) soltar = f;
        else f();
      })
      .catch(() => {});
    return () => {
      vivo = false;
      soltar?.();
      largarCorrida();
    };
  }, []);
}

// ---- estado de cada quadro (faixa, "Próximo trecho fraco", "Aceitar ausência") ----

/** `ancora` = o perito marcou; `ok` = medido (ou segurado, no manual); `nada` = sem caixa e sem linha no denso. */
export type MarcaQuadro = "ancora" | "ok" | "incerto" | "interpolado" | "ausente" | "aceita" | "nada";
/** Trecho fraco (§8.6): incerto, interpolado ou sem o alvo. */
export const FRACAS: ReadonlySet<MarcaQuadro> = new Set<MarcaQuadro>(["incerto", "interpolado", "ausente", "nada"]);
/** O que "Aceitar ausência" cobre: quadros sem caixa. A ausência só desculpa o "nada" (§6.1): onde há caixa
 *  (medida, incerta ou interpolada) o motor a desenha mesmo dentro da ausência. */
const SEM_ALVO: ReadonlySet<MarcaQuadro> = new Set<MarcaQuadro>(["ausente", "nada"]);

const linhas = new WeakMap<ArquivoDenso, Map<number, number>>();
function linhaDo(d: ArquivoDenso, pts: Pts): number | undefined {
  let m = linhas.get(d);
  if (!m) {
    m = new Map();
    for (let i = 0; i < d.pts.length; i++) m.set(d.pts[i] as number, i);
    linhas.set(d, m);
  }
  return m.get(pts);
}

/** Estado do quadro como o motor o usa: âncora do quadro; a linha do denso (estado e tipo gravados); sem linha, as
 *  âncoras (interpolação ou a última segurada no manual). Sem caixa dentro de ausência aceita: "aceita". */
export function marcaDoQuadro(r: Rastreio, d: ArquivoDenso | null, t: Seg, pts: Pts): MarcaQuadro {
  const a = avaliar(r, { t, pts });
  if (a?.como === "ancora") return "ancora";
  const i = d ? linhaDo(d, pts) : undefined;
  let m: MarcaQuadro;
  if (d && i !== undefined) {
    const e = d.estado[i];
    m = e === ESTADO_AUSENTE ? "ausente" : e === ESTADO_INCERTO ? "incerto" : d.tipo[i] === TIPO_MEDIDO ? "ok" : "interpolado";
  } else m = !a ? "nada" : a.como === "interpolado" ? "interpolado" : "ok";
  return SEM_ALVO.has(m) && emAusencia(r, t) ? "aceita" : m;
}

/** Marca dos quadros j0..j1 (inclusive) da tabela. */
export function marcas(r: Rastreio, d: ArquivoDenso | null, tab: TabelaQuadros, j0: number, j1: number): MarcaQuadro[] {
  const v = ptsDaTabela(tab);
  const saida: MarcaQuadro[] = [];
  for (let j = Math.max(0, j0); j <= Math.min(j1, v.length - 1); j++) saida.push(marcaDoQuadro(r, d, tempoDoQuadro(tab, j), v[j] as number));
  return saida;
}

/** Quadros da faixa [de, ate] da mídia; `ateExclusivo`: `ate` é o fim de trecho (o quadro dele fica fora). */
export function quadrosDaFaixa(tab: TabelaQuadros, de: Seg, ate: Seg, ateExclusivo: boolean): [number, number] | null {
  const n = nQuadros(tab);
  let j0 = exibido(tab, de);
  if (tempoDoQuadro(tab, j0) < de - 1e-9 && j0 < n - 1) j0++;
  let j1 = exibido(tab, ate);
  if (ateExclusivo && tempoDoQuadro(tab, j1) >= ate - 1e-9) j1--;
  return j1 >= j0 ? [j0, j1] : null;
}

/** Começo do próximo trecho fraco depois de `i` (índice em `m`); sem nenhum depois, o primeiro. null = nenhum. */
export function proximoFraco(m: readonly MarcaQuadro[], i: number): number | null {
  const comeca = (j: number) => FRACAS.has(m[j] as MarcaQuadro) && (j === 0 || !FRACAS.has(m[j - 1] as MarcaQuadro));
  for (let j = Math.max(i + 1, 0); j < m.length; j++) if (comeca(j)) return j;
  for (let j = 0; j <= Math.min(i, m.length - 1); j++) if (comeca(j)) return j;
  return null;
}

/** Trecho sem o alvo em volta de `i`: [primeiro, último] (índices em `m`); null = o quadro tem caixa medida. */
export function trechoSemAlvo(m: readonly MarcaQuadro[], i: number): [number, number] | null {
  if (!SEM_ALVO.has(m[i] as MarcaQuadro)) return null;
  let a = i;
  let b = i;
  while (a > 0 && SEM_ALVO.has(m[a - 1] as MarcaQuadro)) a--;
  while (b < m.length - 1 && SEM_ALVO.has(m[b + 1] as MarcaQuadro)) b++;
  return [a, b];
}

// ---- o alvo das ações: efeito, rastreio, clipe e a faixa em que ele vale ----

export interface AlvoRastreio {
  efeito: Efeito;
  rastreio: Rastreio;
  clipe: ClipeTrecho;
  /** Faixa do efeito em tempo de fonte; `ateExclusivo` quando `ate` é o fim do clipe. */
  de: Seg;
  ate: Seg;
  ateExclusivo: boolean;
  /** Tabela confiável da mídia (null = não dá para converter pts e tempo). */
  tabela: TabelaQuadros | null;
}

/** Tabela da mídia que o palco já leu, só quando confiável (a mesma regra do "De" e "Até"). */
function tabelaConfiavel(sha: string | undefined): TabelaQuadros | null {
  const r = recursoDe(sha);
  return r.info?.marcavel ? r.tabela : null;
}

/** Efeito com rastreio de mídia num trecho da principal; `clipeId` = o clipe aberto (para a ocultação da mídia). */
export function alvoDoEfeito(doc: ProjetoEdicao, efeitoId: Id, clipeId: Id | null, tabela?: TabelaQuadros | null): AlvoRastreio | null {
  const a = acharEfeito(doc, efeitoId);
  if (!a) return null;
  const rid = rastreioDe(a.efeito);
  const r = rid ? acharRastreio(doc, rid) : null;
  const achado = a.clipe ?? clipeId;
  const c = achado ? acharClipe(doc, achado) : null;
  if (!r || !c || c.onde !== "principal" || c.clipe.tipo !== "trecho" || r.base !== "fonte" || r.midia !== c.clipe.midia) return null;
  if (a.midia && a.midia !== c.clipe.midia) return null;
  const clipe = c.clipe;
  const ativo = a.efeito.ativo;
  const ate = ativo?.ate ?? null;
  return {
    efeito: a.efeito,
    rastreio: r,
    clipe,
    de: ativo?.de ?? clipe.inicio_s,
    ate: ate ?? clipe.fim_s,
    ateExclusivo: ate == null,
    tabela: tabela !== undefined ? tabela : tabelaConfiavel(acharMidia(doc, clipe.midia)?.sha256),
  };
}

/** Tempo do cursor no clipe do alvo (o começo do efeito quando o cursor está em outro clipe). */
function cursorNo(x: AlvoRastreio): Seg {
  const c = edicao().cursor;
  return c.modo === "clipe" && c.clipe === x.clipe.id ? c.t : x.de;
}

const DIF = 1e-6;
/** Mesmo quadro ou depois/antes dele: pelo pts quando os dois têm. */
const naoAntes = (k: Chave<Caixa>, t: Seg, pts: Pts | null) => (k.pts != null && pts != null ? k.pts >= pts : k.t >= t - DIF);
const naoDepois = (k: Chave<Caixa>, t: Seg, pts: Pts | null) => (k.pts != null && pts != null ? k.pts <= pts : k.t <= t + DIF);

/** Âncora de onde "Seguir" parte: a do quadro do cursor ou a última antes dele (para trás: a primeira depois);
 *  sem nenhuma desse lado, a mais próxima do outro. */
export function ancoraDeSeguir(r: Rastreio, t: Seg, pts: Pts | null, sentido: Sentido): Chave<Caixa> | null {
  const ks = r.ancoras;
  if (ks.length === 0) return null;
  if (sentido === "frente") return [...ks].reverse().find((k) => naoDepois(k, t, pts)) ?? (ks[0] as Chave<Caixa>);
  return ks.find((k) => naoAntes(k, t, pts)) ?? (ks[ks.length - 1] as Chave<Caixa>);
}

/** Âncora de onde "Retomar" parte: a marcada no quadro onde parou ou a primeira depois dele, no sentido da corrida. */
export function ancoraDeRetomar(r: Rastreio, p: ParouSentido, t: Seg): Chave<Caixa> | null {
  if (p.sentido === "frente") return r.ancoras.find((k) => naoAntes(k, t, p.pts)) ?? null;
  return [...r.ancoras].reverse().find((k) => naoDepois(k, t, p.pts)) ?? null;
}

/** Parada que pede o perito ("Perdido em…" ou fora de cena) da corrida que gravou o denso atual. */
export function paradaDe(r: Rastreio, paradas: Record<string, ParouSentido[]>): ParouSentido | null {
  const ps = r.denso ? paradas[r.denso.sha256] : undefined;
  return ps?.find((p) => (p.motivo === "perdido" || p.motivo === "saiu_do_quadro") && p.pts != null) ?? null;
}

const tempoDoPts = (tab: TabelaQuadros, pts: Pts): Seg => (pts * tab.tb.num) / tab.tb.den;

/** pts do quadro mostrado em `t` (null sem tabela). */
function ptsEm(tab: TabelaQuadros | null, t: Seg): Pts | null {
  return tab ? (ptsDaTabela(tab)[exibido(tab, t)] as number) : null;
}

/** O pedido de "Seguir" ("seguir") ou "Retomar" ("retomar") do alvo; null = não há de onde partir. */
export function pedidoDe(x: AlvoRastreio, sentido: Sentido, modo: "seguir" | "retomar", paradas: Record<string, ParouSentido[]>): PedidoSeguir | null {
  let ancora: Chave<Caixa> | null;
  if (modo === "retomar") {
    const p = paradaDe(x.rastreio, paradas);
    if (!p || p.pts == null || !x.tabela) return null;
    sentido = p.sentido;
    ancora = ancoraDeRetomar(x.rastreio, p, tempoDoPts(x.tabela, p.pts));
  } else {
    const t = cursorNo(x);
    ancora = ancoraDeSeguir(x.rastreio, t, ptsEm(x.tabela, t), sentido);
  }
  if (!ancora) return null;
  // até a borda do efeito, no último (ou primeiro) quadro dele
  const q = x.tabela ? quadrosDaFaixa(x.tabela, x.de, x.ate, x.ateExclusivo) : null;
  const fim = sentido === "frente" ? (q && x.tabela ? tempoDoQuadro(x.tabela, q[1]) : x.ate) : q && x.tabela ? tempoDoQuadro(x.tabela, q[0]) : x.de;
  if (sentido === "frente" ? ancora.t >= fim - DIF : ancora.t <= fim + DIF) return null;
  return { rastreio_id: x.rastreio.id, clipe_id: x.clipe.id, ancora, sentido, ate_s: fim, motor: "auto" };
}

/** Intermediário de origem do clipe em preparo: "Seguir" desligado nas anotações (§7.5). */
export function seguirParado(doc: ProjetoEdicao, efeitoId: Id): boolean {
  return marcacaoParada(doc, { tipo: "caixa", efeito: efeitoId }, useOrigemStore.getState().porClipe);
}

/** "Seguir" / "Seguir para trás" / "Retomar" do efeito (painel, palco e atalhos). */
export function seguirEfeito(efeitoId: Id, clipeId: Id | null, sentido: Sentido, modo: "seguir" | "retomar" = "seguir", tabela?: TabelaQuadros | null): Promise<void> {
  const doc = edicao().doc;
  const x = doc && !seguirParado(doc, efeitoId) ? alvoDoEfeito(doc, efeitoId, clipeId, tabela) : null;
  const p = x ? pedidoDe(x, sentido, modo, useRastreioStore.getState().paradas) : null;
  return p ? useRastreioStore.getState().seguir(p) : Promise.resolve();
}

/** Marcas dos quadros da faixa do efeito; `j0` = quadro da primeira. */
export interface MarcasDoAlvo {
  m: MarcaQuadro[];
  j0: number;
}

/** Marcas guardadas por tabela e rastreio: refeitas só quando muda o que elas olham (quadro e pts das âncoras,
 *  origem, denso, ausências, faixa), nunca a caixa de uma âncora (cada passo do arrasto de uma alça). */
const marcasGuardadas = new WeakMap<TabelaQuadros, Map<Id, { chave: string; denso: ArquivoDenso | null; mm: MarcasDoAlvo }>>();

function chaveDasMarcas(r: Rastreio, j0: number, j1: number): string {
  const ancoras = r.ancoras.map((k) => `${k.t}/${k.pts ?? ""}`).join(",");
  const ausencias = r.ausencias.map((a) => `${a.de_s}/${a.ate_s}`).join(",");
  return `${j0}|${j1}|${r.origem.tipo}|${ancoras}|${ausencias}`;
}

export function marcasDoAlvo(x: AlvoRastreio): MarcasDoAlvo | null {
  const tab = x.tabela;
  const q = tab ? quadrosDaFaixa(tab, x.de, x.ate, x.ateExclusivo) : null;
  if (!tab || !q) return null;
  const d = densoEmCache(x.rastreio);
  const chave = chaveDasMarcas(x.rastreio, q[0], q[1]);
  let porRastreio = marcasGuardadas.get(tab);
  if (!porRastreio) marcasGuardadas.set(tab, (porRastreio = new Map()));
  const g = porRastreio.get(x.rastreio.id);
  if (g && g.chave === chave && g.denso === d) return g.mm;
  const mm = { m: marcas(x.rastreio, d, tab, q[0], q[1]), j0: q[0] };
  porRastreio.set(x.rastreio.id, { chave, denso: d, mm });
  return mm;
}

/** Há trecho fraco nas marcas? (guardado por marcas: o painel pergunta a cada desenho) */
const comFraco = new WeakMap<MarcasDoAlvo, boolean>();
export function temFraco(mm: MarcasDoAlvo): boolean {
  let v = comFraco.get(mm);
  if (v === undefined) comFraco.set(mm, (v = mm.m.some((k) => FRACAS.has(k))));
  return v;
}

/** Índice do quadro do cursor nas marcas (fora da faixa: -1 antes, o tamanho depois). */
function noCursor(x: AlvoRastreio, mm: MarcasDoAlvo): number {
  if (!x.tabela) return -1;
  return Math.max(-1, Math.min(exibido(x.tabela, cursorNo(x)) - mm.j0, mm.m.length));
}

/** "Próximo trecho fraco": o cursor vai ao primeiro quadro do trecho seguinte. */
export function irAoProximoFraco(x: AlvoRastreio, mm: MarcasDoAlvo | null = marcasDoAlvo(x)): boolean {
  const k = mm ? proximoFraco(mm.m, noCursor(x, mm)) : null;
  if (!mm || k == null || !x.tabela) return false;
  edicao().posicionar({ modo: "clipe", clipe: x.clipe.id, t: tempoDoQuadro(x.tabela, mm.j0 + k) });
  return true;
}

/** Trecho sem o alvo em volta do cursor, em tempo de fonte (os instantes do primeiro e do último quadro). */
export function ausenciaNoCursor(x: AlvoRastreio, mm: MarcasDoAlvo | null = marcasDoAlvo(x)): { de_s: Seg; ate_s: Seg } | null {
  const i = mm ? noCursor(x, mm) : -1;
  if (!mm || !x.tabela || i < 0 || i >= mm.m.length) return null;
  const tr = trechoSemAlvo(mm.m, i);
  return tr ? { de_s: tempoDoQuadro(x.tabela, mm.j0 + tr[0]), ate_s: tempoDoQuadro(x.tabela, mm.j0 + tr[1]) } : null;
}

type Motivo = Rastreio["ausencias"][number]["motivo"];

/** Frase de `perito_ausente` no catálogo (`edicao/mensagens.rs`; o teste confere). */
export const FRASE_PERITO_AUSENTE = "Caso sem perito";

/** Quem registra (ausência aceita, confirmação de trilha revisada): o perito das Configurações. Sem nome, a recusa da
 *  Fase 1 para o perito ausente (`perito_ausente`): nada é gravado e a frase vai para o aviso do app. */
export function peritoQueRegistra(): string | null {
  const nome = useSettingsStore.getState().settings.profile.full_name.trim();
  if (nome) return nome;
  pushToast("error", FRASE_PERITO_AUSENTE);
  return null;
}

/** "Aceitar ausência" → motivo: grava a lacuna com quem aceitou (o perito das Configurações) e quando. */
export function aceitarAusencia(rid: Id, de_s: Seg, ate_s: Seg, motivo: Motivo): void {
  const por = peritoQueRegistra();
  if (!por) return;
  const aceita_em = new Date().toISOString();
  edicao().alterar("Aceitar ausência", (d) => {
    const r = d.rastreios.find((x) => x.id === rid);
    if (r) r.ausencias.push({ de_s, ate_s, motivo, aceita_em, por });
  });
}

/** "Apagar daqui em diante" e "Simplificar": o backend grava o denso novo; o front grava as âncoras quando vêm e o
 *  ponteiro do denso sempre (um passo). */
export async function editarRastreio(rid: Id, op: OpRastreio, rotulo: string): Promise<void> {
  const e = edicao();
  if (!e.doc || !e.ws || e.soLeitura) return;
  let ed: Editado;
  try {
    ed = await commands.edicaoRastreioEditar(e.ws, e.doc, rid, op);
  } catch (x) {
    avisar(x);
    return;
  }
  edicao().alterar(rotulo, (d) => {
    const r = d.rastreios.find((x) => x.id === rid);
    if (!r) return;
    if (ed.ancoras) {
      // as âncoras que o programa escolheu ficam marcadas (proveniência e PDF); mexer nelas à mão tira a marca
      if (op.tipo === "simplificar") {
        const programa = ancorasDoSimplificar(r as Rastreio, ed.ancoras);
        if (programa.length > 0) r.ancoras_do_programa = programa;
        else delete r.ancoras_do_programa;
      }
      r.ancoras = ed.ancoras;
    }
    r.denso = ed.denso;
    if (!ed.denso) r.silhuetas = null;
  });
  if (ed.denso) void lerDenso(rid, ed.denso.sha256);
}

/** "Apagar daqui em diante": do quadro do cursor até o fim. */
export function apagarDaquiEmDiante(x: AlvoRastreio): Promise<void> {
  const pts = ptsEm(x.tabela, cursorNo(x));
  return pts == null ? Promise.resolve() : editarRastreio(x.rastreio.id, { tipo: "apagar_faixa", de_pts: pts, ate_pts: null }, "Apagar daqui em diante");
}

/** Aponta o efeito para outro rastreio. */
function apontar(e: Draft<Efeito>, rid: Id) {
  if (e.tipo === "destaque" || e.tipo === "lupa") e.rastreio = rid;
  else if (e.tipo === "ocultar" || e.tipo === "holofote") e.alvo.rastreio = rid;
  else if (e.tipo === "velocidade_exibida" && e.onde.modo === "segue") e.onde.rastreio = rid;
}

function usado(d: ProjetoEdicao, rid: Id, tipos?: readonly Efeito["tipo"][]): boolean {
  const em = (l: readonly unknown[]) => l.some((e) => efeitoConhecido(e as Efeito) && rastreioDe(e as Efeito) === rid && (!tipos || tipos.includes((e as Efeito).tipo)));
  for (const c of d.principal) if (c && typeof c === "object" && Array.isArray((c as { efeitos?: unknown }).efeitos) && em((c as { efeitos: unknown[] }).efeitos)) return true;
  for (const t of d.sobreposicoes) for (const c of t.clipes) if (Array.isArray(c.efeitos) && em(c.efeitos)) return true;
  return d.midias.some((m) => em(m.ocultacoes));
}

/** "Simplificar": ocultação e holofote pedem a caixa fiel ao alvo; o resto só precisa andar liso (suaviza antes). */
export function opSimplificar(d: ProjetoEdicao, rid: Id): OpRastreio {
  return usado(d, rid, ["ocultar", "holofote"])
    ? { tipo: "simplificar", tolerancia: TOLERANCIA_SIMPLIFICAR }
    : { tipo: "simplificar", tolerancia: TOLERANCIA_SIMPLIFICAR, suavizar_s: SUAVIZAR_ANOTACAO_S };
}

/** Sem denso, o "Simplificar" ainda alisa as âncoras do programa (o de antes deixou uma por quadro) quando suaviza. */
export function podeSimplificar(d: ProjetoEdicao, r: Rastreio): boolean {
  const op = opSimplificar(d, r.id);
  return !!r.denso || (op.tipo === "simplificar" && op.suavizar_s !== undefined && (r.ancoras_do_programa?.length ?? 0) >= 3);
}

/** "Da análise" e "Atualizar": a trilha da pessoa vira rastreio do projeto (o backend copia o denso) e o efeito passa
 *  a usá-lo; o rastreio anterior sai se ninguém mais o usa. Um passo. */
export async function trazerDaAnalise(efeitoId: Id, midiaId: Id, sha256: string, pessoaId: string, rotulo: string): Promise<void> {
  const e = edicao();
  if (!e.doc || !e.ws || !e.aberta || e.soLeitura) return;
  let novo: Rastreio;
  try {
    novo = await commands.edicaoRastreioDePessoas(e.ws, e.aberta.projetoId, midiaId, sha256, pessoaId);
  } catch (x) {
    if (separarErro(x).codigo !== "cancelado") avisar(x);
    return;
  }
  // trilha revisada entra confirmada: quem confirma é o perito das Configurações (a análise não guarda quem revisou)
  if (novo.confirmacao) {
    const por = peritoQueRegistra();
    if (!por) return;
    novo = { ...novo, confirmacao: { ...novo.confirmacao, por } };
  }
  edicao().alterar(rotulo, (d) => {
    const a = acharEfeito(d as ProjetoEdicao, efeitoId);
    if (!a) return;
    const antigo = rastreioDe(a.efeito);
    d.rastreios = d.rastreios.filter((r) => r.id !== novo.id);
    d.rastreios.push(novo);
    apontar(a.efeito as Draft<Efeito>, novo.id);
    if (antigo && antigo !== novo.id && !usado(d as ProjetoEdicao, antigo)) d.rastreios = d.rastreios.filter((r) => r.id !== antigo);
  });
  if (novo.denso) void lerDenso(novo.id, novo.denso.sha256);
}

/** Aviso `retrato_desatualizado` do rastreio (trilha da análise que mudou depois da cópia, §6.4). */
export function desatualizado(r: Rastreio, efeitoId: Id, problemas: readonly { codigo: string; efeito: Id | null; campo: string | null }[]): boolean {
  if (r.origem.tipo !== "pessoas") return false;
  return problemas.some((p) => p.codigo === "retrato_desatualizado" && (p.efeito === r.id || p.campo === r.id || (p.efeito === efeitoId && p.campo === "rastreio")));
}

// ---- atalhos (§8.5) ----

/** Efeito aberto (ou o da ferramenta de marcar) e o clipe do cursor. */
function alvoSelecionado(): AlvoRastreio | null {
  const e = edicao();
  const ef = e.selecao.efeito ?? useInspetorUi.getState().ferramenta?.efeito ?? null;
  if (!e.doc || !ef) return null;
  const clipe = e.cursor.modo === "clipe" ? e.cursor.clipe : e.selecao.foco?.k === "clipe" ? e.selecao.foco.id : null;
  return alvoDoEfeito(e.doc, ef, clipe);
}

/** Atalhos da Edição que o rastreio atende (`edicao.seguirFrente`, `edicao.seguirTras`, `edicao.trechoFraco`);
 *  vazio = seguem com o que o editor já faz. */
export function atalhosDoRastreio(): Record<string, () => void> {
  if (!RASTREIO_PRONTO) return {};
  const seguir = (s: Sentido) => () => {
    const x = alvoSelecionado();
    if (x) void seguirEfeito(x.efeito.id, x.clipe.id, s);
  };
  return {
    "edicao.seguirFrente": seguir("frente"),
    "edicao.seguirTras": seguir("tras"),
    "edicao.trechoFraco": () => {
      const x = alvoSelecionado();
      if (x) irAoProximoFraco(x);
    },
  };
}

/** Esc ("Parar") com corrida em andamento: para e devolve true. */
export function pararCorrida(): boolean {
  if (!RASTREIO_PRONTO) return false;
  if (useRastreioStore.getState().corrida === null) return false;
  void useRastreioStore.getState().parar();
  return true;
}
