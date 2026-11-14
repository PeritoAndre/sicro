/**
 * Store da Edição (§8.4): documento aberto, histórico, seleção, cursor, salvamento e análise.
 * `alterar` é a única porta de mudança do documento (immer; congelado em desenvolvimento e nos testes).
 * O que o backend calcula e pertence ao projeto (`Analise.prender`, `saida_auto`) também entra por aqui,
 * sem passo de histórico.
 */

import { create } from "zustand";
import { Immer, freeze, type Draft } from "immer";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type { Analise, Cursor, Id, MidiaRef, Problema, ProjetoAberto, ProjetoEdicao, Selecao } from "@domain/edicao";
import { Autosave, type EstadoSalvo } from "./autosave";
import { desfazer as voltar, refazer as avancar, registrar, type Passo, type Retrato } from "./historico";
import { animaveisVazios, aplicarPrender, aplicarSaidaAuto, comoChegou, completar, garantirMidia, novoProjeto, novoTrecho } from "../modelo/projeto";

/** Documento congelado em desenvolvimento e nos testes: atribuição fora do `alterar` vira exceção. */
const CONGELAR = import.meta.env.DEV || import.meta.env.MODE === "test";
const immer = new Immer({ autoFreeze: CONGELAR });

/** Análise pedida este tempo depois da última mudança. */
export const ANALISE_MS = 250;

export const SELECAO_VAZIA: Selecao = Object.freeze({ itens: [], foco: null, efeito: null }) as Selecao;
export const CURSOR_INICIAL: Cursor = Object.freeze({ modo: "programa", q: 0 }) as Cursor;

/** Erro de comando: `Validation("<código>: <frase>")` chega como "validation error: <código>: <frase>" (contratos (f) 10). */
export interface ErroComando {
  codigo: string | null;
  frase: string;
}

export function separarErro(e: unknown): ErroComando {
  const se = toSicroError(e);
  const msg = se.message.replace(/^(workspace|database|filesystem|validation|i\/o|serde|sqlite) error: /, "");
  if (se.kind === "validation") {
    const m = /^([a-z][a-z0-9_]*)(?:: ([\s\S]*))?$/.exec(msg);
    if (m) return { codigo: m[1] as string, frase: m[2] ?? "" };
  }
  return { codigo: null, frase: msg };
}

export interface Gesto {
  alterar(f: (d: Draft<ProjetoEdicao>) => void): void;
  concluir(): void;
  cancelar(): void;
}

// CONTRATO: `op.agrupar` (chave do agrupamento por tempo) é acréscimo ao §8.4.
export interface OpcoesAlterar {
  /** Tecla repetida e campo numérico: mesma chave em menos de 1 s = um passo. */
  agrupar?: string;
}

export interface EdicaoState {
  aberta: { projetoId: Id } | null;
  corrente: Id | null;
  doc: ProjetoEdicao | null;
  soLeitura: boolean;
  revisao: number;
  revisaoSalva: number;
  salvando: EstadoSalvo;
  analise: Analise | null;
  selecao: Selecao;
  cursor: Cursor;
  passado: Passo[];
  futuro: Passo[];
  // CONTRATO: campos a mais em relação ao §8.4.
  /** Caso do projeto aberto. */
  ws: string | null;
  /** `ProjetoAberto.raiz_cache`: todo `relativo` de resposta é resolvido a partir dela. */
  raizCache: string | null;
  importadoDe: number | null;
  /** Avisos da abertura. */
  avisos: Problema[];
  /** Revisão do documento que a `analise` descreve. */
  analiseRevisao: number;
  erro: ErroComando | null;

