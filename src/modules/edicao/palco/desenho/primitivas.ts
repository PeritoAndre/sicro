/**
 * Espelho de `camada::formas::primitivas` (§5.5, §8.3): efeito + quadro → `Primitiva`s em 0..1 do canvas.
 * Mesmas contas, na mesma ordem (fixtures `primitivas-*.json`): só `+ − × ÷`, `floor` e `abs`, e o `cosSenGraus`
 * daqui (nada de `Math.sin`, `hypot` nem raiz). A conta corre em px do canvas e volta a 0..1 com `+ 0` (−0 vira 0);
 * regiões ficam em 0..1 direto. Efeito de pixel puro e efeito desligado devolvem vazio; sprite, atlas ou imagem que
 * o backend ainda não entregou também (o efeito aparece quando chegar). Erro = `Error("<código>: <detalhe>")`.
 */

import type {
  AlvoRegiao,
  Caixa,
  EfCarimbo,
  EfDestaque,
  Efeito,
  Lugar,
  PedidoSprite,
  Primitiva,
  QuadroPalco,
  Traco,
} from "@domain/edicao";
import { caixaNoCanvas, comMargem, ponto, type Afim } from "../../modelo/geometria";

type Seg = Extract<Primitiva, { k: "caminho" }>["d"][number];
type Traco2 = NonNullable<Extract<Primitiva, { k: "caminho" }>["traco"]>;

/** Largura do contorno do traço em relação à do traço. */
export const CONTORNO = 1.5;
export const COR_CONTORNO = "#000000bf";
const RAIZ2_2 = 0.7071067811865476;
const COS30 = 0.8660254037844386;
const SEN30 = 0.5;
/** Asa da seta: fração do comprimento. */
const ASA = 0.3;
/** Braço do colchete: fração do lado menor. */
const BRACO = 0.25;
/** Largura máxima do rótulo do destaque: fração da largura do canvas. */
const ROTULO_LARGURA = 0.5;

const z = (v: number) => v + 0;
const erro = (codigo: string, detalhe: string) => new Error(`${codigo}: ${detalhe}`);

/** (cos, sen) de 0..45°, Horner. */
function serie(g: number): [number, number] {
  const x = g * (Math.PI / 180);
  const x2 = x * x;
  const s =
    x *
    (1 +
      x2 *
        (-1 / 6 +
          x2 * (1 / 120 + x2 * (-1 / 5040 + x2 * (1 / 362880 + x2 * (-1 / 39916800 + x2 * (1 / 6227020800 + x2 * (-1 / 1307674368000 + x2 * (1 / 355687428096000)))))))));
  const c =
    1 + x2 * (-0.5 + x2 * (1 / 24 + x2 * (-1 / 720 + x2 * (1 / 40320 + x2 * (-1 / 3628800 + x2 * (1 / 479001600 + x2 * (-1 / 87178291200 + x2 * (1 / 20922789888000))))))));
  return [c, s];
}

/** (cos, sen) de `graus`, anti-horário: redução a 0..45° e série (igual em toda máquina, ao contrário de `Math.sin`). */
export function cosSenGraus(graus: number): [number, number] {
  if (!Number.isFinite(graus)) return [1, 0];
  let g = graus - 360 * Math.floor(graus / 360);
  if (!(g >= 0 && g < 360)) g = 0;
  const [quadrante, r] = g < 90 ? [0, g] : g < 180 ? [1, g - 90] : g < 270 ? [2, g - 180] : [3, g - 270];
  let [c, s] = r <= 45 ? serie(r) : (([cc, ss]) => [ss, cc] as [number, number])(serie(90 - r));
  [c, s] = quadrante === 0 ? [c, s] : quadrante === 1 ? [-s, c] : quadrante === 2 ? [-c, -s] : [s, -c];
  return [z(c), z(s)];
}

const fatorH = (h: Lugar["h"]) => (h === "esq" ? 0 : h === "centro" ? 0.5 : 1);
const fatorV = (v: Lugar["v"]) => (v === "topo" ? 0 : v === "meio" ? 0.5 : 1);
/** Arredonda ao pixel (meio sobe). */
const aoPixel = (v: number) => Math.floor(v + 0.5);

