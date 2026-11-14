/**
 * Espelho de `camada::titulo` (§5.5): título → `Primitiva`s em 0..1 do canvas (fundo e um sprite por bloco, na
 * ordem) e o pedido de sprite de cada bloco. Mesmas contas, na mesma ordem (fixture `primitivas-titulo.json`).
 * `modelo` é só rótulo: cada bloco é posto pelo próprio `lugar`, como o efeito texto. Fundo de imagem cobre o canvas
 * sem distorcer, centrado. Sprite ou imagem que o backend ainda não entregou: fica sem desenho até chegar.
 */

import type { Caixa, Lugar, PedidoSprite, Primitiva, QuadroPalco, Titulo } from "@domain/edicao";

const z = (v: number) => v + 0;
const fatorH = (h: Lugar["h"]) => (h === "esq" ? 0 : h === "centro" ? 0.5 : 1);
const fatorV = (v: Lugar["v"]) => (v === "topo" ? 0 : v === "meio" ? 0.5 : 1);
/** Arredonda ao pixel (meio sobe). */
const aoPixel = (v: number) => Math.floor(v + 0.5);
/** Canto de bloco mais longe que isto do canvas (px, 2^30): recusado, como no Rust. */
const MAX_LUGAR_PX = 1073741824;
const longe = (v: number) => !Number.isFinite(v) || Math.abs(v) > MAX_LUGAR_PX;

/** Pedido de sprite do bloco `bloco` para esse canvas: `largura_max` passa de fração da largura (bloco) a fração da
 *  ALTURA (pedido). Índice fora da lista: pedido sem bloco e de largura 0 (o backend recusa). */
export function pedido(titulo: Titulo, bloco: number, canvas: { w: number; h: number }): PedidoSprite {
  const b = titulo.blocos[bloco];
  if (!b) return { blocos: [], largura_max: 0, altura_canvas: canvas.h, fundo: true };
  return { blocos: [b], largura_max: (b.largura_max * canvas.w) / canvas.h, altura_canvas: canvas.h, fundo: true };
}

/** Primitivas do título no quadro. Bloco de conteúdo vazio não desenha. Erro = `Error("<código>: <detalhe>")`:
 *  `midia_ausente` (imagem de tamanho 0) e `item_invalido` (canto do bloco a mais de 2^30 px do canvas). */
export function primitivas(titulo: Titulo, q: Pick<QuadroPalco, "canvas" | "sprite" | "midia">): Primitiva[] {
  const { w: W, h: H } = q.canvas;
  const caixa = (x: number, y: number, w: number, h: number): Caixa => ({ x: z(x / W), y: z(y / H), w: z(w / W), h: z(h / H) });
  const saida: Primitiva[] = [];
  const f = titulo.fundo;
  if (f.tipo === "cor") {
    saida.push({ k: "caminho", d: [["M", 0, 0], ["L", 1, 0], ["L", 1, 1], ["L", 0, 1], ["Z"]], preenche: f.cor });
  } else if (f.tipo === "imagem") {
    const tam = q.midia(f.midia);
    if (tam) {
      if (!(tam.w > 0 && tam.h > 0)) throw new Error(`midia_ausente: ${f.midia}`);
      // a maior das duas escalas cobre o canvas; a mesma nos dois eixos não distorce
      const e = Math.max(W / tam.w, H / tam.h);
      const [sw, sh] = [tam.w * e, tam.h * e];
      saida.push({ k: "sprite", src: `midia:${f.midia}`, caixa: caixa((W - sw) * 0.5, (H - sh) * 0.5, sw, sh), opacidade: 1 });
    }
  }
  titulo.blocos.forEach((b, i) => {
    if (b.conteudo === "") return;
    const s = q.sprite(pedido(titulo, i, q.canvas));
    if (!s) return;
    const x0 = aoPixel(b.lugar.x * W - s.w * fatorH(b.lugar.h));
    const y0 = aoPixel(b.lugar.y * H - s.h * fatorV(b.lugar.v));
    if (longe(x0) || longe(y0)) throw new Error(`item_invalido: lugar do bloco ${b.id}`);
    saida.push({ k: "sprite", src: s.relativo, caixa: caixa(x0, y0, s.w, s.h), opacidade: 1 });
  });
  return saida;
}
