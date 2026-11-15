/**
 * Espelho de `camada/medidas.rs` e das contas de `edicao/medidas.rs` (P1.3): pontas da régua pela homografia do
 * retrato, texto da velocidade e as primitivas (fixtures `primitivas-medidas.json` e `medidas-*.json`). Mesmas contas,
 * na mesma ordem: só `+ − × ÷`, `floor`, `abs` e o `cosSenGraus`; a homografia pelo `applyH`/`invert3` da análise.
 * Erro = `Error("<código>: <campo>")`.
 */

import type { Caixa, EfRegua, EfVelocidadeExibida, EstiloTexto, Lugar, PedidoSprite, Ponto, Primitiva, QuadroPalco, RetratoCalculo, Traco } from "@domain/edicao";
import { applyH, invert3 } from "@modules/video/editor/medir/geo";
import { caixaNoCanvas, ponto, type Afim } from "../../modelo/geometria";
import { COR_CONTORNO, cosSenGraus } from "./primitivas";

type Seg = Extract<Primitiva, { k: "caminho" }>["d"][number];

/** Teto de divisões da régua (o mesmo do Rust). */
export const MAX_DIVISOES = 100;
/** Teto do comprimento da régua (m). */
export const MAX_COMPRIMENTO_M = 1000;
const KMH_COM_CASA = 10;
const KMH_MAX = 1e9;
const TIQUE_MEIO = 0.0125;
const TIQUE_PONTA = 0.05;
const TIQUE_MIN_MEIO = 1.5;
const TIQUE_MIN_PONTA = 2.5;
const ROTULO_TAMANHO = 0.033;
const ROTULO_CONTORNO = 0.003;
const LARGURA_TEXTO = 0.5;

const z = (v: number) => v + 0;
const aoPixel = (v: number) => Math.floor(v + 0.5);
const fatorH = (h: Lugar["h"]) => (h === "esq" ? 0 : h === "centro" ? 0.5 : 1);
const fatorV = (v: Lugar["v"]) => (v === "topo" ? 0 : v === "meio" ? 0.5 : 1);
const falha = (codigo: string, campo: string) => new Error(`${codigo}: ${campo}`);

/** Pontas e divisões em 0..1 da origem (a primeira é a âncora): `plane` e `line` pela inversa da homografia;
 *  `cross_ratio` (1D) na direção da linha de referência, só o sentido da direção vale. */
