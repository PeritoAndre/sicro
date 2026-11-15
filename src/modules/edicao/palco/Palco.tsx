/**
 * Palco no modo clipe (§7.1, §7.2, §7.4, Fase 0): a mídia do clipe pela `PreviaMidia` (um `<video>` de 2×2 px
 * dentro do palco) pintada num canvas com A(τ) e as primitivas do clipe no instante; alças, ferramenta de
 * marcação, transporte da mídia inteira, zoom de vista, "Quadro final" e "Prévia do render".
 * O tempo do elemento só passa pela porta (`motor/elemento.ts` + `relogioMidia`); busca no meio do quadro, uma por
 * vez, pintura no `seeked` mais um rAF. Toda mudança do documento passa por `alterar`/`iniciarGesto`.
 * Fase 1 (§7.5, §7.6): clipe com efeito de origem toca o intermediário quando pronto (o original para marcar
 * `ocultar` e fora da faixa do clipe); anotação só se marca sobre ele. A busca pausada é medida uma vez por mídia e,
 * lenta (ou com o clipe abaixo de 0,25×), o palco pede o proxy. Ré = modo guiado (elemento parado, quadro por busca).
 */

import { forwardRef, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState, type WheelEvent as WE } from "react";
import { CircleAlert, Gauge, Loader2, VideoOff } from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands } from "@core/commands";
import { mediaSrc } from "@core/mediaSrc";
import { caminhoDoCaso } from "@core/caminhoDoCaso";
import type { Alca, Caixa, ClipePrincipal, ClipeTrecho, Id, Lugar, MidiaRef, PalcoProps, Ponto, PreviaMidia, Primitiva, Pts, Seg, TabelaQuadros } from "@domain/edicao";
import { useEdicaoStore, type Gesto } from "../store/edicaoStore";
import { useRenderStore } from "../store/renderStore";
import { useFundoStore, fundoPorChave, chaveFundo } from "../store/fundoStore";
import { acharClipe, acharEfeito, acharMidia, acharRastreio, aplicarMudanca } from "../modelo/projeto";
import { aDe, caixaNoCanvas, inversa, ponto as aplicar, type Afim } from "../modelo/geometria";
import { exibido, mapaTrecho, nQuadros, ptsDaTabela, resolver, tempoDoQuadro, type MapaQuadros } from "../modelo/tempo";
import { tabelaDoClipe } from "../modelo/realce";
import { avaliar as avaliarCurva } from "../modelo/curva";
import { relogioMidia, type RelogioMidia } from "./motor/relogioMidia";
import { elementoDe, type ElementoBusca } from "./motor/elemento";
import { Buscador } from "./motor/busca";
import { pintar } from "./desenho/pintar";
import { cenaDoClipe, duracaoLocal, instanteDaAncora, instanteDaChave, midiaDoClipe, type FontesCena, type Instante } from "./cena";
import { LADO_MINIMO, aDaGirada, aDaOrigem, alcaDaAncora, alcaDeLugar, alcaDoLugarFixo, alcaNaOrigem, alcasDaRegiao, alcasDeCaixa, alcasDePonto, proporcaoDaSaida } from "./alcas";
import { bordaArrastada, entradaNoCursor, faixaDoClipe, marcaLivre, marcarBloqueado, meioDoQuadro, saidaNoCursor } from "./faixa";
import { QuadroFiel, type Fiel } from "./quadroFiel";
import { CamadaEdicao, type Modificadores } from "./camadaEdicao";
import { Transporte, type EstadoRender, type Fase } from "./Transporte";
import { alvosDaMedida, atlasDe, garantirMidia, imagemDe, p50, pedirProxy, precisaDeProxy, previaDaMidia, proxyPedido, recursoDe, registrarBusca, spriteDe, urlDoCache, useRecursos } from "./recursos";
import { camposPorQuadro, marcaForaDaOrigem, mudaAImagem, previaDoPalco, relogioDaOrigem, slotsDoQuadroDaFonte } from "./origem";
import { useOrigem } from "../inspetor/origemDoClipe";
import { useOrigemStore } from "../store/origemStore";
import { PASSO_ZOOM, VISTA_INICIAL, caixaMedida, contido, deslocar, limitar, mostrado, zoomEm, zoomMaximo, type Tamanho, type Vista } from "./vista";
import styles from "./Palco.module.css";
// Fase 2 (contratos-fase2.md (c)): corrida do rastreio no palco (P2.4), só com a bandeira dele
import { FerramentaSeguir } from "../rastreio/FerramentaSeguir";
import { RASTREIO_PRONTO } from "../store/rastreioStore";

/** Tempo parado (ms) depois do último quadro pintado até o palco medir a busca da mídia (§7.6). */
export const MEDIR_APOS_MS = 800;
/** Tentativas de medida por mídia (cada busca do perito no meio cancela uma). */
export const MEDIDAS_MAX = 3;

export interface PalcoApi {
  tocarPausar(): void;
  // CONTRATO: acréscimo (fecho da Fase 1, O3; contratos-fase1 F2).
  /** Toca para a frente (1) ou dá ré (−1, modo guiado) até `pausar`; já tocando nesse sentido, nada muda. */
  tocar(d: 1 | -1): void;
  /** Para de tocar (a Edição saiu da frente). */
  pausar(): void;
  passo(d: 1 | -1): void;
  inicio(): void;
  fim(): void;
  /** I e O: grava o quadro exibido (pode alargar o clipe). */
  entrada(): void;
  saida(): void;
  zoom(f: number | "ajustar"): void;
  quadroFinal(): void;
  previaRender(): void;
  /** Esc: sai da prévia do render; true se havia o que parar. */
  cancelar(): boolean;
  /** Tempo do efeito no cursor (base do clipe). */
  tempo(): Seg;
  /** Par (t, pts) que uma marca grava agora: com `rastreio`, o da âncora dele (base fonte ou local); sem, o da
   *  chave de efeito do clipe; null sem clipe. */
  instante(rastreio?: Id): { t: Seg; pts: Pts | null } | null;
  irPara(t: Seg): void;
}

interface Props extends PalcoProps {
  expandido: boolean;
  onExpandir(): void;
  /** Som do elemento (atalho de mudo). */
  mudo?: boolean;
}

/** Endereço que o `<video>` toca; null enquanto a prévia não está pronta. */
export function srcDaPrevia(ws: string, raizCache: string | null, p: PreviaMidia): string | null {
  if (!p.pronta) return null;
  if (p.tipo === "original") return mediaSrc(caminhoDoCaso(ws, p.relativo));
  return raizCache ? mediaSrc(`${raizCache.replace(/[\\/]+$/, "")}/${p.relativo}`) : null;
}

/** Imagem de primitiva: "midia:<id>" = imagem do projeto (relativa ao caso); o resto, do cache da Edição. */
function urlDaPrimitiva(src: string, ws: string, raizCache: string | null, midias: readonly MidiaRef[]): string | null {
  if (src.startsWith("midia:")) {
    const m = midias.find((x) => x.id === src.slice(6));
    return m ? convertFileSrc(caminhoDoCaso(ws, m.relativo)) : null;
  }
  return raizCache ? urlDoCache(raizCache, src) : null;
}

const tamanhoDaImagem = (img: HTMLImageElement | null) => (img ? { w: img.naturalWidth, h: img.naturalHeight } : null);

