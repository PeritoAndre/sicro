/** Uma trilha da linha do tempo: cabeça como na maquete (sigla, nome, ver, travar; faixa de efeito com o ponto da cor)
 *  e a pista com os itens visíveis e, nas trilhas de som, as ondas num canvas só. Dono: P4.1. */

import { memo, useCallback, type PointerEvent as PE } from "react";
import { Eye, EyeOff, Lock, LockOpen } from "lucide-react";
import type { Id, Racional, Selecao } from "@domain/edicao";
import type { ItemNaLinha, TrilhaNaLinha } from "./modelo";
import { ClipeNaLinha, type AlvoNoItem, type FonteDoItem } from "./ClipeNaLinha";
import { OndasDaTrilha } from "./Onda";
import type { BlocoTira } from "./tiras";
import styles from "./Linha.module.css";

/** Altura de cada tipo de trilha (px, as da maquete aprovada); a faixa de efeito tem uma raia por linha de barras. */
export const ALTURA_PADRAO = { sobreposicao: 34, principal: 54, efeito: 20, som: 34, audio: 28, legendas: 22 } as const;
export const RAIA_PX = 20;

/** Item sendo arrastado: onde vai (slots e trilha) e a borda pega. */
export interface Fantasma {
  id: Id;
  clipe?: Id;
  de_q: number;
  ate_q: number;
  /** Trilha onde o fantasma aparece (a de origem, ou a de destino ao trocar de trilha). */
  trilha: string;
  origem: string;
  borda: "inicio" | "fim" | null;
}

export interface TrilhaProps {
  trilha: TrilhaNaLinha;
  pxPorS: number;
  altura: number;
  // CONTRATO: acréscimos do P4.1 (props do próprio componente).
  fps: Racional;
  janela: readonly [number, number];
  /** Largura da pista (px, no zoom desenhado) e a mesma em CSS (acompanha o zoom). */
  largura: number;
  larguraCss?: string;
  selecao: Selecao;
  travada: boolean;
  /** V1, Som e áudio: o conteúdo (miniaturas, ondas) aparece? (o "Ver" dessas trilhas é da tela) */
  semConteudo?: boolean;
  soLeitura: boolean;
  fantasmas: readonly Fantasma[];
  /** O item de um fantasma que veio de outra trilha. */
  itemDe(f: Fantasma): ItemNaLinha | null;
  fonteDe(it: ItemNaLinha): FonteDoItem | null;
  blocos: ReadonlyMap<string, readonly BlocoTira[]>;
  ws: string | null;
  raizCache: string | null;
  onPegar(e: PE<HTMLElement>, item: ItemNaLinha, alvo: AlvoNoItem, trilha: TrilhaNaLinha): void;
  onEntrada(id: Id): void;
  onVer(t: TrilhaNaLinha): void;
  onTravar(t: TrilhaNaLinha): void;
  onAltura(e: PE<HTMLElement>, t: TrilhaNaLinha): void;
}

export const chaveDoItem = (it: Pick<ItemNaLinha, "id" | "clipe">) => `${it.id}@${it.clipe ?? ""}`;

export function selecionado(s: Selecao, it: ItemNaLinha): boolean {
  if (it.k === "efeito") {
    // barra emendada: qualquer efeito dela
    if (it.partes) return it.partes.some((x) => s.efeito === x.id && s.foco?.id === x.clipe);
    return s.efeito === it.id && (!it.clipe || s.foco?.id === it.clipe);
  }
  return s.itens.some((x) => x.id === it.id) || s.foco?.id === it.id;
}

function Cabeca({ t, travada, semConteudo, onVer, onTravar }: { t: TrilhaNaLinha; travada: boolean; semConteudo: boolean; onVer(): void; onTravar(): void }) {
  if (t.tipo === "efeito") {
    return (
      <>
        <span className={styles.ponto} style={{ background: t.cor }} aria-hidden />
        <span className={styles.nomeTrilha} title={t.nome}>
          {t.nome}
        </span>
      </>
    );
  }
  // sobreposição: "Ver" é a trilha oculta no vídeo (documento); nas outras, o conteúdo na linha (tela)
  const ver = t.tipo === "sobreposicao" ? !t.oculta : !semConteudo;
  return (
    <>
      {t.sigla && <b className={styles.sigla}>{t.sigla}</b>}
      {t.nome && (
        <span className={styles.nomeTrilha} title={t.nome}>
          {t.nome}
        </span>
      )}
      <span className={styles.icones}>
        {t.tipo !== "legendas" && (
          <button type="button" className={styles.mini} title="Ver" aria-label="Ver" aria-pressed={ver} onClick={onVer} data-ver={t.id}>
            {ver ? <Eye size={13} /> : <EyeOff size={13} />}
          </button>
        )}
        <button type="button" className={styles.mini} title="Travar" aria-label="Travar" aria-pressed={travada} onClick={onTravar} data-travar={t.id}>
          {travada ? <Lock size={13} /> : <LockOpen size={13} />}
        </button>
      </span>
    </>
  );
}