function traco2(t: Traco): Traco2 {
  return t.contorno ? { cor: t.cor, largura: t.espessura, contorno: COR_CONTORNO } : { cor: t.cor, largura: t.espessura };
}

/** Canvas em px: contas em px, saída em 0..1. */
function tela(c: { w: number; h: number }) {
  const { w: W, h: H } = c;
  return {
    W,
    H,
    caixa: (x: number, y: number, w: number, h: number): Caixa => ({ x: z(x / W), y: z(y / H), w: z(w / W), h: z(h / H) }),
    m: (x: number, y: number): Seg => ["M", z(x / W), z(y / H)],
    l: (x: number, y: number): Seg => ["L", z(x / W), z(y / H)],
  };
}
type Tela = ReturnType<typeof tela>;

/** Forma da região: caixa com a margem (retângulo, elipse inscrita ou polígono em 0..1 da caixa).
 *  `a` = levar ao canvas (holofote); sem `a` fica no espaço do rastreio (ocultar, na fonte). */
function formaRegiao(alvo: AlvoRegiao, caixa: Caixa, a: Afim | null): Primitiva {
  const c = comMargem(caixa, alvo.margem);
  const b = a ? caixaNoCanvas(a, c) : c;
  switch (alvo.forma) {
    case "retangulo":
      return { k: "caminho", d: [["M", b.x, b.y], ["L", z(b.x + b.w), b.y], ["L", z(b.x + b.w), z(b.y + b.h)], ["L", b.x, z(b.y + b.h)], ["Z"]] };
    case "elipse":
      return { k: "elipse", cx: z(b.x + b.w * 0.5), cy: z(b.y + b.h * 0.5), rx: z(b.w * 0.5), ry: z(b.h * 0.5) };
    case "poligono": {
      const v = alvo.vertices;
      if (!v || v.length < 3) throw erro("item_invalido", "vertices");
      const d: Seg[] = v.map((p, i) => {
        const o = { x: c.x + p.x * c.w, y: c.y + p.y * c.h };
        const q = a ? ponto(a, o) : o;
        return [i === 0 ? "M" : "L", z(q.x), z(q.y)];
      });
      d.push(["Z"]);
      return { k: "caminho", d };
    }
    default:
      // silhueta: Fase 5
      throw erro("nao_suportado", "silhueta");
  }
}

function pedidoDeTexto(id: string, conteudo: string, estilo: PedidoSprite["blocos"][number]["estilo"], lugar: Lugar, larguraMax: number, t: Tela): PedidoSprite {
  return {
    blocos: [{ id, papel: "corpo", conteudo, estilo, lugar, largura_max: larguraMax }],
    largura_max: (larguraMax * t.W) / t.H,
    altura_canvas: t.H,
    fundo: true,
  };
}