/** Primitivas que o clipe sem fonte desenha por si (título e imagem). */
function primitivasProprias(c: ClipePrincipal, W: number, H: number, fontes: FontesCena): Primitiva[] {
  if (c.tipo === "imagem") {
    const t = fontes.midia(c.midia);
    if (!t || !(t.w > 0 && t.h > 0)) return [];
    // imagem inteira contida no canvas
    const r = Math.min(W / t.w, H / t.h);
    const w = (t.w * r) / W;
    const h = (t.h * r) / H;
    return [{ k: "sprite", src: `midia:${c.midia}`, caixa: { x: (1 - w) / 2, y: (1 - h) / 2, w, h }, opacidade: 1 }];
  }
  if (c.tipo !== "titulo") return [];
  const saida: Primitiva[] = [];
  const f = c.titulo.fundo;
  if (f.tipo === "cor") saida.push({ k: "caminho", d: [["M", 0, 0], ["L", 1, 0], ["L", 1, 1], ["L", 0, 1], ["Z"]], preenche: f.cor });
  else if (f.tipo === "imagem") saida.push(...primitivasProprias({ ...c, tipo: "imagem", midia: f.midia } as unknown as ClipePrincipal, W, H, fontes));
  for (const b of c.titulo.blocos) {
    if (b.conteudo === "") continue;
    const s = fontes.sprite({ blocos: [b], largura_max: (b.largura_max * W) / H, altura_canvas: H, fundo: true });
    if (!s) continue;
    const fh = b.lugar.h === "esq" ? 0 : b.lugar.h === "centro" ? 0.5 : 1;
    const fv = b.lugar.v === "topo" ? 0 : b.lugar.v === "meio" ? 0.5 : 1;
    const x = Math.floor(b.lugar.x * W - s.w * fh + 0.5);
    const y = Math.floor(b.lugar.y * H - s.h * fv + 0.5);
    saida.push({ k: "sprite", src: s.relativo, caixa: { x: x / W, y: y / H, w: s.w / W, h: s.h / H }, opacidade: 1 });
  }
  return saida;
}

/** Rastreio que o efeito selecionado usa (destaque, lupa, ocultar, holofote, velocidade que acompanha). */
function rastreioDoEfeito(e: unknown): Id | null {
  const x = e as { tipo?: string; rastreio?: Id; alvo?: { rastreio?: Id }; onde?: { modo?: string; rastreio?: Id } };
  if (x.tipo === "destaque" || x.tipo === "lupa") return x.rastreio ?? null;
  if (x.tipo === "ocultar" || x.tipo === "holofote") return x.alvo?.rastreio ?? null;
  if (x.tipo === "velocidade_exibida" && x.onde?.modo === "segue") return x.onde.rastreio ?? null;
  return null;
}

/** Tamanho do canvas: o mostrado na tela (px reais), sem passar da saída. */
function tamanhoDoCanvas(mostradoW: number, mostradoH: number, saida: Tamanho): Tamanho {
  const dpr = typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1;
  const k = Math.min(1, saida.w / Math.max(1, mostradoW * dpr), saida.h / Math.max(1, mostradoH * dpr));
  return { w: Math.max(1, Math.round(mostradoW * dpr * k)), h: Math.max(1, Math.round(mostradoH * dpr * k)) };
}

/** Pede e acompanha o intermediário de origem do clipe do palco, mesmo com a aba Clipe fechada (§7.5). */
function PedidoDeOrigem({ clipe }: { clipe: ClipePrincipal }) {
  useOrigem(clipe);
  return null;
}

/** Medidas de busca já tentadas por mídia nesta sessão. */
const tentativas = new Map<string, number>();
/** Só para testes. */
export const esquecerMedidas = () => tentativas.clear();

