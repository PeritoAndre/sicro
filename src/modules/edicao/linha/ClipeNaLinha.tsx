/** Um item na pista: corpo arrastável, bordas de aparar, tira (V1), losangos (efeito). A onda dos itens de som é da
 *  trilha (um canvas por trilha, `Onda.tsx`). Dono: P4.1. */

import { memo, useEffect, useMemo, useRef, type PointerEvent as PE } from "react";
import { Film, Image as IconeImagem, Snowflake, Type } from "lucide-react";
import type { Racional, Seg } from "@domain/edicao";
import type { ItemNaLinha, TipoTrilha } from "./modelo";
import { imagemDaTira, janelasDaTira, tiras, useTirasVersao, type BlocoTira } from "./tiras";
import { CurvasNaLinha, itemComCurva } from "./curvas/NaLinha";
import styles from "./Linha.module.css";

export type AlvoNoItem = "corpo" | "inicio" | "fim";

/** De onde vem a imagem ou o som do item: a mídia e a faixa de fonte que ele mostra. */
export interface FonteDoItem {
  sha256: string;
  de_s: Seg;
  ate_s: Seg;
}

export interface ClipeNaLinhaProps {
  item: ItemNaLinha;
  pxPorS: number;
  selecionado: boolean;
  // CONTRATO: acréscimos do P4.1 (props do próprio componente).
  fps: Racional;
  tipoTrilha: TipoTrilha;
  /** Altura da pista (px). */
  altura: number;
  /** Faixa de efeito: cor da barra e altura de uma raia. */
  cor?: string;
  raiaPx?: number;
  /** Parte da pista desenhada (px desde o começo do programa): tira só nela. */
  janela: readonly [number, number];
  /** Arrastando: onde o item vai (slots; null = não arrasta), e a borda pega. Primitivos: o `memo` só redesenha o item
   *  que mudou. */
  fantasmaDe?: number | null;
  fantasmaAte?: number | null;
  bordaAtiva?: "inicio" | "fim" | null;
  /** O que pode ser pego (trilha travada, só leitura, apagado: nada). */
  pegaCorpo: boolean;
  pegaInicio: boolean;
  pegaFim: boolean;
  blocos?: readonly BlocoTira[];
  ws: string | null;
  raizCache: string | null;
  onPegar(e: PE<HTMLElement>, item: ItemNaLinha, alvo: AlvoNoItem): void;
  onEntrada?(id: string): void;
}

const CLASSE: Record<string, string> = {
  trecho: "video",
  tela: "video",
  video: "video",
  titulo: "titulo",
  congelar: "congelar",
  imagem: "imagem",
};

function classeDo(it: ItemNaLinha, t: TipoTrilha): string {
  if (t === "som") return "som";
  if (t === "audio") return "nar";
  if (t === "legendas") return "legenda";
  if (t === "sobreposicao" && it.tipo === "titulo") return "texto";
  return CLASSE[it.tipo ?? ""] ?? "video";
}

type Icone = "titulo" | "congelar" | "imagem" | "video" | null;

function iconeDo(it: ItemNaLinha, t: TipoTrilha): Icone {
  if (t !== "principal" && t !== "sobreposicao") return null;
  if (t === "principal" && it.tipo === "titulo") return "titulo";
  if (it.tipo === "congelar") return "congelar";
  if (it.tipo === "imagem") return "imagem";
  if (t === "sobreposicao" && it.tipo === "video") return "video";
  return null;
}

function IconeDo({ i }: { i: Icone }) {
  if (i === "titulo") return <Type size={11} aria-hidden />;
  if (i === "congelar") return <Snowflake size={11} aria-hidden />;
  if (i === "imagem") return <IconeImagem size={11} aria-hidden />;
  if (i === "video") return <Film size={11} aria-hidden />;
  return null;
}

/** Cor do texto sobre a barra: escuro em cor clara, claro em cor escura. */
function textoSobre(cor: string | undefined): string {
  const m = /^#?([0-9a-f]{6})/i.exec(cor ?? "");
  if (!m) return "#0b1220";
  const n = parseInt(m[1] as string, 16);
  const l = (0.2126 * ((n >> 16) & 255) + 0.7152 * ((n >> 8) & 255) + 0.0722 * (n & 255)) / 255;
  return l > 0.55 ? "#0b1220" : "#f4f7fb";
}

