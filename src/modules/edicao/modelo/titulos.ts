/**
 * Espelho de `camada::titulo` (§5.5): título → `Primitiva`s em 0..1 do canvas (fundo e um sprite por bloco, na
 * ordem) e o pedido de sprite de cada bloco. Mesmas contas, na mesma ordem (fixture `primitivas-titulo.json`).
 * `modelo` é só rótulo: cada bloco é posto pelo próprio `lugar`, como o efeito texto. Fundo de imagem cobre o canvas
 * sem distorcer, centrado. Sprite ou imagem que o backend ainda não entregou: fica sem desenho até chegar.
 *
 * Os 5 modelos do §8.6 (P1.7) são arranjos dos blocos e do fundo, gravados quando o perito escolhe o modelo; o texto
 * sai sempre do sprite único do backend, o mesmo arquivo na prévia e no render (fixture `titulos-modelos.json`).
 */

import type { BlocoTexto, Caixa, EstiloTexto, Id, Lugar, PedidoSprite, Primitiva, QuadroPalco, Titulo } from "@domain/edicao";

const z = (v: number) => v + 0;
const fatorH = (h: Lugar["h"]) => (h === "esq" ? 0 : h === "centro" ? 0.5 : 1);
const fatorV = (v: Lugar["v"]) => (v === "topo" ? 0 : v === "meio" ? 0.5 : 1);
/** Arredonda ao pixel (meio sobe). */
const aoPixel = (v: number) => Math.floor(v + 0.5);
/** Canto de bloco mais longe que isto do canvas (px, 2^30): recusado, como no Rust. */
const MAX_LUGAR_PX = 1073741824;
const longe = (v: number) => !Number.isFinite(v) || Math.abs(v) > MAX_LUGAR_PX;

type Tamanho = { w: number; h: number };

/** Pedido de sprite do bloco `bloco` para esse canvas: `largura_max` passa de fração da largura (bloco) a fração da
 *  ALTURA (pedido). Índice fora da lista: pedido sem bloco e de largura 0 (o backend recusa). */
export function pedido(titulo: Titulo, bloco: number, canvas: Tamanho): PedidoSprite {
  const b = titulo.blocos[bloco];
  if (!b) return { blocos: [], largura_max: 0, altura_canvas: canvas.h, fundo: true };
  return { blocos: [b], largura_max: (b.largura_max * canvas.w) / canvas.h, altura_canvas: canvas.h, fundo: true };
}

/** Bloco com texto posto no canvas: id, sprite e caixa (0..1). */
export interface BlocoNoCanvas {
  id: Id;
  src: string;
  caixa: Caixa;
}

type Fontes = Pick<QuadroPalco, "canvas" | "sprite" | "midia">;

/** Fundo e blocos do título no quadro, na ordem do desenho. */
function montar(titulo: Titulo, q: Fontes): { fundo: Primitiva | null; blocos: BlocoNoCanvas[] } {
  const { w: W, h: H } = q.canvas;
  const caixa = (x: number, y: number, w: number, h: number): Caixa => ({ x: z(x / W), y: z(y / H), w: z(w / W), h: z(h / H) });
  let fundo: Primitiva | null = null;
  const f = titulo.fundo;
  if (f.tipo === "cor") {
    fundo = { k: "caminho", d: [["M", 0, 0], ["L", 1, 0], ["L", 1, 1], ["L", 0, 1], ["Z"]], preenche: f.cor };
  } else if (f.tipo === "imagem") {
    const tam = q.midia(f.midia);
    if (tam) {
      if (!(tam.w > 0 && tam.h > 0)) throw new Error(`midia_ausente: ${f.midia}`);
      // a maior das duas escalas cobre o canvas; a mesma nos dois eixos não distorce
      const e = Math.max(W / tam.w, H / tam.h);
      const [sw, sh] = [tam.w * e, tam.h * e];
      fundo = { k: "sprite", src: `midia:${f.midia}`, caixa: caixa((W - sw) * 0.5, (H - sh) * 0.5, sw, sh), opacidade: 1 };
    }
  }
  const blocos: BlocoNoCanvas[] = [];
  titulo.blocos.forEach((b, i) => {
    if (b.conteudo === "") return;
    const s = q.sprite(pedido(titulo, i, q.canvas));
    if (!s) return;
    const x0 = aoPixel(b.lugar.x * W - s.w * fatorH(b.lugar.h));
    const y0 = aoPixel(b.lugar.y * H - s.h * fatorV(b.lugar.v));
    if (longe(x0) || longe(y0)) throw new Error(`item_invalido: lugar do bloco ${b.id}`);
    blocos.push({ id: b.id, src: s.relativo, caixa: caixa(x0, y0, s.w, s.h) });
  });
  return { fundo, blocos };
}

/** Primitivas do título no quadro. Bloco de conteúdo vazio não desenha. Erro = `Error("<código>: <detalhe>")`:
 *  `midia_ausente` (imagem de tamanho 0) e `item_invalido` (canto do bloco a mais de 2^30 px do canvas). */
