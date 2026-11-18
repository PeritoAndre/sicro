/**
 * Alças do palco (§8.3 `Alca`): caixa e ponto de rastreio (âncora no quadro mostrado) e região do enquadramento
 * (Fase 4, ZS: na proporção da saída; zoom animado em "Preencher"; no que acompanha o rastreio, só o tamanho).
 * `em`, `de` e `para` em 0..1 do canvas; a conta volta ao espaço do dado pela inversa de A. `arrastar` sempre parte
 * do valor do começo do gesto (o palco chama com o `de` do pointerdown), então o mesmo gesto pode ser repetido.
 * Shift: mantém a proporção (na região, solta a proporção da saída). Alt: a partir do centro.
 */

import type { Alca, Animavel, Caixa, Chave, Efeito, EfEnquadramento, EfLupa, EfRegua, EfVelocidadeExibida, Id, Mudanca, Ponto, Pts, Seg } from "@domain/edicao";
import { efeitoConhecido, porChave } from "../modelo/projeto";
import { aDe, inversa, ponto, seguidaDe, type Afim, type Tamanho } from "../modelo/geometria";
import { ehChaves } from "../modelo/curva";

/** Menor lado de uma caixa marcada (fração). */
export const LADO_MINIMO = 0.002;

const lim = (v: number) => Math.min(Math.max(v, 0), 1);

