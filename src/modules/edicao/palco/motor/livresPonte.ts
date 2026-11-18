/**
 * Ponto único do palco para as funções do texto livre que a base da onda 2 ainda não tem (`dentroDoTexto`,
 * `cantosDoTexto`, pacote TL): lidas de `palco/livres.ts` pelo nome; sem elas, nada é clicável nem contornado. O fecho
 * do bloco junta o `livres.ts` do TL e isto passa a valer sem mudar.
 */

import type { Ponto } from "@domain/edicao";
import * as livres from "../livres";
import type { LivreNoQuadro } from "../livres";

type Tela = { w: number; h: number };
const doTL = livres as unknown as {
  dentroDoTexto?: (item: LivreNoQuadro, canvas: Tela, p: Ponto) => boolean;
  cantosDoTexto?: (item: LivreNoQuadro, canvas: Tela) => Ponto[];
};

/** O ponto (0..1 do canvas) cai na caixa (girada) do texto livre. */
export const dentroDoTexto = (item: LivreNoQuadro, canvas: Tela, p: Ponto): boolean => doTL.dentroDoTexto?.(item, canvas, p) ?? false;

/** Cantos da caixa do texto livre na tela (0..1), no, ne, se, so: o contorno da seleção. */
export const cantosDoTexto = (item: LivreNoQuadro, canvas: Tela): Ponto[] => doTL.cantosDoTexto?.(item, canvas) ?? [];