/** Destaque sobre caixa ou ponto, levado ao canvas por A. Círculo e elipse sobre caixa passam pelos cantos dela. */
function destaque(d: EfDestaque, q: QuadroPalco, t: Tela): Primitiva[] {
  const r = q.rastreio(d.rastreio);
  if (!r) return [];
  const pt = !(r.w > 0) && !(r.h > 0);
  let x0: number, y0: number, w: number, h: number, cx: number, cy: number;
  if (pt) {
    const p = ponto(q.A, { x: r.x, y: r.y });
    const l = d.tamanho * t.H;
    cx = p.x * t.W;
    cy = p.y * t.H;
    [x0, y0, w, h] = [cx - l * 0.5, cy - l * 0.5, l, l];
  } else {
    const c = caixaNoCanvas(q.A, comMargem(r, d.margem));
    [x0, y0, w, h] = [c.x * t.W, c.y * t.H, c.w * t.W, c.h * t.H];
    cx = x0 + w * 0.5;
    cy = y0 + h * 0.5;
  }
  const traco = traco2(d.traco);
  let forma: Primitiva;
  // envolvente da forma (px), para o rótulo
  let env: [number, number, number, number];
  switch (d.forma) {
    case "circulo":
    case "elipse": {
      let rx: number, ry: number;
      if (pt) [rx, ry] = [w * 0.5, h * 0.5];
      else if (d.forma === "circulo") {
        const m = RAIZ2_2 * Math.max(w, h);
        [rx, ry] = [m, m];
      } else [rx, ry] = [RAIZ2_2 * w, RAIZ2_2 * h];
      forma = { k: "elipse", cx: z(cx / t.W), cy: z(cy / t.H), rx: z(rx / t.W), ry: z(ry / t.H), traco };
      env = [cx - rx, cy - ry, cx + rx, cy + ry];
      break;
    }
    case "retangulo": {
      const [x1, y1] = [x0 + w, y0 + h];
      forma = { k: "caminho", d: [t.m(x0, y0), t.l(x1, y0), t.l(x1, y1), t.l(x0, y1), ["Z"]], traco };
      env = [x0, y0, x1, y1];
      break;
    }
    case "colchetes": {
      const [x1, y1] = [x0 + w, y0 + h];
      const b = BRACO * Math.min(w, h);
      forma = {
        k: "caminho",
        d: [
          t.m(x0, y0 + b), t.l(x0, y0), t.l(x0 + b, y0),
          t.m(x1 - b, y0), t.l(x1, y0), t.l(x1, y0 + b),
          t.m(x1, y1 - b), t.l(x1, y1), t.l(x1 - b, y1),
          t.m(x0 + b, y1), t.l(x0, y1), t.l(x0, y1 - b),
        ],
        traco,
      };
      env = [x0, y0, x1, y1];
      break;
    }
    case "seta": {
      // u: da ponta para a cauda (y para baixo); a cauda vem de `angulo_graus` (anti-horário a partir da direita)
      const [c, s] = cosSenGraus(d.angulo_graus);
      const [ux, uy] = [c, -s];
      let px = cx;
      let py = cy;
      if (!pt) {
        // saída do raio do centro pela borda da caixa
        const kx = ux !== 0 ? (w * 0.5) / Math.abs(ux) : Infinity;
        const ky = uy !== 0 ? (h * 0.5) / Math.abs(uy) : Infinity;
        const k = Math.min(kx, ky);
        [px, py] = [cx + k * ux, cy + k * uy];
      }
      const l = d.tamanho * t.H;
      const [tx, ty] = [px + l * ux, py + l * uy];
      const a = l * ASA;
      const [a1x, a1y] = [px + a * (ux * COS30 - uy * SEN30), py + a * (ux * SEN30 + uy * COS30)];
      const [a2x, a2y] = [px + a * (ux * COS30 + uy * SEN30), py + a * (uy * COS30 - ux * SEN30)];
      forma = { k: "caminho", d: [t.m(tx, ty), t.l(px, py), t.m(a1x, a1y), t.l(px, py), t.l(a2x, a2y)], traco };
      env = [Math.min(tx, px, a1x, a2x), Math.min(ty, py, a1y, a2y), Math.max(tx, px, a1x, a2x), Math.max(ty, py, a1y, a2y)];
      break;
    }
  }
  const saida: Primitiva[] = [forma];
  const rot = d.rotulo;
  if (rot && rot.texto !== "") {
    const s = q.sprite(pedidoDeTexto(d.id, rot.texto, rot.estilo, { x: 0, y: 0, h: "esq", v: "topo" }, ROTULO_LARGURA, t));
    if (s) {
      const [bx0, by0, bx1, by1] = env;
      const g = d.traco.espessura * t.H;
      const [sw, sh] = [s.w, s.h];
      const [x, y] =
        rot.lado === "cima"
          ? [(bx0 + bx1) * 0.5 - sw * 0.5, by0 - g - sh]
          : rot.lado === "baixo"
            ? [(bx0 + bx1) * 0.5 - sw * 0.5, by1 + g]
            : rot.lado === "esq"
              ? [bx0 - g - sw, (by0 + by1) * 0.5 - sh * 0.5]
              : [bx1 + g, (by0 + by1) * 0.5 - sh * 0.5];
      saida.push({ k: "sprite", src: s.relativo, caixa: t.caixa(aoPixel(x), aoPixel(y), sw, sh), opacidade: 1 });
    }
  }
  return saida;
}