export const Palco = forwardRef<PalcoApi, Props>(function Palco(props, ref) {
  const { ferramenta, onMarcado } = props;
  const doc = useEdicaoStore((s) => s.doc);
  const ws = useEdicaoStore((s) => s.ws);
  const raizCache = useEdicaoStore((s) => s.raizCache);
  const cursor = useEdicaoStore((s) => s.cursor);
  const selecao = useEdicaoStore((s) => s.selecao);
  const soLeitura = useEdicaoStore((s) => s.soLeitura);
  const versao = useRecursos((s) => s.versao);
  const fundo = useFundoStore((s) => s.tarefas);

  const clipeId = cursor.modo === "clipe" ? cursor.clipe : selecao.foco?.k === "clipe" ? selecao.foco.id : null;
  const achado = doc && clipeId ? acharClipe(doc, clipeId) : null;
  const clipe = achado?.onde === "principal" ? achado.clipe : null;
  const midiaId = clipe ? midiaDoClipe(clipe) : null;
  const ref_ = doc && midiaId ? acharMidia(doc, midiaId) : null;
  const sha = ref_?.sha256 ?? null;
  useEffect(() => {
    if (ws && sha) garantirMidia(ws, sha);
  }, [ws, sha]);
  void versao;
  const rec = recursoDe(sha);
  const info = rec.info;
  const tabela: TabelaQuadros | null = rec.tabela;
  // ---- o que o elemento toca: o intermediário de origem do clipe (§7.5) ou a prévia da mídia (o proxy pedido, §7.6)
  const origem = useOrigemStore((s) => (clipe ? s.porClipe[clipe.id] : undefined));
  const alvoDaMarca = ferramenta?.efeito ?? selecao.efeito;
  /** A marca em curso mora no espaço ORIGINAL (ocultar): o palco mostra o original. */
  const marcaNoOriginal = !!doc && !!alvoDaMarca && acharEfeito(doc, alvoDaMarca)?.efeito.tipo === "ocultar";
  // instante mostrado (o cursor da store: vale já no primeiro desenho do clipe)
  const tNoClipe = clipe?.tipo === "trecho" ? (cursor.modo === "clipe" && cursor.clipe === clipe.id ? cursor.t : clipe.inicio_s) : 0;
  const escolhida = previaDoPalco({ origem: clipe?.tipo === "trecho" ? origem : undefined, daMidia: previaDaMidia(rec), t: tNoClipe, original: marcaNoOriginal });
  const previa = escolhida.previa;
  const daOrigem = escolhida.daOrigem;
  const campos: 1 | 2 = clipe && tabela ? camposPorQuadro(tabela, clipe.efeitos) : 1;
  const relogio: RelogioMidia | null = useMemo(
    () => (previa && tabela && previa.pronta ? (daOrigem ? relogioDaOrigem(previa, tabela, campos) : relogioMidia(previa, tabela)) : null),
    [previa?.relativo, previa?.tipo, previa?.pronta, previa?.desloc_s, previa?.de_s, previa?.ate_s, tabela, daOrigem, campos], // eslint-disable-line react-hooks/exhaustive-deps
  );
  const src = ws && previa ? srcDaPrevia(ws, raizCache, previa) : null;
  /** Anotação só se marca sobre a imagem do intermediário: o clipe muda a imagem e o palco mostra outra. */
  const semImagemDaOrigem = !!clipe && mudaAImagem(clipe) && !daOrigem;
  const saida: Tamanho = { w: doc?.saida.largura ?? 1920, h: doc?.saida.altura ?? 1080 };
  const fps = doc?.saida.fps ?? { num: 30, den: 1 };

  // ---- linha resolvida e mapa do clipe (slot do programa no cursor) ----
  const linha = useMemo(() => (doc ? resolver(doc, { tabela: (id) => recursoDe(acharMidia(doc, id)?.sha256).tabela }) : null), [doc, versao]); // eslint-disable-line react-hooks/exhaustive-deps
  const resolvido = linha?.clipes.find((x) => x.id === clipe?.id) ?? null;
  // o mapa é feito na tabela do CLIPE (a de campos no desentrelace por campo), como o resolver e o motor
  const mapa: MapaQuadros | null = useMemo(() => {
    if (!clipe || clipe.tipo !== "trecho" || !tabela) return null;
    try {
      const tab = tabelaDoClipe(tabela, clipe.efeitos);
      const [a, b] = faixaDoClipe(clipe, tab);
      return mapaTrecho(tab, a, b, clipe.velocidade, fps);
    } catch {
      return null;
    }
  }, [clipe, tabela, fps]);

  // ---- estado do motor (fora do React) ----
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const caixaRef = useRef<HTMLDivElement | null>(null);
  const rascunhoRef = useRef<HTMLCanvasElement | null>(null);
  const elRef = useRef<ElementoBusca | null>(null);
  const buscadorRef = useRef<Buscador | null>(null);
  /** Quadro mostrado (mídia) ou tempo local (clipe sem fonte). */
  const jRef = useRef<number | null>(null);
  /** Quadro pedido que ainda não chegou: se o arquivo tocado muda antes (o passo cruzou a faixa do intermediário), o
   *  elemento novo vai a ELE, não ao que estava na tela. */
  const pedidoRef = useRef<number | null>(null);
  const tLocalRef = useRef(0);
  const [tocando, setTocando] = useState(false);
  const [mostra, setMostra] = useState<{ t: Seg; j: number | null }>({ t: 0, j: null });
  const [alcas, setAlcas] = useState<Alca[]>([]);
  const [contorno, setContorno] = useState<Caixa | null>(null);
  const [caixa, setCaixa] = useState<Tamanho>({ w: 0, h: 0 });
  const [vista, setVista] = useState<Vista>(VISTA_INICIAL);
  const [fiel, setFiel] = useState<Fiel>("viva");
  const [render, setRender] = useState<{ tarefa: Id | null; src: string | null } | null>(null);
  /** A última prévia do render falhou (ou não entrou na fila): ícone no botão até pedir de novo ou trocar de clipe. */
  const [renderFalhou, setRenderFalhou] = useState(false);
  /** Endereço que o elemento não conseguiu abrir (arquivo sumiu, codec sem decodificador). */
  const [falhaSrc, setFalhaSrc] = useState<string | null>(null);
  const gestoRef = useRef<{ g: Gesto } | null>(null);
  /** Último cursor que o palco mesmo pôs no store (para seguir só o que veio de fora: desfazer, inspetor). */
  const postadoRef = useRef<string>("");

  const img = useMemo(() => contido(caixa, saida), [caixa.w, caixa.h, saida.w, saida.h]); // eslint-disable-line react-hooks/exhaustive-deps
  const zMax = zoomMaximo(img, saida);
  const m = mostrado(vista, img);
  const tamCanvas = tamanhoDoCanvas(m.w, m.h, saida);

  // vista de origem: enquadramento selecionado (ou ferramenta de região)
  const efeitoSel = doc && selecao.efeito ? acharEfeito(doc, selecao.efeito) : null;
  const modoOrigem = !!clipe && (efeitoSel?.efeito.tipo === "enquadramento" || ferramenta?.tipo === "regiao");

  const bloqueado = !clipe || clipe.tipo !== "trecho" || !ref_ || marcarBloqueado(info, tabela) || soLeitura;

  // ---- referências vivas para o laço e os eventos ----
  const vivo = useRef({ doc, clipe, tabela, relogio, mapa, resolvido, ws, raizCache, ref_, info, modoOrigem, efeitoSel, ferramenta, saida, fps, sha, previa, daOrigem, campos, semImagemDaOrigem });
  vivo.current = { doc, clipe, tabela, relogio, mapa, resolvido, ws, raizCache, ref_, info, modoOrigem, efeitoSel, ferramenta, saida, fps, sha, previa, daOrigem, campos, semImagemDaOrigem };

  const fontes: FontesCena = useMemo(
    () => ({
      sprite: (p) => (vivo.current.ws ? spriteDe(vivo.current.ws, p) : null),
      atlas: (e, h) => (vivo.current.ws ? atlasDe(vivo.current.ws, e, h) : null),
      midia: (id) => {
        const v = vivo.current;
        if (!v.doc || !v.ws) return null;
        const url = urlDaPrimitiva(`midia:${id}`, v.ws, v.raizCache, v.doc.midias);
        return url ? tamanhoDaImagem(imagemDe(url)) : { w: 0, h: 0 };
      },
    }),
    [],
  );

  /** Instante do cursor: tempo na base do clipe, quadro, slot. */
  const instante = useCallback((): Instante | null => {
    const v = vivo.current;
    const c = v.clipe;
    if (!c || !v.doc) return null;
    const ini = v.resolvido?.inicio_q ?? 0;
    const slot = (q: number) => ({ q, programa_s: (q * v.fps.den) / v.fps.num });
    if (c.tipo === "trecho" && v.tabela) {
      const j = jRef.current ?? exibido(v.tabela, c.inicio_s);
      const local = v.mapa ? slotsDoQuadroDaFonte(v.mapa, j, v.campos) : null;
      const q = ini + (local ? local[0] : j * v.campos < (v.mapa?.j_in ?? 0) ? 0 : Math.max(0, (v.resolvido?.n_q ?? 1) - 1));
      return { t: tempoDoQuadro(v.tabela, j), pts: ptsDaTabela(v.tabela)[j] as number, indice: j, ...slot(q) };
    }
    const tl = tLocalRef.current;
    const q = ini + Math.min(Math.max(0, Math.round((tl * v.fps.num) / v.fps.den)), Math.max(0, (v.resolvido?.n_q ?? 1) - 1));
    if (c.tipo === "congelar" && v.tabela) {
      const j = exibido(v.tabela, c.instante_s);
      return { t: tl, pts: c.instante_pts, indice: j, ...slot(q) };
    }
    return { t: tl, pts: null, indice: null, ...slot(q) };
  }, []);

  /** Marca no quadro mostrado só com a imagem dele na tela (h 17 e 27). `naOrigem`: marca de anotação (espaço de
   *  origem do clipe), que só vale sobre a imagem do intermediário (§7.5). */
  const livre = useCallback((naOrigem = false): boolean => {
    const v = vivo.current;
    if (naOrigem && v.semImagemDaOrigem) return false;
    return marcaLivre(v.clipe?.tipo ?? null, v.info, v.tabela, buscadorRef.current, jRef.current);
  }, []);

  /** Slot que o "Quadro final" pede; null fora do clipe. */
  const slotDoCursor = useCallback((): number | null => {
    const v = vivo.current;
    const c = v.clipe;
    if (!c || !v.resolvido) return null;
    if (c.tipo === "trecho") {
      const j = jRef.current;
      if (j === null || !v.mapa) return null;
      const l = slotsDoQuadroDaFonte(v.mapa, j, v.campos);
      return l ? v.resolvido.inicio_q + l[0] : null;
    }
    return instante()?.q ?? null;
  }, [instante]);

  // ---- pintura ----
  const pintarAgora = useCallback(() => {
    // medindo a busca o elemento está em outro quadro: o que pede pintura (edição, recurso que chegou) cancela a
    // medida e volta ao quadro mostrado; a pintura sai quando ele chegar
    const bus = buscadorRef.current;
    if (bus?.medindo) {
      bus.cancelarMedida();
      if (jRef.current !== null) bus.ir(jRef.current);
      return;
    }
    // a imagem do elemento ainda é a de uma busca da medida: a pintura sai quando ele chegar ao quadro pedido
    // (trecho tocando para a frente: o laço pinta o quadro que o elemento diz)
    if (bus?.imagemAlheia && !(tocandoRef.current && sentidoRef.current === 1 && vivo.current.clipe?.tipo === "trecho")) return;
    const cv = canvasRef.current;
    const ctx = cv?.getContext("2d", { alpha: false });
    const v = vivo.current;
    if (!cv || !ctx || !v.doc) return;
    const c = v.clipe;
    const W = cv.width;
    const H = cv.height;
    const canvasSaida = { w: v.saida.w, h: v.saida.h };
    const imagem = (s: string) => {
      const url = v.ws ? urlDaPrimitiva(s, v.ws, v.raizCache, v.doc!.midias) : null;
      return url ? imagemDe(url) : null;
    };
    if (!c) {
      pintar({ ctx, w: W, h: H, fundo: v.doc.saida.cor_fundo, video: null, A: [1, 0, 0, 1, 0, 0], primitivas: [], imagem });
      return;
    }
    const inst = instante();
    if (!inst) return;
    const origem = v.info ? { w: v.info.exibido.w, h: v.info.exibido.h } : canvasSaida;
    const temVideo = (c.tipo === "trecho" || c.tipo === "congelar") && !!v.relogio && !!videoRef.current && videoRef.current.readyState >= 2;
    let A: Afim;
    let prims: Primitiva[] = [];
    if (v.modoOrigem) {
      A = aDaOrigem(c.efeitos, origem, canvasSaida);
    } else {
      A = aDe(c.efeitos, inst.t, origem, canvasSaida);
      const cena = cenaDoClipe(v.doc, c, inst, origem, fontes, A);
      prims = [...primitivasProprias(c, canvasSaida.w, canvasSaida.h, fontes), ...cena.primitivas];
    }
    pintar({
      ctx,
      w: W,
      h: H,
      fundo: v.doc.saida.cor_fundo,
      video: temVideo ? videoRef.current : null,
      A,
      primitivas: prims,
      imagem,
      rascunho: () => (rascunhoRef.current ??= document.createElement("canvas")),
    });
    // alças do item selecionado neste quadro
    const novas: Alca[] = [];
    let cont: Caixa | null = null;
    const e = v.efeitoSel?.efeito;
    if (e && !v.ferramenta && !soLeituraRef.current) {
      if (e.tipo === "enquadramento" && v.modoOrigem) {
        const g = aDaGirada(c.efeitos, origem, canvasSaida);
        const r = e.regiao;
        const regiao = Array.isArray(r) && r.length === 0 ? { x: 0, y: 0, w: 1, h: 1 } : avaliarCurva(r, inst.t);
        const k = instanteDaChave(c, inst);
        novas.push(...alcasDaRegiao(e, regiao, k.t, k.pts, g, proporcaoDaSaida(c.efeitos, origem, canvasSaida)));
        cont = caixaNoCanvas(g, regiao);
      } else if (e.tipo === "texto" || e.tipo === "carimbo" || e.tipo === "imagem") {
        novas.push(alcaDeLugar(e));
      } else if (e.tipo === "regua") {
        novas.push(...alcaDaAncora(e, A));
      } else {
        // centro da lupa fixa e lugar da velocidade fixa (canvas); o alvo delas, pelo rastreio
        if (e.tipo === "lupa" || e.tipo === "velocidade_exibida") novas.push(...alcaDoLugarFixo(e));
        const rid = rastreioDoEfeito(e);
        const r = rid ? acharRastreio(v.doc, rid) : null;
        const q = r ? cenaDoClipe(v.doc, c, inst, origem, fontes, A).quadro : null;
        const cx = r && q ? q.rastreio(r.id) : null;
        if (r && cx) {
          // ocultar mora no espaço original: o A do palco leva fonte → canvas
          const em = instanteDaAncora(c, r.base, inst);
          if (r.tipo === "ponto") novas.push(...alcasDePonto(r.id, { x: cx.x, y: cx.y }, A, em.t, em.pts));
          else novas.push(...alcasDeCaixa(r.id, cx, A, em.t, em.pts));
        }
      }
    }
    // tocando sem seleção nada muda: não redesenha a árvore a cada quadro
    setAlcas((a) => (a.length === 0 && novas.length === 0 ? a : novas));
    setContorno((c0) => (c0 === null && cont === null ? c0 : cont));
  }, [fontes, instante]);
  const soLeituraRef = useRef(soLeitura);
  soLeituraRef.current = soLeitura;

  // ---- elemento, relógio e busca ----
  useEffect(() => {
    const v = videoRef.current;
    if (!v || !relogio || !src) {
      elRef.current = null;
      buscadorRef.current?.fechar();
      buscadorRef.current = null;
      return;
    }
    const el = elementoDe(v, relogio);
    elRef.current = el;
    const b = new Buscador(el, (j, final) => {
      jRef.current = j;
      if (final && pedidoRef.current === j) pedidoRef.current = null;
      // o congelado mostra o tempo local dele; o trecho, o meio do quadro
      setMostra((m0) => (vivo.current.clipe?.tipo === "trecho" ? { t: relogio.meio(j), j } : { t: m0.t, j }));
      pintarAgora();
      if (final) agendarMedida();
    });
    buscadorRef.current = b;
    const pronto = () => {
      const c = vivo.current.clipe;
      const tab = vivo.current.tabela;
      if (!c || !tab) return;
      const alvo = c.tipo === "congelar" ? exibido(tab, c.instante_s) : pedidoRef.current ?? jRef.current ?? exibido(tab, cursorT.current ?? (c.tipo === "trecho" ? c.inicio_s : 0));
      b.ir(alvo);
    };
    v.addEventListener("loadeddata", pronto);
    if (v.readyState >= 2) pronto();
    return () => {
      v.removeEventListener("loadeddata", pronto);
      desmarcarMedida();
      b.fechar();
      if (buscadorRef.current === b) buscadorRef.current = null;
      // o arquivo tocado mudou (proxy ou intermediário que ficou pronto) com o clipe tocando: para no quadro mostrado
      if (tocandoRef.current && vivo.current.clipe?.tipo === "trecho") {
        if (laco.current !== null) cancelAnimationFrame(laco.current);
        laco.current = null;
        tocandoRef.current = false;
        setTocando(false);
        el.pausar();
      }
    };
  }, [relogio, src, pintarAgora]); // eslint-disable-line react-hooks/exhaustive-deps

  // ---- busca medida (§7.6): 3 buscas pausadas com a mídia parada; p50 lento pede o proxy ----
  const relogioDaMedida = useRef<ReturnType<typeof setTimeout> | null>(null);
  /** Ponteiro sobre o palco: quem vai marcar ou arrastar uma alça não pode achar o elemento em outro quadro. */
  const sobreRef = useRef(false);
  const desmarcarMedida = useCallback(() => {
    if (relogioDaMedida.current !== null) clearTimeout(relogioDaMedida.current);
    relogioDaMedida.current = null;
  }, []);
  const agendarMedida = useCallback(() => {
    desmarcarMedida();
    const v = vivo.current;
    if (!v.ws || !v.sha || recursoDe(v.sha).busca_p50_ms !== undefined) return;
    relogioDaMedida.current = setTimeout(() => {
      relogioDaMedida.current = null;
      const x = vivo.current;
      const b = buscadorRef.current;
      const [ws0, sha0] = [x.ws, x.sha];
      if (!b || !ws0 || !sha0 || !x.tabela || tocandoRef.current || b.alvo !== null) return;
      if (recursoDe(sha0).busca_p50_ms !== undefined) return;
      // marcação em curso ou prestes (ferramenta ligada, ponteiro no palco): durante a medida ela seria recusada; a
      // medida fica para depois
      if (x.ferramenta || sobreRef.current) return agendarMedida();
      // só o original e a cópia se medem; com o proxy já na fila de fundo não há o que decidir
      const mede = !x.daOrigem && (x.previa?.tipo === "original" || x.previa?.tipo === "copia") && !fundoPorChave(useFundoStore.getState().tarefas, chaveFundo("proxy", sha0));
      const alvos = mede ? alvosDaMedida(x.tabela, b.ultimo) : [];
      const n = (tentativas.get(sha0) ?? 0) + 1;
      if (alvos.length < 3 || n > MEDIDAS_MAX) return void (mede && registrarBusca(ws0, sha0, null));
      tentativas.set(sha0, n);
      void b.medir(alvos).then((tempos) => {
        if (tempos && tempos.length > 0) registrarBusca(ws0, sha0, p50(tempos));
      });
    }, MEDIR_APOS_MS);
  }, [desmarcarMedida]);
  useEffect(() => desmarcarMedida, [desmarcarMedida]);

  // o palco pede o proxy: busca medida lenta, ou clipe com rampa, ré ou abaixo de 0,25× (§7.6)
  const buscaMs = rec.busca_p50_ms;
  const pedeProxy = !!clipe && clipe.tipo === "trecho" && !!sha && !!tabela && precisaDeProxy(clipe, buscaMs);
  useEffect(() => {
    if (!ws || !clipe || !sha || !pedeProxy || proxyPedido(sha)) return;
    void pedirProxy(ws, clipe.id, sha, buscaMs ?? null);
  }, [ws, clipe?.id, sha, pedeProxy, buscaMs]); // eslint-disable-line react-hooks/exhaustive-deps

  const cursorT = useRef<number | null>(null);
  cursorT.current = cursor.modo === "clipe" ? cursor.t : null;

  /** Põe o cursor no store (ao pausar, buscar e a cada 250 ms tocando). */
  const postar = useCallback((t: Seg) => {
    const c = vivo.current.clipe;
    if (!c) return;
    const chave = `${c.id}:${t}`;
    if (postadoRef.current === chave) return;
    postadoRef.current = chave;
    useEdicaoStore.getState().posicionar({ modo: "clipe", clipe: c.id, t });
  }, []);

  const fielRef = useRef<QuadroFiel | null>(null);
  if (!fielRef.current) {
    fielRef.current = new QuadroFiel(async (q) => {
      const v = vivo.current;
      if (!v.ws || !v.doc || !v.raizCache) throw new Error("sem caso");
      const r = await commands.edicaoQuadroFinal(v.ws, v.doc, q, Math.min(v.saida.w, Math.max(64, canvasRef.current?.width ?? v.saida.w)));
      return urlDoCache(v.raizCache, r.relativo);
    }, setFiel);
  }
  useEffect(() => () => fielRef.current?.fechar(), []);

  /** Vai ao quadro j (mídia) ou ao tempo local t (sem fonte). */
  const irQuadro = useCallback(
    (j: number, passo = false) => {
      const v = vivo.current;
      if (!v.tabela) return;
      const k = Math.min(Math.max(j, 0), nQuadros(v.tabela) - 1);
      const t = meioDoQuadro(v.tabela, k);
      pedidoRef.current = k;
      postar(t);
      const b = buscadorRef.current;
      // o número do quadro acompanha a IMAGEM: com busca em curso só muda quando o quadro chega
      setMostra((m0) => ({ t, j: b ? m0.j : k }));
      if (b) b.ir(k);
      else {
        jRef.current = k;
        pintarAgora();
      }
      const q = (() => {
        if (!v.mapa || !v.resolvido) return null;
        const l = slotsDoQuadroDaFonte(v.mapa, k, v.campos);
        return l ? v.resolvido.inicio_q + l[0] : null;
      })();
      if (passo) fielRef.current?.passo(q);
      else fielRef.current?.pausou(q);
    },
    [postar, pintarAgora],
  );

  const irLocal = useCallback(
    (t: Seg) => {
      const c = vivo.current.clipe;
      if (!c || c.tipo === "trecho") return;
      const tl = Math.min(Math.max(t, 0), duracaoLocal(c));
      tLocalRef.current = tl;
      setMostra((m0) => ({ t: tl, j: m0.j }));
      postar(tl);
      pintarAgora();
      fielRef.current?.pausou(slotDoCursor());
    },
    [postar, pintarAgora, slotDoCursor],
  );

  // clipe trocado: vai ao cursor (ou ao começo)
  useEffect(() => {
    const c = clipe;
    jRef.current = null;
    pedidoRef.current = null;
    tLocalRef.current = 0;
    setRender(null);
    setRenderFalhou(false);
    if (!c) {
      pintarAgora();
      return;
    }
    if (c.tipo === "trecho" && tabela) {
      const j = exibido(tabela, cursorT.current ?? c.inicio_s);
      jRef.current = j;
      setMostra({ t: meioDoQuadro(tabela, j), j });
      buscadorRef.current?.ir(j);
    } else if (c.tipo === "congelar") {
      tLocalRef.current = Math.min(Math.max(cursorT.current ?? 0, 0), duracaoLocal(c));
      const j = tabela ? exibido(tabela, c.instante_s) : null;
      jRef.current = j;
      setMostra({ t: tLocalRef.current, j });
      if (j !== null) buscadorRef.current?.ir(j);
    } else if (c.tipo !== "trecho") {
      tLocalRef.current = Math.min(Math.max(cursorT.current ?? 0, 0), duracaoLocal(c));
      setMostra({ t: tLocalRef.current, j: null });
    }
    pintarAgora();
    // parado no clipe novo: o quadro final é pedido como depois de uma pausa
    fielRef.current?.pausou(slotDoCursor());
  }, [clipe?.id, tabela]); // eslint-disable-line react-hooks/exhaustive-deps

  // cursor mudado de fora (desfazer, inspetor): segue
  useEffect(() => {
    if (cursor.modo !== "clipe" || !clipe || cursor.clipe !== clipe.id) return;
    if (postadoRef.current === `${clipe.id}:${cursor.t}`) return;
    postadoRef.current = `${clipe.id}:${cursor.t}`;
    if (clipe.tipo === "trecho" && tabela) {
      const j = exibido(tabela, cursor.t);
      if (j !== jRef.current) irQuadro(j);
    } else if (clipe.tipo !== "trecho") irLocal(cursor.t);
  }, [cursor]); // eslint-disable-line react-hooks/exhaustive-deps

  // documento, recursos ou ferramenta mudaram: repinta parado; o quadro final é pedido de novo
  useEffect(() => {
    if (!tocandoRef.current) pintarAgora();
  }, [doc, versao, selecao, ferramenta, tamCanvas.w, tamCanvas.h, pintarAgora]);
  useEffect(() => {
    fielRef.current?.editou();
  }, [doc]);

  // ---- tocar ----
  const tocandoRef = useRef(false);
  const laco = useRef<number | null>(null);
  const parar = useCallback(() => {
    if (laco.current !== null) cancelAnimationFrame(laco.current);
    laco.current = null;
    if (!tocandoRef.current) return;
    tocandoRef.current = false;
    setTocando(false);
    const v = vivo.current;
    const el = elRef.current;
    if (el && v.tabela && v.clipe?.tipo === "trecho") {
      el.pausar();
      // para no quadro exibido, no meio dele (na ré, o último pedido: o elemento já está parado nele)
      irQuadro(el.quadro());
    } else {
      irLocal(tLocalRef.current);
    }
  }, [irQuadro, irLocal]);

  /** Sentido em que o palco anda agora (1 = tocando, −1 = ré guiada). */
  const sentidoRef = useRef<1 | -1>(1);

  /** Ré (modo guiado, §7.3): elemento parado; o palco pede por busca o quadro que o relógio de parede manda, um
   *  pedido por vez (busca lenta pula quadros, nunca atrasa o relógio). Para no começo do clipe (ou da mídia). */
  const darRe = useCallback(() => {
    const v = vivo.current;
    const c = v.clipe;
    if (!c) return;
    if (tocandoRef.current) parar();
    desmarcarMedida();
    fielRef.current?.tocou();
    sentidoRef.current = -1;
    tocandoRef.current = true;
    setTocando(true);
    const t0 = performance.now();
    let ultimoPost = t0;
    if (c.tipo === "trecho") {
      const bus = buscadorRef.current;
      const tab = v.tabela;
      if (!bus || !tab) {
        tocandoRef.current = false;
        setTocando(false);
        return;
      }
      const [a] = faixaDoClipe(c, tab);
      const j0 = jRef.current ?? a;
      // dentro do clipe: para no primeiro quadro dele; antes dele: no começo da mídia
      const limite = j0 >= a ? a : 0;
      const tDe = tempoDoQuadro(tab, j0);
      let pedido = j0;
      const passo = () => {
        if (!tocandoRef.current) return;
        const agora = performance.now();
        const j = Math.max(limite, exibido(tab, tDe - (agora - t0) / 1000));
        if (j !== pedido) {
          pedido = j;
          bus.ir(j);
        }
        if (agora - ultimoPost >= 250 && jRef.current !== null) {
          ultimoPost = agora;
          postar(meioDoQuadro(tab, jRef.current));
        }
        if (j <= limite) {
          tocandoRef.current = false;
          setTocando(false);
          irQuadro(limite);
          return;
        }
        laco.current = requestAnimationFrame(passo);
      };
      laco.current = requestAnimationFrame(passo);
      return;
    }
    // sem fonte: relógio de parede, para trás
    const de0 = tLocalRef.current;
    const passo = () => {
      if (!tocandoRef.current) return;
      const t = de0 - (performance.now() - t0) / 1000;
      tLocalRef.current = Math.max(t, 0);
      pintarAgora();
      if (performance.now() - ultimoPost >= 250) {
        ultimoPost = performance.now();
        setMostra({ t: tLocalRef.current, j: null });
        postar(tLocalRef.current);
      }
      if (t <= 0) return parar();
      laco.current = requestAnimationFrame(passo);
    };
    laco.current = requestAnimationFrame(passo);
  }, [parar, pintarAgora, postar, irQuadro, desmarcarMedida]);

  const tocarPausar = useCallback(() => {
    if (tocandoRef.current) return parar();
    const v = vivo.current;
    const c = v.clipe;
    if (!c) return;
    desmarcarMedida();
    fielRef.current?.tocou();
    // andando, vale o quadro que o elemento mostra: o passo pedido antes fica para trás
    pedidoRef.current = null;
    sentidoRef.current = 1;
    tocandoRef.current = true;
    setTocando(true);
    let ultimoPost = performance.now();
    if (c.tipo === "trecho") {
      const el = elRef.current;
      const tab = v.tabela;
      if (!el || !tab) {
        tocandoRef.current = false;
        setTocando(false);
        return;
      }
      // a medida da busca deixou o elemento em outro quadro: volta ao mostrado antes de tocar
      const bus = buscadorRef.current;
      if (bus?.medindo) {
        bus.cancelarMedida();
        if (jRef.current !== null) el.buscar(jRef.current);
      }
      const [a, b] = faixaDoClipe(c, tab);
      const j0 = jRef.current ?? a;
      // dentro do clipe: para no fim dele; fora: até o fim da mídia
      const limite = j0 >= a && j0 < b ? b : nQuadros(tab);
      void el.tocar().catch(() => parar());
      const passo = () => {
        if (!tocandoRef.current) return;
        const j = el.quadro();
        if (j !== jRef.current) {
          jRef.current = j;
          pintarAgora();
        }
        const agora = performance.now();
        if (agora - ultimoPost >= 250) {
          ultimoPost = agora;
          // o tempo do quadro mostrado (o meio dele, como parado), nunca a posição do elemento
          const t = meioDoQuadro(tab, j);
          setMostra({ t, j });
          postar(t);
        }
        // fim do clipe (o arquivo intermediário acaba ali e para sozinho: vale o limite, não o quadro seguinte)
        if (j >= limite) {
          el.pausar();
          tocandoRef.current = false;
          setTocando(false);
          irQuadro(limite - 1);
          return;
        }
        // o elemento parou sozinho (fim do arquivo, falha de leitura): o transporte para junto
        if (el.pausado()) return parar();
        laco.current = requestAnimationFrame(passo);
      };
      laco.current = requestAnimationFrame(passo);
      return;
    }
    // sem fonte: relógio de parede
    const dur = duracaoLocal(c);
    let t0 = performance.now() - tLocalRef.current * 1000;
    if (tLocalRef.current >= dur) t0 = performance.now();
    const passo = () => {
      if (!tocandoRef.current) return;
      const t = (performance.now() - t0) / 1000;
      tLocalRef.current = Math.min(t, dur);
      pintarAgora();
      if (performance.now() - ultimoPost >= 250) {
        ultimoPost = performance.now();
        setMostra({ t: tLocalRef.current, j: null });
        postar(tLocalRef.current);
      }
      if (t >= dur) return parar();
      laco.current = requestAnimationFrame(passo);
    };
    laco.current = requestAnimationFrame(passo);
  }, [parar, pintarAgora, postar, irQuadro, desmarcarMedida]);

  /** `PalcoApi.tocar`: anda no sentido pedido; já andando nele, nada muda; no outro, troca. */
  const tocarNoSentido = useCallback(
    (d: 1 | -1) => {
      if (tocandoRef.current && sentidoRef.current === d) return;
      if (tocandoRef.current) parar();
      if (d === 1) tocarPausar();
      else darRe();
    },
    [parar, tocarPausar, darRe],
  );

  useEffect(() => () => {
    if (laco.current !== null) cancelAnimationFrame(laco.current);
  }, []);
  // trocar de clipe para de tocar
  useEffect(() => {
    if (tocandoRef.current) parar();
  }, [clipe?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  // ---- transporte ----
  const faixaT: [Seg, Seg] | null = clipe && clipe.tipo === "trecho" ? [clipe.inicio_s, clipe.fim_s] : null;
  // trecho: a mídia inteira; sem fonte (e congelado): o tempo local do clipe
  // (a faixa do intermediário é só a do clipe: com ele no palco vale a da mídia)
  const inteira = daOrigem ? info : (previa ?? info);
  const de = clipe?.tipo === "trecho" ? (inteira?.de_s ?? clipe.inicio_s) : 0;
  const ate = clipe?.tipo === "trecho" ? (inteira?.ate_s ?? clipe.fim_s) : clipe ? duracaoLocal(clipe) : 1;

  const buscarEm = useCallback(
    (t: Seg, fase: Fase) => {
      if (tocandoRef.current) parar();
      const v = vivo.current;
      const c = v.clipe;
      if (!c) return;
      if (c.tipo === "trecho") {
        if (v.tabela) irQuadro(exibido(v.tabela, t), fase === "move");
      } else irLocal(t);
    },
    [irQuadro, irLocal, parar],
  );

  const bordaGesto = useRef<{ g: Gesto; c0: Pick<ClipeTrecho, "inicio_s" | "inicio_pts" | "fim_s" | "fim_pts">; id: Id; t0: Seg } | null>(null);
  const pistaRef = useRef<[Seg, Seg]>([de, ate]);
  pistaRef.current = [de, ate];
  const onBorda = useCallback(
    (borda: "inicio" | "fim", t: Seg, fase: Fase) => {
      const v = vivo.current;
      const c = v.clipe;
      if (!c || c.tipo !== "trecho" || !v.tabela || bloqueado) return;
      if (fase === "inicio") {
        bordaGesto.current = { g: useEdicaoStore.getState().iniciarGesto(borda === "inicio" ? "Início" : "Fim"), c0: { inicio_s: c.inicio_s, inicio_pts: c.inicio_pts, fim_s: c.fim_s, fim_pts: c.fim_pts }, id: c.id, t0: t };
        return;
      }
      const g = bordaGesto.current;
      if (!g || g.id !== c.id) return;
      // pelo quanto o ponteiro andou: pegar a borda sem mover não muda o clipe
      const campos = bordaArrastada(g.c0, borda, g.t0, t, v.tabela, pistaRef.current);
      g.g.alterar((d) => {
        aplicarMudanca(d, { clipe: g.id, campos: campos ?? g.c0 });
      });
      if (fase === "fim") {
        g.g.concluir();
        bordaGesto.current = null;
      }
    },
    [bloqueado],
  );

  const marcarPonta = useCallback(
    (qual: "entrada" | "saida") => {
      const v = vivo.current;
      const c = v.clipe;
      if (!c || c.tipo !== "trecho" || !v.tabela || bloqueado || !livre()) return;
      const j = jRef.current;
      if (j === null) return;
      const t = meioDoQuadro(v.tabela, j);
      const campos = qual === "entrada" ? entradaNoCursor(c, v.tabela, t) : saidaNoCursor(c, v.tabela, t);
      if (!campos) return;
      useEdicaoStore.getState().alterar(qual === "entrada" ? "Início" : "Fim", (d) => {
        aplicarMudanca(d, { clipe: c.id, campos });
      });
    },
    [bloqueado, livre],
  );

  // ---- alças e ferramenta ----
  const onArrasto = useCallback((a: Alca, de0: Ponto, para: Ponto, mod: Modificadores, fase: Fase) => {
    if (fase === "inicio") {
      // alça de lugar ou de centro mora no canvas e a do `ocultar`, no original: as outras, no espaço de origem
      const naOrigem = alcaNaOrigem(a) && vivo.current.efeitoSel?.efeito.tipo !== "ocultar";
      if (!livre(naOrigem)) return;
      gestoRef.current = { g: useEdicaoStore.getState().iniciarGesto("Marcar") };
    }
    const g = gestoRef.current;
    if (!g) return;
    if (fase !== "inicio") {
      const mud = a.arrastar(de0, para, mod);
      g.g.alterar((d) => {
        aplicarMudanca(d, mud);
      });
    }
    if (fase === "fim") {
      g.g.concluir();
      gestoRef.current = null;
    }
  }, [livre]);

  const onMarcar = useCallback(
    (v: Caixa | Ponto) => {
      const f = vivo.current.ferramenta;
      const c = vivo.current.clipe;
      const inst = instante();
      const doc0 = vivo.current.doc;
      if (!f || !c || !inst || !doc0 || !livre(!marcaForaDaOrigem(doc0, f))) return;
      // o par (t, pts) é o da base de quem recebe a marca (âncora do rastreio ou chave do efeito)
      const r0 = f.rastreio && vivo.current.doc ? acharRastreio(vivo.current.doc, f.rastreio) : null;
      const em = r0 ? instanteDaAncora(c, r0.base, inst) : instanteDaChave(c, inst);
      const origem = vivo.current.info ? { w: vivo.current.info.exibido.w, h: vivo.current.info.exibido.h } : vivo.current.saida;
      const canvas = vivo.current.saida;
      // canvas → espaço do dado: região pela origem girada; caixa e ponto pela inversa de A
      const m0 = f.tipo === "regiao" ? aDaGirada(c.efeitos, origem, canvas) : aDe(c.efeitos, inst.t, origem, canvas);
      const inv = inversa(m0);
      if (f.tipo === "lugar") {
        const l: Lugar = { x: (v as Ponto).x, y: (v as Ponto).y, h: "centro", v: "meio" };
        onMarcado(l, em.t, em.pts);
        return;
      }
      if (!inv) return;
      if (f.tipo === "ponto") {
        const p = aplicar(inv, v as Ponto);
        onMarcado({ x: p.x, y: p.y, w: 0, h: 0 }, em.t, em.pts);
        return;
      }
      const cx = v as Caixa;
      const a = aplicar(inv, { x: cx.x, y: cx.y });
      const b = aplicar(inv, { x: cx.x + cx.w, y: cx.y + cx.h });
      const r = { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), w: Math.abs(b.x - a.x), h: Math.abs(b.y - a.y) };
      // clique sem arrasto: caixa sem área não vira âncora (um ocultar ficaria sem cobrir nada)
      if (!(r.w >= LADO_MINIMO && r.h >= LADO_MINIMO)) return;
      onMarcado(r, em.t, em.pts);
    },
    [instante, onMarcado, livre],
  );

  // ---- vista ----
  useLayoutEffect(() => {
    const el = caixaRef.current;
    if (!el) return;
    // escondido (a análise na frente) mede 0×0: a vista (zoom e deslocamento) fica como estava
    const medir = () => setCaixa((c) => caixaMedida(c, el.clientWidth, el.clientHeight));
    medir();
    const ro = new ResizeObserver(medir);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  useEffect(() => {
    setVista((v) => limitar(v, caixa, img, zMax));
  }, [caixa.w, caixa.h, img.w, img.h]); // eslint-disable-line react-hooks/exhaustive-deps

  const onRoda = (e: WE<HTMLDivElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const f = e.deltaY < 0 ? PASSO_ZOOM : 1 / PASSO_ZOOM;
    setVista((v) => zoomEm(v, f, { x: e.clientX - r.left, y: e.clientY - r.top }, caixa, img, zMax));
  };
  const onFundo = useCallback(
    (dx: number, dy: number) => setVista((v) => deslocar(v, dx, dy, caixa, img, zMax)),
    [caixa, img, zMax],
  );

  // ---- prévia do render ----
  const tarefas = useRenderStore((s) => s.tarefas);
  useEffect(() => {
    if (!render?.tarefa || render.src) return;
    const t = tarefas.find((x) => x.id === render.tarefa);
    if (!t) return;
    if (t.estado.tipo === "pronta" && raizCache) setRender({ tarefa: render.tarefa, src: mediaSrc(`${raizCache.replace(/[\\/]+$/, "")}/${t.estado.relativo}`) });
    else if (t.estado.tipo === "falhou" || t.estado.tipo === "cancelada") {
      setRender(null);
      setRenderFalhou(t.estado.tipo === "falhou");
    }
  }, [tarefas, render, raizCache]);

  const previaRender = useCallback(() => {
    const v = vivo.current;
    if (render) {
      setRender(null);
      return;
    }
    if (!v.resolvido) return;
    if (tocandoRef.current) parar();
    setRenderFalhou(false);
    setRender({ tarefa: null, src: null });
    const de0 = Math.max(0, v.resolvido.inicio_q);
    useRenderStore
      .getState()
      .exportar({ tipo: "previa", altura: Math.min(480, v.saida.h), de_q: de0, ate_q: v.resolvido.inicio_q + v.resolvido.n_q }, [])
      .then((tarefa) => setRender((r) => (r ? { tarefa, src: null } : r)))
      .catch(() => {
        setRender(null);
        setRenderFalhou(true);
      });
  }, [render, parar]);

  // ---- API para os atalhos e o inspetor ----
  useImperativeHandle(
    ref,
    (): PalcoApi => ({
      tocarPausar,
      tocar: tocarNoSentido,
      pausar: () => {
        if (tocandoRef.current) parar();
      },
      passo: (d) => {
        if (tocandoRef.current) parar();
        const v = vivo.current;
        if (v.clipe?.tipo === "trecho" && v.tabela) irQuadro((jRef.current ?? 0) + d, true);
        else if (v.clipe) irLocal(tLocalRef.current + (d * v.fps.den) / v.fps.num);
      },
      inicio: () => {
        const v = vivo.current;
        if (v.clipe?.tipo === "trecho" && v.tabela) irQuadro(faixaDoClipe(v.clipe, v.tabela)[0]);
        else irLocal(0);
      },
      fim: () => {
        const v = vivo.current;
        if (v.clipe?.tipo === "trecho" && v.tabela) irQuadro(faixaDoClipe(v.clipe, v.tabela)[1] - 1);
        else if (v.clipe && v.clipe.tipo !== "trecho") irLocal(duracaoLocal(v.clipe));
      },
      entrada: () => marcarPonta("entrada"),
      saida: () => marcarPonta("saida"),
      zoom: (f) => {
        if (f === "ajustar") setVista(VISTA_INICIAL);
        else setVista((v) => zoomEm(v, f, { x: caixa.w / 2, y: caixa.h / 2 }, caixa, img, zMax));
      },
      quadroFinal: () => {
        const q = slotDoCursor();
        if (q !== null) fielRef.current?.pedirJa(q);
      },
      previaRender,
      cancelar: () => {
        if (render) {
          setRender(null);
          return true;
        }
        return false;
      },
      tempo: () => instante()?.t ?? 0,
      instante: (rastreio) => {
        const i = instante();
        const v = vivo.current;
        // Enter com a ferramenta ligada: a mesma regra do clique (anotação só sobre o intermediário)
        const naOrigem = !!v.ferramenta && !!v.doc && !marcaForaDaOrigem(v.doc, v.ferramenta);
        if (!i || !v.clipe || !livre(naOrigem)) return null;
        const r = rastreio && v.doc ? acharRastreio(v.doc, rastreio) : null;
        return r ? instanteDaAncora(v.clipe, r.base, i) : instanteDaChave(v.clipe, i);
      },
      irPara: (t) => {
        const v = vivo.current;
        if (v.clipe?.tipo === "trecho" && v.tabela) irQuadro(exibido(v.tabela, t));
        else irLocal(t);
      },
    }),
    [tocarPausar, tocarNoSentido, parar, irQuadro, irLocal, marcarPonta, caixa, img, zMax, slotDoCursor, previaRender, render, instante, livre],
  );

  // ---- estado da mídia (ícones, sem texto) ----
  const fundoDaMidia = sha && previa && !previa.pronta ? fundoPorChave(fundo, chaveFundo(previa.tipo === "proxy" ? "proxy" : "copia_previa", sha)) : null;
  // intermediário do clipe em preparo ou que falhou, e proxy em uso: ícone no canto, sem texto (contratos-fase1 (f))
  const origemFazendo = !!clipe && mudaAImagem(clipe) && origem?.tipo === "fazendo" ? origem : null;
  const origemFalhou = !!clipe && mudaAImagem(clipe) && origem?.tipo === "falhou" ? origem : null;
  const proxyEmUso = !!src && previa?.tipo === "proxy";
  const falhouElemento = !!src && falhaSrc === src;
  // cópia ou proxy que falhou na fila de fundo: o ícone de vídeo ausente, não a espera sem fim
  const previaFalhou = fundoDaMidia?.estado.tipo === "falhou";
  const preparando = !!clipe && !!midiaId && !rec.erro && !previaFalhou && (!info || !tabela || !src);
  const semVideo = !!clipe && !!midiaId && (!!rec.erro || falhouElemento || previaFalhou);
  const fielSrc = typeof fiel === "object" ? fiel.src : null;
  const pixelado = m.w > saida.w;

  return (
    <div className={styles.palco} data-palco="">
      <div ref={caixaRef} className={styles.caixa} onWheel={onRoda} onPointerEnter={() => void (sobreRef.current = true)} onPointerLeave={() => void (sobreRef.current = false)} data-caixa="">
        <canvas
          ref={canvasRef}
          width={tamCanvas.w}
          height={tamCanvas.h}
          className={`${styles.canvas} ${pixelado ? styles.pixelado : ""}`}
          style={{ left: m.x, top: m.y, width: m.w, height: m.h }}
          data-canvas=""
        />
        {fielSrc && !render && <img src={fielSrc} alt="" className={styles.fiel} style={{ left: m.x, top: m.y, width: m.w, height: m.h }} data-fiel="" />}
        {render?.src && <video src={render.src} className={styles.render} style={{ left: m.x, top: m.y, width: m.w, height: m.h }} autoPlay controls data-render="" />}
        {!render && (
          <CamadaEdicao
            alcas={alcas}
            vista={vista}
            img={img}
            contorno={contorno}
            ferramenta={soLeitura ? null : ferramenta}
            saida={saida}
            onArrasto={onArrasto}
            onMarcar={onMarcar}
            onFundo={(dx, dy) => onFundo(dx, dy)}
          />
        )}
        {RASTREIO_PRONTO && !render && <FerramentaSeguir area={{ x: m.x, y: m.y, w: m.w, h: m.h }} />}
        {(preparando || render?.tarefa === null || (render && !render.src)) && (
          <span className={styles.estado} data-estado="preparando">
            <Loader2 size={22} className={styles.gira} aria-hidden />
            {fundoDaMidia && fundoDaMidia.feito > 0 && <span className={styles.barraFundo} style={{ width: `${Math.round(fundoDaMidia.feito * 100)}%` }} />}
          </span>
        )}
        {semVideo && (
          <span className={styles.estado} data-estado="sem-video" title={rec.erro?.frase || undefined}>
            <VideoOff size={22} aria-hidden />
          </span>
        )}
        {(origemFazendo || origemFalhou || proxyEmUso || rec.erroDoProxy) && (
          <span className={styles.cantos}>
            {origemFazendo && (
              <span className={styles.canto} data-estado="origem-fazendo">
                <Loader2 size={14} className={styles.gira} aria-hidden />
                <span className={styles.trilho}>
                  <span className={styles.barraFundo} style={{ width: `${Math.round(origemFazendo.feito * 100)}%` }} />
                </span>
              </span>
            )}
            {origemFalhou && (
              <span className={styles.canto} data-estado="origem-falhou" title={origemFalhou.erro.frase || origemFalhou.erro.codigo || undefined}>
                <CircleAlert size={14} aria-hidden />
              </span>
            )}
            {proxyEmUso && (
              <span className={styles.canto} data-estado="proxy">
                <Gauge size={14} aria-hidden />
              </span>
            )}
            {rec.erroDoProxy && (
              <span className={styles.canto} data-estado="proxy-falhou" title={rec.erroDoProxy.frase || rec.erroDoProxy.codigo || undefined}>
                <CircleAlert size={14} aria-hidden />
              </span>
            )}
          </span>
        )}
        {clipe && <PedidoDeOrigem key={clipe.id} clipe={clipe} />}
        {/* o elemento fica dentro do palco: o WebKit pode pausar vídeo fora da área visível */}
        {src && <video ref={videoRef} key={src} src={src} className={styles.elemento} preload="auto" playsInline muted={props.mudo ?? false} onError={() => setFalhaSrc(src)} data-elemento="" />}
      </div>
      <Transporte
        de={de}
        ate={ate}
        faixa={faixaT}
        sha256={sha}
        cursor={mostra.t}
        quadro={mostra.j}
        base={clipe?.tipo === "trecho" ? "fonte" : "programa"}
        tocando={tocando}
        bordas={!bloqueado}
        fiel={fiel}
        render={render ? (render.src ? "tocando" : "pedindo") : renderFalhou ? "falhou" : "nada"}
        expandido={props.expandido}
        desligado={!clipe}
        onBuscar={buscarEm}
        onBorda={onBorda}
        onTocar={tocarPausar}
        onPasso={(d) => {
          if (tocandoRef.current) parar();
          if (clipe?.tipo === "trecho" && tabela) irQuadro((jRef.current ?? 0) + d, true);
          else if (clipe) irLocal(tLocalRef.current + (d * fps.den) / fps.num);
        }}
        onInicio={() => (clipe?.tipo === "trecho" && tabela ? irQuadro(faixaDoClipe(clipe, tabela)[0]) : irLocal(0))}
        onFim={() => (clipe?.tipo === "trecho" && tabela ? irQuadro(faixaDoClipe(clipe, tabela)[1] - 1) : clipe && clipe.tipo !== "trecho" ? irLocal(duracaoLocal(clipe)) : undefined)}
        onQuadroFinal={() => {
          const q = slotDoCursor();
          if (q !== null) fielRef.current?.pedirJa(q);
        }}
        onPreviaRender={previaRender}
        onAjustar={() => setVista(VISTA_INICIAL)}
        onExpandir={props.onExpandir}
      />
    </div>
  );
});

export type { EstadoRender };
export const _paraTeste = { primitivasProprias, rastreioDoEfeito, tamanhoDoCanvas, urlDaPrimitiva };