export function primitivas(titulo: Titulo, q: Fontes): Primitiva[] {
  const { fundo, blocos } = montar(titulo, q);
  const saida: Primitiva[] = fundo ? [fundo] : [];
  for (const b of blocos) saida.push({ k: "sprite", src: b.src, caixa: b.caixa, opacidade: 1 });
  return saida;
}

/** Os blocos com texto e sprite já chegado, postos no canvas (o que `primitivas` desenha depois do fundo). */
export function blocosNoCanvas(titulo: Titulo, q: Fontes): BlocoNoCanvas[] {
  return montar(titulo, q).blocos;
}

// ---- modelos (P1.7) ----

/** Área segura do texto, em fração de cada lado do canvas. */
export const AREA_SEGURA = 0.02;
/** Fundo que um modelo de cartão põe quando o título não tem nenhum. */
export const FUNDO_CHEIO = "#000000";
const BRANCO = "#ffffff";
const CINZA = "#c8c8c8";
const FAIXA = "#0000008c";

export type ModeloTitulo = Titulo["modelo"];
export type Papel = BlocoTexto["papel"];

/** Como um papel fica num modelo (espelho de `titulo::Arranjo`). */
export interface Arranjo {
  lugar: Lugar;
  largura_max: number;
  tamanho: number;
  negrito: boolean;
  alinhamento: Lugar["h"];
  cor: string;
  caixa: EstiloTexto["caixa"];
}

type Linha = [x: number, y: number, h: Lugar["h"], v: Lugar["v"], largura: number, tamanho: number, negrito: boolean, cor: string, faixa: number | null];

const CARTELA: Record<Papel, Linha> = {
  titulo: [0.5, 0.44, "centro", "base", 0.86, 0.078, true, BRANCO, null],
  subtitulo: [0.5, 0.5, "centro", "topo", 0.86, 0.052, false, BRANCO, null],
  corpo: [0.5, 0.62, "centro", "topo", 0.86, 0.042, false, BRANCO, null],
  rodape: [0.5, 0.93, "centro", "base", 0.86, 0.033, false, BRANCO, null],
};

const TABELA: Record<ModeloTitulo, Record<Papel, Linha>> = {
  cartela: CARTELA,
  livre: CARTELA,
  abertura: {
    titulo: [0.08, 0.34, "esq", "base", 0.84, 0.078, true, BRANCO, null],
    subtitulo: [0.08, 0.38, "esq", "topo", 0.84, 0.046, false, BRANCO, null],
    corpo: [0.08, 0.52, "esq", "topo", 0.84, 0.036, false, BRANCO, null],
    rodape: [0.08, 0.93, "esq", "base", 0.84, 0.03, false, CINZA, null],
  },
  evento: {
    titulo: [0.5, 0.05, "centro", "topo", 0.9, 0.052, true, BRANCO, 0.012],
    subtitulo: [0.5, 0.15, "centro", "topo", 0.9, 0.036, false, BRANCO, 0.012],
    corpo: [0.5, 0.225, "centro", "topo", 0.9, 0.033, false, BRANCO, 0.012],
    rodape: [0.5, 0.95, "centro", "base", 0.9, 0.028, false, BRANCO, 0.012],
  },
  terco_inferior: {
    titulo: [0.06, 0.76, "esq", "base", 0.7, 0.052, true, BRANCO, 0.013],
    subtitulo: [0.06, 0.775, "esq", "topo", 0.7, 0.033, false, BRANCO, 0.013],
    corpo: [0.06, 0.855, "esq", "topo", 0.7, 0.028, false, BRANCO, 0.013],
    rodape: [0.94, 0.06, "dir", "topo", 0.4, 0.024, false, BRANCO, 0.01],
  },
  encerramento: {
    titulo: [0.5, 0.42, "centro", "base", 0.86, 0.078, true, BRANCO, null],
    subtitulo: [0.5, 0.46, "centro", "topo", 0.86, 0.042, false, BRANCO, null],
    corpo: [0.5, 0.58, "centro", "topo", 0.86, 0.036, false, BRANCO, null],
    rodape: [0.5, 0.94, "centro", "base", 0.86, 0.028, false, CINZA, null],
  },
};

/** Arranjo do papel no modelo. `livre` não arranja: bloco novo nele vai como na cartela. */
export function arranjo(modelo: ModeloTitulo, papel: Papel): Arranjo {
  const [x, y, h, v, largura_max, tamanho, negrito, cor, faixa] = TABELA[modelo][papel];
  return { lugar: { x, y, h, v }, largura_max, tamanho, negrito, alinhamento: h, cor, caixa: faixa === null ? null : { cor: FAIXA, margem: faixa, raio: 0 } };
}

/** Escala do texto do modelo no canvas: 1 de 4:3 para cima; mais estreito (vertical), a largura manda. */
export function escalaDoCanvas(canvas: Tamanho): number {
  const k = (canvas.w * 3) / (canvas.h * 4);
  return Number.isFinite(k) && k > 0 ? Math.min(k, 1) : 1;
}

function naEscala(a: Arranjo, k: number): Arranjo {
  return { ...a, tamanho: a.tamanho * k, caixa: a.caixa ? { ...a.caixa, margem: a.caixa.margem * k } : null };
}