// ---- carimbo ----

const msDe = (s: number) => Math.floor(s * 1000 + 0.5);
const dig = (v: number, n: number) => String(v).padStart(n, "0");
/** "HH:MM:SS,mmm" (ou "HH:MM:SS") de milissegundos ≥ 0. */
function hms(ms: number, comMs: boolean): string {
  const base = `${dig(Math.floor(ms / 3_600_000), 2)}:${dig(Math.floor(ms / 60_000) % 60, 2)}:${dig(Math.floor(ms / 1000) % 60, 2)}`;
  return comMs ? `${base},${dig(ms % 1000, 3)}` : base;
}
/** Tempo de mídia ou de programa; `data_hora` não tem data aqui e sai com milissegundos. */
function tempo(s: number, f: EfCarimbo["formato"]): string {
  const ms = msDe(s);
  return `${ms < 0 ? "-" : ""}${hms(Math.abs(ms), f !== "hms")}`;
}
const divE = (a: number, b: number) => Math.floor(a / b);
/** Dias desde 1970-01-01 (gregoriano proléptico; H. Hinnant). */
function diasDeCivil(a0: number, m: number, d: number): number {
  const a = m <= 2 ? a0 - 1 : a0;
  const era = divE(a, 400);
  const aoe = a - era * 400;
  const mp = (m + 9) % 12;
  const doy = Math.trunc((153 * mp + 2) / 5) + d - 1;
  const doe = aoe * 365 + Math.trunc(aoe / 4) - Math.trunc(aoe / 100) + doy;
  return era * 146097 + doe - 719468;
}
function civilDeDias(z0: number): [number, number, number] {
  const zz = z0 + 719468;
  const era = divE(zz, 146097);
  const doe = zz - era * 146097;
  const aoe = Math.trunc((doe - Math.trunc(doe / 1460) + Math.trunc(doe / 36524) - Math.trunc(doe / 146096)) / 365);
  const doy = doe - (365 * aoe + Math.trunc(aoe / 4) - Math.trunc(aoe / 100));
  const mp = Math.trunc((5 * doy + 2) / 153);
  const d = doy - Math.trunc((153 * mp + 2) / 5) + 1;
  const m = mp < 10 ? mp + 3 : mp - 9;
  return [aoe + era * 400 + (m <= 2 ? 1 : 0), m, d];
}
function lerData(s: string): [number, number, number] | null {
  const p = s.split("-");
  if (p.length !== 3 || !p.every((x) => /^[+-]?\d+$/.test(x))) return null;
  const [a, m, d] = p.map(Number) as [number, number, number];
  return m >= 1 && m <= 12 && d >= 1 && d <= 31 ? [a, m, d] : null;
}
/** Relógio da câmera: `clock_seconds + (t − media_time_s)`, com virada de dia; `data_hora` = "DD/MM/AAAA HH:MM:SS". */
function relogio(r: NonNullable<EfCarimbo["relogio"]>, t: number, f: EfCarimbo["formato"]): string {
  const ms = msDe(r.clock_seconds + (t - r.media_time_s));
  const dias = divE(ms, 86_400_000);
  const resto = ms - dias * 86_400_000;
  if (f === "hms_ms") return hms(resto, true);
  if (f === "hms" || r.clock_date === null) return hms(resto, false);
  const data = lerData(r.clock_date);
  if (!data) throw erro("item_invalido", "clock_date");
  const [a, m, d] = civilDeDias(diasDeCivil(...data) + dias);
  return `${dig(d, 2)}/${dig(m, 2)}/${dig(a, 4)} ${hms(resto, false)}`;
}
/** Texto do carimbo, com o prefixo fixo; null = nada a mostrar (quadro sem fonte). */
function textoCarimbo(c: EfCarimbo, q: QuadroPalco): string | null {
  switch (c.mostra) {
    case "tempo_fonte":
      return `Original ${tempo(q.t, c.formato)}`;
    case "tempo_programa":
      return `Vídeo ${tempo(q.programa_s, c.formato)}`;
    case "quadro":
      return q.indice != null ? `Quadro ${q.indice}` : null;
    case "relogio":
      if (!c.relogio) throw erro("item_invalido", "relogio");
      return `Câmera ${relogio(c.relogio, q.t, c.formato)}`;
  }
}

