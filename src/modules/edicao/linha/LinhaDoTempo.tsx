/**
 * Painel da linha do tempo (Fase 4, §8.2; maquete aprovada em 17/11): barra (Selecionar, Lâmina, Ímã, zoom), cabeças
 * das trilhas, régua com marcadores, trilhas e a cabeça de leitura. Edita o mesmo documento e a mesma seleção da
 * Sequência e do Inspetor (contratos-fase4.md D1); cada arrasto vira UM passo ao soltar (Esc desiste). Rolagem nativa
 * nos dois eixos; só a parte visível (mais uma tela de cada lado) é desenhada; a cabeça anda fora do React.
 * `VistaLinha` não lê a store (o Diagnóstico mede 200 clipes com ela). Dono: P4.1.
 */

import { startTransition, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type PointerEvent as PE } from "react";
import { flushSync } from "react-dom";
import { Magnet, Maximize2, MousePointer2, Scissors, ZoomIn, ZoomOut } from "lucide-react";
import type { Draft } from "immer";
import type { Id, ItemSel, LinhaResolvida, ProjetoEdicao, Problema, Selecao, Seg } from "@domain/edicao";
import { useShortcuts } from "@core/useShortcuts";
import type { PalcoApi } from "../palco/Palco";
import { useEdicaoStore } from "../store/edicaoStore";
import { acharEfeito } from "../modelo/projeto";
import { resolver, type ContextoTempo } from "../modelo/tempo";
import { garantirMidia, useRecursos } from "../palco/recursos";
import { capturar } from "../palco/capturar";
import { dividir as dividirNoCursor, selecionarClipe } from "../sequencia/acoes";
import { useInspetorUi } from "../inspetor/ui";
import { PREVIA_VIVA_PRONTA } from "../palco/motor/motor";
import { acharItem, marcadoresNaLinha, pxDoSlot, trilhasDaLinha, type ItemNaLinha, type TrilhaNaLinha } from "./modelo";
import { alvosDasTrilhas, aparar, apararLivre, destinoNaPrincipal, dividirSemFonte, fronteiraDoDestino, imantar, IMA_PX, moverLivre, moverNaPrincipal, type Ferramenta } from "./arrastar";
import { ativoDoArrasto } from "./efeitos";
import { cabecaNoCursor, cursorNoSlot } from "./cursor";
import { tabelasDaTela } from "./tabelas";
import { alturaPedida, blocosDoTrecho, tiras, useTirasVisiveis, type BlocoTira } from "./tiras";
import { ouvirOndas } from "./ondas";
import { ALTURA_PADRAO, RAIA_PX, Trilha, type Fantasma } from "./Trilha";
import { Regua } from "./Regua";
import type { AlvoNoItem, FonteDoItem } from "./ClipeNaLinha";
import { ALTURA_PAINEL_MIN, ALTURA_TRILHA_MAX, ALTURA_TRILHA_MIN, alturaDoPainel, PX_POR_S_MAX, PX_POR_S_MIN, useLinhaUi, type LinhaUi } from "./estado";
import styles from "./Linha.module.css";

/** O pacote liga ao entregar. */
export const LINHA_PRONTA = true;

export interface LinhaProps {
  /** O palco (tocar, pausar, ir para): a cabeça de leitura segue o cursor da store. */
  palco: () => PalcoApi | null;
  /** Atalhos da tela ligados (`atalhosLigados` do editor): `edicao.ima`, `edicao.selecionar`, `edicao.lamina` e o
   *  zoom da linha (`video.timelineZoom*`) só valem com ele. */
  atalhos: boolean;
}

/** Largura das cabeças e altura da régua (px; as do CSS). */
export const CAB_PX = 150;
const REGUA_PX = 26;
/** Arrasto só começa depois de andar isto (px): clique fica clique. */
const LIMIAR_PX = 3;
/** Maior largura que o navegador desenha num elemento (px): o zoom não passa dela. */
const LARGURA_MAX_PX = 3e7;
const VAZIO: readonly Fantasma[] = Object.freeze([]) as readonly Fantasma[];

/** O que a vista pede para fora (store e palco na tela; nada na bancada do Diagnóstico). */
export interface AcoesLinha {
  selecionar(s: Selecao): void;
  /** Cursor no slot `q`: no `clipe` dado (mesmo fora dele, `livre`), senão no clipe sob `q`. */
  posicionar(q: number, clipe?: Id | null, livre?: boolean): void;
  /** Um passo do histórico (um arrasto = um passo). */
  alterar(rotulo: string, f: (d: Draft<ProjetoEdicao>) => void): void;
  /** Lâmina no slot `q` do clipe da principal. */
  dividir(clipe: Id, q: number): void;
  /** Marca da transição: o clipe em foco e a seção "Entrada" do Inspetor. */
  abrirEntrada(clipe: Id): void;
  /** J e L. */
  tocar(sentido: 1 | -1): void;
}

/** Onde está a cabeça de leitura (lida a cada quadro de tela enquanto muda) e quem avisa que mudou. */
export interface FonteCabeca {
  ler(): { q: number; fora: boolean } | null;
  assinar(f: () => void): () => void;
}

export type UiDaLinha = Pick<LinhaUi, "pxPorS" | "ferramenta" | "ima" | "rolagem_s" | "alturas" | "travadas" | "altura"> & Partial<Pick<LinhaUi, "ajustarAoAbrir">>;

export interface VistaLinhaProps {
  doc: ProjetoEdicao;
  linha: LinhaResolvida;
  ctx: ContextoTempo;
  selecao: Selecao;
  soLeitura: boolean;
  problemas: readonly Problema[];
  ws: string | null;
  raizCache: string | null;
  ui: UiDaLinha;
  definirUi(p: Partial<UiDaLinha>): void;
  cabeca: FonteCabeca;
  acoes: AcoesLinha;
  atalhos: boolean;
}

// ---- painel ligado à store e ao palco ----

