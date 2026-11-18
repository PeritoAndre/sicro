/**
 * Painel da linha do tempo (Fase 4, §8.2; maquete aprovada em 17/11): barra (Selecionar, Lâmina, Ímã, zoom), cabeças
 * das trilhas, régua com marcadores, trilhas e a cabeça de leitura. Edita o mesmo documento e a mesma seleção da
 * Sequência e do Inspetor (contratos-fase4.md D1). O editor monta o painel só com `LINHA_PRONTA` (regra 3).
 * Dono: P4.1 (casca da abertura).
 */

import type { PalcoApi } from "../palco/Palco";

/** O pacote liga ao entregar. */
export const LINHA_PRONTA = false;

export interface LinhaProps {
  /** O palco (tocar, pausar, ir para): a cabeça de leitura segue o cursor da store. */
  palco: () => PalcoApi | null;
  /** Atalhos da tela ligados (`atalhosLigados` do editor): `edicao.ima`, `edicao.selecionar`, `edicao.lamina` e o
   *  zoom da linha (`video.timelineZoom*`) só valem com ele. */
  atalhos: boolean;
}

export function LinhaDoTempo(props: LinhaProps) {
  void props;
  return null;
}
