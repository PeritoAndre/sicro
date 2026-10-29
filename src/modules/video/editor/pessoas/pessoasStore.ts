/** Estado da aba Pessoas: trilhas do vídeo aberto, seleção, opções de desenho e tarefa em andamento. */

import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type { EdicaoPessoa, OpcoesDesenho, PessoasEstado, PessoasProgresso } from "@domain/video_pessoas";
import type { Cadencia } from "./pessoasMath";

const PREFS_KEY = "sicro.video.pessoas.prefs.v1";

interface Prefs {
  desenho: OpcoesDesenho;
  limiar: number;
  passo: number;
  mostrar: boolean;
}

const PADRAO: Prefs = { desenho: { modo: "completo", fonte: 1 }, limiar: 0.5, passo: 1, mostrar: true };

function lerPrefs(): Prefs {
  try {
    const raw = localStorage.getItem(PREFS_KEY);
    if (raw) return { ...PADRAO, ...(JSON.parse(raw) as Partial<Prefs>) };
  } catch {
    /* sem preferências guardadas */
  }
  return PADRAO;
}

function gravarPrefs(p: Prefs) {
  try {
    localStorage.setItem(PREFS_KEY, JSON.stringify(p));
  } catch {
    /* só não lembra */
  }
}

export interface Tarefa {
  fase: "rastrear" | "exportar";
  feitos: number;
  total: number;
  pessoas: number;
}

interface PessoasState extends Prefs {
  mediaHash: string | null;
  estado: PessoasEstado | null;
  selecionada: string | null;
  /** Pessoa que vai receber outra ao clicar ("juntar com…"). */
  juntando: string | null;
  tarefa: Tarefa | null;
  erro: string | null;

  carregar: (ws: string, mediaHash: string) => Promise<void>;
  rastrear: (ws: string, mediaHash: string) => Promise<void>;
  cancelar: () => void;
  editar: (ws: string, mediaHash: string, e: EdicaoPessoa) => Promise<void>;
  selecionar: (id: string | null) => void;
  setJuntando: (id: string | null) => void;
  setMostrar: (v: boolean) => void;
  setDesenho: (d: Partial<OpcoesDesenho>) => void;
  setLimiar: (v: number) => void;
  setPasso: (v: number) => void;
  setTarefa: (t: Tarefa | null) => void;
  setErro: (e: string | null) => void;
}

let ouvindo = false;

/** Progresso das tarefas longas (rastrear / gravar o vídeo), vindo do Rust. */
function ouvirProgresso() {
  if (ouvindo) return;
  ouvindo = true;
  void listen<PessoasProgresso>("pessoas-progress", (e) => {
    const p = e.payload;
    usePessoasStore.setState((s) =>
      s.tarefa ? { tarefa: { fase: p.fase, feitos: p.feitos, total: p.total, pessoas: p.pessoas } } : {},
    );
  });
}

export const usePessoasStore = create<PessoasState>((set, get) => {
  const salvar = () => {
    const s = get();
    gravarPrefs({ desenho: s.desenho, limiar: s.limiar, passo: s.passo, mostrar: s.mostrar });
  };
  return {
    ...lerPrefs(),
    mediaHash: null,
    estado: null,
    selecionada: null,
    juntando: null,
    tarefa: null,
    erro: null,

    async carregar(ws, mediaHash) {
      ouvirProgresso();
      if (get().mediaHash !== mediaHash) {
        set({ mediaHash, estado: null, selecionada: null, juntando: null, erro: null });
      }
      try {
        const estado = await commands.pessoasListar(ws, mediaHash);
        if (get().mediaHash === mediaHash) set({ estado });
      } catch (e) {
        set({ erro: toSicroError(e).message });
      }
    },

    async rastrear(ws, mediaHash) {
      const { limiar, passo } = get();
      set({ erro: null, tarefa: { fase: "rastrear", feitos: 0, total: 0, pessoas: 0 } });
      try {
        const estado = await commands.pessoasRastrear(ws, mediaHash, { limiar, passo });
        if (get().mediaHash === mediaHash) set({ estado, selecionada: null, juntando: null });
      } catch (e) {
        const msg = toSicroError(e).message;
        if (!/cancelado/.test(msg)) set({ erro: msg });
      } finally {
        set({ tarefa: null });
      }
    },

    cancelar() {
      void commands.pessoasCancelar();
    },

    async editar(ws, mediaHash, e) {
      try {
        const estado = await commands.pessoasEditar(ws, mediaHash, e);
        if (get().mediaHash !== mediaHash) return;
        const sel = get().selecionada;
        set({
          estado,
          erro: null,
          selecionada: sel && estado.pessoas.some((p) => p.id === sel) ? sel : null,
        });
      } catch (err) {
        set({ erro: toSicroError(err).message });
      }
    },

    selecionar: (id) => set({ selecionada: id }),
    setJuntando: (id) => set({ juntando: id }),
    setMostrar: (mostrar) => {
      set({ mostrar });
      salvar();
    },
    setDesenho: (d) => {
      set((s) => ({ desenho: { ...s.desenho, ...d } }));
      salvar();
    },
    setLimiar: (limiar) => {
      set({ limiar });
      salvar();
    },
    setPasso: (passo) => {
      set({ passo });
      salvar();
    },
    setTarefa: (tarefa) => set({ tarefa }),
    setErro: (erro) => set({ erro }),
  };
});

export function cadenciaDe(estado: PessoasEstado | null): Cadencia | null {
  const r = estado?.run;
  return r ? { frameDur: r.frame_dur_s, passo: Math.max(1, r.step) } : null;
}