export function LinhaDoTempo({ palco, atalhos }: LinhaProps) {
  const doc = useEdicaoStore((s) => s.doc);
  const ws = useEdicaoStore((s) => s.ws);
  const raizCache = useEdicaoStore((s) => s.raizCache);
  const selecao = useEdicaoStore((s) => s.selecao);
  const soLeitura = useEdicaoStore((s) => s.soLeitura);
  const analise = useEdicaoStore((s) => s.analise);
  const avisos = useEdicaoStore((s) => s.avisos);
  const versao = useRecursos((s) => s.versao);
  const ui = useLinhaUi();
  const projeto = doc?.id ?? null;
  useEffect(() => ui.carregar(projeto), [projeto]); // eslint-disable-line react-hooks/exhaustive-deps
  // as tabelas de todas as mídias (as da Sequência): a linha resolve igual
  useEffect(() => {
    if (!ws || !doc) return;
    for (const m of doc.midias) if (m.tipo === "video") garantirMidia(ws, m.sha256);
  }, [ws, doc?.midias]); // eslint-disable-line react-hooks/exhaustive-deps
  // troca de caso: as tiras do caso anterior saem
  const wsAnterior = useRef(ws);
  if (wsAnterior.current !== ws) {
    wsAnterior.current = ws;
    tiras.limpar();
  }
  useEffect(() => ouvirOndas(), []);
  const ctx = useMemo(() => (doc ? tabelasDaTela(doc) : null), [doc, versao]);
  const linha = useMemo(() => (doc && ctx ? resolver(doc, ctx) : null), [doc, ctx]);
  const problemas = useMemo(() => [...avisos, ...(analise?.problemas ?? [])], [avisos, analise]);
  const vivo = useRef({ doc, linha, ctx });
  vivo.current = { doc, linha, ctx };

  const acoes = useMemo<AcoesLinha>(() => {
    const st = () => useEdicaoStore.getState();
    return {
      selecionar: (s) => {
        st().selecionar(s);
        if (s.efeito) useInspetorUi.getState().abrirAba("clipe");
      },
      posicionar: (q, clipe, livre) => {
        const { doc: d, linha: l, ctx: c } = vivo.current;
        if (!d || !l || !c) return;
        if (PREVIA_VIVA_PRONTA && !clipe) {
          st().posicionar({ modo: "programa", q: Math.max(0, Math.round(q)) });
          return;
        }
        const cur = cursorNoSlot(d, l, q, c, clipe, livre);
        if (cur) st().posicionar(cur);
      },
      alterar: (rotulo, f) => {
        const g = st().iniciarGesto(rotulo);
        g.alterar(f);
        g.concluir();
      },
      dividir: (clipe, q) => {
        const { doc: d, linha: l, ctx: c } = vivo.current;
        if (!d || !l || !c) return;
        palco()?.pausar();
        const cl = d.principal.find((x) => (x as { id?: Id }).id === clipe) as { tipo?: string } | undefined;
        if (cl?.tipo === "trecho") {
          // a lâmina é o "Dividir no cursor" no slot clicado
          const cur = cursorNoSlot(d, l, q, c, clipe);
          if (!cur) return;
          st().posicionar(cur);
          dividirNoCursor();
          return;
        }
        let novo: Id | null = null;
        st().alterar("Dividir no cursor", (dd) => {
          novo = dividirSemFonte(dd, l, clipe, q);
        });
        if (novo) selecionarClipe(novo);
      },
      abrirEntrada: (clipe) => {
        selecionarClipe(clipe);
        useInspetorUi.getState().abrirAba("clipe");
        requestAnimationFrame(() => {
          const h = [...document.querySelectorAll("[data-inspetor] h3")].find((x) => x.textContent === "Entrada");
          h?.scrollIntoView?.({ block: "nearest" });
        });
      },
      tocar: (d) => palco()?.tocar(d),
    };
  }, [palco]);

  const cabeca = useMemo<FonteCabeca>(() => {
    // tocando, o palco põe o cursor na store a cada 250 ms: duas mudanças seguidas e recentes = tocando
    const mudancas = [0, 0];
    const tocando = () => performance.now() - (mudancas[1] as number) < 400 && (mudancas[1] as number) - (mudancas[0] as number) < 400;
    return {
      ler: () => {
        const { doc: d, linha: l, ctx: c } = vivo.current;
        const cur = useEdicaoStore.getState().cursor;
        if (!d || !l || !c) return null;
        // entre duas posições da store, o palco diz o quadro mostrado agora
        const t = cur.modo === "clipe" && tocando() ? palco()?.tempo() : undefined;
        return cabecaNoCursor(d, l, cur.modo === "clipe" && t !== undefined && Number.isFinite(t) ? { ...cur, t } : cur, c);
      },
      assinar: (f) =>
        useEdicaoStore.subscribe((s, a) => {
          if (s.cursor !== a.cursor) {
            mudancas[0] = mudancas[1] as number;
            mudancas[1] = performance.now();
          }
          if (s.cursor !== a.cursor || s.doc !== a.doc) f();
        }),
    };
  }, [palco]);

  // o estado de tela do projeto carrega antes (zoom e rolagem guardados valem já na montagem)
  if (!doc || !linha || !ctx || ui.projeto !== doc.id) return null;
  return (
    <VistaLinha
      key={doc.id}
      doc={doc}
      linha={linha}
      ctx={ctx}
      selecao={selecao}
      soLeitura={soLeitura}
      problemas={problemas}
      ws={ws}
      raizCache={raizCache}
      ui={ui}
      definirUi={ui.definir}
      cabeca={cabeca}
      acoes={acoes}
      atalhos={atalhos}
    />
  );
}

// ---- a vista ----

/** Arrasto em curso (fora do React; o fantasma vai para o estado a cada quadro de tela). */
interface Gesto {
  item: ItemNaLinha;
  trilha: TrilhaNaLinha;
  alvo: AlvoNoItem;
  x0: number;
  y0: number;
  ativo: boolean;
  alvos: number[];
  /** Outros livres selecionados que andam junto (mesmo deslocamento). */
  grupo: ItemNaLinha[];
  /** Último resultado (slots) e destino. */
  de_q: number;
  ate_q: number;
  q: number;
  trilhaDestino: string | "nova" | null;
  alt: boolean;
  /** Clique num item já selecionado (de um grupo): a seleção vira só ele ao soltar sem arrastar. */
  soEleAoSoltar: boolean;
}

const tipoItemSel = (it: ItemNaLinha): ItemSel["k"] | null => (it.k === "efeito" ? null : it.k);

