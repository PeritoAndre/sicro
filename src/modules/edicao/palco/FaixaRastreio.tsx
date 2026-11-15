/**
 * Faixa do rastreio no transporte (§8.2): estado por quadro do rastreio do efeito aberto (ok, incerto, ausente,
 * interpolado, âncora, ausência aceita) ao longo da mídia, sobre a pista. Sem texto. Dono: P2.4.
 * Abertura: casca; montada no `Transporte` só com `RASTREIO_PRONTO`.
 */

import type { Seg } from "@domain/edicao";
import { RASTREIO_PRONTO } from "../store/rastreioStore";

export interface FaixaRastreioProps {
  /** Escala da pista (tempo de fonte) e largura em px. */
  de: Seg;
  ate: Seg;
  largura: number;
}

export function FaixaRastreio(p: FaixaRastreioProps) {
  if (!RASTREIO_PRONTO) return null;
  void p;
  return null;
}
