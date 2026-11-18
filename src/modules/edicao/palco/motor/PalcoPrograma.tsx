/**
 * Palco no modo "Programa" (P3.3, D10): a sequência inteira pela prévia viva (`criarMotor`): transições,
 * sobrepostos, títulos, texto livre, efeitos e o som, com o relógio fora do React. O cursor vai ao store ao pausar,
 * ao buscar e a cada 250 ms; o que muda o cursor de fora (régua da linha, desfazer) leva o motor junto. Parado, o
 * "Quadro final" pede ao motor real o slot; as alças do efeito ou do sobreposto selecionado aparecem quando o clipe
 * dele está sozinho no slot. Marcar (ferramenta do Inspetor) é do modo clipe: com a ferramenta ligada o palco vai
 * ao clipe dela, no mesmo quadro quando ele está no slot.
 */

import { forwardRef, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState, type WheelEvent as WE } from "react";
import { Loader2 } from "lucide-react";
import { commands } from "@core/commands";
import { mediaSrc } from "@core/mediaSrc";
import { caminhoDoCaso } from "@core/caminhoDoCaso";
import type { Alca, ClipePrincipal, Id, PalcoProps, Ponto, Primitiva, Seg } from "@domain/edicao";
import { useEdicaoStore, type Gesto } from "../../store/edicaoStore";
import { useRenderStore } from "../../store/renderStore";
import { useOrigemStore } from "../../store/origemStore";
import { acharClipe, acharEfeito, acharMidia, acharRastreio, aplicarMudanca } from "../../modelo/projeto";
import { aDe, IDENTIDADE } from "../../modelo/geometria";
import { ptsDaTabela, resolver } from "../../modelo/tempo";
import { efeitosNoInstante } from "../../modelo/segue";
import { useOrigem } from "../../inspetor/origemDoClipe";
import { primitivas as primitivasDoEfeito } from "../desenho/primitivas";
import { cenaDoClipe, instanteDaAncora, instanteDaChave, midiaDoClipe, quadroDaCena, type FontesCena } from "../cena";
import { alcasDoLivre, livresNoQuadro } from "../livres";
import { cantosDoTexto, dentroDoTexto } from "./livresPonte";
import { alcaDaAncora, alcaDeLugar, alcaDoLugarFixo, alcasDeCaixa, alcasDePonto } from "../alcas";
import { CamadaEdicao, type Modificadores } from "../camadaEdicao";
import { Transporte, type EstadoRender, type Fase } from "../Transporte";
import { QuadroFiel, type Fiel } from "../quadroFiel";
import { entradaNoCursor, meioDoQuadro, saidaNoCursor } from "../faixa";
import { mudaAImagem, camposPorQuadro, previaDoPalco, relogioDaOrigem } from "../origem";
import { atlasDe, garantirMidia, imagemDe, pedirProxy, precisaDeProxy, previaDaMidia, proxyPedido, recursoDe, spriteDe, urlDoCache, useRecursos } from "../recursos";
import { PASSO_ZOOM, VISTA_INICIAL, caixaMedida, contido, deslocar, limitar, mostrado, zoomEm, zoomMaximo, type Tamanho, type Vista } from "../vista";
import { areaVisivelNaOrigem, useAreaVisivel } from "../areaVisivel";
import { BarraDoPalco, cursorNoModo, rastreioDoEfeito, srcDaPrevia, tamanhoDaImagem, tamanhoDoCanvas, urlDaPrimitiva, type PalcoApi } from "../Palco";
import { relogioMidia, type RelogioMidia } from "./relogioMidia";
import { criarMotor, type FontesMotor, type MotorPrevia, type Pintada } from "./motor";
import { estadoEm, planoDe, segmentoDoSlot, tempoNoSegmento, type CamadaPrevia, type EstadoPrevia, type SegmentoPrevia } from "./plano";
import { globalDe, textosNoSlot, type Global } from "./global";
import styles from "../Palco.module.css";

interface Props extends PalcoProps {
  expandido: boolean;
  onExpandir(): void;
  mudo?: boolean;
}