export function VistaLinha(p: VistaLinhaProps) {
  const { doc, linha, ctx, ui } = p;
  const fps = linha.fps;
  const rolo = useRef<HTMLDivElement>(null);
  const conteudo = useRef<HTMLDivElement>(null);
  const cabecaRef = useRef<HTMLDivElement>(null);
  const cabecaTopoRef = useRef<HTMLDivElement>(null);
  const laminaRef = useRef<HTMLDivElement>(null);
  const [vw, setVw] = useState(0);
  /** Parte desenhada e parte visível (para pedir tiras), em s do programa: o zoom vivo não as invalida. */
  const [janelaS, setJanelaS] = useState<readonly [number, number]>(() => {
    // antes de medir: a largura da janela do app (a montagem não desenha o programa inteiro)
    const W = typeof window !== "undefined" && window.innerWidth > 0 ? window.innerWidth / Math.max(ui.pxPorS, PX_POR_S_MIN) : Infinity;
    return W === Infinity ? [0, Infinity] : [Math.max(0, ui.rolagem_s - W), ui.rolagem_s + 2 * W];
  });
  const [vistaS, setVistaS] = useState<readonly [number, number]>([0, Infinity]);
  /** Zoom com que a árvore foi desenhada (px/s). O vivo (roda, botões) muda antes só a variável CSS `--px` do
   *  conteúdo, sem React; a árvore assenta depois, numa transição (o React reparte o trabalho em pedaços). */
  const [pxS, setPxS] = useState(ui.pxPorS);
  const pxVivo = useRef(ui.pxPorS);
  const [fantasmas, setFantasmas] = useState<readonly Fantasma[]>(VAZIO);
  const [insercao, setInsercao] = useState<number | null>(null);
  const [guia, setGuia] = useState<number | null>(null);
  const [area, setArea] = useState<{ x: number; y: number; w: number; h: number } | null>(null);

  const trilhas = useMemo(() => {
    const ts = trilhasDaLinha(doc, linha, ctx);
    // item citado num problema da análise: risco vermelho
    const citados = new Set<string>();
    for (const pr of p.problemas) {
      if (pr.efeito) citados.add(pr.efeito);
      else if (pr.clipe) citados.add(pr.clipe);
    }
    if (citados.size) for (const t of ts) for (const it of t.itens) if (citados.has(it.id)) it.erro = true;
    return ts;
  }, [doc, linha, ctx, p.problemas]);
  const marcadores = useMemo(() => marcadoresNaLinha(doc, linha, ctx), [doc, linha, ctx]);
  const px = pxDoSlot(pxS, fps);
  const fimQ = useMemo(() => {
    let f = linha.total_q;
    for (const t of trilhas) for (const it of t.itens) if (it.ate_q > f) f = it.ate_q;
    for (const m of marcadores) if (m.q > f) f = m.q;
    return f;
  }, [trilhas, marcadores, linha.total_q]);
  const fimS = (fimQ * fps.den) / fps.num;
  const largura = Math.max(fimQ * px + Math.max(240, vw * 0.5), vw - CAB_PX);
  // a largura acompanha o zoom vivo pela variável CSS
  const larguraCss = `max(${Math.max(0, vw - CAB_PX)}px, calc(var(--px) * ${fimQ}px + ${Math.max(240, vw * 0.5)}px))`;
  const janela = useMemo(() => [janelaS[0] * pxS, janelaS[1] * pxS] as const, [janelaS, pxS]);
  const pxMax = Math.max(PX_POR_S_MIN, Math.min(PX_POR_S_MAX, LARGURA_MAX_PX / Math.max(fimS, 1)));
  const alturaDe = useCallback((t: TrilhaNaLinha) => (t.tipo === "efeito" ? (t.raias ?? 1) * RAIA_PX : (ui.alturas[t.id] ?? ALTURA_PADRAO[t.tipo])), [ui.alturas]);

  // ---- referências vivas para os eventos ----
  const v = useRef({ p, trilhas, marcadores, px, largura, pxMax, fimS, vw });
  v.current = { p, trilhas, marcadores, px, largura, pxMax, fimS, vw };
  /** px por slot no zoom que está na tela agora (gestos, cabeça). */
  const pxAgora = () => pxDoSlot(pxVivo.current, fps);

  // ---- tamanho e rolagem: janela desenhada e área visível ----
  const janelaAtual = useRef(janelaS);
  /** `ja`: na montagem e no redimensionamento, antes de pintar; na rolagem e no zoom, numa transição. */
  const atualizarJanela = useCallback((ja = false) => {
    const el = rolo.current;
    if (!el) return;
    const w = el.clientWidth - CAB_PX;
    const j = janelaAtual.current;
    if (w <= 0) {
      if (!(j[0] === 0 && j[1] === Infinity)) {
        janelaAtual.current = [0, Infinity];
        setJanelaS(janelaAtual.current);
      }
      return;
    }
    const k = pxVivo.current;
    const a = el.scrollLeft / k;
    const b = (el.scrollLeft + w) / k;
    // a inicial (tudo, antes de medir) também é trocada assim que há largura
    if (j[1] !== Infinity && a >= j[0] && b <= j[1]) return;
    // fora da parte desenhada: outra, com uma tela de cada lado (render repartido: a rolagem não para)
    const novo = [Math.max(0, a - w / k), b + w / k] as const;
    janelaAtual.current = novo;
    if (ja) setJanelaS(novo);
    else startTransition(() => setJanelaS(novo));
  }, []);
  useLayoutEffect(() => {
    const el = rolo.current;
    if (!el) return;
    const medir = () => {
      setVw(el.clientWidth);
      atualizarJanela(true);
    };
    medir();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(medir);
    ro.observe(el);
    return () => ro.disconnect();
  }, [atualizarJanela]);
  // rolagem guardada do projeto (uma vez)
  const restaurada = useRef<string | null>(null);
  useLayoutEffect(() => {
    const el = rolo.current;
    if (!el || restaurada.current === doc.id) return;
    restaurada.current = doc.id;
    if (ui.ajustarAoAbrir) {
      p.definirUi({ ajustarAoAbrir: false });
      zoom("ajustar");
      return;
    }
    el.scrollLeft = ui.rolagem_s * ui.pxPorS;
    atualizarJanela();
  }, [doc.id]); // eslint-disable-line react-hooks/exhaustive-deps
  const fimDaRolagem = useRef<ReturnType<typeof setTimeout> | null>(null);
  const aoRolar = useCallback(() => {
    atualizarJanela();
    if (fimDaRolagem.current !== null) clearTimeout(fimDaRolagem.current);
    fimDaRolagem.current = setTimeout(() => {
      fimDaRolagem.current = null;
      const el = rolo.current;
      if (!el) return;
      const w = el.clientWidth - CAB_PX;
      const k = pxVivo.current;
      setVistaS(w > 0 ? [el.scrollLeft / k, (el.scrollLeft + w) / k] : [0, Infinity]);
      v.current.p.definirUi({ rolagem_s: el.scrollLeft / k });
    }, 120);
  }, [atualizarJanela]);
  useEffect(() => {
    const el = rolo.current;
    if (!el) return;
    const w = el.clientWidth - CAB_PX;
    const k = pxVivo.current;
    setVistaS(w > 0 ? [el.scrollLeft / k, (el.scrollLeft + w) / k] : [0, Infinity]);
  }, [vw, pxS]);
  useEffect(() => () => void (fimDaRolagem.current !== null && clearTimeout(fimDaRolagem.current)), []);

  // ---- zoom ----
  const moverCabeca = useRef<() => void>(() => {});
  /** Zoom na tela já: só a variável `--px` e a rolagem (o ponto sob `ancoraX` fica no lugar; null = o começo). */
  const zoomVivo = useCallback(
    (n: number, ancoraX: number | null) => {
      const el = rolo.current;
      const antes = pxVivo.current;
      const w = el ? el.clientWidth - CAB_PX : 0;
      const ax = ancoraX ?? 0;
      const t = ancoraX === null ? 0 : ((el?.scrollLeft ?? 0) + ax) / antes;
      pxVivo.current = n;
      conteudo.current?.style.setProperty("--px", String(pxDoSlot(n, fps)));
      if (el && w > 0) el.scrollLeft = Math.max(0, t * n - ax);
      atualizarJanela();
      moverCabeca.current();
    },
    [fps, atualizarJanela],
  );
  const assentar = useRef<ReturnType<typeof setTimeout> | null>(null);
  const zoom = useCallback(
    (f: number | "ajustar", ancoraX?: number) => {
      const el = rolo.current;
      const { p: pp, pxMax: max, fimS: fs } = v.current;
      const w = el ? el.clientWidth - CAB_PX : 0;
      const antes = pxVivo.current;
      const n = f === "ajustar" ? Math.min(Math.max((w > 0 ? w - 24 : 800) / Math.max(fs, 1), PX_POR_S_MIN), max) : Math.min(Math.max(antes * f, PX_POR_S_MIN), max);
      if (n !== antes || f === "ajustar") zoomVivo(n, f === "ajustar" ? null : (ancoraX ?? (w > 0 ? w / 2 : 0)));
      // a árvore (rótulos, tiras, régua) assenta quando o zoom para
      if (assentar.current !== null) clearTimeout(assentar.current);
      assentar.current = setTimeout(() => {
        assentar.current = null;
        pp.definirUi({ pxPorS: pxVivo.current });
      }, 120);
    },
    [zoomVivo],
  );
  useEffect(() => () => void (assentar.current !== null && clearTimeout(assentar.current)), []);
  // a árvore que assentou com um zoom mais velho não desfaz o vivo
  useLayoutEffect(() => {
    if (pxVivo.current !== pxS) conteudo.current?.style.setProperty("--px", String(pxDoSlot(pxVivo.current, fps)));
  });
  // zoom de fora (bancada) ou o vivo que parou: a árvore assenta numa transição. Só quando o do estado MUDA: na
  // montagem o ajuste do projeto novo já pôs o vivo à frente dele.
  const pxDoEstado = useRef(ui.pxPorS);
  useEffect(() => {
    if (ui.pxPorS === pxDoEstado.current) return;
    pxDoEstado.current = ui.pxPorS;
    if (ui.pxPorS !== pxVivo.current) zoomVivo(ui.pxPorS, Math.max(0, ((rolo.current?.clientWidth ?? CAB_PX) - CAB_PX) / 2));
    if (ui.pxPorS !== pxS) startTransition(() => setPxS(ui.pxPorS));
  }, [ui.pxPorS]); // eslint-disable-line react-hooks/exhaustive-deps

  // Ctrl+roda = zoom no ponteiro; roda sem rolagem vertical = rola na horizontal
  useEffect(() => {
    const el = rolo.current;
    if (!el) return;
    const roda = (e: WheelEvent) => {
      const dy = e.deltaMode === 1 ? e.deltaY * 33 : e.deltaY;
      if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        const r = el.getBoundingClientRect();
        zoom(Math.pow(1.0018, -dy), Math.max(0, e.clientX - r.left - CAB_PX));
        return;
      }
      if (!e.shiftKey && Math.abs(dy) > Math.abs(e.deltaX) && el.scrollHeight <= el.clientHeight + 1) {
        e.preventDefault();
        el.scrollLeft += dy;
      }
    };
    el.addEventListener("wheel", roda, { passive: false });
    return () => el.removeEventListener("wheel", roda);
  }, [zoom]);

  // ---- cabeça de leitura (fora do React) ----
  const cabecaQ = useRef<number | null>(null);
  useEffect(() => {
    let raf = 0;
    let parado = 0;
    let ultimo = "";
    const aplicar = (): boolean => {
      const c = p.cabeca.ler();
      const chave = c ? `${c.q}:${c.fora}` : "";
      const pxs = pxAgora();
      const mudou = chave !== ultimo;
      const antes = cabecaQ.current;
      ultimo = chave;
      cabecaQ.current = c?.q ?? null;
      for (const el of [cabecaRef.current, cabecaTopoRef.current]) {
        if (!el) continue;
        el.style.display = c ? "" : "none";
        if (c) {
          el.style.transform = `translateX(${c.q * pxs}px)`;
          el.classList.toggle(styles.fora as string, c.fora);
        }
      }
      // tocando: a página acompanha quando a cabeça passa da borda direita
      const r = rolo.current;
      if (mudou && c && antes !== null && c.q > antes && r) {
        const w = r.clientWidth - CAB_PX;
        const x = c.q * pxs;
        const xa = antes * pxs;
        if (w > 0 && xa >= r.scrollLeft && xa <= r.scrollLeft + w && x > r.scrollLeft + w - 24) r.scrollLeft = x - w * 0.15;
      }
      return mudou;
    };
    const passo = () => {
      parado = aplicar() ? 0 : parado + 1;
      raf = parado > 45 ? 0 : requestAnimationFrame(passo);
    };
    const acordar = () => {
      parado = 0;
      if (!raf) raf = requestAnimationFrame(passo);
    };
    aplicar();
    moverCabeca.current = () => void aplicar();
    const sair = p.cabeca.assinar(acordar);
    return () => {
      sair();
      if (raf) cancelAnimationFrame(raf);
    };
  }, [p.cabeca, doc, linha]); // eslint-disable-line react-hooks/exhaustive-deps

  // ---- itens: fonte (onda) e tiras ----
  const porId = useMemo(() => {
    const m = new Map<Id, unknown>();
    for (const c of doc.principal) {
      const id = (c as { id?: unknown }).id;
      if (typeof id === "string") m.set(id, c);
    }
    for (const t of doc.audios) for (const c of t.clipes) m.set(c.id, c);
    return m;
  }, [doc]);
  // um objeto por item (o `memo` do item não redesenha à toa)
  const fontes = useMemo(() => new WeakMap<ItemNaLinha, FonteDoItem | null>(), [doc]);
  const fonteDe = useCallback(
    (it: ItemNaLinha): FonteDoItem | null => {
      if (fontes.has(it)) return fontes.get(it) ?? null;
      const f = fonteCalculada(it);
      fontes.set(it, f);
      return f;
    },
    [fontes], // eslint-disable-line react-hooks/exhaustive-deps
  );
  const fonteCalculada = (it: ItemNaLinha): FonteDoItem | null => {
      const m = it.midia ? doc.midias.find((x) => x.id === it.midia) : null;
      const c = porId.get(it.id) as { tipo?: string; inicio_s?: Seg; fim_s?: Seg; de_s?: Seg; ate_s?: Seg | null } | undefined;
      if (!m || !c) return null;
      if (it.k === "audio") {
        const ini = m.inicio_s ?? 0;
        const ate = c.ate_s ?? m.duracao_s;
        return ate == null ? null : { sha256: m.sha256, de_s: ini + (c.de_s ?? 0), ate_s: ini + ate };
      }
      if (c.tipo === "trecho" || c.tipo === "tela") return { sha256: m.sha256, de_s: c.inicio_s as Seg, ate_s: c.fim_s as Seg };
      return null;
  };
  const v1 = trilhas.find((t) => t.tipo === "principal");
  const alturaV1 = v1 ? alturaDe(v1) : ALTURA_PADRAO.principal;
  const dpr = typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1;
  const { blocos, pedidos } = useMemo(() => {
    const porItem = new Map<string, BlocoTira[]>();
    const pedir: BlocoTira[] = [];
    if (!v1) return { blocos: porItem, pedidos: pedir };
    const h = alturaPedida(Math.max(16, alturaV1 - 6 - 18), dpr);
    for (const it of v1.itens) {
      if (it.tipo !== "trecho" || it.ate_q <= it.de_q) continue;
      const x = it.de_q * px;
      const w = (it.ate_q - it.de_q) * px;
      if (x + w < janela[0] || x > janela[1]) continue;
      const f = fonteDe({ ...it, k: "clipe" });
      if (!f) continue;
      const bs = blocosDoTrecho({ clipe: it.id, sha256: f.sha256, de_s: f.de_s, ate_s: f.ate_s, largura: w, altura: h, vis0: janela[0] - x, vis1: janela[1] - x });
      porItem.set(it.id, bs);
      for (const b of bs) if (x + b.x + b.w >= vistaS[0] * pxS && x + b.x <= vistaS[1] * pxS) pedir.push(b);
    }
    return { blocos: porItem, pedidos: pedir };
  }, [v1, px, pxS, janela, vistaS, fonteDe, alturaV1, dpr]);
  useTirasVisiveis(p.ws, pedidos);

  // ---- seleção ----
  const donoSel = useCallback((clipe: Id): ItemSel => {
    for (const t of doc.sobreposicoes) if (t.clipes.some((c) => (c as { id?: Id }).id === clipe)) return { k: "livre", id: clipe, trilha: t.id };
    return { k: "clipe", id: clipe };
  }, [doc]);
  const selecaoDe = useCallback(
    (it: ItemNaLinha, t: TrilhaNaLinha, shift: boolean): Selecao => {
      const atual = v.current.p.selecao;
      if (it.k === "efeito") {
        const foco = donoSel(it.clipe as Id);
        return { itens: [foco], foco, efeito: it.id };
      }
      const k = tipoItemSel(it) as ItemSel["k"];
      const item: ItemSel = t.tipo === "sobreposicao" || t.tipo === "audio" ? { k, id: it.id, trilha: t.id } : { k, id: it.id };
      if (!shift) return { itens: [item], foco: item, efeito: null };
      const tem = atual.itens.some((x) => x.id === it.id);
      const itens = tem ? atual.itens.filter((x) => x.id !== it.id) : [...atual.itens, item];
      return { itens, foco: tem ? (itens[itens.length - 1] ?? null) : item, efeito: null };
    },
    [donoSel],
  );

  // ---- gestos ----
  const gesto = useRef<Gesto | null>(null);
  const quadroPendente = useRef(0);

  const slotDoPonteiro = (clientX: number): number => {
    const el = rolo.current;
    if (!el) return 0;
    const r = el.getBoundingClientRect();
    return (clientX - r.left - CAB_PX + el.scrollLeft) / pxAgora();
  };

  const trilhaSob = (clientY: number): { id: string; tipo: string } | "acima" | null => {
    const el = conteudo.current;
    if (!el) return null;
    const linhas = el.querySelectorAll<HTMLElement>("[data-trilha]");
    for (const l of linhas) {
      const r = l.getBoundingClientRect();
      if (clientY >= r.top && clientY < r.bottom) return { id: l.dataset.trilha as string, tipo: l.dataset.tipo as string };
    }
    const primeira = linhas[0]?.getBoundingClientRect();
    return primeira && clientY < primeira.top ? "acima" : null;
  };

  /** Limites do clipe dono de uma barra de efeito (slots). */
  const limitesDoDono = (it: ItemNaLinha): [number, number] => {
    const c = linha.clipes.find((x) => x.id === it.clipe) ?? linha.livres.find((x) => x.id === it.clipe);
    return c ? [c.inicio_q, c.inicio_q + c.n_q] : [it.de_q, it.ate_q];
  };

  const calcular = (g: Gesto, clientX: number, clientY: number, alt: boolean) => {
    const { p: pp } = v.current;
    const pxs = pxAgora();
    const dq = (clientX - g.x0) / pxs;
    const raio = pp.ui.ima ? IMA_PX / pxs : -1;
    const ima = (q: number) => (raio >= 0 ? imantar(q, g.alvos, raio) : q);
    const it = g.item;
    const len = it.ate_q - it.de_q;
    let guiaQ: number | null = null;
    let ins: number | null = null;
    g.alt = alt;
    const tipo = g.trilha.tipo;
    if (g.alvo === "corpo" && (tipo === "principal" || tipo === "som")) {
      g.q = slotDoPonteiro(clientX);
      g.de_q = it.de_q + dq;
      g.ate_q = g.de_q + len;
      ins = fronteiraDoDestino(linha, it.id, g.q);
    } else if (g.alvo === "corpo") {
      let de = Math.round(it.de_q + dq);
      if (tipo === "efeito") {
        const [a, b] = limitesDoDono(it);
        de = Math.min(Math.max(de, a), b - len);
      }
      const s1 = ima(de);
      const s2 = ima(de + len) - len;
      const usa = Math.abs(s1 - de) > 0 && (Math.abs(s1 - de) <= Math.abs(s2 - de) || s2 === de) ? s1 : s2 !== de ? s2 : de;
      if (usa !== de) guiaQ = usa === s1 ? s1 : s2 + len;
      de = Math.max(0, usa);
      if (tipo === "efeito") {
        const [a, b] = limitesDoDono(it);
        de = Math.min(Math.max(de, a), b - len);
      }
      g.de_q = de;
      g.ate_q = de + len;
      g.trilhaDestino = null;
      if ((tipo === "sobreposicao" || tipo === "audio") && g.grupo.length === 0) {
        const sob = trilhaSob(clientY);
        if (sob === "acima" && tipo === "sobreposicao") g.trilhaDestino = "nova";
        else if (sob && sob !== "acima" && sob.tipo === tipo && sob.id !== g.trilha.id) g.trilhaDestino = sob.id;
      }
    } else {
      const borda = g.alvo === "inicio" ? it.de_q : it.ate_q;
      let q = ima(Math.round(borda + dq));
      if (q !== Math.round(borda + dq)) guiaQ = q;
      if (tipo === "efeito") {
        const [a, b] = limitesDoDono(it);
        q = Math.min(Math.max(q, a), b);
      }
      if (g.alvo === "inicio") {
        q = Math.min(q, it.ate_q - 1);
        if (tipo !== "principal" && tipo !== "som") q = Math.max(q, 0);
        g.de_q = q;
        g.ate_q = it.ate_q;
      } else {
        q = Math.max(q, it.de_q + 1);
        g.de_q = it.de_q;
        g.ate_q = q;
      }
      g.q = q;
    }
    return { guiaQ, ins };
  };

  const fantasmasDe = (g: Gesto): Fantasma[] => {
    const destino = g.trilhaDestino === "nova" ? (trilhas.find((t) => t.tipo === "sobreposicao")?.id ?? g.trilha.id) : (g.trilhaDestino ?? g.trilha.id);
    const borda = g.alvo === "corpo" ? null : g.alvo;
    const r: Fantasma[] = [{ id: g.item.id, clipe: g.item.clipe, de_q: g.de_q, ate_q: g.ate_q, trilha: g.trilha.tipo === "som" ? g.trilha.id : destino, origem: g.trilha.id, borda }];
    const delta = g.de_q - g.item.de_q;
    for (const o of g.grupo) r.push({ id: o.id, de_q: o.de_q + delta, ate_q: o.ate_q + delta, trilha: o.trilha, origem: o.trilha, borda: null });
    return r;
  };

  /** Borda arrastada: o palco mostra o quadro dela (trecho e efeito; o fim mostra o último quadro que fica). */
  const mostrarBorda = (g: Gesto) => {
    const pp = v.current.p;
    const tipo = g.trilha.tipo;
    if (g.alvo === "corpo") return;
    const clipe = tipo === "efeito" ? (g.item.clipe ?? null) : tipo === "principal" || tipo === "som" ? g.item.id : null;
    if (!clipe || !linha.clipes.some((c) => c.id === clipe)) return;
    pp.acoes.posicionar(g.alvo === "inicio" ? g.q : g.q - 1, clipe, tipo !== "efeito");
  };

  const terminar = useRef<(() => void) | null>(null);

  const soltar = (g: Gesto) => {
    const pp = v.current.p;
    const it = g.item;
    const tipo = g.trilha.tipo;
    if (!g.ativo) {
      if (g.soEleAoSoltar) pp.acoes.selecionar(selecaoDe(it, g.trilha, false));
      return;
    }
    if (g.alvo === "corpo" && (tipo === "principal" || tipo === "som")) {
      const i = doc.principal.findIndex((c) => (c as { id?: Id }).id === it.id);
      const para = destinoNaPrincipal(doc, linha, it.id, g.q);
      if (para !== i && para >= 0) pp.acoes.alterar("Mover", (d) => moverNaPrincipal(d, it.id, para));
      return;
    }
    if (tipo === "principal" || tipo === "som") {
      if ((g.alvo === "inicio" ? g.de_q : g.ate_q) === (g.alvo === "inicio" ? it.de_q : it.ate_q)) return;
      pp.acoes.alterar("Aparar", (d) => aparar(d, linha, it.id, g.alvo as "inicio" | "fim", g.alvo === "inicio" ? g.de_q : g.ate_q, ctx));
      return;
    }
    if (tipo === "efeito") {
      if (g.de_q === it.de_q && g.ate_q === it.ate_q) return;
      const clipe = it.clipe as Id;
      const novo = ativoDoArrasto(doc, linha, clipe, g.de_q, g.ate_q, ctx, { daMidia: it.daMidia });
      pp.acoes.alterar(g.alvo === "inicio" ? "De" : g.alvo === "fim" ? "Até" : "Mover", (d) => {
        const a = acharEfeito(d as ProjetoEdicao, it.id);
        if (!a) return;
        const antes = a.efeito.ativo;
        // só o lado arrastado muda: o outro guarda o valor que tinha
        const at = g.alvo === "inicio" ? { de: novo.de, ate: antes?.ate ?? null } : g.alvo === "fim" ? { de: antes?.de ?? null, ate: novo.ate } : novo;
        a.efeito.ativo = at.de == null && at.ate == null && !a.midia ? null : at;
      });
      return;
    }
    if (tipo === "sobreposicao" || tipo === "audio") {
      if (g.alvo === "corpo") {
        const delta = g.de_q - it.de_q;
        if (delta === 0 && !g.trilhaDestino) return;
        pp.acoes.alterar("Mover", (d) => {
          moverLivre(d, linha, it.id, g.de_q, { trilha: g.trilhaDestino ?? undefined, manterAncora: g.alt });
          for (const o of g.grupo) moverLivre(d, linha, o.id, o.de_q + delta, { manterAncora: g.alt });
        });
      } else {
        const q = g.alvo === "inicio" ? g.de_q : g.ate_q;
        if (q === (g.alvo === "inicio" ? it.de_q : it.ate_q)) return;
        pp.acoes.alterar("Aparar", (d) => apararLivre(d, linha, it.id, g.alvo as "inicio" | "fim", q));
      }
    }
  };

  const pegar = (e: PE<HTMLElement>, it: ItemNaLinha, alvo: AlvoNoItem, t: TrilhaNaLinha) => {
      if (e.button !== 0) return;
      e.stopPropagation();
      e.preventDefault();
      const pp = v.current.p;
      const travada = !!pp.ui.travadas[t.id] || pp.soLeitura;
      // lâmina: corta o clipe da principal no slot sob o ponteiro
      if (pp.ui.ferramenta === "lamina" && (t.tipo === "principal" || t.tipo === "som")) {
        if (!travada && !it.apagado && it.ate_q > it.de_q) pp.acoes.dividir(it.id, Math.floor(slotDoPonteiro(e.clientX)));
        return;
      }
      const jaSel = pp.selecao.itens.some((x) => x.id === it.id) && pp.selecao.itens.length > 1 && !e.shiftKey && it.k !== "efeito";
      if (!jaSel) pp.acoes.selecionar(selecaoDe(it, t, e.shiftKey));
      const q0 = slotDoPonteiro(e.clientX);
      if (!e.shiftKey && it.k !== "marcador") {
        // clique leva a cabeça ao ponto (no clipe da principal clicado, ou no clipe sob o ponto)
        const clipe = t.tipo === "principal" || t.tipo === "som" ? it.id : it.k === "efeito" && linha.clipes.some((c) => c.id === it.clipe) ? (it.clipe ?? null) : null;
        if (it.ate_q > it.de_q) pp.acoes.posicionar(Math.floor(q0), clipe);
      }
      if (travada || it.apagado || it.k === "legenda") return;
      if (alvo === "corpo" && it.cortado && (it.cortado.de || it.cortado.ate)) return;
      // a captura fica no rolo: o item (ou a borda) pode sair da tela no meio do arrasto
      const alvo2 = capturarEm(rolo.current ?? e.currentTarget, e.pointerId);
      const sel = pp.selecao.itens;
      const grupo =
        alvo === "corpo" && (t.tipo === "sobreposicao" || t.tipo === "audio")
          ? v.current.trilhas.flatMap((tt) => tt.itens.filter((o) => o.id !== it.id && (o.k === "livre" || o.k === "audio") && sel.some((x) => x.id === o.id) && (jaSel || e.shiftKey)))
          : [];
      const g: Gesto = {
        item: it,
        trilha: t,
        alvo,
        x0: e.clientX,
        y0: e.clientY,
        ativo: false,
        alvos: [],
        grupo,
        de_q: it.de_q,
        ate_q: it.ate_q,
        q: Math.round(q0),
        trilhaDestino: null,
        alt: e.altKey,
        soEleAoSoltar: jaSel,
      };
      gesto.current = g;
      let ultimoEv: PointerEvent | null = null;
      const aplicar = () => {
        quadroPendente.current = 0;
        const ev = ultimoEv;
        if (!ev || gesto.current !== g) return;
        const { guiaQ, ins } = calcular(g, ev.clientX, ev.clientY, ev.altKey);
        // no mesmo quadro de tela (sem esperar a vez do React): o fantasma acompanha o ponteiro
        flushSync(() => {
          setFantasmas(fantasmasDe(g));
          setGuia(guiaQ);
          setInsercao(ins);
        });
        mostrarBorda(g);
      };
      const mover = (ev: PointerEvent) => {
        if (!g.ativo) {
          if (Math.abs(ev.clientX - g.x0) < LIMIAR_PX && Math.abs(ev.clientY - g.y0) < LIMIAR_PX) return;
          g.ativo = true;
          g.alvos = alvosDasTrilhas(v.current.trilhas, v.current.marcadores, cabecaQ.current, it.id);
        }
        ultimoEv = ev;
        if (!quadroPendente.current) quadroPendente.current = requestAnimationFrame(aplicar);
      };
      const fim = (ok: boolean) => {
        alvo2.removeEventListener("pointermove", mover);
        alvo2.removeEventListener("pointerup", subir);
        alvo2.removeEventListener("pointercancel", cancelar);
        window.removeEventListener("keydown", tecla, true);
        if (quadroPendente.current) cancelAnimationFrame(quadroPendente.current);
        quadroPendente.current = 0;
        gesto.current = null;
        terminar.current = null;
        setFantasmas(VAZIO);
        setGuia(null);
        setInsercao(null);
        if (ok) soltar(g);
      };
      const subir = (ev: PointerEvent) => {
        if (g.ativo) calcular(g, ev.clientX, ev.clientY, ev.altKey);
        fim(true);
      };
      const cancelar = () => fim(false);
      // Esc desiste do arrasto (sem passo); não chega aos atalhos da tela
      const tecla = (ev: KeyboardEvent) => {
        if (ev.key !== "Escape") return;
        ev.preventDefault();
        ev.stopPropagation();
        fim(false);
      };
      terminar.current = cancelar;
      alvo2.addEventListener("pointermove", mover);
      alvo2.addEventListener("pointerup", subir);
      alvo2.addEventListener("pointercancel", cancelar);
      window.addEventListener("keydown", tecla, true);
  };
  const pegarRef = useRef(pegar);
  pegarRef.current = pegar;
  const onPegar = useCallback((e: PE<HTMLElement>, it: ItemNaLinha, alvo: AlvoNoItem, t: TrilhaNaLinha) => pegarRef.current(e, it, alvo, t), []);
  useEffect(() => () => terminar.current?.(), []);

  // ---- fundo da pista: clique leva a cabeça; arrasto seleciona por área ----
  const noFundo = (e: PE<HTMLDivElement>) => {
    if (e.button !== 0) return;
    const alvo = e.target as HTMLElement;
    if (!alvo.dataset.pista) return;
    e.preventDefault();
    const pp = v.current.p;
    const el = conteudo.current;
    if (!el) return;
    const r0 = el.getBoundingClientRect();
    const x0 = e.clientX - r0.left;
    const y0 = e.clientY - r0.top;
    const q0 = Math.floor(slotDoPonteiro(e.clientX));
    const cap = capturarEm(e.currentTarget, e.pointerId);
    let ativo = false;
    let caixa: { x: number; y: number; w: number; h: number } | null = null;
    const mover = (ev: PointerEvent) => {
      const r = el.getBoundingClientRect();
      const x = ev.clientX - r.left;
      const y = ev.clientY - r.top;
      if (!ativo && Math.abs(x - x0) < LIMIAR_PX && Math.abs(y - y0) < LIMIAR_PX) return;
      ativo = true;
      caixa = { x: Math.min(x, x0), y: Math.min(y, y0), w: Math.abs(x - x0), h: Math.abs(y - y0) };
      setArea(caixa);
    };
    const subir = () => {
      cap.removeEventListener("pointermove", mover);
      cap.removeEventListener("pointerup", subir);
      cap.removeEventListener("pointercancel", subir);
      setArea(null);
      if (!ativo || !caixa) {
        if (!e.shiftKey) pp.acoes.selecionar({ itens: [], foco: null, efeito: null });
        pp.acoes.posicionar(q0);
        return;
      }
      pp.acoes.selecionar(selecaoDaArea(caixa, e.shiftKey));
    };
    cap.addEventListener("pointermove", mover);
    cap.addEventListener("pointerup", subir);
    cap.addEventListener("pointercancel", subir);
  };

  /** Itens cuja caixa cruza a área (em px do conteúdo). Barras de efeito ficam de fora (não são item da seleção). */
  const selecaoDaArea = (c: { x: number; y: number; w: number; h: number }, somar: boolean): Selecao => {
    const { trilhas: ts, p: pp } = v.current;
    const pxs = pxAgora();
    const itens: ItemSel[] = somar ? [...pp.selecao.itens] : [];
    let y = REGUA_PX;
    for (const t of ts) {
      const h = alturaDe(t);
      if (y < c.y + c.h && y + h > c.y)
        for (const it of t.itens) {
          if (it.k === "efeito" || it.apagado) continue;
          const x = CAB_PX + it.de_q * pxs;
          const x1 = CAB_PX + it.ate_q * pxs;
          if (x1 < c.x || x > c.x + c.w || itens.some((s) => s.id === it.id)) continue;
          const k = tipoItemSel(it) as ItemSel["k"];
          itens.push(t.tipo === "sobreposicao" || t.tipo === "audio" ? { k, id: it.id, trilha: t.id } : { k, id: it.id });
        }
      y += h;
    }
    return { itens, foco: itens[itens.length - 1] ?? null, efeito: null };
  };

  // ---- cabeças: ver, travar, altura ----
  const onVer = useCallback((t: TrilhaNaLinha) => {
    const pp = v.current.p;
    if (pp.soLeitura) return;
    if (t.tipo === "sobreposicao")
      pp.acoes.alterar("Ver", (d) => {
        const x = d.sobreposicoes.find((s) => s.id === t.id);
        if (x) x.oculta = !x.oculta;
      });
    else if (t.tipo === "audio")
      pp.acoes.alterar("Mudo", (d) => {
        const x = d.audios.find((s) => s.id === t.id);
        if (x) x.mudo = !x.mudo;
      });
  }, []);
  const onTravar = useCallback((t: TrilhaNaLinha) => {
    const pp = v.current.p;
    const tr = { ...pp.ui.travadas };
    if (tr[t.id]) delete tr[t.id];
    else tr[t.id] = true;
    pp.definirUi({ travadas: tr });
  }, []);
  const onAltura = useCallback(
    (e: PE<HTMLElement>, t: TrilhaNaLinha) => {
      if (e.button !== 0 || t.tipo === "efeito") return;
      e.preventDefault();
      e.stopPropagation();
      const el = capturarEm(e.currentTarget, e.pointerId);
      const y0 = e.clientY;
      const h0 = alturaDe(t);
      const mover = (ev: PointerEvent) => {
        const h = Math.min(Math.max(h0 + ev.clientY - y0, ALTURA_TRILHA_MIN), ALTURA_TRILHA_MAX);
        v.current.p.definirUi({ alturas: { ...v.current.p.ui.alturas, [t.id]: Math.round(h) } });
      };
      const subir = () => {
        el.removeEventListener("pointermove", mover);
        el.removeEventListener("pointerup", subir);
        el.removeEventListener("pointercancel", subir);
      };
      el.addEventListener("pointermove", mover);
      el.addEventListener("pointerup", subir);
      el.addEventListener("pointercancel", subir);
    },
    [alturaDe],
  );
  const onEntrada = useCallback((id: Id) => v.current.p.acoes.abrirEntrada(id), []);
  const itemDe = useCallback((f: Fantasma) => acharItem(v.current.trilhas, f.id, f.clipe), []);

  // ---- altura do painel ----
  const puxarPainel = (e: PE<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const el = capturarEm(e.currentTarget, e.pointerId);
    const y0 = e.clientY;
    const h0 = alturaDoPainel(p.ui.altura, typeof window !== "undefined" ? window.innerHeight : 0);
    const max = typeof window !== "undefined" ? Math.max(ALTURA_PAINEL_MIN, window.innerHeight * 0.7) : 2000;
    const mover = (ev: PointerEvent) => v.current.p.definirUi({ altura: Math.round(Math.min(Math.max(h0 - (ev.clientY - y0), ALTURA_PAINEL_MIN), max)) });
    const subir = () => {
      el.removeEventListener("pointermove", mover);
      el.removeEventListener("pointerup", subir);
      el.removeEventListener("pointercancel", subir);
    };
    el.addEventListener("pointermove", mover);
    el.addEventListener("pointerup", subir);
    el.addEventListener("pointercancel", subir);
  };

  // ---- lâmina: o corte segue o ponteiro ----
  const sobreConteudo = (e: PE<HTMLDivElement>) => {
    const l = laminaRef.current;
    if (!l) return;
    if (v.current.p.ui.ferramenta !== "lamina") {
      l.style.display = "none";
      return;
    }
    const q = Math.floor(slotDoPonteiro(e.clientX));
    l.style.display = "";
    l.style.transform = `translateX(${q * pxAgora()}px)`;
  };

  // ---- atalhos ----
  const ferramenta = (f: Ferramenta) => p.definirUi({ ferramenta: f });
  useShortcuts(
    {
      "edicao.ima": () => v.current.p.definirUi({ ima: !v.current.p.ui.ima }),
      "edicao.selecionar": () => v.current.p.definirUi({ ferramenta: "selecionar" }),
      "edicao.lamina": () => v.current.p.definirUi({ ferramenta: "lamina" }),
      "video.timelineZoomIn": () => zoom(1.5),
      "video.timelineZoomOut": () => zoom(1 / 1.5),
      "video.timelineZoomFit": () => zoom("ajustar"),
      // J e L: ré e à frente (K pausa pelo atalho do palco)
      "video.reverse": () => v.current.p.acoes.tocar(-1),
      "video.forward": () => v.current.p.acoes.tocar(1),
    },
    { enabled: p.atalhos },
  );

  const fantasmasDaTrilha = (t: TrilhaNaLinha): readonly Fantasma[] => {
    if (fantasmas.length === 0) return VAZIO;
    const r = fantasmas.filter((f) => f.trilha === t.id || f.origem === t.id);
    return r.length ? r : VAZIO;
  };
  const alturaPainel = alturaDoPainel(ui.altura, typeof window !== "undefined" ? window.innerHeight : 0);
  const alturaTotal = REGUA_PX + trilhas.reduce((s, t) => s + alturaDe(t), 0);
  const lamina = ui.ferramenta === "lamina";
  return (
    <div className={`${styles.painel} ${lamina ? styles.lamina : ""}`} style={{ height: alturaPainel }} data-linha-painel="" data-ferramenta={ui.ferramenta}>
      <div className={styles.puxador} onPointerDown={puxarPainel} aria-hidden />
      <div className={styles.barra}>
        <div className={styles.grupo} role="group">
          <button type="button" className={styles.ferr} aria-pressed={!lamina} onClick={() => ferramenta("selecionar")} data-ferramenta-botao="selecionar">
            <MousePointer2 size={14} aria-hidden />
            Selecionar
          </button>
          <button type="button" className={styles.ferr} aria-pressed={lamina} onClick={() => ferramenta("lamina")} data-ferramenta-botao="lamina">
            <Scissors size={14} aria-hidden />
            Lâmina
          </button>
        </div>
        <div className={styles.grupo}>
          <button type="button" className={styles.ferr} aria-pressed={ui.ima} onClick={() => p.definirUi({ ima: !ui.ima })} data-ima="">
            <Magnet size={14} aria-hidden />
            Ímã
          </button>
        </div>
        <span className={styles.espaco} />
        <button type="button" className={styles.icone} title="Afastar" aria-label="Afastar" onClick={() => zoom(1 / 1.5)} disabled={ui.pxPorS <= PX_POR_S_MIN} data-zoom="afastar">
          <ZoomOut size={15} />
        </button>
        <button type="button" className={styles.icone} title="Aproximar" aria-label="Aproximar" onClick={() => zoom(1.5)} disabled={ui.pxPorS >= pxMax} data-zoom="aproximar">
          <ZoomIn size={15} />
        </button>
        <button type="button" className={styles.icone} title="Ajustar à tela" aria-label="Ajustar à tela" onClick={() => zoom("ajustar")} data-zoom="ajustar">
          <Maximize2 size={14} />
        </button>
      </div>
      <div ref={rolo} className={styles.rolo} onScroll={aoRolar} data-linha-rolo="">
        <div
          ref={conteudo}
          className={styles.conteudo}
          style={{ width: `calc(${CAB_PX}px + ${larguraCss})`, height: alturaTotal, "--px": px } as CSSProperties}
          onPointerDown={noFundo}
          onPointerMove={sobreConteudo}
        >
          <div className={styles.reguaLinha} style={{ width: `calc(${CAB_PX}px + ${larguraCss})` }}>
            <div className={styles.canto} />
            <Regua
              pxPorS={pxS}
              rolagem_s={0}
              largura={largura}
              larguraCss={larguraCss}
              fps={fps}
              janela={janela}
              marcadores={marcadores}
              cabecaRef={cabecaTopoRef}
              onIr={(q) => p.acoes.posicionar(q)}
              onMarcador={(id, q) => {
                p.acoes.selecionar({ itens: [{ k: "marcador", id }], foco: { k: "marcador", id }, efeito: null });
                p.acoes.posicionar(q);
              }}
            />
          </div>
          {trilhas.map((t) => (
            <Trilha
              key={t.id}
              trilha={t}
              pxPorS={pxS}
              altura={alturaDe(t)}
              fps={fps}
              janela={janela}
              largura={largura}
              larguraCss={larguraCss}
              selecao={p.selecao}
              travada={!!ui.travadas[t.id]}
              soLeitura={p.soLeitura}
              fantasmas={fantasmasDaTrilha(t)}
              itemDe={itemDe}
              fonteDe={fonteDe}
              blocos={blocos}
              ws={p.ws}
              raizCache={p.raizCache}
              onPegar={onPegar}
              onEntrada={onEntrada}
              onVer={onVer}
              onTravar={onTravar}
              onAltura={onAltura}
            />
          ))}
          <div ref={cabecaRef} className={styles.cabeca} style={{ left: CAB_PX }} data-cabeca="" />
          <div ref={laminaRef} className={styles.guia} style={{ left: CAB_PX, display: "none", borderColor: "var(--sicro-danger)" }} aria-hidden />
          {guia !== null && <div className={styles.guia} style={{ left: `calc(${CAB_PX}px + var(--px) * ${guia}px)` }} data-guia={guia} />}
          {insercao !== null && <div className={styles.insercao} style={{ left: `calc(${CAB_PX}px + var(--px) * ${insercao}px)` }} data-insercao={insercao} />}
          {area && <div className={styles.area} style={{ left: area.x, top: area.y, width: area.w, height: area.h }} />}
        </div>
      </div>
    </div>
  );
}

/** Captura o ponteiro e devolve o elemento que recebe os eventos do arrasto. */
function capturarEm(el: Element, pointerId: number): HTMLElement {
  capturar(el, pointerId);
  return el as HTMLElement;
}
