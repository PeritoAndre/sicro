/**
 * Listas de domínio reutilizadas na criação de ocorrência e no perfil do perito:
 * municípios do Amapá e tipos de perícia. São DEFAULTS editáveis — o tipo de
 * perícia aceita valor livre (combobox), e o município é escolhido entre os 16.
 */

/** 16 municípios do Amapá — Macapá e Santana primeiro; o resto em ordem alfabética. */
export const MUNICIPIOS_AP: readonly string[] = [
  "Macapá",
  "Santana",
  "Amapá",
  "Calçoene",
  "Cutias",
  "Ferreira Gomes",
  "Itaubal",
  "Laranjal do Jari",
  "Mazagão",
  "Oiapoque",
  "Pedra Branca do Amapari",
  "Porto Grande",
  "Pracuúba",
  "Serra do Navio",
  "Tartarugalzinho",
  "Vitória do Jari",
];

/** Principais tipos de perícia (sugestões do dropdown — aceita digitar outro). */
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
