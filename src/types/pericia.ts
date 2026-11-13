/** Listas padrão usadas na criação de ocorrência e no perfil do perito. */

import municipiosBr from "./municipios-br.json";

/** Municípios por UF (IBGE), em ordem alfabética. */
export const MUNICIPIOS_POR_UF: Readonly<Record<string, readonly string[]>> = municipiosBr;

export const UFS: readonly string[] = Object.keys(municipiosBr);

/** Municípios do Amapá (diálogo de caso). */
export const MUNICIPIOS_AP: readonly string[] = MUNICIPIOS_POR_UF.AP ?? [];

/** Sugestões do combobox; aceita digitar outro. */
export const TIPOS_PERICIA: readonly string[] = [
  "Sinistro de Trânsito",
  "Identificação Veicular",
  "Local de Morte Violenta",
  "Local de Crime contra o Patrimônio",
  "Documentoscopia",
  "Constatação/Exame de Substância",
  "Balística",
  "Informática / Mídias (áudio·vídeo·imagem)",
  "Engenharia Legal / Local de Incêndio",
  "Meio Ambiente",
  "Avaliação / Merceologia",
];