/** Uma imagem de tira (um bloco) num canvas: a janela k de `n` (o `n` da resposta) com a célula dela. */
const TiraDoBloco = memo(function TiraDoBloco({ bloco, raizCache, altura }: { bloco: BlocoTira; raizCache: string | null; altura: number }) {
  useTirasVersao((s) => s.versao);
  const ref = useRef<HTMLCanvasElement>(null);
  const t = tiras.tira(bloco.pedido);
  const img = t && raizCache ? imagemDaTira(raizCache, t) : null;
  const dpr = typeof window !== "undefined" ? Math.min(window.devicePixelRatio || 1, 2) : 1;
  const w = Math.max(1, Math.round(bloco.w * dpr));
  const h = Math.max(1, Math.round(altura * dpr));
  useEffect(() => {
    const g = ref.current?.getContext?.("2d");
    if (!g || !t || !img) return;
    g.clearRect(0, 0, w, h);
    for (const j of janelasDaTira(t, w, h)) g.drawImage(img, j.sx, 0, j.sw, t.h, j.x, 0, j.largura, h);
  }, [t, img, w, h]);
  if (!t || !img) return null;
  return <canvas ref={ref} width={w} height={h} style={{ left: bloco.x, width: bloco.w }} data-tira={bloco.canal} data-n={t.n} aria-hidden />;
});

/** O "X" da transição na emenda (maquete). */
const XDaEmenda = () => (
  <svg viewBox="0 0 16 22" width="16" height="22" aria-hidden>
    <path d="M0 0 16 22M16 0 0 22" stroke="#fff" strokeWidth="1.5" opacity=".8" />
  </svg>
);

type Pegar = Record<AlvoNoItem, (e: PE<HTMLElement>) => void>;

const mesmosBlocos = (x: readonly BlocoTira[] | undefined, y: readonly BlocoTira[] | undefined) => {
  const xa = x ?? [];
  const ya = y ?? [];
  return xa.length === ya.length && xa.every((v, i) => v.canal === ya[i]!.canal && v.x === ya[i]!.x && v.w === ya[i]!.w && mesmoValor(v.pedido, ya[i]!.pedido));
};

interface Bordas {
  pegaInicio: boolean;
  pegaFim: boolean;
  bordaAtiva: "inicio" | "fim" | null;
  pegar: Pegar;
}

const BordasDoItem = ({ pegaInicio, pegaFim, bordaAtiva, pegar }: Bordas) => (
  <>
    {pegaInicio && <span className={`${styles.borda} ${styles.bordaE} ${bordaAtiva === "inicio" ? styles.bordaAtiva : ""}`} onPointerDown={pegar.inicio} data-borda="inicio" />}
    {pegaFim && <span className={`${styles.borda} ${styles.bordaD} ${bordaAtiva === "fim" ? styles.bordaAtiva : ""}`} onPointerDown={pegar.fim} data-borda="fim" />}
  </>
);

/** Dentro da barra de efeito (relativo a ela): anda junto sem redesenhar. `chaves` = slots desde o começo, "a,b". */
const MioloBarra = memo(function MioloBarra(p: Bordas & { rotulo: string; chaves: string }) {
  const L = (q: number) => `calc(var(--px) * ${q}px)`;
  return (
    <>
      <span>{p.rotulo}</span>
      {p.chaves &&
        p.chaves.split(",").map((x) => {
          const q = Number(x);
          // na ponta da barra o losango não sai cortado ao meio
          return <i key={x} className={styles.losango} style={{ left: `clamp(5px, ${L(q + 0.5)}, calc(100% - 5px))` }} data-chave={q} />;
        })}
      <BordasDoItem {...p} />
    </>
  );
});

