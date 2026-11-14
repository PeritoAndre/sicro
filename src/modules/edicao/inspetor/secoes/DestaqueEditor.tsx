/** Destaque: forma, cor, espessura, contorno, rótulo (com o lado), tamanho e direção da seta, margem. */

import { ArrowDown, ArrowLeft, ArrowRight, ArrowUp } from "lucide-react";
import type { Draft } from "immer";
import type { EfDestaque } from "@domain/edicao";
import { noEfeito } from "../acoes";
import { Cor, Escolha, Grade, Marca, Numero, Texto } from "../campos";
import { estiloPadrao, TAMANHO_TEXTO } from "../novos";
import type { EditorEfeitoProps } from "../registro";
import { round } from "./comum";

/** Desenho de cada forma (as opções não têm nome na tela). */
export function FormaIcone({ forma }: { forma: EfDestaque["forma"] | "poligono" | "silhueta" }) {
  const p = { fill: "none", stroke: "currentColor", strokeWidth: 1.6, strokeLinecap: "round" as const, strokeLinejoin: "round" as const };
  return (
    <svg width={16} height={14} viewBox="0 0 16 14" aria-hidden>
      {forma === "circulo" && <circle cx={8} cy={7} r={5.5} {...p} />}
      {forma === "elipse" && <ellipse cx={8} cy={7} rx={7} ry={4.5} {...p} />}
      {forma === "retangulo" && <rect x={1.5} y={2.5} width={13} height={9} rx={0.5} {...p} />}
      {forma === "seta" && <path d="M2 12 L13 2 M7.5 2 H13 V7.5" {...p} />}
      {forma === "colchetes" && <path d="M5 2.5 H2 V11.5 H5 M11 2.5 H14 V11.5 H11" {...p} />}
      {forma === "poligono" && <path d="M3 3 L13 2 L14 11 L6 12 Z" {...p} />}
      {forma === "silhueta" && <path d="M8 2 a2 2 0 1 1 0.01 0 M5 13 C5 7 11 7 11 13" {...p} />}
    </svg>
  );
}

const FORMAS: EfDestaque["forma"][] = ["retangulo", "elipse", "circulo", "seta", "colchetes"];
const LADOS = [
  { v: "cima" as const, icone: <ArrowUp size={13} /> },
  { v: "baixo" as const, icone: <ArrowDown size={13} /> },
  { v: "esq" as const, icone: <ArrowLeft size={13} /> },
  { v: "dir" as const, icone: <ArrowRight size={13} /> },
];

export function DestaqueEditor({ valor: e }: EditorEfeitoProps<EfDestaque>) {
  const id = e.id;
  const em = (f: (x: Draft<EfDestaque>) => void) => noEfeito<EfDestaque>(id, f);
  const sobrePonto = e.forma === "circulo" || e.forma === "seta";
  return (
    <Grade>
      <Escolha rotulo="Forma" larga valor={e.forma} opcoes={FORMAS.map((v) => ({ v, icone: <FormaIcone forma={v} /> }))} aplicar={(v) => em((x) => void (x.forma = v))} />
      <Cor rotulo="Cor" item={id} valor={e.traco.cor} aplicar={(v) => em((x) => void (x.traco.cor = v))} />
      <Numero
        rotulo="Espessura"
        item={id}
        valor={round(e.traco.espessura * 100, 2)}
        min={0.1}
        max={5}
        passo={0.1}
        casas={1}
        unidade="%"
        aplicar={(v) => em((x) => void (x.traco.espessura = v / 100))}
      />
      <Marca rotulo="Contorno" larga valor={e.traco.contorno} aplicar={(v) => em((x) => void (x.traco.contorno = v))} />
      {sobrePonto && (
        <Numero rotulo="Tamanho" item={id} valor={round(e.tamanho * 100, 1)} min={1} max={100} passo={1} casas={1} unidade="%" aplicar={(v) => em((x) => void (x.tamanho = v / 100))} />
      )}
      {e.forma === "seta" && (
        <Numero
          rotulo="Direção"
          item={id}
          valor={e.angulo_graus}
          min={-360}
          max={360}
          passo={15}
          casas={0}
          unidade="°"
          aplicar={(v) => em((x) => void (x.angulo_graus = v))}
        />
      )}
      {!sobrePonto && (
        <Numero rotulo="Margem" item={id} valor={round(e.margem * 100, 1)} min={-50} max={100} passo={1} casas={1} unidade="%" aplicar={(v) => em((x) => void (x.margem = v / 100))} />
      )}
      <Texto
        rotulo="Rótulo"
        larga
        valor={e.rotulo?.texto ?? ""}
        aplicar={(v) =>
          em((x) => {
            if (v === "") x.rotulo = null;
            else if (x.rotulo) x.rotulo.texto = v;
            else x.rotulo = { texto: v, estilo: { ...estiloPadrao(TAMANHO_TEXTO.p), cor: x.traco.cor }, lado: "cima" };
          })
        }
      />
      {e.rotulo && <Escolha rotulo="Rótulo" semRotulo larga valor={e.rotulo.lado} opcoes={LADOS} aplicar={(v) => em((x) => void (x.rotulo && (x.rotulo.lado = v)))} />}
    </Grade>
  );
}
