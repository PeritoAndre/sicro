/** Alças com a ferramenta de rastreio ligada (teste do dono, 16/11: depois do Seguir as alças sumiam). */
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Alca, PedidoMarcacao } from "@domain/edicao";
import { CamadaEdicao, alcasComFerramenta } from "./camadaEdicao";

const alca: Alca = { id: "r1:mover", tipo: "mover", em: { x: 0.5, y: 0.5 }, cursor: "move", arrastar: () => ({ rastreio: "r1", ancora: { t: 0, pts: null, v: { x: 0, y: 0, w: 0.1, h: 0.1 } } }) };
const caixa: PedidoMarcacao = { tipo: "caixa", rastreio: "r1", efeito: "e1" };

describe("alças com a ferramenta", () => {
  it("só com a ferramenta de rastreio do próprio efeito (ou sem ferramenta)", () => {
    expect(alcasComFerramenta(null, "e1")).toBe(true);
    expect(alcasComFerramenta(caixa, "e1")).toBe(true);
    expect(alcasComFerramenta(caixa, "e2")).toBe(false);
    expect(alcasComFerramenta({ tipo: "regiao", efeito: "e1" } as PedidoMarcacao, "e1")).toBe(false);
  });

  it("a camada desenha as alças que recebe, com a ferramenta ligada", () => {
    const html = renderToStaticMarkup(
      <CamadaEdicao
        alcas={[alca]}
        vista={{ zoom: 1, x: 0, y: 0 } as never}
        img={{ x: 0, y: 0, w: 640, h: 360 } as never}
        contorno={null}
        ferramenta={caixa}
        saida={{ w: 640, h: 360 }}
        onArrasto={() => {}}
        onMarcar={() => {}}
        onFundo={() => {}}
      />,
    );
    expect(html).toContain('data-alca="r1:mover"');
  });
});