/** Dentro do clipe: nome, tira e bordas. Mover o clipe (aparar o de antes) não redesenha isto. */
const MioloClipe = memo(
  function MioloClipe(p: Bordas & { icone: Icone; rotulo: string | null; px: number; blocos?: readonly BlocoTira[]; raizCache: string | null; alturaTira: number }) {
    return (
      <>
        <span className={styles.nome}>
          <IconeDo i={p.icone} />
          {p.rotulo !== null && <span>{p.rotulo}</span>}
        </span>
        {p.blocos && p.blocos.length > 0 && (
          // até a árvore assentar no zoom novo, a tira estica com ele
          <div className={styles.faixaTira} style={{ transform: `scaleX(calc(var(--px) / ${p.px}))` }}>
            {p.blocos.map((b) => (
              <TiraDoBloco key={b.canal} bloco={b} raizCache={p.raizCache} altura={p.alturaTira} />
            ))}
          </div>
        )}
        <BordasDoItem {...p} />
      </>
    );
  },
  (a, b) => {
    for (const k of Object.keys(b) as (keyof typeof b)[]) if (k !== "blocos" && a[k] !== b[k]) return false;
    return mesmosBlocos(a.blocos, b.blocos);
  },
);

function ClipeNaLinhaBase(p: ClipeNaLinhaProps) {
  const { item: it, fps } = p;
  // as funções de pegar leem as props de agora: o miolo não redesenha quando o item só anda
  const atual = useRef(p);
  atual.current = p;
  const pegar = useMemo<Pegar>(() => {
    const f = (alvo: AlvoNoItem) => (e: PE<HTMLElement>) => atual.current.onPegar(e, atual.current.item, alvo);
    return { corpo: f("corpo"), inicio: f("inicio"), fim: f("fim") };
  }, []);
  const px = (p.pxPorS * fps.den) / fps.num;
  const fantasma = p.fantasmaDe != null && p.fantasmaAte != null;
  const de = fantasma ? (p.fantasmaDe as number) : it.de_q;
  const ate = fantasma ? (p.fantasmaAte as number) : it.ate_q;
  const w = Math.max(0, (ate - de) * px);
  // posição e largura pela variável `--px` do conteúdo (o zoom assentado; o vivo é uma transformação da pista)
  const L = (q: number) => `calc(var(--px) * ${q}px)`;
  const dados = { "data-item": it.id, "data-k": it.k, "data-de-q": de, "data-ate-q": ate, ...(it.clipe ? { "data-clipe": it.clipe } : {}) };

  // item sem duração (clipe desligado ou de versão futura na principal): uma marca
  if (it.ate_q <= it.de_q && !fantasma) {
    return <div className={`${styles.nota} ${p.selecionado ? styles.sel : ""}`} style={{ left: L(de) }} title={it.rotulo} onPointerDown={pegar.corpo} {...dados} />;
  }

  if (p.tipoTrilha === "efeito") {
    const raia = p.raiaPx ?? 20;
    const classes = [styles.barraEf, p.selecionado && styles.sel, it.desligado && styles.desligado, it.apagado && styles.apagado, fantasma && styles.fantasma, it.cortado?.de && styles.cortadoE, it.cortado?.ate && styles.cortadoD];
    return (
      <div
        className={classes.filter(Boolean).join(" ")}
        style={{ left: L(de), width: `max(4px, ${L(ate - de)})`, top: (it.raia ?? 0) * raia + 3, height: raia - 6, background: p.cor, color: textoSobre(p.cor), cursor: p.pegaCorpo ? undefined : "pointer" }}
        onPointerDown={pegar.corpo}
        title={it.rotulo}
        {...dados}
      >
        <MioloBarra
          rotulo={w > 28 ? it.rotulo : ""}
          chaves={(it.chaves ?? [])
            .filter((q) => !it.curva?.includes(q))
            .map((q) => q - de)
            .join(",")}
          pegaInicio={p.pegaInicio && w > 14}
          pegaFim={p.pegaFim && w > 14}
          bordaAtiva={p.bordaAtiva ?? null}
          pegar={pegar}
        />
        {/* P4.2: chaves da região do zoom, uma faixa por efeito (barra emendada: cada pedaço no lugar dele) */}
        {!fantasma &&
          it.tipo === "enquadramento" &&
          (it.partes ?? [{ id: it.id, de_q: it.de_q, ate_q: it.ate_q }]).map((x) => (
            <span key={x.id} className={styles.curvaDaParte} style={{ left: L(x.de_q - de), width: L(x.ate_q - x.de_q) }}>
              <CurvasNaLinha item={{ k: "efeito", id: x.id, trilha: it.trilha, tipo: "enquadramento", de_q: x.de_q, ate_q: x.ate_q }} />
            </span>
          ))}
      </div>
    );
  }

  const classes = [styles.item, styles[classeDo(it, p.tipoTrilha)], p.selecionado && styles.sel, it.erro && styles.erro, it.apagado && styles.apagado, it.desligado && styles.desligado, it.mudo && styles.mudo, fantasma && styles.fantasma];
  const entrada = p.tipoTrilha === "principal" && it.entrada && !fantasma ? it.entrada : null;
  return (
    <>
      <div className={classes.filter(Boolean).join(" ")} style={{ left: L(de), width: `max(2px, ${L(ate - de)})`, cursor: p.pegaCorpo ? undefined : "pointer" }} onPointerDown={pegar.corpo} title={w < 60 ? it.rotulo : undefined} {...dados}>
        <MioloClipe
          icone={iconeDo(it, p.tipoTrilha)}
          rotulo={w > 24 ? it.rotulo : null}
          px={px}
          blocos={fantasma ? undefined : p.blocos}
          raizCache={p.raizCache}
          alturaTira={Math.max(1, p.altura - 6 - 18)}
          pegaInicio={p.pegaInicio && w > 10}
          pegaFim={p.pegaFim && w > 10}
          bordaAtiva={p.bordaAtiva ?? null}
          pegar={pegar}
        />
        {!fantasma && itemComCurva(it) && <CurvasNaLinha item={it} />}
      </div>
      {entrada && entrada.sobrepoe_q > 0 && <span className={styles.sobrepoe} style={{ left: L(de), width: L(entrada.sobrepoe_q) }} aria-hidden />}
      {entrada && (
        <button
          type="button"
          className={styles.entrada}
          style={{ left: L(de + entrada.sobrepoe_q / 2) }}
          title="Entrada"
          aria-label="Entrada"
          data-entrada={it.id}
          onPointerDown={(e) => e.stopPropagation()}
          onClick={() => p.onEntrada?.(it.id)}
        >
          <XDaEmenda />
        </button>
      )}
    </>
  );
}