/** Primitivas de um efeito num quadro. `ativo` não é olhado: quem chama só pede os quadros em que o efeito vale.
 *  Lupa, régua e velocidade exibida (ligadas) são da Fase 1: erro `nao_suportado`. */
export function primitivas(e: Efeito, q: QuadroPalco): Primitiva[] {
  // desligado vem antes de tudo (o Rust também): lupa desligada não é erro
  if (!e.ligado) return [];
  const t = tela(q.canvas);
  switch (e.tipo) {
    case "lupa":
    case "regua":
    case "velocidade_exibida":
      throw erro("nao_suportado", e.tipo);
    default:
  }
  switch (e.tipo) {
    case "ocultar": {
      const r = q.rastreio(e.alvo.rastreio);
      if (!r) return [];
      const p: Primitiva = {
        k: "regiao",
        modo: e.modo,
        forma: formaRegiao(e.alvo, r, null),
        intensidade: e.intensidade,
        borda_suave: e.alvo.borda_suave,
        inverter: e.alvo.inverter,
        espaco: "fonte",
      };
      return [e.modo === "solido" ? { ...p, cor: e.cor } : p];
    }
    case "holofote": {
      const r = q.rastreio(e.alvo.rastreio);
      if (!r) return [];
      // escurece FORA do alvo: a máscara é a do alvo, invertida
      return [{ k: "regiao", modo: "escurecer", forma: formaRegiao(e.alvo, r, q.A), intensidade: e.escurecer, borda_suave: e.alvo.borda_suave, inverter: !e.alvo.inverter, espaco: "canvas" }];
    }
    case "destaque":
      return destaque(e, q, t);
    case "texto": {
      if (e.conteudo === "") return [];
      const s = q.sprite(pedidoDeTexto(e.id, e.conteudo, e.estilo, e.lugar, e.largura_max, t));
      if (!s) return [];
      const x0 = aoPixel(e.lugar.x * t.W - s.w * fatorH(e.lugar.h));
      const y0 = aoPixel(e.lugar.y * t.H - s.h * fatorV(e.lugar.v));
      return [{ k: "sprite", src: s.relativo, caixa: t.caixa(x0, y0, s.w, s.h), opacidade: 1 }];
    }
    case "carimbo": {
      const texto = textoCarimbo(e, q);
      if (texto === null) return [];
      const a = q.atlas(e.estilo, q.canvas.h);
      if (!a) return [];
      return [{ k: "atlas", src: a.relativo, celula: a.celula, glifos: a.glifos, texto: e.estilo.caixa ? ` ${texto} ` : texto, lugar: e.lugar, altura: z(a.celula.h / t.H) }];
    }
    case "imagem": {
      const tam = q.midia(e.midia);
      // ainda não chegou: sem desenho; tamanho inválido é mídia ausente
      if (!tam) return [];
      if (!(tam.w > 0 && tam.h > 0)) throw erro("midia_ausente", e.midia);
      const sw = e.largura * t.W;
      const sh = (sw * tam.h) / tam.w;
      const x0 = e.lugar.x * t.W - sw * fatorH(e.lugar.h);
      const y0 = e.lugar.y * t.H - sh * fatorV(e.lugar.v);
      return [{ k: "sprite", src: `midia:${e.midia}`, caixa: t.caixa(x0, y0, sw, sh), opacidade: e.opacidade }];
    }
    default:
      // estabilizar, lente, desentrelaçar, média, ajustes, nitidez, girar, enquadramento: pixel
      return [];
  }
}
