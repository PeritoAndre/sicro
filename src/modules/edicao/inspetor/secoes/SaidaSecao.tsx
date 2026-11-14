/** Saída: "Automática" segue as fontes (§8.1) até o perito mexer em tamanho ou taxa; fundo e áudio. */

import type { Racional, Saida } from "@domain/edicao";
import { useEdicaoStore } from "../../store/edicaoStore";
import { Cor, Grade, Lista, Marca, Numero, Secao } from "../campos";

/** Pares, 64..7680 × 64..4320 (§2.1). */
export const ladoValido = (max: number) => (v: number) => Number.isInteger(v) && v % 2 === 0 && v >= 64 && v <= max;

const TAXAS: Racional[] = [
  { num: 25, den: 2 },
  { num: 15, den: 1 },
  { num: 24000, den: 1001 },
  { num: 24, den: 1 },
  { num: 25, den: 1 },
  { num: 30000, den: 1001 },
  { num: 30, den: 1 },
  { num: 50, den: 1 },
  { num: 60000, den: 1001 },
  { num: 60, den: 1 },
];

const chave = (r: Racional) => `${r.num}/${r.den}`;
/** "29,97", "12,5", "30": até 3 casas, sem zeros à direita. */
export const taxaTexto = (r: Racional) => String(Math.round((r.num / r.den) * 1000) / 1000).replace(".", ",");

export function SaidaSecao() {
  const s = useEdicaoStore((x) => x.doc?.saida ?? null);
  if (!s) return null;
  const item = "saida";
  const manual = (f: (x: Saida) => void) => (d: { saida: Saida }) => {
    f(d.saida);
    d.saida.auto = false;
  };
  const taxas = TAXAS.some((t) => chave(t) === chave(s.fps)) ? TAXAS : [...TAXAS, s.fps];
  return (
    <Secao>
      <Grade>
        <Marca rotulo="Automática" larga valor={s.auto} aplicar={(v) => (d) => void (d.saida.auto = v)} />
        <Numero rotulo="Largura" item={item} valor={s.largura} min={64} max={7680} passo={2} casas={0} valido={ladoValido(7680)} aplicar={(v) => manual((x) => void (x.largura = v))} />
        <Numero rotulo="Altura" item={item} valor={s.altura} min={64} max={4320} passo={2} casas={0} valido={ladoValido(4320)} aplicar={(v) => manual((x) => void (x.altura = v))} />
        <Lista
          rotulo="Quadros/s"
          valor={chave(s.fps)}
          opcoes={taxas.map((t) => ({ v: chave(t), r: taxaTexto(t) }))}
          aplicar={(v) =>
            manual((x) => {
              const [num, den] = v.split("/").map(Number) as [number, number];
              x.fps = { num, den };
            })
          }
        />
        <Cor rotulo="Fundo" item={item} valor={s.cor_fundo} aplicar={(v) => (d) => void (d.saida.cor_fundo = v)} />
        <Marca rotulo="Áudio" larga valor={s.audio} aplicar={(v) => (d) => void (d.saida.audio = v)} />
      </Grade>
    </Secao>
  );
}
