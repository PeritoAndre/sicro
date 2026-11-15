/** Saída · selos (P1.R): "Selo de velocidade" · "Selo de ampliação" · "Selo de realce". Nascem ligados; desligado
 *  vira o aviso `selo_desligado` e sai no relatório. A média de quadros e a imagem por IA têm selo fixo (sem controle). */

import type { Saida } from "@domain/edicao";
import { useEdicaoStore } from "../../store/edicaoStore";
import { Grade, Marca, Secao } from "../campos";

export const SELOS_PRONTOS = true;

type Selo = keyof Saida["selos"];

/** Na ordem do §8.6. */
export const SELOS: readonly { k: Selo; r: string }[] = [
  { k: "velocidade", r: "Selo de velocidade" },
  { k: "ampliacao", r: "Selo de ampliação" },
  { k: "realce", r: "Selo de realce" },
];

export function SelosSecao() {
  const selos = useEdicaoStore((x) => x.doc?.saida.selos ?? null);
  if (!selos) return null;
  return (
    <Secao>
      <Grade>
        {SELOS.map(({ k, r }) => (
          <Marca key={k} rotulo={r} larga valor={selos[k]} aplicar={(v) => (d) => void (d.saida.selos[k] = v)} />
        ))}
      </Grade>
    </Secao>
  );
}
