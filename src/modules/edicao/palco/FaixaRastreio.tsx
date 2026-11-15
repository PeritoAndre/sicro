/**
 * Faixa do rastreio no transporte (§8.2): estado por quadro do rastreio do efeito aberto (ok, incerto, sem o alvo,
 * interpolado, ausência aceita) na faixa em que o efeito vale, sobre a pista; as âncoras como traços; durante a
 * corrida, onde ela está. Sem texto. Dono: P2.4. Montada no `Transporte` (com a bandeira `RASTREIO_PRONTO`).
 */

import { useMemo } from "react";
import type { MarcaQuadro } from "../store/rastreioStore";
import type { Seg } from "@domain/edicao";
import { useEdicaoStore } from "../store/edicaoStore";
import { useInspetorUi } from "../inspetor/ui";
import { useRecursos } from "./recursos";
import { tempoDoQuadro } from "../modelo/tempo";
import { alvoDoEfeito, marcasDoAlvo, RASTREIO_PRONTO, useRastreioStore } from "../store/rastreioStore";

export interface FaixaRastreioProps {
  /** Escala da pista (tempo de fonte) e largura em px. */
  de: Seg;
  ate: Seg;
  largura: number;
}

/** Cor de cada estado; `nada` dentro da faixa do efeito é sem cobertura (como o ausente). */
export const COR_MARCA: Record<Exclude<MarcaQuadro, "nada" | "ancora">, string> = {
  ok: "var(--sicro-success)",
  incerto: "var(--sicro-warning)",
  interpolado: "var(--sicro-info)",
  ausente: "var(--sicro-danger)",
  aceita: "var(--sicro-fg-dim)",
};
/** O pior estado da coluna aparece (um quadro incerto entre vinte bons não some). */
const PESO: Record<MarcaQuadro, number> = { nada: 6, ausente: 6, incerto: 5, interpolado: 4, aceita: 3, ok: 2, ancora: 2 };
const naFaixa = (m: MarcaQuadro): Exclude<MarcaQuadro, "nada" | "ancora"> => (m === "nada" ? "ausente" : m === "ancora" ? "ok" : m);

export interface Pedaco {
  x0: number;
  x1: number;
  marca: Exclude<MarcaQuadro, "nada" | "ancora">;
}

/** Pedaços da faixa em px: cada coluna fica com o pior estado dos quadros que caem nela; colunas iguais se juntam. */
export function pedacos(marcas: readonly MarcaQuadro[], tempos: readonly Seg[], de: Seg, ate: Seg, largura: number): Pedaco[] {
  const w = Math.floor(largura);
  if (w <= 0 || !(ate > de) || marcas.length === 0) return [];
  const col: (MarcaQuadro | null)[] = new Array(w).fill(null);
  for (let i = 0; i < marcas.length; i++) {
    const t = tempos[i] as number;
    const x = Math.floor(((t - de) / (ate - de)) * w);
    if (x < 0 || x >= w) continue;
    const m = marcas[i] as MarcaQuadro;
    const atual = col[x];
    if (atual == null || PESO[m] > PESO[atual]) col[x] = m;
  }
  // quadro mais largo que uma coluna: as colunas entre dois quadros seguidos ficam com o primeiro
  let ultimo: MarcaQuadro | null = null;
  const x0 = Math.floor((((tempos[0] as number) - de) / (ate - de)) * w);
  const xn = Math.floor((((tempos[tempos.length - 1] as number) - de) / (ate - de)) * w);
  for (let x = Math.max(0, x0); x <= Math.min(w - 1, xn); x++) {
    if (col[x] == null) col[x] = ultimo;
    else ultimo = col[x] as MarcaQuadro;
  }
  const saida: Pedaco[] = [];
  for (let x = 0; x < w; x++) {
    const m = col[x];
    if (m == null) continue;
    const marca = naFaixa(m);
    const fim = saida[saida.length - 1];
    if (fim && fim.x1 === x && fim.marca === marca) fim.x1 = x + 1;
    else saida.push({ x0: x, x1: x + 1, marca });
  }
  return saida;
}

const ALTURA = 22;

export function FaixaRastreio({ de, ate, largura }: FaixaRastreioProps) {
  const doc = useEdicaoStore((s) => s.doc);
  const efeitoAberto = useEdicaoStore((s) => s.selecao.efeito);
  const cursor = useEdicaoStore((s) => s.cursor);
  const daFerramenta = useInspetorUi((s) => s.ferramenta?.efeito ?? null);
  const densos = useRastreioStore((s) => s.densos);
  const corrida = useRastreioStore((s) => s.corrida);
  const versao = useRecursos((s) => s.versao);
  const ef = efeitoAberto ?? daFerramenta;
  const clipe = cursor.modo === "clipe" ? cursor.clipe : null;
  const x = RASTREIO_PRONTO && doc && ef ? alvoDoEfeito(doc, ef, clipe) : null;
  const r = x?.rastreio ?? null;
  const denso = r?.denso ? (densos[r.denso.sha256] ?? null) : null;
  const tab = x?.tabela ?? null;
  const chave = x ? `${x.de}|${x.ate}|${x.ateExclusivo}` : "";
  const desenho = useMemo(() => {
    if (!x || !tab) return null;
    const mm = marcasDoAlvo(x);
    if (!mm) return null;
    const tempos = mm.m.map((_, i) => tempoDoQuadro(tab, mm.j0 + i));
    return { pedacos: pedacos(mm.m, tempos, de, ate, largura), ancoras: x.rastreio.ancoras.map((k) => k.t) };
  }, [r, denso, tab, chave, de, ate, largura, versao]); // eslint-disable-line react-hooks/exhaustive-deps
  if (!x || !desenho || largura <= 0 || !(ate > de)) return null;
  const w = Math.max(1, Math.floor(largura));
  const px = (t: Seg) => ((t - de) / (ate - de)) * w;
  const cabeca = corrida && corrida.rastreio === x.rastreio.id && corrida.ultimo && tab ? px((corrida.ultimo.pts * tab.tb.num) / tab.tb.den) : null;
  return (
    <svg
      viewBox={`0 0 ${w} ${ALTURA}`}
      preserveAspectRatio="none"
      aria-hidden
      data-faixa-rastreio={x.rastreio.id}
      style={{ position: "absolute", inset: 0, width: "100%", height: "100%", pointerEvents: "none" }}
    >
      {desenho.pedacos.map((p) => (
        <rect key={p.x0} x={p.x0} y={ALTURA - 5} width={p.x1 - p.x0} height={5} fill={COR_MARCA[p.marca]} data-marca={p.marca} />
      ))}
      {desenho.ancoras.map((t, i) => {
        const a = px(t);
        return a >= 0 && a <= w ? <rect key={`a${i}`} x={a - 0.5} y={ALTURA - 9} width={1} height={9} fill="var(--sicro-fg)" data-ancora="" /> : null;
      })}
      {cabeca != null && <rect x={cabeca - 0.5} y={0} width={1} height={ALTURA} fill="var(--sicro-accent)" data-cabeca="" />}
    </svg>
  );
}
