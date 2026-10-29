/** Estado da aba Realce: quadro de trabalho, região marcada, resultado em comparação e tarefa em andamento. */

import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type {
  ArquivoRealce,
  PerfilLente,
  PreviaLente,
  QuadroTrabalho,
  RealceProgresso,
  Roi,
} from "@domain/video_realce";
import type { Pt } from "../medir/medirStore";

export type Aba = "media" | "superres" | "estab" | "lente" | "perspectiva";

/** Duas imagens lado a lado (original × realce) com zoom e arraste em conjunto. */
export interface Comparacao {
  esquerda: { src: string; rotulo: string };
  direita: { src: string; rotulo: string };
  /** Linha curta sob as imagens (quantos quadros entraram, tempo…). */
  nota: string;
  aviso?: string;
}

export interface Tarefa {
  fase: RealceProgresso["fase"] | "trabalho";
  feitos: number;
  total: number;
}

export const LENTE_PADRAO: PerfilLente = { nome: "", k1: 0, k2: 0, cx: 0.5, cy: 0.5 };

export const assetSrc = (absoluto: string): string => convertFileSrc(absoluto);

interface RealceState {
  mediaHash: string | null;
  aba: Aba;
  quadro: QuadroTrabalho | null;
  /** Cliques da região (0 a 2) e a região pronta. */
  cantos: Pt[];
  roi: Roi | null;
  comparacao: Comparacao | null;
  /** "resultado" mostra a comparação; "quadro" o quadro de trabalho. */
  vista: "quadro" | "resultado";
  resultados: ArquivoRealce[];
  /** Última média: a super-resolução pode partir dela. */
  ultimaMedia: ArquivoRealce | null;
  tarefa: Tarefa | null;
  erro: string | null;
  msg: string | null;
  /** O vídeo tem perfil de lente salvo; `usarLente` aplica esse perfil ao quadro e ao realce. */
  temPerfil: boolean;
  usarLente: boolean;
  lente: PerfilLente;
  lentePrevia: PreviaLente | null;
  grade: boolean;
  persp: { pts: Pt[]; largura: string; profundidade: string; margem: number; calId: string | null };

  bind: (hash: string) => void;
  setAba: (a: Aba) => void;
  setQuadro: (q: QuadroTrabalho | null) => void;
  addCanto: (p: Pt) => void;
  limparRegiao: () => void;
  setComparacao: (c: Comparacao | null, vista?: "quadro" | "resultado") => void;
  setVista: (v: "quadro" | "resultado") => void;
  setResultados: (r: ArquivoRealce[]) => void;
  setUltimaMedia: (r: ArquivoRealce | null) => void;
  setTarefa: (t: Tarefa | null) => void;
  setErro: (e: string | null) => void;
  setMsg: (e: string | null) => void;
  setTemPerfil: (v: boolean) => void;
  setUsarLente: (v: boolean) => void;
  setLente: (p: Partial<PerfilLente>) => void;
  setLenteCompleta: (p: PerfilLente) => void;
  setLentePrevia: (p: PreviaLente | null) => void;
  setGrade: (g: boolean) => void;
  patchPersp: (p: Partial<RealceState["persp"]>) => void;
  /** Roda uma tarefa longa mostrando progresso e o erro (cancelamento não é erro). */
  executar: <T>(fase: Tarefa["fase"], fn: () => Promise<T>) => Promise<T | null>;
  cancelar: () => void;
  recarregar: (ws: string, hash: string) => Promise<void>;
}

const PERSP_VAZIA = { pts: [] as Pt[], largura: "", profundidade: "", margem: 0.25, calId: null as string | null };

let ouvindo = false;
function ouvirProgresso() {
  if (ouvindo) return;
  ouvindo = true;
  void listen<RealceProgresso>("realce-progress", (e) => {
    const p = e.payload;
    useRealceStore.setState((s) =>
      s.tarefa ? { tarefa: { fase: p.fase, feitos: p.feitos, total: p.total } } : {},
    );
  });
}