/** Canvas tocando: até 1280×720 (o WebKitGTK cai a 32–34 fps com 1080p); lento, 960×540 (§7.2). */
export function tamanhoTocando(t: Tamanho, lento: boolean): Tamanho {
  const [W, H] = lento ? [960, 540] : [1280, 720];
  const k = Math.min(1, W / Math.max(t.w, 1), H / Math.max(t.h, 1));
  return { w: Math.max(1, Math.round(t.w * k)), h: Math.max(1, Math.round(t.h * k)) };
}

/** Pede e acompanha o intermediário de origem do clipe sob o cursor (como no modo clipe, §7.5). */
function PedidoDeOrigem({ clipe }: { clipe: ClipePrincipal }) {
  useOrigem(clipe);
  return null;
}

export const PalcoPrograma = forwardRef<PalcoApi, Props>(function PalcoPrograma(props, ref) {
  const { ferramenta } = props;
  const doc = useEdicaoStore((s) => s.doc);
  const ws = useEdicaoStore((s) => s.ws);
  const raizCache = useEdicaoStore((s) => s.raizCache);
  const cursor = useEdicaoStore((s) => s.cursor);
  const selecao = useEdicaoStore((s) => s.selecao);
  const versao = useRecursos((s) => s.versao);
  const origens = useOrigemStore((s) => s.porClipe);
  const saida: Tamanho = { w: doc?.saida.largura ?? 1920, h: doc?.saida.altura ?? 1080 };

  // ---- plano: o mesmo resolver da linha, com as tabelas que a tela tem ----
  const ctxTempo = useMemo(() => (doc ? { tabela: (id: Id) => recursoDe(acharMidia(doc, id)?.sha256).tabela } : null), [doc, versao]); // eslint-disable-line react-hooks/exhaustive-deps
  const linha = useMemo(() => (doc && ctxTempo ? resolver(doc, ctxTempo) : null), [doc, ctxTempo]);
  const plano = useMemo(() => (doc && linha && ctxTempo ? planoDe(doc, linha, ctxTempo) : null), [doc, linha, ctxTempo]);
  const global = useMemo<Global | null>(() => (doc && linha ? globalDe(doc, linha) : null), [doc, linha]);

  // mídias do programa: info e tabela; o proxy onde o clipe ou a busca medida pedem (§7.6)
  useEffect(() => {
    if (!ws || !doc || !plano) return;
    for (const s of plano.segmentos) {
      if (!s.fonte?.sha256) continue;
      garantirMidia(ws, s.fonte.sha256);
      const a = acharClipe(doc, s.clipe);
      const c = a?.onde === "principal" ? a.clipe : null;
      const p50 = recursoDe(s.fonte.sha256).busca_p50_ms;
      if (c?.tipo === "trecho" && recursoDe(s.fonte.sha256).tabela && precisaDeProxy(c, p50) && !proxyPedido(s.fonte.sha256)) void pedirProxy(ws, c.id, s.fonte.sha256, p50 ?? null);
    }
  }, [ws, doc, plano]);

  // ---- referências vivas para o motor ----
  const vivo = useRef({ doc, ws, raizCache, linha, plano, global, origens, saida });
  vivo.current = { doc, ws, raizCache, linha, plano, global, origens, saida };

  const fontesCena: FontesCena = useMemo(
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

  /** Relógios dos arquivos já montados (o motor reconhece o arquivo pelo objeto). */
  const relogios = useRef(new Map<string, { src: string; relogio: RelogioMidia; tab: unknown }>());
  const fontes: FontesMotor = useMemo(
    () => ({
      midia: (seg: SegmentoPrevia) => {
        const v = vivo.current;
        if (!seg.fonte || !v.ws || !v.doc) return null;
        const rec = recursoDe(seg.fonte.sha256);
        const tab = rec.tabela;
        if (!tab) return null;
        const a = acharClipe(v.doc, seg.clipe);
        const c = a?.onde === "principal" ? a.clipe : null;
        const esc = previaDoPalco({ origem: c?.tipo === "trecho" ? v.origens[c.id] : undefined, daMidia: previaDaMidia(rec), t: seg.fonte.inicio_s, original: false });
        const previa = esc.previa;
        if (!previa?.pronta) return null;
        const src = srcDaPrevia(v.ws, v.raizCache, previa);
        if (!src) return null;
        const campos = c ? camposPorQuadro(tab, c.efeitos) : 1;
        const chave = `${esc.daOrigem ? seg.clipe : ""}|${src}|${previa.desloc_s}|${campos}`;
        const ja = relogios.current.get(chave);
        if (ja && ja.tab === tab) return ja;
        const novo = { src, relogio: esc.daOrigem ? relogioDaOrigem(previa, tab, campos) : relogioMidia(previa, tab), tab };
        relogios.current.set(chave, novo);
        return novo;
      },
      som: (s) => {
        const v = vivo.current;
        const m = v.doc ? acharMidia(v.doc, s.midia) : null;
        return m && v.ws ? mediaSrc(caminhoDoCaso(v.ws, m.relativo)) : null;
      },
      pintor: {
        imagem: (src: string) => {
          const v = vivo.current;
          const url = v.ws && v.doc ? urlDaPrimitiva(src, v.ws, v.raizCache, v.doc.midias) : null;
          return url ? imagemDe(url) : null;
        },
        cena: fontesCena,
        origem: (midia: Id) => {
          const v = vivo.current;
          const sha = v.doc ? acharMidia(v.doc, midia)?.sha256 : null;
          const info = recursoDe(sha).info;
          return info ? { w: info.exibido.w, h: info.exibido.h } : null;
        },
        livres: (q: number) => {
          const v = vivo.current;
          return v.doc && v.linha ? livresNoQuadro(v.doc, v.linha, q, fontesCena) : [];
        },
        globais: (q: number) => {
          const v = vivo.current;
          if (!v.doc || !v.global) return [];
          const textos = textosNoSlot(v.doc, v.global, q);
          if (textos.length === 0) return [];
          const f = v.doc.saida.fps;
          const quadro = quadroDaCena(v.doc, [], { t: 0, pts: null, indice: null, q, programa_s: (q * f.den) / f.num }, v.saida, fontesCena, IDENTIDADE);
          const out: Primitiva[] = [];
          for (const e of textos) out.push(...primitivasDoEfeito(e, quadro));
          return out;
        },
      },
    }),
    [fontesCena],
  );

  // ---- estado na tela (o motor manda; o React só mostra) ----
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const hostRef = useRef<HTMLDivElement | null>(null);
  const caixaRef = useRef<HTMLDivElement | null>(null);
  const [q, setQ] = useState(cursor.modo === "programa" ? cursor.q : 0);
  const [tocando, setTocando] = useState(false);
  const [esperando, setEsperando] = useState(false);
  const [lento, setLento] = useState(false);
  const [alcas, setAlcas] = useState<Alca[]>([]);
  const [poligono, setPoligono] = useState<Ponto[] | null>(null);
  const [caixa, setCaixa] = useState<Tamanho>({ w: 0, h: 0 });
  const [vista, setVista] = useState<Vista>(VISTA_INICIAL);
  const [fiel, setFiel] = useState<Fiel>("viva");
  const [render, setRender] = useState<{ tarefa: Id | null; src: string | null } | null>(null);
  const [renderFalhou, setRenderFalhou] = useState(false);
  const postadoRef = useRef<number | null>(null);
  const estadoRef = useRef<EstadoPrevia | null>(null);
  const gestoRef = useRef<{ g: Gesto } | null>(null);

  /** Cursor do motor para o store (pausar, buscar, 250 ms tocando). */
  const postar = useCallback((n: number) => {
    postadoRef.current = n;
    setQ(n);
    const c = useEdicaoStore.getState().cursor;
    if (c.modo !== "programa" || c.q !== n) useEdicaoStore.getState().posicionar({ modo: "programa", q: n });
  }, []);

  const fielRef = useRef<QuadroFiel | null>(null);
  if (!fielRef.current) {
    fielRef.current = new QuadroFiel(async (n) => {
      const v = vivo.current;
      if (!v.ws || !v.doc || !v.raizCache) throw new Error("sem caso");
      const r = await commands.edicaoQuadroFinal(v.ws, v.doc, n, Math.min(v.saida.w, Math.max(64, canvasRef.current?.width ?? v.saida.w)));
      return urlDoCache(v.raizCache, r.relativo);
    }, setFiel);
  }
  useEffect(() => () => fielRef.current?.fechar(), []);

  /** Alças do selecionado no slot parado: efeito do clipe sozinho no slot, ou o sobreposto. */
  const calcularAlcas = useCallback(
    (estado: EstadoPrevia) => {
      const v = vivo.current;
      const st = useEdicaoStore.getState();
      if (!v.doc || !v.linha || st.soLeitura) return [];
      const novas: Alca[] = [];
      const foco = st.selecao.foco;
      let poligono: Ponto[] | null = null;
      if (foco?.k === "livre") {
        const l = livresNoQuadro(v.doc, v.linha, estado.q, fontesCena).find((x) => x.clipe.id === foco.id);
        if (l) {
          novas.push(...alcasDoLivre(v.doc, l, v.saida));
          poligono = cantosDoTexto(l, v.saida);
        }
      }
      setPoligono((x) => (x === null && (poligono === null || poligono.length === 0) ? x : poligono));
      const efeito = st.selecao.efeito ? acharEfeito(v.doc, st.selecao.efeito) : null;
      const c: CamadaPrevia | undefined = estado.camadas.length === 1 && !estado.transicao ? estado.camadas[0] : undefined;
      if (!efeito || !c || efeito.clipe !== c.segmento.clipe) return novas;
      const a = acharClipe(v.doc, c.segmento.clipe);
      const clipe = a?.onde === "principal" ? a.clipe : null;
      if (!clipe) return novas;
      const e = efeito.efeito;
      const midia = midiaDoClipe(clipe);
      const info = midia ? recursoDe(acharMidia(v.doc, midia)?.sha256).info : null;
      const origem = info ? { w: info.exibido.w, h: info.exibido.h } : v.saida;
      const A = aDe(efeitosNoInstante(v.doc, clipe.efeitos, c.t), c.t, origem, v.saida);
      if (e.tipo === "texto" || e.tipo === "carimbo" || e.tipo === "imagem") novas.push(alcaDeLugar(e));
      else if (e.tipo === "regua") novas.push(...alcaDaAncora(e, A));
      else {
        if (e.tipo === "lupa" || e.tipo === "velocidade_exibida") novas.push(...alcaDoLugarFixo(e));
        const rid = rastreioDoEfeito(e);
        const r = rid ? acharRastreio(v.doc, rid) : null;
        const inst = { t: c.t, pts: instantePts(c), indice: c.indice, q: estado.q, programa_s: estado.programa_s };
        const cx = r ? cenaDoClipe(v.doc, clipe, inst, origem, fontesCena, A).quadro.rastreio(r.id) : null;
        if (r && cx) {
          const em = instanteDaAncora(clipe, r.base, inst);
          if (r.tipo === "ponto") novas.push(...alcasDePonto(r.id, { x: cx.x, y: cx.y }, A, em.t, em.pts));
          else novas.push(...alcasDeCaixa(r.id, cx, A, em.t, em.pts));
        }
      }
      return novas;
    },
    [fontesCena],
  );

  const pintou = useCallback(
    (p: Pintada) => {
      estadoRef.current = p.estado;
      // tocando nada muda nas alças (não redesenha a árvore a cada quadro)
      if (motorRef.current?.relogio.estado() !== "parado") {
        setAlcas((a) => (a.length === 0 ? a : []));
        setPoligono(null);
        return;
      }
      const novas = calcularAlcas(p.estado);
      setAlcas((a) => (a.length === 0 && novas.length === 0 ? a : novas));
    },
    [calcularAlcas],
  );

  const motorRef = useRef<MotorPrevia | null>(null);
  if (!motorRef.current) motorRef.current = criarMotor({ fontes, publicar: postar, pintou, lento: () => setLento(true) });
  const motor = motorRef.current;

  useLayoutEffect(() => {
    const cv = canvasRef.current;
    const host = hostRef.current;
    if (!cv || !host) return;
    motor.ligar(cv, host);
    return () => motor.desligar();
  }, [motor]);

  // antes dos efeitos abaixo: o plano dá o tamanho do programa ao relógio (e outra taxa troca o relógio)
  useEffect(() => {
    if (plano && doc) motor.plano(plano, doc);
  }, [motor, plano, doc]);

  // o relógio diz se toca (o React não recebe cada quadro)
  useEffect(() => {
    const r = motor.relogio;
    const ver = () => {
      const e = r.estado();
      setTocando(e !== "parado");
      setEsperando(e === "esperando");
    };
    ver();
    return r.assinar((_, e) => {
      setTocando(e !== "parado");
      setEsperando(e === "esperando");
    });
  }, [motor, doc?.saida.fps.num, doc?.saida.fps.den]);
  useEffect(() => {
    motor.mistura?.mestre(props.mudo ? 0 : 1);
  }, [motor, props.mudo]);
  useEffect(() => {
    motor.repintar();
  }, [motor, versao, selecao, origens]);
  useEffect(() => {
    fielRef.current?.editou();
  }, [doc]);

  // alças também sem pintura nova (seleção ou documento mudou com o palco parado)
  useEffect(() => {
    if (tocando || !plano) return;
    const novas = calcularAlcas(estadoEm(plano, motor.relogio.q()));
    setAlcas((a) => (a.length === 0 && novas.length === 0 ? a : novas));
  }, [plano, selecao, tocando, q, calcularAlcas, motor]);

  // cursor mudado de fora (régua da linha, desfazer): o motor vai junto
  useEffect(() => {
    if (cursor.modo !== "programa") return;
    if (postadoRef.current === cursor.q) return;
    postadoRef.current = cursor.q;
    setQ(cursor.q);
    motor.irPara(cursor.q);
    if (motor.relogio.estado() === "parado") fielRef.current?.pausou(cursor.q);
  }, [cursor, motor]);

  // ferramenta do Inspetor ligada: marcar é no modo clipe, no clipe dela
  useEffect(() => {
    if (!ferramenta || !doc) return;
    const st = useEdicaoStore.getState();
    const ef = ferramenta.efeito ?? st.selecao.efeito;
    const dono = ef ? acharEfeito(doc, ef)?.clipe : null;
    const alvo = dono ?? (st.selecao.foco?.k === "clipe" ? st.selecao.foco.id : null);
    if (!alvo) return;
    motor.pausar();
    const aqui = cursorNoModo(doc, st.cursor, "clipe", alvo);
    if (aqui?.modo === "clipe" && aqui.clipe === alvo) st.posicionar(aqui);
    else {
      const s = vivo.current.plano?.segmentos.find((x) => x.clipe === alvo);
      const t = s && vivo.current.plano ? tempoNoSegmento(s, 0, vivo.current.plano.fps) : null;
      const tab = s?.tabela;
      st.posicionar({ modo: "clipe", clipe: alvo, t: t ? (s?.fonte && !s.fonte.congelado && tab && t.indice !== null ? meioDoQuadro(tab, t.indice) : t.t) : 0 });
    }
  }, [ferramenta]); // eslint-disable-line react-hooks/exhaustive-deps

  // ---- vista ----
  useLayoutEffect(() => {
    const el = caixaRef.current;
    if (!el) return;
    const medir = () => setCaixa((c) => caixaMedida(c, el.clientWidth, el.clientHeight));
    medir();
    const ro = new ResizeObserver(medir);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const img = useMemo(() => contido(caixa, saida), [caixa.w, caixa.h, saida.w, saida.h]); // eslint-disable-line react-hooks/exhaustive-deps
  const zMax = zoomMaximo(img, saida);
  const m = mostrado(vista, img);
  const parado = tamanhoDoCanvas(m.w, m.h, saida);
  const tamCanvas = tocando ? tamanhoTocando(parado, lento) : parado;
  useEffect(() => {
    setVista((v) => limitar(v, caixa, img, zMax));
  }, [caixa.w, caixa.h, img.w, img.h]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    if (!tocando) setLento(false);
  }, [tocando]);
  useEffect(() => {
    motor.repintar();
  }, [motor, tamCanvas.w, tamCanvas.h]);
  const onRoda = (e: WE<HTMLDivElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const f = e.deltaY < 0 ? PASSO_ZOOM : 1 / PASSO_ZOOM;
    setVista((v) => zoomEm(v, f, { x: e.clientX - r.left, y: e.clientY - r.top }, caixa, img, zMax));
  };

  // ---- o slot mostrado: clipe dono, tempo da fonte ----
  const dono = plano ? segmentoDoSlot(plano, q) : null;
  const donoClipe = (() => {
    const a = doc && dono ? acharClipe(doc, dono.clipe) : null;
    return a?.onde === "principal" ? a.clipe : null;
  })();
  const naFonte = dono && plano ? tempoNoSegmento(dono, q - dono.de_q, plano.fps) : null;

  // área visível (D9, D12) do clipe dono do slot: vista, tamanho do palco, clipe e instante; sem clipe, nada
  useEffect(() => {
    const pub = useAreaVisivel.getState().publicar;
    if (!doc || !donoClipe || !naFonte || caixa.w <= 0 || caixa.h <= 0) return pub(null, null);
    const midia = midiaDoClipe(donoClipe);
    const info = midia ? recursoDe(acharMidia(doc, midia)?.sha256).info : null;
    const origem = info ? { w: info.exibido.w, h: info.exibido.h } : saida;
    const A = aDe(efeitosNoInstante(doc, donoClipe.efeitos, naFonte.t), naFonte.t, origem, saida);
    pub(donoClipe.id, areaVisivelNaOrigem(vista, caixa, saida, A));
  }, [donoClipe, q, vista, caixa, versao]); // eslint-disable-line react-hooks/exhaustive-deps

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

  // ---- transporte ----
  const qAtual = () => motor.relogio.q();
  const pausar = useCallback(() => {
    if (motor.relogio.estado() === "parado") return;
    motor.pausar();
    fielRef.current?.pausou(motor.relogio.q());
  }, [motor]);
  const tocar = useCallback(
    (v: number) => {
      fielRef.current?.tocou();
      setRender(null);
      motor.tocar(v);
    },
    [motor],
  );
  const irPara = useCallback(
    (n: number, passo = false) => {
      const total = vivo.current.plano?.total_q ?? 0;
      const alvo = Math.min(Math.max(Math.round(n), 0), Math.max(total - 1, 0));
      motor.irPara(alvo);
      if (motor.relogio.estado() !== "parado") return;
      if (passo) fielRef.current?.passo(alvo);
      else fielRef.current?.pausou(alvo);
    },
    [motor],
  );

  const marcarPonta = useCallback(
    (qual: "entrada" | "saida") => {
      const v = vivo.current;
      const n = qAtual();
      const s = v.plano ? segmentoDoSlot(v.plano, n) : null;
      const a = v.doc && s ? acharClipe(v.doc, s.clipe) : null;
      const c = a?.onde === "principal" ? a.clipe : null;
      if (!c || c.tipo !== "trecho" || !s?.tabela || !v.plano || useEdicaoStore.getState().soLeitura) return;
      const x = tempoNoSegmento(s, n - s.de_q, v.plano.fps);
      if (x.indice === null) return;
      const t = meioDoQuadro(s.tabela, x.indice);
      const campos = qual === "entrada" ? entradaNoCursor(c, s.tabela, t) : saidaNoCursor(c, s.tabela, t);
      if (!campos) return;
      useEdicaoStore.getState().alterar(qual === "entrada" ? "Início" : "Fim", (d) => {
        aplicarMudanca(d, { clipe: c.id, campos });
      });
    },
    [], // eslint-disable-line react-hooks/exhaustive-deps
  );

  const previaRender = useCallback(() => {
    const v = vivo.current;
    if (render) {
      setRender(null);
      return;
    }
    const s = v.plano ? segmentoDoSlot(v.plano, qAtual()) : null;
    if (!s) return;
    pausar();
    setRenderFalhou(false);
    setRender({ tarefa: null, src: null });
    useRenderStore
      .getState()
      .exportar({ tipo: "previa", altura: Math.min(480, v.saida.h), de_q: s.de_q, ate_q: s.ate_q }, [])
      .then((tarefa) => setRender((r) => (r ? { tarefa, src: null } : r)))
      .catch(() => {
        setRender(null);
        setRenderFalhou(true);
      });
  }, [render, pausar]); // eslint-disable-line react-hooks/exhaustive-deps

  /** Tempo do clipe dono do slot (base dele). */
  const tempoDoDono = useCallback((): Seg => {
    const v = vivo.current;
    const n = qAtual();
    const s = v.plano ? segmentoDoSlot(v.plano, n) : null;
    return s && v.plano ? tempoNoSegmento(s, n - s.de_q, v.plano.fps).t : 0;
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  useImperativeHandle(
    ref,
    (): PalcoApi => ({
      tocarPausar: () => (motor.relogio.estado() === "parado" ? tocar(1) : pausar()),
      tocar: (d) => {
        const e = motor.relogio.estado();
        if (e !== "parado" && Math.sign(motor.relogio.velocidade()) === d) return;
        tocar(d);
      },
      pausar,
      passo: (d) => {
        if (motor.relogio.estado() !== "parado") motor.pausar();
        irPara(qAtual() + d, true);
      },
      inicio: () => irPara(0),
      fim: () => irPara((vivo.current.plano?.total_q ?? 1) - 1),
      entrada: () => marcarPonta("entrada"),
      saida: () => marcarPonta("saida"),
      zoom: (f) => {
        if (f === "ajustar") setVista(VISTA_INICIAL);
        else setVista((v) => zoomEm(v, f, { x: caixa.w / 2, y: caixa.h / 2 }, caixa, img, zMax));
      },
      quadroFinal: () => fielRef.current?.pedirJa(qAtual()),
      previaRender,
      cancelar: () => {
        if (render) {
          setRender(null);
          return true;
        }
        return false;
      },
      tempo: tempoDoDono,
      instante: (rastreio) => {
        const v = vivo.current;
        const n = qAtual();
        const s = v.plano ? segmentoDoSlot(v.plano, n) : null;
        const a = v.doc && s ? acharClipe(v.doc, s.clipe) : null;
        const c = a?.onde === "principal" ? a.clipe : null;
        if (!c || !s || !v.plano) return null;
        const x = tempoNoSegmento(s, n - s.de_q, v.plano.fps);
        const camada = { segmento: s, t: x.t, indice: x.indice, peso: 1, k: n - s.de_q, luz: 1 };
        const inst = { t: x.t, pts: instantePts(camada) };
        const r = rastreio && v.doc ? acharRastreio(v.doc, rastreio) : null;
        return r ? instanteDaAncora(c, r.base, inst) : instanteDaChave(c, inst);
      },
      // Shift+←/→ e o Inspetor: o tempo na base do clipe dono vira o mesmo deslocamento em slots
      irPara: (t) => {
        const v = vivo.current;
        const n = qAtual();
        const s = v.plano ? segmentoDoSlot(v.plano, n) : null;
        if (!s || !v.plano) return;
        const f = v.plano.fps;
        const fator = s.fonte && !s.fonte.congelado && s.fonte.velocidade.tipo === "constante" ? s.fonte.velocidade.fator : 1;
        irPara(n + Math.round(((t - tempoDoDono()) / fator) * (f.num / f.den)));
      },
    }),
    [motor, tocar, pausar, irPara, marcarPonta, caixa, img, zMax, previaRender, render, tempoDoDono],
  );

  // ---- alças (parado) ----
  const onArrasto = useCallback((a: Alca, de0: Ponto, para: Ponto, mod: Modificadores, fase: Fase) => {
    if (fase === "inicio") {
      const m0 = motorRef.current;
      // sobreposto: arrasta parado (pausa se tocava); alça de efeito e de rastreio, só com o quadro exato na tela
      if (a.id.startsWith("livre:")) {
        if (m0 && m0.relogio.estado() !== "parado") m0.pausar();
      } else if (m0?.relogio.estado() !== "parado" || !m0.exato()) return;
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
  }, []);

  /** Clique dentro da caixa de um texto livre no slot (o de cima primeiro): seleciona e arrasta pela alça "mover". */
  const pegar = useCallback((p: Ponto): Alca | null => {
    const v = vivo.current;
    if (!v.doc || !v.linha || useEdicaoStore.getState().soLeitura) return null;
    const itens = livresNoQuadro(v.doc, v.linha, motor.relogio.q(), fontesCena);
    for (let i = itens.length - 1; i >= 0; i--) {
      const l = itens[i]!;
      if (!dentroDoTexto(l, v.saida, p)) continue;
      const st = useEdicaoStore.getState();
      const item = { k: "livre" as const, id: l.clipe.id, trilha: l.trilha };
      if (st.selecao.foco?.k !== "livre" || st.selecao.foco.id !== l.clipe.id) st.selecionar({ itens: [item], foco: item, efeito: null });
      return alcasDoLivre(v.doc, l, v.saida).find((a) => a.tipo === "mover") ?? null;
    }
    return null;
  }, [motor, fontesCena]);

  const fielSrc = typeof fiel === "object" ? fiel.src : null;
  const pixelado = m.w > saida.w;
  const total_q = plano?.total_q ?? 0;
  const f = doc?.saida.fps ?? { num: 30, den: 1 };
  const seg = (n: number) => (n * f.den) / f.num;
  const estadoRender: EstadoRender = render ? (render.src ? "tocando" : "pedindo") : renderFalhou ? "falhou" : "nada";
  const fonteNoCursor = dono?.fonte && naFonte ? { t: naFonte.t, quadro: naFonte.indice } : null;

  return (
    <div className={styles.palco} data-palco="" data-modo-palco="programa">
      <BarraDoPalco modo="programa" nome={donoClipe?.rotulo ?? ""} clipe={!!dono} />
      <div ref={caixaRef} className={styles.caixa} onWheel={onRoda} data-caixa="">
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
            contorno={null}
            poligono={poligono}
            pegar={pegar}
            ferramenta={null}
            saida={saida}
            onArrasto={onArrasto}
            onMarcar={() => {}}
            onFundo={(dx, dy) => setVista((v) => deslocar(v, dx, dy, caixa, img, zMax))}
          />
        )}
        {(esperando || render?.tarefa === null || (render && !render.src)) && (
          <span className={styles.cantos}>
            <span className={styles.canto} data-estado="esperando">
              <Loader2 size={14} className={styles.gira} aria-hidden />
            </span>
          </span>
        )}
        {donoClipe && mudaAImagem(donoClipe) && <PedidoDeOrigem key={donoClipe.id} clipe={donoClipe} />}
        {/* o pool fica dentro do palco: o WebKit pode pausar vídeo fora da área visível */}
        <div ref={hostRef} className={styles.host} data-pool-host="" />
      </div>
      <Transporte
        de={0}
        ate={Math.max(seg(total_q), 1e-3)}
        faixa={dono ? [seg(dono.de_q), seg(dono.ate_q)] : null}
        sha256={null}
        cursor={seg(q)}
        quadro={null}
        base="programa"
        fonte={fonteNoCursor}
        pontas="programa"
        tocando={tocando}
        bordas={false}
        fiel={fiel}
        render={estadoRender}
        expandido={props.expandido}
        desligado={total_q === 0}
        onBuscar={(t, fase) => {
          if (motor.relogio.estado() !== "parado") motor.pausar();
          irPara(Math.floor((t * f.num) / f.den + 1e-9), fase === "move");
        }}
        onBorda={() => {}}
        onTocar={() => (motor.relogio.estado() === "parado" ? tocar(1) : pausar())}
        onPasso={(d) => {
          if (motor.relogio.estado() !== "parado") motor.pausar();
          irPara(qAtual() + d, true);
        }}
        onInicio={() => irPara(0)}
        onFim={() => irPara(total_q - 1)}
        onQuadroFinal={() => fielRef.current?.pedirJa(qAtual())}
        onPreviaRender={previaRender}
        onAjustar={() => setVista(VISTA_INICIAL)}
        onExpandir={props.onExpandir}
      />
    </div>
  );
});

/** pts do quadro da camada (trecho; o congelado e os sem fonte não têm). */
function instantePts(c: Pick<CamadaPrevia, "segmento" | "indice">): number | null {
  const tab = c.segmento.tabela;
  if (!tab || c.indice === null || !c.segmento.fonte || c.segmento.fonte.congelado) return null;
  return (ptsDaTabela(tab)[c.indice] as number | undefined) ?? null;
}