  alterar(rotulo: string, f: (d: Draft<ProjetoEdicao>) => void, op?: OpcoesAlterar): void;
  iniciarGesto(rotulo: string): Gesto;
  desfazer(): void;
  refazer(): void;
  selecionar(s: Selecao): void;
  posicionar(c: Cursor): void;
  novo(ws: string, midia?: MidiaRef): Promise<Id>;
  abrir(ws: string, id: Id): Promise<void>;
  /** Botão "Edição" do trilho: abre a edição corrente do caso; cria uma se não houver ou se ela sumiu. */
  abrirCorrente(ws: string): Promise<Id>;
  // CONTRATO: `op.descartar` é acréscimo ao §8.4. Sem ele, com a gravação falhando (disco cheio, pasta só de
  // leitura), `fechar` rejeita sempre e não há como sair do projeto nem abrir outro.
  /** Grava o pendente e fecha; rejeita (e fica aberto) se a gravação falhar. `descartar`: fecha sem gravar. */
  fechar(op?: { descartar?: boolean }): Promise<void>;
  excluir(ws: string, id: Id): Promise<void>;
  /** Grava já (Ctrl+S). */
  salvar(): Promise<void>;
  /** Espera a gravação pendente: antes de exportar, fechar, trocar de caso e fechar a janela. Rejeita se falhar. */
  descarregar(): Promise<void>;
}

// ---- edição corrente do caso (a última aberta), como em video/editor/resume.ts ----

const CHAVE_CORRENTE = "sicro.edicao.corrente.v1";
const correntes = new Map<string, Id | null>();

export function correnteDe(ws: string): Id | null {
  if (correntes.has(ws)) return correntes.get(ws) ?? null;
  try {
    const v = (JSON.parse(localStorage.getItem(CHAVE_CORRENTE) ?? "{}") as Record<string, unknown>)[ws];
    return typeof v === "string" ? v : null;
  } catch {
    return null;
  }
}

export function definirCorrente(ws: string, id: Id | null): void {
  correntes.set(ws, id);
  try {
    const o = JSON.parse(localStorage.getItem(CHAVE_CORRENTE) ?? "{}") as Record<string, unknown>;
    if (id) o[ws] = id;
    else delete o[ws];
    localStorage.setItem(CHAVE_CORRENTE, JSON.stringify(o));
  } catch {
    /* localStorage indisponível: vale só nesta sessão */
  }
  const s = useEdicaoStore.getState();
  if (s.ws === ws || s.ws === null) useEdicaoStore.setState({ corrente: id });
}

// ---- abrir, criar, fechar, excluir e a coleta com a tela fechada: um de cada vez ----

let vez: Promise<unknown> = Promise.resolve();
// CONTRATO: acréscimo ao §8.4 (a coleta usa; quem mais ler e gravar um projeto fora da store também deve usar).
/** Corre `f` depois do que já está na fila. Sem isso, um `abrir` lento montava o documento lido ANTES de uma
 *  coleta gravar (a gravação seguinte apagava o trecho) ou por cima de um projeto novo com mudança pendente. */
export function emSerie<T>(f: () => Promise<T>): Promise<T> {
  const p = vez.then(f);
  vez = p.catch(() => {});
  return p;
}

// ---- estado de execução do projeto aberto (fora do React) ----

interface Aberto {
  ws: string;
  id: Id;
  autosave: Autosave;
  timerAnalise: ReturnType<typeof setTimeout> | null;
  /** Último pedido de análise: a resposta dele vence. */
  seq: number;
  gesto: Gesto | null;
  /** Já existe no disco (projeto novo só é gravado depois da primeira mudança). */
  gravado: boolean;
  /** Arquivo sem campos de padrão: como chegou e como ficou completo (`comoChegou` ao gravar); null = veio completo. */
  incompleto: { original: ProjetoEdicao; completado: ProjetoEdicao } | null;
  /** `aprenderRaiz` já foi chamado (uma vez por projeto: `edicao_abrir` regrava o `projeto.anterior.json`). */
  raizPedida: boolean;
}
let rt: Aberto | null = null;
/** `raiz_cache` por caso (é a mesma para todo projeto do caso). */
const raizes = new Map<string, string>();

const fechado = () => ({
  aberta: null,
  doc: null,
  soLeitura: false,
  revisao: 0,
  revisaoSalva: 0,
  salvando: "salvo" as EstadoSalvo,
  analise: null,
  analiseRevisao: -1,
  selecao: SELECAO_VAZIA,
  cursor: CURSOR_INICIAL,
  passado: [] as Passo[],
  futuro: [] as Passo[],
  ws: null,
  raizCache: null,
  importadoDe: null,
  avisos: [] as Problema[],
  erro: null,
});

/** Aplica `f` a uma cópia; o documento devolvido é congelado (dev/teste). Mudança que deixa lista de chaves
 *  vazia é recusada (exceção em desenvolvimento): tirar a última chave vira valor fixo (`tirarChave`). */
