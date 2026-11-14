/** Clipe · Entrada: transição de entrada. Fase 0: corte ou escurecer (catálogo `juncao::TRANSICOES`). */

import type { ClipePrincipal } from "@domain/edicao";
import { noClipe, transicaoEm } from "../acoes";
import { Escolha, Grade, Numero, Secao } from "../campos";
import type { SecaoProps } from "../registro";

const CORTE = "corte";
const OPCOES = [
  { v: CORTE, r: "Corte" },
  { v: "escurecer", r: "Escurecer" },
];

export function EntradaSecao({ clipe: c }: SecaoProps<ClipePrincipal>) {
  const id = c.id;
  const tr = c.transicao_entrada;
  return (
    <Secao titulo="Entrada">
      <Grade>
        {/* transição de versão futura: nenhuma opção marcada até o perito escolher */}
        <Escolha rotulo="Transição" larga valor={tr?.tipo ?? CORTE} opcoes={OPCOES} aplicar={(v) => transicaoEm(id, v === CORTE ? null : v)} />
        {tr && (
          <Numero
            rotulo="Duração (s)"
            item={id}
            valor={tr.duracao_s}
            min={0.04}
            max={10}
            passo={0.1}
            casas={2}
            valido={(v) => v > 0}
            aplicar={(v) => noClipe<ClipePrincipal>(id, (x) => void (x.transicao_entrada && (x.transicao_entrada.duracao_s = v)))}
          />
        )}
      </Grade>
    </Secao>
  );
}