/** Caixa a partir de dois cantos, limitada a 0..1. */
export function caixaDeCantos(a: Ponto, b: Ponto): Caixa {
  const x0 = lim(Math.min(a.x, b.x));
  const y0 = lim(Math.min(a.y, b.y));
  const x1 = lim(Math.max(a.x, b.x));
  const y1 = lim(Math.max(a.y, b.y));
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

type Lado = -1 | 0 | 1;
/** Alças da caixa: (sx, sy) = de que lado da caixa (−1 começo, 1 fim, 0 meio). */
const POSICOES: { id: string; tipo: Alca["tipo"]; s: [Lado, Lado] }[] = [
  { id: "no", tipo: "canto", s: [-1, -1] },
  { id: "ne", tipo: "canto", s: [1, -1] },
  { id: "se", tipo: "canto", s: [1, 1] },
  { id: "so", tipo: "canto", s: [-1, 1] },
  { id: "n", tipo: "borda", s: [0, -1] },
  { id: "l", tipo: "borda", s: [1, 0] },
  { id: "s", tipo: "borda", s: [0, 1] },
  { id: "o", tipo: "borda", s: [-1, 0] },
  { id: "mover", tipo: "mover", s: [0, 0] },
];

const naCaixa = (c: Caixa, sx: Lado, sy: Lado): Ponto => ({ x: c.x + ((sx + 1) / 2) * c.w, y: c.y + ((sy + 1) / 2) * c.h });

/** Cursor pela direção da alça NA TELA (o giro pode trocar os eixos). */
function cursorDe(tipo: Alca["tipo"], dx: number, dy: number): string {
  if (tipo === "mover") return "move";
  if (tipo === "borda") return Math.abs(dx) > Math.abs(dy) ? "ew-resize" : "ns-resize";
  return dx * dy > 0 ? "nwse-resize" : "nesw-resize";
}

/**
 * Nova caixa ao levar a alça (sx, sy) de `de` a `para` (no espaço da caixa).
 * `proporcao` = largura/altura a manter (no espaço da caixa); null = livre.
 */
export function caixaArrastada(c: Caixa, s: [Lado, Lado], de: Ponto, para: Ponto, mod: { shift: boolean; alt: boolean }, proporcao: number | null): Caixa {
  const [sx, sy] = s;
  if (sx === 0 && sy === 0) {
    // mover: a caixa inteira fica dentro de 0..1
    const x = Math.min(Math.max(c.x + para.x - de.x, 0), 1 - c.w);
    const y = Math.min(Math.max(c.y + para.y - de.y, 0), 1 - c.h);
    return { x, y, w: c.w, h: c.h };
  }
  const cx = c.x + c.w / 2;
  const cy = c.y + c.h / 2;
  // ponta que se move e ponta fixa, em cada eixo
  let x0 = c.x;
  let x1 = c.x + c.w;
  let y0 = c.y;
  let y1 = c.y + c.h;
  const dx = para.x - de.x;
  const dy = para.y - de.y;
  if (sx === -1) x0 += dx;
  if (sx === 1) x1 += dx;
  if (sy === -1) y0 += dy;
  if (sy === 1) y1 += dy;
  if (mod.alt) {
    if (sx === -1) x1 = 2 * cx - x0;
    if (sx === 1) x0 = 2 * cx - x1;
    if (sy === -1) y1 = 2 * cy - y0;
    if (sy === 1) y0 = 2 * cy - y1;
  }
  let r = caixaDeCantos({ x: x0, y: y0 }, { x: x1, y: y1 });
  // eixo que a alça não move fica com os mesmos bits (sem arrasto acumular erro)
  if (sx === 0) r = { ...r, x: c.x, w: c.w };
  if (sy === 0) r = { ...r, y: c.y, h: c.h };
  if (proporcao !== null && proporcao > 0 && r.w > 0 && r.h > 0) {
    // canto: vale o eixo que mais andou; borda: o outro eixo acompanha, centrado
    const pelaLargura = sy === 0 || (sx !== 0 && r.w / r.h >= proporcao);
    let w = pelaLargura ? r.w : r.h * proporcao;
    let h = pelaLargura ? r.w / proporcao : r.h;
    // ponto fixo de cada eixo: o lado oposto à alça, ou o centro
    const fx = mod.alt || sx === 0 ? cx : sx === 1 ? r.x : r.x + r.w;
    const fy = mod.alt || sy === 0 ? cy : sy === 1 ? r.y : r.y + r.h;
    const cabe = (f: number, s: Lado, centro: boolean, tam: number) => (centro ? Math.min(f, 1 - f) / (tam / 2) : s === 1 ? (1 - f) / tam : f / tam);
    // passou da borda: encolhe os dois lados juntos (a proporção fica)
    const k = Math.min(1, cabe(fx, sx, mod.alt || sx === 0, w), cabe(fy, sy, mod.alt || sy === 0, h));
    w *= k;
    h *= k;
    const ax = mod.alt || sx === 0 ? fx - w / 2 : sx === 1 ? fx : fx - w;
    const ay = mod.alt || sy === 0 ? fy - h / 2 : sy === 1 ? fy : fy - h;
    r = caixaDeCantos({ x: ax, y: ay }, { x: ax + w, y: ay + h });
  }
  return r.w >= LADO_MINIMO && r.h >= LADO_MINIMO ? r : c;
}

/** Alças de uma caixa no espaço `m` (dado → canvas). `mudanca` recebe a caixa nova. `centrada`: sem "mover" e
 *  o tamanho sempre a partir do centro (a posição é de outro: o zoom que acompanha o rastreio). */
function alcasDaCaixa(prefixo: string, c: Caixa, m: Afim, proporcao: (mod: { shift: boolean }) => number | null, mudanca: (c: Caixa) => Mudanca, centrada = false): Alca[] {
  const inv = inversa(m);
  if (!inv) return [];
  const centro = ponto(m, { x: c.x + c.w / 2, y: c.y + c.h / 2 });
  return POSICOES.filter((p) => !centrada || p.tipo !== "mover").map(({ id, tipo, s }) => {
    const em = ponto(m, naCaixa(c, s[0], s[1]));
    return {
      id: `${prefixo}:${id}`,
      tipo,
      em,
      cursor: cursorDe(tipo, em.x - centro.x, em.y - centro.y),
      arrastar: (de, para, mod) =>
        mudanca(caixaArrastada(c, s, ponto(inv, de), ponto(inv, para), centrada ? { ...mod, alt: true } : mod, tipo === "canto" || tipo === "borda" ? proporcao(mod) : null)),
    };
  });
}

/** Âncora no quadro mostrado (mesmo pts, ou mesmo t sem pts). */
function ancora(t: Seg, pts: Pts | null, v: Caixa): Chave<Caixa> {
  return { t, pts, v };
}

/** Caixa de rastreio no quadro: âncora nova (ou trocada) em (t, pts). `A` = espaço do rastreio → canvas. */
export function alcasDeCaixa(rastreio: Id, c: Caixa, A: Afim, t: Seg, pts: Pts | null): Alca[] {
  const prop = c.h > 0 ? c.w / c.h : null;
  return alcasDaCaixa(`caixa:${rastreio}`, c, A, (mod) => (mod.shift ? prop : null), (v) => ({ rastreio, ancora: ancora(t, pts, v) }));
}

/** Ponto de rastreio (w = h = 0). */
export function alcasDePonto(rastreio: Id, p: Ponto, A: Afim, t: Seg, pts: Pts | null): Alca[] {
  const inv = inversa(A);
  if (!inv) return [];
  return [
    {
      id: `ponto:${rastreio}`,
      tipo: "ponto",
      em: ponto(A, p),
      cursor: "move",
      arrastar: (de, para) => {
        const a = ponto(inv, de);
        const b = ponto(inv, para);
        return { rastreio, ancora: ancora(t, pts, { x: lim(p.x + b.x - a.x), y: lim(p.y + b.y - a.y), w: 0, h: 0 }) };
      },
    },
  ];
}

// ---- enquadramento: editado sobre a origem inteira ----

/** Quartos de volta horários somados dos `girar` ligados (a paridade diz se a origem troca largura e altura). */
function quartos(efeitos: readonly (Efeito | { tipo: string })[]): number {
  let q = 0;
  for (const e of efeitos) if (efeitoConhecido(e as Efeito) && (e as Efeito).tipo === "girar" && (e as { ligado: boolean }).ligado) q += Math.round(((e as { graus: number }).graus ?? 0) / 90);
  return ((q % 4) + 4) % 4;
}

/** Origem girada inteira contida no canvas: 0..1 da origem girada → canvas. */
export function aDaGirada(efeitos: readonly (Efeito | { tipo: string })[], origem: Tamanho, canvas: Tamanho): Afim {
  const [wo, ho] = quartos(efeitos) % 2 === 1 ? [origem.h, origem.w] : [origem.w, origem.h];
  const p = wo * canvas.h;
  const q = ho * canvas.w;
  if (!(p > 0 && q > 0)) return [1, 0, 0, 1, 0, 0];
  const [sx, sy] = p <= q ? [p / q, 1] : [1, q / p];
  return [sx, 0, 0, sy, (1 - sx) * 0.5, (1 - sy) * 0.5];
}

/** A da vista de origem (fonte → canvas): o giro do clipe e a origem inteira, sem a região. */
export function aDaOrigem(efeitos: readonly (Efeito | { tipo: string })[], origem: Tamanho, canvas: Tamanho): Afim {
  const sem = efeitos.filter((e) => (e as { tipo: string }).tipo !== "enquadramento") as Efeito[];
  // origem e canvas 1×1 sem região: o encaixe é a identidade e sobra só o giro
  const g = aDe(sem, 0, { w: 1, h: 1 }, { w: 1, h: 1 });
  return seguidaDe(g, aDaGirada(efeitos, origem, canvas));
}

/** Região do enquadramento no instante `tau` e as alças dela, no espaço da origem girada (`m` = girada → canvas).
 *  No zoom que acompanha o rastreio, só o tamanho (a partir do centro). */
export function alcasDaRegiao(e: EfEnquadramento, regiao: Caixa, tau: Seg, pts: Pts | null, m: Afim, proporcao: number | null): Alca[] {
  return alcasDaCaixa(`regiao:${e.id}`, regiao, m, (mod) => (mod.shift ? null : proporcao), (v) => ({ efeito: comRegiao(e, v, tau, pts) }), !!e.segue);
}

const mesmaCaixa = (a: Caixa, b: Caixa) => a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h;

/** Região animada de verdade: chaves com valores diferentes (todas iguais valem como fixa, como no motor). */
export function regiaoAnimada(r: Animavel<Caixa>): boolean {
  return ehChaves(r) && r.some((k, i) => i > 0 && !mesmaCaixa(k.v, (r[i - 1] as Chave<Caixa>).v));
}

/** Zoom animado (ou que acompanha) sai sempre em "Preencher", e "Vizinho" só vale com região fixa (D8). */
export function comAjusteDoZoom(e: EfEnquadramento): EfEnquadramento {
  if (!regiaoAnimada(e.regiao) && !e.segue) return e;
  if (e.ajuste === "cobrir" && e.interpolacao !== "vizinho") return e;
  return { ...e, ajuste: "cobrir", interpolacao: e.interpolacao === "vizinho" ? "bilinear" : e.interpolacao };
}

/** Interpolação da chave NOVA do zoom: suave (entra e sai devagar, como na maquete aprovada); a que troca o valor
 *  de uma chave que já existe fica com a dela (`porChave`). */
export const INTERP_DO_ZOOM = "suave" as const;

/** A chave do zoom em (t, pts) na lista: nova nasce suave; a do mesmo quadro troca o valor e deixa a curva. Sem
 *  `pts` (clipe de tempo local) a chave sai sem ele. */
export function chaveDoZoom(ks: readonly Chave<Caixa>[], t: Seg, pts: Pts | null, v: Caixa): Chave<Caixa>[] {
  const k: Chave<Caixa> = pts === null ? { t, v } : { t, pts, v };
  const existe = ks.some((o) => (o.pts != null && pts !== null ? o.pts === pts : o.t === t));
  return porChave(ks, existe ? k : { ...k, interp: INTERP_DO_ZOOM });
}

/** Efeito com a região nova: fixa troca o valor; animada ganha (ou troca) a chave do instante (e fica em "Preencher"). */
export function comRegiao(e: EfEnquadramento, v: Caixa, tau: Seg, pts: Pts | null): EfEnquadramento {
  if (ehChaves(e.regiao)) return comAjusteDoZoom({ ...e, regiao: chaveDoZoom(e.regiao, tau, pts, v) });
  return { ...e, regiao: v };
}

/** Região marcada na proporção da saída (`proporcao`, a de `proporcaoDaSaida`; null = como veio), com o mesmo
 *  centro e inteira dentro da imagem: o que passa da borda encolhe os dois lados juntos. */
export function regiaoNaSaida(c: Caixa, proporcao: number | null): Caixa {
  let w = c.w;
  let h = c.h;
  if (proporcao !== null && proporcao > 0 && w > 0 && h > 0) {
    // vale o lado que sobra (a caixa nova contém a marcada)
    if (w / h >= proporcao) h = w / proporcao;
    else w = h * proporcao;
  }
  const k = Math.min(1, w > 0 ? 1 / w : 1, h > 0 ? 1 / h : 1);
  w *= k;
  h *= k;
  const cx = c.x + c.w / 2;
  const cy = c.y + c.h / 2;
  return { x: Math.min(Math.max(cx - w / 2, 0), 1 - w), y: Math.min(Math.max(cy - h / 2, 0), 1 - h), w, h };
}

/** Proporção da saída no espaço 0..1 da origem girada (largura/altura de uma região que enche o canvas sem barras). */
export function proporcaoDaSaida(efeitos: readonly (Efeito | { tipo: string })[], origem: Tamanho, canvas: Tamanho): number | null {
  const [wo, ho] = quartos(efeitos) % 2 === 1 ? [origem.h, origem.w] : [origem.w, origem.h];
  if (!(wo > 0 && ho > 0 && canvas.w > 0 && canvas.h > 0)) return null;
  return (canvas.w / canvas.h) * (ho / wo);
}

/** Ponto de ancoragem de texto, carimbo e imagem (`lugar`, canvas): arrastar move o efeito. */
export function alcaDeLugar(e: Extract<Efeito, { lugar: unknown }>): Alca {
  const l = e.lugar;
  return {
    id: `lugar:${e.id}`,
    tipo: "mover",
    em: { x: l.x, y: l.y },
    cursor: "move",
    arrastar: (de, para) => ({ efeito: { ...e, lugar: { ...l, x: lim(l.x + para.x - de.x), y: lim(l.y + para.y - de.y) } } as Efeito }),
  };
}

// CONTRATO: acréscimos (contratos-fase1 F9): `alcaDaAncora`, `alcaDoLugarFixo`, `alcaNaOrigem`.
/** Âncora da régua (ponto no espaço de origem do clipe): o arrasto volta ao espaço dela pela inversa de A. */
export function alcaDaAncora(e: EfRegua, A: Afim): Alca[] {
  const inv = inversa(A);
  if (!inv) return [];
  const p = e.ancora;
  return [
    {
      id: `ancora:${e.id}`,
      tipo: "ponto",
      em: ponto(A, p),
      cursor: "move",
      arrastar: (de, para) => {
        const a = ponto(inv, de);
        const b = ponto(inv, para);
        return { efeito: { ...e, ancora: { x: lim(p.x + b.x - a.x), y: lim(p.y + b.y - a.y) } } };
      },
    },
  ];
}

/** Centro da lupa fixa e lugar da velocidade fixa (canvas): mover. Na que acompanha, não há o que arrastar aqui. */
export function alcaDoLugarFixo(e: EfLupa | EfVelocidadeExibida): Alca[] {
  const mover = (de: Ponto, para: Ponto, p: Ponto): Ponto => ({ x: lim(p.x + para.x - de.x), y: lim(p.y + para.y - de.y) });
  if (e.tipo === "lupa") {
    const o = e.onde;
    if (o.modo !== "fixa") return [];
    return [{ id: `centro:${e.id}`, tipo: "mover", em: { x: o.x, y: o.y }, cursor: "move", arrastar: (de, para) => ({ efeito: { ...e, onde: { modo: "fixa", ...mover(de, para, o) } } }) }];
  }
  const o = e.onde;
  if (o.modo !== "fixa") return [];
  const l = o.lugar;
  return [{ id: `centro:${e.id}`, tipo: "mover", em: { x: l.x, y: l.y }, cursor: "move", arrastar: (de, para) => ({ efeito: { ...e, onde: { modo: "fixa", lugar: { ...l, ...mover(de, para, l) } } } }) }];
}

/** A alça mexe em dado do espaço de ORIGEM do clipe (caixa, ponto, região, âncora)? As de `lugar` e de centro moram
 *  no canvas. */
export const alcaNaOrigem = (a: Pick<Alca, "id">): boolean => !(a.id.startsWith("lugar:") || a.id.startsWith("centro:"));
