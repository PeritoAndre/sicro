/** Clipe · Entrada: transição de entrada pelo catálogo `juncao::TRANSICOES` (corte e escurecer; as do `xfade` e o "Som"
 *  no P1.1). `FechoSecao`: a transição de saída do último clipe (`fecho`), na aba Saída, rótulo "Fecho" (P1.1). */

import type { ClipePrincipal } from "@domain/edicao";
import { noClipe, transicaoEm } from "../acoes";
import { Escolha, Grade, Numero, Secao } from "../campos";
import type { SecaoProps } from "../registro";

const CORTE = "corte";
const OPCOES = [
  { v: CORTE, r: "Corte" },
  { v: "escurecer", r: "Escurecer" },
];

/** O P1.1 liga ao entregar: a seção "Fecho" aparece na aba Saída. */
export const FECHO_PRONTO = false;

/** Saída · "Fecho": "Transição" · "Duração (s)" · "Som" (Acompanhar, Cortar). Fase 0: casca. */
export function FechoSecao() {
  return null;
}

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
