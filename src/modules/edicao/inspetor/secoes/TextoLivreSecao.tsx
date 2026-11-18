/**
 * Seção do texto livre no Inspetor (contratos-fase4.md (i)): "Texto" · "Fonte" · "Tamanho" · "Cor" · "Fundo" (Nenhum,
 * Faixa, Caixa) · "Contorno" · "Sombra" · "Entrada" / "Saída" (Corte, Aparecer, Deslizar) · "Duração (s)" · "Lado" ·
 * "Posição" · "Giro" · "Largura" · "De" · "Até" · "Começar no cursor" · "Terminar no cursor". Montada pelo registro
 * (`livre:texto`) só com `TEXTO_LIVRE_PRONTO`. Dono: pacote do texto livre (casca da abertura).
 */

import type { SobTitulo } from "@domain/edicao";
import type { SecaoProps } from "../registro";

export function TextoLivreSecao({ clipe }: SecaoProps<SobTitulo>) {
  void clipe;
  return null;
}
