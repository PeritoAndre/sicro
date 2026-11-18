/** Tabelas de quadros que a tela já tem (as mesmas da Sequência e do palco: `palco/recursos`). Dono: P4.1. */

import type { ProjetoEdicao } from "@domain/edicao";
import type { ContextoTempo } from "../modelo/tempo";
import { acharMidia } from "../modelo/projeto";
import { recursoDe } from "../palco/recursos";

export function tabelasDaTela(doc: ProjetoEdicao): ContextoTempo {
  return { tabela: (id) => recursoDe(acharMidia(doc, id)?.sha256).tabela };
}