/** Mesmo valor para o desenho: números e textos iguais; listas e objetos pequenos pelo conteúdo. */
function mesmoValor(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  return JSON.stringify(a) === JSON.stringify(b);
}

/** Item igual pelo que se vê: a lista de trilhas é refeita a cada mudança do documento (objetos novos). */
function mesmoItem(a: ItemNaLinha, b: ItemNaLinha): boolean {
  if (a === b) return true;
  const ka = Object.keys(a) as (keyof ItemNaLinha)[];
  if (ka.length !== Object.keys(b).length) return false;
  return ka.every((k) => mesmoValor(a[k], b[k]));
}

/** Props iguais: as funções vêm presas à trilha e ao item, que entram na comparação. */
function mesmasProps(a: ClipeNaLinhaProps, b: ClipeNaLinhaProps): boolean {
  for (const k of Object.keys(b) as (keyof ClipeNaLinhaProps)[]) {
    const x = a[k];
    const y = b[k];
    if (x === y || (typeof x === "function" && typeof y === "function")) continue;
    if (k === "item") {
      if (!mesmoItem(x as ItemNaLinha, y as ItemNaLinha)) return false;
      continue;
    }
    if (k === "blocos") {
      if (!mesmosBlocos(x as readonly BlocoTira[] | undefined, y as readonly BlocoTira[] | undefined)) return false;
      continue;
    }
    if (!mesmoValor(x, y)) return false;
  }
  return Object.keys(a).length === Object.keys(b).length;
}

export const ClipeNaLinha = memo(ClipeNaLinhaBase, mesmasProps);