/** Faixa sobre o vídeo (presa a um clipe, sem fundo). */
export const sobreOVideo = (modelo: ModeloTitulo) => modelo === "evento" || modelo === "terco_inferior";

/** Cópia funda do bloco (mantém −0, que o JSON perderia). */
function copia(b: BlocoTexto): BlocoTexto {
  const e = b.estilo;
  return { ...b, estilo: { ...e, caixa: e.caixa ? { ...e.caixa } : null, contorno: e.contorno ? { ...e.contorno } : null }, lugar: { ...b.lugar } };
}

function arranjado(b: BlocoTexto, a: Arranjo): BlocoTexto {
  return {
    id: b.id,
    papel: b.papel,
    conteudo: b.conteudo,
    estilo: { fonte: b.estilo.fonte, negrito: a.negrito, italico: b.estilo.italico, tamanho: a.tamanho, cor: a.cor, alinhamento: a.alinhamento, caixa: a.caixa ? { ...a.caixa } : null, contorno: null },
    lugar: { ...a.lugar },
    largura_max: a.largura_max,
  };
}

/** O título no modelo, para o canvas da saída: cada bloco pelo arranjo do papel (conteúdo, id, ordem, fonte e itálico
 *  ficam). Faixa presa a um clipe fica sem fundo; o resto mantém cor ou imagem, e o que não tinha ganha `FUNDO_CHEIO`.
 *  `livre` só troca o rótulo. Puro: devolve um título novo. */
export function aplicarModelo(t: Titulo, modelo: ModeloTitulo, sobreposto: boolean, canvas: Tamanho): Titulo {
  if (modelo === "livre") return { modelo, fundo: { ...t.fundo }, blocos: t.blocos.map(copia) };
  const fundo: Titulo["fundo"] = sobreposto && sobreOVideo(modelo) ? { tipo: "transparente" } : t.fundo.tipo === "transparente" ? { tipo: "cor", cor: FUNDO_CHEIO } : { ...t.fundo };
  const k = escalaDoCanvas(canvas);
  return { modelo, fundo, blocos: t.blocos.map((b) => arranjado(b, naEscala(arranjo(modelo, b.papel), k))) };
}

/** Os blocos estão no lugar e na largura que o modelo dá a cada papel? (tamanho, cor e fonte são do perito; `livre`
 *  não arranja). Falso = escolher o modelo de novo rearranja. */
export function noArranjo(t: Pick<Titulo, "modelo" | "blocos">): boolean {
  if (t.modelo === "livre") return false;
  return t.blocos.every((b) => {
    const a = arranjo(t.modelo, b.papel);
    return b.lugar.x === a.lugar.x && b.lugar.y === a.lugar.y && b.lugar.h === a.lugar.h && b.lugar.v === a.lugar.v && b.largura_max === a.largura_max;
  });
}

/** Bloco novo do papel no modelo do título, para o canvas da saída. Do primeiro bloco vêm a fonte, o itálico, a escala
 *  do "Tamanho" e a cor, quando o perito trocou a do modelo. */
export function blocoNoModelo(t: Pick<Titulo, "modelo" | "blocos">, papel: Papel, id: Id, conteudo: string, canvas: Tamanho): BlocoTexto {
  const k = escalaDoCanvas(canvas);
  const a = naEscala(arranjo(t.modelo, papel), k);
  const estilo: EstiloTexto = { fonte: "sans", negrito: a.negrito, italico: false, tamanho: a.tamanho, cor: a.cor, alinhamento: a.alinhamento, caixa: a.caixa ? { ...a.caixa } : null, contorno: null };
  const r = t.blocos[0];
  if (r) {
    const ar = naEscala(arranjo(t.modelo, r.papel), k);
    estilo.fonte = r.estilo.fonte;
    estilo.italico = r.estilo.italico;
    const e = r.estilo.tamanho / ar.tamanho;
    if (Number.isFinite(e) && e > 0) estilo.tamanho = a.tamanho * e;
    if (r.estilo.cor !== ar.cor) estilo.cor = r.estilo.cor;
  }
  return { id, papel, conteudo, estilo, lugar: { ...a.lugar }, largura_max: a.largura_max };
}

/** Título novo do modelo: o fundo do modelo e um bloco de título vazio (que não desenha até ter texto). */
export function tituloNoModelo(modelo: ModeloTitulo, sobreposto: boolean, idBloco: Id, canvas: Tamanho): Titulo {
  const vazio: Titulo = { modelo, fundo: { tipo: "transparente" }, blocos: [] };
  return aplicarModelo({ ...vazio, blocos: [blocoNoModelo(vazio, "titulo", idBloco, "", canvas)] }, modelo, sobreposto, canvas);
}

/** Caixa (0..1 do canvas) dentro da área segura, com folga de 1e-9. */
export function naAreaSegura(c: Caixa): boolean {
  const EPS = 1e-9;
  const [a, b] = [AREA_SEGURA - EPS, 1 - AREA_SEGURA + EPS];
  return c.x >= a && c.y >= a && c.x + c.w <= b && c.y + c.h <= b;
}
