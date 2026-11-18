/** Um item na pista: corpo arrastável, bordas de aparar, tira (V1), onda (áudio), losangos (efeito). Dono: P4.1. */

import { memo, useEffect, useRef, type PointerEvent as PE } from "react";
import { Blend, Film, Image as IconeImagem, Snowflake, Type } from "lucide-react";
import type { Racional, Seg } from "@domain/edicao";
import type { ItemNaLinha, TipoTrilha } from "./modelo";
import { Onda } from "./Onda";
import { ondas, useOndasVersao } from "./ondas";
import { imagemDaTira, janelasDaTira, tiras, useTirasVersao, type BlocoTira } from "./tiras";
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
  /** Parte da pista desenhada (px desde o começo do programa): tira e onda só nela. */
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
  fonte?: FonteDoItem | null;
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

function IconeDo({ it, t }: { it: ItemNaLinha; t: TipoTrilha }) {
  if (t !== "principal" && t !== "sobreposicao") return null;
  if (it.tipo === "titulo") return <Type size={11} aria-hidden />;
  if (it.tipo === "congelar") return <Snowflake size={11} aria-hidden />;
  if (it.tipo === "imagem") return <IconeImagem size={11} aria-hidden />;
  if (t === "sobreposicao" && it.tipo === "video") return <Film size={11} aria-hidden />;
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

/** A onda da parte visível de um item de som. */
function OndaDoItem({ fonte, ws, x, w, largura, altura }: { fonte: FonteDoItem; ws: string; x: number; w: number; largura: number; altura: number }) {
  useOndasVersao((s) => s.versao);
  const o = ondas.onda(ws, fonte.sha256);
  if (!o || w < 1 || largura <= 0) return null;
  const k = (fonte.ate_s - fonte.de_s) / largura;
  return <Onda picos={o} de_s={fonte.de_s + x * k} ate_s={fonte.de_s + (x + w) * k} largura={w} altura={altura} x={x} />;
}

export const ClipeNaLinha = memo(function ClipeNaLinha(p: ClipeNaLinhaProps) {
  const { item: it, fps } = p;
  const px = (p.pxPorS * fps.den) / fps.num;
  const fantasma = p.fantasmaDe != null && p.fantasmaAte != null;
  const de = fantasma ? (p.fantasmaDe as number) : it.de_q;
  const ate = fantasma ? (p.fantasmaAte as number) : it.ate_q;
  const x = de * px;
  const w = Math.max(0, (ate - de) * px);
  // posição e largura pela variável `--px` do conteúdo: o zoom vivo não passa pelo React
  const L = (q: number) => `calc(var(--px) * ${q}px)`;
  const pegar = (alvo: AlvoNoItem) => (e: PE<HTMLElement>) => p.onPegar(e, it, alvo);
  const dados = { "data-item": it.id, "data-k": it.k, "data-de-q": de, "data-ate-q": ate, ...(it.clipe ? { "data-clipe": it.clipe } : {}) };

  // item sem duração (clipe desligado ou de versão futura na principal): uma marca
  if (it.ate_q <= it.de_q && !fantasma) {
    return <div className={`${styles.nota} ${p.selecionado ? styles.sel : ""}`} style={{ left: L(de) }} title={it.rotulo} onPointerDown={pegar("corpo")} {...dados} />;
  }

  if (p.tipoTrilha === "efeito") {
    const raia = p.raiaPx ?? 20;
    const classes = [styles.barraEf, p.selecionado && styles.sel, it.desligado && styles.desligado, it.apagado && styles.apagado, fantasma && styles.fantasma, it.cortado?.de && styles.cortadoE, it.cortado?.ate && styles.cortadoD];
    return (
      <div
        className={classes.filter(Boolean).join(" ")}
        style={{ left: L(de), width: `max(4px, ${L(ate - de)})`, top: (it.raia ?? 0) * raia + (raia - 14) / 2, background: p.cor, color: textoSobre(p.cor), cursor: p.pegaCorpo ? undefined : "pointer" }}
        onPointerDown={pegar("corpo")}
        title={it.rotulo}
        {...dados}
      >
        <span>{w > 28 ? it.rotulo : ""}</span>
        {(it.chaves ?? []).map((q) => (
          <i key={q} className={styles.losango} style={{ left: L(q - de + 0.5) }} data-chave={q} />
        ))}
        {p.pegaInicio && w > 14 && <span className={`${styles.borda} ${styles.bordaE} ${p.bordaAtiva === "inicio" ? styles.bordaAtiva : ""}`} onPointerDown={pegar("inicio")} data-borda="inicio" />}
        {p.pegaFim && w > 14 && <span className={`${styles.borda} ${styles.bordaD} ${p.bordaAtiva === "fim" ? styles.bordaAtiva : ""}`} onPointerDown={pegar("fim")} data-borda="fim" />}
      </div>
    );
  }

  const classes = [styles.item, styles[classeDo(it, p.tipoTrilha)], p.selecionado && styles.sel, it.erro && styles.erro, it.apagado && styles.apagado, it.desligado && styles.desligado, it.mudo && styles.mudo, fantasma && styles.fantasma];
  // tira e onda só na parte visível
  const v0 = Math.max(0, p.janela[0] - x);
  const v1 = Math.min(w, p.janela[1] - x);
  const somVisivel = (p.tipoTrilha === "som" || p.tipoTrilha === "audio") && !!p.fonte && !!p.ws && v1 > v0 && !fantasma;
  const alturaFaixa = Math.max(1, p.altura - 6 - 16);
  const entrada = p.tipoTrilha === "principal" && it.entrada && !fantasma ? it.entrada : null;
  return (
    <>
      <div className={classes.filter(Boolean).join(" ")} style={{ left: L(de), width: `max(2px, ${L(ate - de)})`, cursor: p.pegaCorpo ? undefined : "pointer" }} onPointerDown={pegar("corpo")} title={w < 60 ? it.rotulo : undefined} {...dados}>
        <span className={styles.nome}>
          <IconeDo it={it} t={p.tipoTrilha} />
          {w > 24 && <span>{it.rotulo}</span>}
        </span>
        {p.blocos && p.blocos.length > 0 && !fantasma && (
          <div className={styles.faixaTira} style={{ transform: `scaleX(calc(var(--px) / ${px}))` }}>
            {p.blocos.map((b) => (
              <TiraDoBloco key={b.canal} bloco={b} raizCache={p.raizCache} altura={Math.max(1, p.altura - 6 - 18)} />
            ))}
          </div>
        )}
        {somVisivel && (
          <div className={styles.faixaOnda} style={{ transform: `scaleX(calc(var(--px) / ${px}))` }}>
            <OndaDoItem fonte={p.fonte as FonteDoItem} ws={p.ws as string} x={v0} w={v1 - v0} largura={w} altura={alturaFaixa} />
          </div>
        )}
        {p.pegaInicio && w > 10 && <span className={`${styles.borda} ${styles.bordaE} ${p.bordaAtiva === "inicio" ? styles.bordaAtiva : ""}`} onPointerDown={pegar("inicio")} data-borda="inicio" />}
        {p.pegaFim && w > 10 && <span className={`${styles.borda} ${styles.bordaD} ${p.bordaAtiva === "fim" ? styles.bordaAtiva : ""}`} onPointerDown={pegar("fim")} data-borda="fim" />}
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
          <Blend size={11} aria-hidden />
        </button>
      )}
    </>
  );
});