export function pontasDaRegua(e: EfRegua): Ponto[] {
  const c = e.calibracao;
  const h = c.h;
  if (h.length !== 9 || !h.every((v) => Number.isFinite(v)) || c.espaco.w === 0 || c.espaco.h === 0) throw falha("item_invalido", "calibracao");
  if (!(e.comprimento_m > 0 && e.comprimento_m <= MAX_COMPRIMENTO_M)) throw falha("item_invalido", "comprimento_m");
  if (e.divisoes > MAX_DIVISOES) throw falha("item_invalido", "divisoes");
  if (!Number.isFinite(e.direcao_graus)) throw falha("item_invalido", "direcao_graus");
  if (!(Number.isFinite(e.ancora.x) && Number.isFinite(e.ancora.y))) throw falha("item_invalido", "ancora");
  const fora = () => falha("regua_fora_da_calibracao", "comprimento_m");
  const [w, a] = [c.espaco.w, c.espaco.h];
  const [px, py] = [e.ancora.x * w, e.ancora.y * a];
  const n = Math.max(e.divisoes, 1);
  const [cs, sn] = cosSenGraus(e.direcao_graus);
  const v: Ponto[] = [e.ancora];
  const [h0, h1, h2, , , , h6, h7, h8] = h as [number, number, number, number, number, number, number, number, number];
  if (c.metodo === "cross_ratio") {
    const [ex, ey] = Math.abs(h0) + Math.abs(h1) >= Math.abs(h6) + Math.abs(h7) ? [h0, h1] : [h6, h7];
    const u0 = h0 * px + h1 * py + h2;
    const w0 = h6 * px + h7 * py + h8;
    const x0 = u0 / w0;
    const alfa = h0 * ex + h1 * ey;
    const gama = h6 * ex + h7 * ey;
    if (!(Number.isFinite(x0) && w0 !== 0)) throw fora();
    const sinal = cs >= 0 ? 1 : -1;
    for (let k = 1; k <= n; k++) {
      const xk = x0 + sinal * ((e.comprimento_m * k) / n);
      const d = (xk * w0 - u0) / (alfa - xk * gama);
      const wk = w0 + gama * d;
      if (!(Number.isFinite(d) && wk * w0 > 0)) throw fora();
      v.push({ x: (px + d * ex) / w + 0, y: (py + d * ey) / a + 0 });
    }
    return v;
  }
  const inv = invert3(h);
  if (!inv) throw fora();
  const peso = (x: number, y: number) => inv[6]! * x + inv[7]! * y + inv[8]!;
  const { x: x0, y: y0 } = applyH(h, px, py);
  // no mundo, o lado da câmera é o do peso da âncora: ponta do outro lado passou do horizonte
  const p0 = peso(x0, y0);
  if (!(Number.isFinite(x0) && Number.isFinite(y0) && Number.isFinite(p0) && p0 !== 0)) throw fora();
  for (let k = 1; k <= n; k++) {
    const d = (e.comprimento_m * k) / n;
    const [xm, ym] = [x0 + d * cs, y0 + d * sn];
    const q = applyH(inv, xm, ym);
    if (!(Number.isFinite(q.x) && Number.isFinite(q.y) && peso(xm, ym) * p0 > 0)) throw fora();
    v.push({ x: q.x / w + 0, y: q.y / a + 0 });
  }
  return v;
}

const EPS = 1e-9;

/** Projeção de `p` na reta de `a` a `b` (em px do espaço) cai entre os dois. */
function noSegmento(p: Ponto, a: Ponto, b: Ponto, espaco: { w: number; h: number }): boolean {
  const [w, h] = [espaco.w, espaco.h];
  const [vx, vy] = [(b.x - a.x) * w, (b.y - a.y) * h];
  const l2 = vx * vx + vy * vy;
  if (!(l2 > 0)) return false;
  const t = ((p.x - a.x) * w * vx + (p.y - a.y) * h * vy) / l2;
  return t >= -EPS && t <= 1 + EPS;
}

/** Par-ímpar; na aresta conta como dentro. */
function noPoligono(p: Ponto, pol: readonly Ponto[]): boolean {
  const n = pol.length;
  let dentro = false;
  for (let i = 0; i < n; i++) {
    const [a, b] = [pol[i]!, pol[(i + n - 1) % n]!];
    const cruz = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    if (Math.abs(cruz) <= EPS && p.x >= Math.min(a.x, b.x) - EPS && p.x <= Math.max(a.x, b.x) + EPS && p.y >= Math.min(a.y, b.y) - EPS && p.y <= Math.max(a.y, b.y) + EPS) return true;
    if (a.y > p.y !== b.y > p.y && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x) dentro = !dentro;
  }
  return dentro;
}

/** `pontasDaRegua` só com a régua inteira na região do retrato (polígono no plano; segmento dos extremos no 1D):
 *  âncora fora = `regua_fora_da_calibracao: ancora`; ponta fora = `…: comprimento_m`. É a que o desenho usa. */
export function pontasConferidas(e: EfRegua): Ponto[] {
  const v = pontasDaRegua(e);
  const r = e.calibracao.regiao;
  const dentro = (p: Ponto) => (r.length < 2 ? true : r.length === 2 ? noSegmento(p, r[0]!, r[1]!, e.calibracao.espaco) : noPoligono(p, r));
  if (!dentro(v[0]!)) throw falha("regua_fora_da_calibracao", "ancora");
  if (!dentro(v[v.length - 1]!)) throw falha("regua_fora_da_calibracao", "comprimento_m");
  return v;
}

