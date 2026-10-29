/** Estado da aba Rastreio: trilhas do vídeo aberto, seleção, classes a detectar, opções de desenho e tarefa em andamento. */

import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type {
  ClasseInfo,
  EdicaoPessoa,
  MarcaMancha,
  OpcoesDesenho,
  PessoasEstado,
  PessoasProgresso,
  ResultadoMancha,
} from "@domain/video_pessoas";
import type { Cadencia } from "./pessoasMath";

const PREFS_KEY = "sicro.video.pessoas.prefs.v1";

interface Prefs {
  desenho: OpcoesDesenho;
  limiar: number;
  passo: number;
  mostrar: boolean;
  /** Classes COCO a detectar (padrão: só pessoa). */
  classes: number[];
}

const PADRAO: Prefs = {
  desenho: { modo: "completo", fonte: 1, classe: false },
  limiar: 0.5,
  passo: 1,
  mostrar: true,
  classes: [0],
};

function lerPrefs(): Prefs {
  try {
    const raw = localStorage.getItem(PREFS_KEY);
    if (raw) {
      const p = JSON.parse(raw) as Partial<Prefs>;
      const classes = Array.isArray(p.classes) && p.classes.length > 0 ? p.classes : PADRAO.classes;
      return { ...PADRAO, ...p, desenho: { ...PADRAO.desenho, ...p.desenho }, classes };
    }
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
  fase: "rastrear" | "exportar" | "mancha";
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
  /** As 80 classes (vindas do Rust). */
  catalogo: ClasseInfo[];
  /** Modo "marcar à mão": a próxima caixa desenhada sobre a imagem vira uma mancha. */
  marcando: { nome: string; direcao: MarcaMancha["direcao"]; id?: string } | null;
  /** Último aviso de onde a mancha parou. */
  aviso: string | null;

  carregar: (ws: string, mediaHash: string) => Promise<void>;
  rastrear: (ws: string, mediaHash: string) => Promise<void>;
  cancelar: () => void;
  editar: (ws: string, mediaHash: string, e: EdicaoPessoa) => Promise<void>;
  selecionar: (id: string | null) => void;
  setJuntando: (id: string | null) => void;
  setMostrar: (v: boolean) => void;
  setDesenho: (d: Partial<OpcoesDesenho>) => void;
  setLimiar: (v: number) => void;
  setClasses: (c: number[]) => void;
  setMarcando: (m: PessoasState["marcando"]) => void;
  seguirMancha: (ws: string, mediaHash: string, marca: MarcaMancha) => Promise<ResultadoMancha | null>;
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
    gravarPrefs({ desenho: s.desenho, limiar: s.limiar, passo: s.passo, mostrar: s.mostrar, classes: s.classes });
  };
  return {
    ...lerPrefs(),
    mediaHash: null,
    estado: null,
    selecionada: null,
    juntando: null,
    tarefa: null,
    erro: null,
    catalogo: [],
    marcando: null,
    aviso: null,

    async carregar(ws, mediaHash) {
      ouvirProgresso();
      if (get().catalogo.length === 0) {
        void commands.pessoasClasses().then((catalogo) => set({ catalogo })).catch(() => {});
      }
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
      const { limiar, passo, classes } = get();
      set({ erro: null, aviso: null, tarefa: { fase: "rastrear", feitos: 0, total: 0, pessoas: 0 } });
      try {
        const estado = await commands.pessoasRastrear(ws, mediaHash, { limiar, passo, classes });
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
    setClasses: (classes) => {
      set({ classes });
      salvar();
    },
    setMarcando: (marcando) => set({ marcando }),
    async seguirMancha(ws, mediaHash, marca) {
      set({ erro: null, aviso: null, tarefa: { fase: "mancha", feitos: 0, total: 0, pessoas: 0 } });
      try {
        const r = await commands.pessoasSeguirMancha(ws, mediaHash, marca);
        if (get().mediaHash === mediaHash) {
          set({ estado: r.estado, selecionada: r.id, aviso: avisoDaMancha(r) });
        }
        return r;
      } catch (e) {
        const msg = toSicroError(e).message;
        if (!/cancelado/.test(msg)) set({ erro: msg });
        return null;
      } finally {
        set({ tarefa: null, marcando: null });
      }
    },
    setPasso: (passo) => {
      set({ passo });
      salvar();
    },
    setTarefa: (tarefa) => set({ tarefa }),
    setErro: (erro) => set({ erro }),
  };
});

const MOTIVOS: Record<string, string> = {
  fim_do_video: "chegou ao fim do vídeo",
  confianca: "perdeu a mancha",
  saiu_do_quadro: "saiu do quadro",
  limite: "chegou ao limite de quadros",
};

const mmss = (t: number) => {
  const m = Math.floor(t / 60);
  return `${m}:${(t - m * 60).toFixed(2).padStart(5, "0")}`;
};

/** Frase curta de onde cada sentido parou. */
export function avisoDaMancha(r: ResultadoMancha): string {
  return r.avisos
    .map((a) => {
      const onde = a.parou_em != null ? ` em ${mmss(a.parou_em)}` : "";
      return `${a.sentido === "frente" ? "Para a frente" : "Para trás"}: ${a.quadros} quadros, ${MOTIVOS[a.motivo] ?? a.motivo}${onde}.`;
    })
    .join(" ");
}

export function cadenciaDe(estado: PessoasEstado | null): Cadencia | null {
  const r = estado?.run;
  return r ? { frameDur: r.frame_dur_s, passo: Math.max(1, r.step) } : null;
}