function produzir(doc: ProjetoEdicao, f: (d: Draft<ProjetoEdicao>) => void): ProjetoEdicao {
  const novo = immer.produce(doc, (d) => {
    f(d);
  });
  if (novo === doc) return doc;
  const antes = new Set(animaveisVazios(doc));
  const vazios = animaveisVazios(novo).filter((c) => !antes.has(c));
  if (vazios.length === 0) return novo;
  if (CONGELAR) throw new Error(`lista de chaves vazia: ${vazios.join(", ")}`);
  return doc;
}

export const useEdicaoStore = create<EdicaoState>((set, get) => {
  const retrato = (): Retrato => {
    const s = get();
    return { doc: s.doc as ProjetoEdicao, selecao: s.selecao, cursor: s.cursor };
  };

  const agendarAnalise = (meu: Aberto, ms = ANALISE_MS) => {
    if (meu.timerAnalise !== null) clearTimeout(meu.timerAnalise);
    meu.timerAnalise = setTimeout(() => {
      meu.timerAnalise = null;
      void pedirAnalise(meu);
    }, ms);
  };

  const mudou = (meu: Aberto) => {
    meu.autosave.mudou();
    agendarAnalise(meu);
  };

  const pedirAnalise = async (meu: Aberto) => {
    const s = get();
    if (rt !== meu || !s.doc) return;
    const pedido = ++meu.seq;
    const rev = s.revisao;
    try {
      const a = await commands.edicaoAnalisar(meu.ws, s.doc);
      if (rt !== meu || pedido !== meu.seq) return;
      set({ analise: a, analiseRevisao: rev });
      if (rev === get().revisao) aplicarAnalise(meu, a);
    } catch (e) {
      if (rt === meu && pedido === meu.seq) set({ erro: separarErro(e) });
    }
  };

  /** Tempos presos a quadro e saída automática: dado do backend, sem passo de histórico. */
  const aplicarAnalise = (meu: Aberto, a: Analise) => {
    const s = get();
    if (!s.doc || s.soLeitura) return;
    let mexeu = false;
    const doc = produzir(s.doc, (d) => {
      if (aplicarSaidaAuto(d, a.saida_auto)) mexeu = true;
      for (const p of a.prender) if (aplicarPrender(d, p)) mexeu = true;
    });
    if (!mexeu || doc === s.doc) return;
    set({ doc, revisao: s.revisao + 1 });
    mudou(meu);
  };

  const aprenderRaiz = (meu: Aberto) => {
    // projeto novo: a raiz do cache só vem de `edicao_abrir`
    meu.raizPedida = true;
    commands
      .edicaoAbrir(meu.ws, meu.id)
      .then((r) => {
        if (r.raiz_cache) raizes.set(meu.ws, r.raiz_cache);
        if (rt === meu) set({ raizCache: r.raiz_cache || null });
      })
      .catch(() => {});
  };

  /** `id` = o da pasta (o pedido). Arquivo com outro `id` dentro abre só para leitura e continua sendo fechado e excluído por este. */
  const montar = (ws: string, id: Id, chegou: ProjetoEdicao, x: { soLeitura: boolean; gravado: boolean; importadoDe: number | null; avisos: Problema[] }) => {
    const completo = completar(chegou);
    const doc = CONGELAR ? freeze(completo, true) : completo;
    // a fila de vez garante que o anterior já foi fechado (e gravado)
    if (rt) encerrar(rt);
    const meu: Aberto = {
      ws,
      id,
      timerAnalise: null,
      seq: 0,
      gesto: null,
      gravado: x.gravado,
      raizPedida: false,
      incompleto: completo === chegou ? null : { original: chegou, completado: doc },
      autosave: new Autosave({
        revisao: () => (rt === meu ? get().revisao : 0),
        salva: 0,
        gravar: async () => {
          if (rt !== meu) throw new Error("projeto fechado");
          const s = get();
          const rev = s.revisao;
          const doc = s.doc as ProjetoEdicao;
          await commands.edicaoSalvar(meu.ws, meu.incompleto ? comoChegou(doc, meu.incompleto.completado, meu.incompleto.original) : doc);
          meu.gravado = true;
          if (!raizes.has(meu.ws) && !meu.raizPedida) aprenderRaiz(meu);
          return rev;
        },
        estado: (e) => rt === meu && set({ salvando: e }),
        gravou: (r) => rt === meu && set((s) => ({ revisaoSalva: Math.max(s.revisaoSalva, r), erro: null })),
        falhou: (e) => rt === meu && set({ erro: separarErro(e) }),
      }),
    };
    rt = meu;
    set({
      ...fechado(),
      aberta: { projetoId: id },
      ws,
      doc,
      soLeitura: x.soLeitura,
      raizCache: raizes.get(ws) ?? null,
      importadoDe: x.importadoDe,
      avisos: x.avisos,
      corrente: correnteDe(ws),
    });
    agendarAnalise(meu, 0);
    return meu;
  };

  const encerrar = (meu: Aberto) => {
    meu.autosave.parar();
    if (meu.timerAnalise !== null) clearTimeout(meu.timerAnalise);
    meu.seq++;
    if (rt === meu) {
      rt = null;
      set({ ...fechado(), corrente: get().corrente });
    }
  };

  // ---- as ações de ciclo de vida, sem a fila (as públicas passam por `emSerie`) ----

  const fecharJa = async (descartar = false) => {
    const meu = rt;
    if (!meu) return;
    meu.gesto?.concluir();
    if (descartar) {
      // a gravação em curso termina antes: a pasta não muda depois de o projeto sair da tela
      await meu.autosave.cancelar();
      if (rt === meu) encerrar(meu);
      return;
    }
    // mudança que chegue entre o fim da gravação e o fechamento também vai para o disco
    for (let i = 0; i < 3; i++) {
      await meu.autosave.descarregar();
      if (rt !== meu || !meu.autosave.pendente) break;
    }
    if (rt === meu) encerrar(meu);
  };

  const novoJa = async (ws: string, midia?: MidiaRef): Promise<Id> => {
    await fecharJa();
    // vídeo inteiro: a faixa é a da tabela de quadros; a duração do contêiner pode acabar antes do último quadro
    let faixa: [number, number] | null = null;
    if (midia?.tipo === "video") {
      if (midia.duracao_s != null) faixa = [midia.inicio_s ?? 0, (midia.inicio_s ?? 0) + midia.duracao_s];
      try {
        const info = await commands.edicaoMidiaInfo(ws, midia.sha256);
        if (info && info.ate_s > info.de_s) faixa = [info.de_s, info.ate_s];
      } catch {
        // sem tabela ainda: vale a duração do retrato
      }
    }
    const doc = novoProjeto();
    montar(ws, doc.id, doc, { soLeitura: false, gravado: false, importadoDe: null, avisos: [] });
    definirCorrente(ws, doc.id);
    if (midia) {
      get().alterar("Trecho", (d) => {
        const id = garantirMidia(d, midia);
        if (faixa) d.principal.push(novoTrecho(id, faixa[0], faixa[1]));
      });
    }
    return doc.id;
  };

  const abrirJa = async (ws: string, id: Id): Promise<void> => {
    if (rt && rt.ws === ws && rt.id === id) return;
    await fecharJa();
    let r: ProjetoAberto;
    try {
      r = await commands.edicaoAbrir(ws, id);
    } catch (e) {
      set({ erro: separarErro(e) });
      throw e;
    }
    if (r.raiz_cache) raizes.set(ws, r.raiz_cache);
    montar(ws, id, r.projeto, { soLeitura: r.so_leitura, gravado: true, importadoDe: r.importado_de, avisos: r.avisos });
    definirCorrente(ws, id);
  };

  return {
    ...fechado(),
    corrente: null,

    alterar(rotulo, f, op) {
      const meu = rt;
      const s = get();
      if (!meu || !s.doc || s.soLeitura) return;
      meu.gesto?.concluir();
      const antes = retrato();
      const doc = produzir(antes.doc, f);
      if (doc === antes.doc) return;
      const h = registrar({ passado: get().passado, futuro: get().futuro }, rotulo, antes, Date.now(), op?.agrupar ?? null);
      set({ doc, revisao: get().revisao + 1, passado: h.passado, futuro: h.futuro });
      mudou(meu);
    },

    iniciarGesto(rotulo) {
      const meu = rt;
      meu?.gesto?.concluir();
      const s = get();
      const nulo: Gesto = { alterar() {}, concluir() {}, cancelar() {} };
      if (!meu || !s.doc || s.soLeitura) return nulo;
      const base = retrato();
      let vivo = true;
      // só o que o gesto mudou conta: a análise pode prender um tempo no meio de um clique
      let mexeu = false;
      const g: Gesto = {
        alterar(f) {
          const d = get().doc;
          if (!vivo || rt !== meu || !d) return;
          const doc = produzir(d, f);
          if (doc === d) return;
          mexeu = true;
          set({ doc, revisao: get().revisao + 1 });
          mudou(meu);
        },
        concluir() {
          if (!vivo) return;
          vivo = false;
          if (meu.gesto === g) meu.gesto = null;
          if (rt !== meu || !mexeu || get().doc === base.doc) return;
          const h = registrar({ passado: get().passado, futuro: get().futuro }, rotulo, base, Date.now());
          set({ passado: h.passado, futuro: h.futuro });
        },
        cancelar() {
          if (!vivo) return;
          vivo = false;
          if (meu.gesto === g) meu.gesto = null;
          if (rt !== meu || !mexeu || get().doc === base.doc) return;
          set({ doc: base.doc, selecao: base.selecao, cursor: base.cursor, revisao: get().revisao + 1 });
          mudou(meu);
        },
      };
      meu.gesto = g;
      return g;
    },

    desfazer() {
      const meu = rt;
      if (!meu || !get().doc || get().soLeitura) return;
      if (meu.gesto) {
        meu.gesto.cancelar();
        return;
      }
      const s = get();
      const r = voltar({ passado: s.passado, futuro: s.futuro }, retrato(), Date.now());
      if (!r) return;
      set({ doc: r.alvo.doc, selecao: r.alvo.selecao, cursor: r.alvo.cursor, passado: r.h.passado, futuro: r.h.futuro, revisao: s.revisao + 1 });
      mudou(meu);
    },

    refazer() {
      const meu = rt;
      if (!meu || !get().doc || get().soLeitura) return;
      meu.gesto?.concluir();
      const s = get();
      const r = avancar({ passado: s.passado, futuro: s.futuro }, retrato(), Date.now());
      if (!r) return;
      set({ doc: r.alvo.doc, selecao: r.alvo.selecao, cursor: r.alvo.cursor, passado: r.h.passado, futuro: r.h.futuro, revisao: s.revisao + 1 });
      mudou(meu);
    },

    selecionar: (selecao) => set({ selecao }),
    posicionar: (cursor) => set({ cursor }),

    novo: (ws, midia) => emSerie(() => novoJa(ws, midia)),
    abrir: (ws, id) => emSerie(() => abrirJa(ws, id)),

    abrirCorrente: (ws) =>
      emSerie(async () => {
        const id = correnteDe(ws);
        if (id) {
          try {
            await abrirJa(ws, id);
            return id;
          } catch (e) {
            // só o projeto que sumiu vira edição nova; o que não abre fica com o erro na tela
            if (separarErro(e).codigo !== "projeto_nao_encontrado") throw e;
          }
        }
        return novoJa(ws);
      }),

    fechar: (op) => emSerie(() => fecharJa(op?.descartar === true)),

    excluir: (ws, id) =>
      emSerie(async () => {
        const meu = rt && rt.ws === ws && rt.id === id ? rt : null;
        if (meu) {
          meu.gesto?.cancelar();
          // um autosave atrasado recriaria a pasta (contratos (f) 13)
          await meu.autosave.cancelar();
          if (meu.timerAnalise !== null) clearTimeout(meu.timerAnalise);
        }
        if (!meu || meu.gravado) {
          try {
            await commands.edicaoExcluir(ws, id);
          } catch (e) {
            if (meu && rt === meu) {
              meu.autosave.retomar();
              agendarAnalise(meu);
              set({ erro: separarErro(e) });
            }
            throw e;
          }
        }
        if (meu) encerrar(meu);
        if (correnteDe(ws) === id) definirCorrente(ws, null);
      }),

    async salvar() {
      await rt?.autosave.descarregar();
    },

    async descarregar() {
      await rt?.autosave.descarregar();
    },
  };
});
