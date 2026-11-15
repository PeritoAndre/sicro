/**
 * Padrão da ocultação por conteúdo (§6.6 (1)): placa, documento e tela → sólido; rosto, corpo e outro → mosaico com
 * intensidade 0,75 (4 células). Regra do P2.6 (contratos-fase2.md (c)); o "Conteúdo" do `OcultarEditor` pergunta aqui.
 * `novos.ts::novoOcultar` só passa a perguntar no fecho (muda `verif-fase0-do-store.json` e `projeto-do-store.json`, que o Rust lê).
 * Abertura: o padrão da Fase 1 (sólido opaco para todo conteúdo).
 */

import type { EfOcultar } from "@domain/edicao";

export function padraoDoConteudo(conteudo: EfOcultar["conteudo"]): Pick<EfOcultar, "modo" | "intensidade"> {
  void conteudo;
  return { modo: "solido", intensidade: 1 };
}