export const Trilha = memo(function Trilha(p: TrilhaProps) {
  const t = p.trilha;
  const px = (p.pxPorS * p.fps.den) / p.fps.num;
  const [j0, j1] = p.janela;
  const meus = p.fantasmas.filter((f) => f.trilha === t.id);
  // o item que vai para outra trilha fica no lugar (o fantasma aparece lá)
  const fantasmaDe = (it: ItemNaLinha) => meus.find((f) => f.id === it.id && (f.clipe ?? "") === (it.clipe ?? "") && f.origem === t.id) ?? null;
  // itens que chegam de outra trilha (arrastados para cá)
  const chegando = meus.filter((f) => f.origem !== t.id);
  const visivel = (it: ItemNaLinha) => it.ate_q * px >= j0 && it.de_q * px <= j1;
  const travada = p.travada || p.soLeitura;
  const { onPegar } = p;
  const pegar = useCallback((e: PE<HTMLElement>, item: ItemNaLinha, alvo: AlvoNoItem) => onPegar(e, item, alvo, t), [onPegar, t]);
  const classe = [styles.trilha, p.travada && styles.travada, t.oculta && styles.oculta].filter(Boolean).join(" ");
  const render = (it: ItemNaLinha, f: Fantasma | null) => {
    const fixo = travada || !!it.apagado;
    const corte = it.cortado;
    // barra emendada: arrasta pelas bordas (a primeira e a última parte), não pelo corpo
    const emendada = !!it.partes;
    return (
      <ClipeNaLinha
        key={chaveDoItem(it)}
        item={it}
        pxPorS={p.pxPorS}
        fps={p.fps}
        selecionado={selecionado(p.selecao, it)}
        tipoTrilha={t.tipo}
        altura={p.altura}
        cor={t.cor}
        raiaPx={RAIA_PX}
        janela={p.janela}
        fantasmaDe={f ? f.de_q : null}
        fantasmaAte={f ? f.ate_q : null}
        bordaAtiva={f?.borda ?? null}
        pegaCorpo={!fixo && !corte?.de && !corte?.ate && !emendada && t.tipo !== "legendas"}
        pegaInicio={!fixo && !corte?.de && t.tipo !== "legendas"}
        pegaFim={!fixo && !corte?.ate && t.tipo !== "legendas"}
        blocos={t.tipo === "principal" && !p.semConteudo ? p.blocos.get(it.id) : undefined}
        ws={p.ws}
        raizCache={p.raizCache}
        onPegar={pegar}
        onEntrada={p.onEntrada}
      />
    );
  };
  const comOnda = (t.tipo === "som" || t.tipo === "audio") && !p.semConteudo && !!p.ws;
  return (
    <div className={classe} style={{ height: p.altura }} data-trilha={t.id} data-tipo={t.tipo}>
      <div className={`${styles.cab} ${t.tipo === "efeito" ? styles.cabEfeito : ""}`}>
        <Cabeca t={t} travada={p.travada} semConteudo={!!p.semConteudo} onVer={() => p.onVer(t)} onTravar={() => p.onTravar(t)} />
        {t.tipo !== "efeito" && <span className={styles.alturaCab} onPointerDown={(e) => p.onAltura(e, t)} aria-hidden />}
      </div>
      <div className={styles.pista} style={{ width: p.larguraCss ?? p.largura }} data-pista={t.id}>
        {t.itens.map((it) => {
          const f = fantasmaDe(it);
          return f || visivel(it) || p.fantasmas.some((x) => x.id === it.id) ? render(it, f) : null;
        })}
        {chegando.map((f) => {
          const original = p.itemDe(f);
          return original ? render({ ...original, trilha: t.id }, f) : null;
        })}
        {comOnda && <OndasDaTrilha itens={t.itens} fonteDe={p.fonteDe} ws={p.ws as string} pxPorS={p.pxPorS} fps={p.fps} janela={p.janela} largura={p.largura} altura={p.altura} />}
      </div>
    </div>
  );
});