export const useRealceStore = create<RealceState>((set, get) => ({
  mediaHash: null,
  aba: "media",
  quadro: null,
  cantos: [],
  roi: null,
  comparacao: null,
  vista: "quadro",
  resultados: [],
  ultimaMedia: null,
  tarefa: null,
  erro: null,
  msg: null,
  temPerfil: false,
  usarLente: false,
  lente: LENTE_PADRAO,
  lentePrevia: null,
  grade: true,
  persp: PERSP_VAZIA,

  bind(hash) {
    if (get().mediaHash === hash) return;
    ouvirProgresso();
    set({
      mediaHash: hash,
      quadro: null,
      cantos: [],
      roi: null,
      comparacao: null,
      vista: "quadro",
      resultados: [],
      ultimaMedia: null,
      tarefa: null,
      erro: null,
      msg: null,
      temPerfil: false,
      usarLente: false,
      lente: LENTE_PADRAO,
      lentePrevia: null,
      persp: PERSP_VAZIA,
    });
  },
  setAba: (aba) => set({ aba, erro: null, msg: null, vista: get().comparacao && aba !== "lente" ? get().vista : "quadro" }),
  setQuadro: (quadro) =>
    set({ quadro, cantos: [], roi: null, comparacao: null, vista: "quadro", lentePrevia: null, persp: { ...get().persp, pts: [], calId: null } }),
  addCanto(p) {
    const c = [...get().cantos, p];
    if (c.length < 2) return set({ cantos: c, roi: null });
    const [a, b] = c as [Pt, Pt];
    const x = Math.round(Math.min(a.x, b.x));
    const y = Math.round(Math.min(a.y, b.y));
    const w = Math.round(Math.abs(a.x - b.x));
    const h = Math.round(Math.abs(a.y - b.y));
    set({ cantos: c, roi: w >= 16 && h >= 16 ? { x, y, w, h } : null, erro: w >= 16 && h >= 16 ? null : "A região é pequena demais: marque de novo." });
    if (w < 16 || h < 16) set({ cantos: [], roi: null });
  },
  limparRegiao: () => set({ cantos: [], roi: null }),
  setComparacao: (comparacao, vista) => set({ comparacao, vista: vista ?? (comparacao ? "resultado" : "quadro") }),
  setVista: (vista) => set({ vista }),
  setResultados: (resultados) => set({ resultados }),
  setUltimaMedia: (ultimaMedia) => set({ ultimaMedia }),
  setTarefa: (tarefa) => set({ tarefa }),
  setErro: (erro) => set({ erro }),
  setMsg: (msg) => set({ msg }),
  setTemPerfil: (temPerfil) => set({ temPerfil, usarLente: temPerfil ? get().usarLente : false }),
  setUsarLente: (usarLente) => set({ usarLente }),
  setLente: (p) => set({ lente: { ...get().lente, ...p } }),
  setLenteCompleta: (lente) => set({ lente }),
  setLentePrevia: (lentePrevia) => set({ lentePrevia }),
  setGrade: (grade) => set({ grade }),
  patchPersp: (p) => set({ persp: { ...get().persp, ...p } }),
  async executar(fase, fn) {
    set({ tarefa: { fase, feitos: 0, total: 0 }, erro: null, msg: null });
    try {
      return await fn();
    } catch (e) {
      const m = toSicroError(e).message;
      if (!/cancelado/.test(m)) set({ erro: m });
      return null;
    } finally {
      set({ tarefa: null });
    }
  },
  cancelar() {
    void commands.realceCancelar().catch(() => undefined);
  },
  async recarregar(ws, hash) {
    try {
      const r = await commands.realceListar(ws, hash);
      if (get().mediaHash === hash) set({ resultados: r });
    } catch {
      /* a lista é só apoio */
    }
  },
}));