/** Inteiro `n` (valor × 10^casas) em decimal com vírgula, sem zeros à direita. */
function decimal(n: number, casas: 0 | 1 | 2): string {
  const esc = casas === 0 ? 1 : casas === 1 ? 10 : 100;
  const int = Math.floor(n / esc);
  const frac = n - int * esc;
  if (casas === 0 || frac === 0) return String(int);
  let f = String(frac).padStart(casas, "0");
  while (f.endsWith("0")) f = f.slice(0, -1);
  return `${int},${f}`;
}

/** "42 km/h (39 a 45)" com faixa, "≈ 42 km/h" sem; abaixo de 10 km/h uma casa; a faixa arredonda para fora. */
export function textoDaVelocidade(c: RetratoCalculo): string {
  const lim = (v: number) => (Number.isNaN(v) ? 0 : Math.min(Math.max(v, 0), KMH_MAX));
  const kmh = lim(c.kmh);
  const casas = kmh < KMH_COM_CASA ? 1 : 0;
  const esc = casas === 1 ? 10 : 1;
  const valor = decimal(Math.floor(kmh * esc + 0.5), casas);
  if (!c.faixa) return `≈ ${valor} km/h`;
  const [a, b] = c.faixa;
  const [lo, hi] = [lim(Math.min(a, b)), lim(Math.max(a, b))];
  return `${valor} km/h (${decimal(Math.floor(lo * esc), casas)} a ${decimal(Math.ceil(hi * esc), casas)})`;
}

/** Rótulo da régua: "1 m", "0,5 m", "2,25 m". */
export function textoDoComprimento(m: number): string {
  const v = Number.isNaN(m) ? 0 : Math.min(Math.max(m, 0), MAX_COMPRIMENTO_M);
  return `${decimal(Math.floor(v * 100 + 0.5), 2)} m`;
}

function pedido(id: string, conteudo: string, estilo: EstiloTexto, q: QuadroPalco): PedidoSprite {
  const { w: W, h: H } = q.canvas;
  return {
    blocos: [{ id, papel: "corpo", conteudo, estilo, lugar: { x: 0, y: 0, h: "esq", v: "topo" }, largura_max: LARGURA_TEXTO }],
    largura_max: (LARGURA_TEXTO * W) / H,
    altura_canvas: H,
    fundo: true,
  };
}

function estiloDoRotulo(t: Traco): EstiloTexto {
  return {
    fonte: "sans",
    negrito: false,
    italico: false,
    tamanho: ROTULO_TAMANHO,
    cor: t.cor,
    alinhamento: "centro",
    caixa: null,
    contorno: t.contorno ? { cor: "#000000", largura: ROTULO_CONTORNO } : null,
  };
}

const caixa = (x: number, y: number, sw: number, sh: number, W: number, H: number): Caixa => ({ x: z(x / W), y: z(y / H), w: z(sw / W), h: z(sh / H) });

/** Régua: linha pelas divisões e um tique em cada uma (meia altura proporcional ao passo local, com mínimo pela largura
 *  do traço), rótulo depois da ponta. */
