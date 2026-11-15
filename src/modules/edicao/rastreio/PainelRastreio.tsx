/**
 * Rastreio do efeito aberto no Inspetor (§8.6 "Rastreio"): "Seguir", "Seguir para trás", "Parar", "Retomar",
 * "Aceitar ausência" (Oculto, Fora de cena, Perdido), "Próximo trecho fraco", "Apagar daqui em diante",
 * "Simplificar" e "Da análise" (trilhas de Pessoas da mídia). Estado por ícone; "Perdido em 00:12,345" quando parou.
 * Dono: P2.4. Abertura: casca (`RASTREIO_PRONTO = false`); montada em `MarcacoesSecao` só com a bandeira.
 */

import type { ClipePrincipal, ClipeSobreposto, ContextoClipe, Efeito, Id, TabelaQuadros } from "@domain/edicao";
import { RASTREIO_PRONTO } from "../store/rastreioStore";

export interface PainelRastreioProps {
  efeito: Efeito;
  rastreio: Id;
  clipe: ClipePrincipal | ClipeSobreposto;
  ctx: ContextoClipe;
  /** Marcação parada (intermediário de origem em preparo): "Seguir" e "Retomar" desligados. */
  parada: boolean;
  /** Tabela da mídia, só quando confiável (a mesma do "De" e "Até"): pts do denso ↔ tempo de fonte. */
  tabela: TabelaQuadros | null;
}

export function PainelRastreio(p: PainelRastreioProps) {
  if (!RASTREIO_PRONTO) return null;
  void p;
  return null;
}