export function primitivasRegua(e: EfRegua, q: QuadroPalco): Primitiva[] {
  const pontos = pontasConferidas(e);
  const { w: W, h: H } = q.canvas;
  const c = pontos.map((p): [number, number] => {
    const r = ponto(q.A as Afim, p);
    return [r.x * W, r.y * H];
  });
  const n = c.length - 1;
  const m = (x: number, y: number): Seg => ["M", z(x / W), z(y / H)];
  const l = (x: number, y: number): Seg => ["L", z(x / W), z(y / H)];
  const d: Seg[] = [m(c[0]![0], c[0]![1])];
  for (const [x, y] of c.slice(1)) d.push(l(x, y));
  const largura = e.traco.espessura * H;
  for (let k = 0; k <= n; k++) {
    const ponta = k === 0 || k === n;
    const [a, b] = k === 0 ? [c[0]!, c[1]!] : k === n ? [c[n - 1]!, c[n]!] : [c[k - 1]!, c[k + 1]!];
    const f = (ponta ? TIQUE_PONTA : TIQUE_MEIO) * n;
    let [nx, ny] = [-(b[1] - a[1]) * f, (b[0] - a[0]) * f];
    // régua curta na imagem: o tique não some dentro da linha
    const meia = Math.sqrt(nx * nx + ny * ny);
    const minimo = (ponta ? TIQUE_MIN_PONTA : TIQUE_MIN_MEIO) * largura;
    if (meia > 0 && meia < minimo) {
      const aumento = minimo / meia;
      nx *= aumento;
      ny *= aumento;
    }
    const [x, y] = c[k]!;
    d.push(m(x - nx, y - ny), l(x + nx, y + ny));
  }
  const traco = e.traco.contorno ? { cor: e.traco.cor, largura: e.traco.espessura, contorno: COR_CONTORNO } : { cor: e.traco.cor, largura: e.traco.espessura };
  const saida: Primitiva[] = [{ k: "caminho", d, traco }];
  if (e.rotulo) {
    const s = q.sprite(pedido(e.id, textoDoComprimento(e.comprimento_m), estiloDoRotulo(e.traco), q));
    if (s) {
      const [sw, sh] = [s.w, s.h];
      const [tx, ty] = c[n]!;
      const [vx, vy] = [tx - c[0]![0], ty - c[0]![1]];
      const g = e.traco.espessura * H * 2;
      // depois da ponta, no sentido da régua
      const [x, y] = Math.abs(vx) >= Math.abs(vy) ? [vx >= 0 ? tx + g : tx - g - sw, ty - sh * 0.5] : [tx - sw * 0.5, vy >= 0 ? ty + g : ty - g - sh];
      saida.push({ k: "sprite", src: s.relativo, caixa: caixa(aoPixel(x), aoPixel(y), sw, sh, W, H), opacidade: 1 });
    }
  }
  return saida;
}

/** Velocidade: o texto no lugar fixo, ou centrado no centro do rastreio (no canvas) mais (`dx`, `dy`). */
export function primitivasVelocidade(e: EfVelocidadeExibida, q: QuadroPalco): Primitiva[] {
  const { w: W, h: H } = q.canvas;
  let centro: [number, number] | null = null;
  if (e.onde.modo === "segue") {
    const r = q.rastreio(e.onde.rastreio);
    if (!r) return [];
    let cx: number, cy: number;
    if (!(r.w > 0) && !(r.h > 0)) {
      const p = ponto(q.A as Afim, { x: r.x, y: r.y });
      [cx, cy] = [p.x * W, p.y * H];
    } else {
      const c = caixaNoCanvas(q.A as Afim, r);
      const [x0, y0, bw, bh] = [c.x * W, c.y * H, c.w * W, c.h * H];
      [cx, cy] = [x0 + bw * 0.5, y0 + bh * 0.5];
    }
    centro = [cx + e.onde.dx * W, cy + e.onde.dy * H];
  }
  const s = q.sprite(pedido(e.id, textoDaVelocidade(e.calculo), e.estilo, q));
  if (!s) return [];
  const [sw, sh] = [s.w, s.h];
  let x0: number, y0: number;
  if (centro) [x0, y0] = [aoPixel(centro[0] - sw * 0.5), aoPixel(centro[1] - sh * 0.5)];
  else if (e.onde.modo === "fixa") {
    const g = e.onde.lugar;
    [x0, y0] = [aoPixel(g.x * W - sw * fatorH(g.h)), aoPixel(g.y * H - sh * fatorV(g.v))];
  } else return [];
  return [{ k: "sprite", src: s.relativo, caixa: caixa(x0, y0, sw, sh, W, H), opacidade: 1 }];
}
