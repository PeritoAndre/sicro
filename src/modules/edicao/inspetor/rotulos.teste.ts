/**
 * Só para testes e bancada: os rótulos do §8.6 que as telas do Inspetor e da exportação podem mostrar, e o que é
 * dado (números, tempos, unidades). Qualquer outro texto visível é frase fora da regra.
 */

export const ROTULOS = new Set<string>([
  // abas e seções
  "Clipe", "Áudio", "Legendas", "Saída", "Trecho", "Entrada", "Enquadramento", "Realce", "Marcações", "Título", "Quadro congelado", "Imagem",
  // Clipe · Trecho, Entrada, Enquadramento
  "Nome", "Vídeo", "Início", "Fim", "Evento", "Velocidade", "Manter o tom", "Som",
  // "Cor" da mídia (§8.6 Mídia: BT.601, BT.709; Limitada, Cheia), no Inspetor desde a abertura da Fase 1 (O4)
  "BT.601 · Limitada", "BT.601 · Cheia", "BT.709 · Limitada", "BT.709 · Cheia",
  "Transição", "Duração (s)", "Corte", "Escurecer",
  "Ajuste", "Inteiro", "Preencher", "Marcar início", "Marcar fim", "Girar", "Espelhar", "Interpolação", "Vizinho", "Bilinear", "Bicúbica",
  // Marcações
  "Adicionar", "Destaque", "Ocultar", "Holofote", "Lupa", "Régua", "Texto", "Tempo", "De", "Até", "Marcar", "Seguir", "Da análise",
  "Conteúdo", "Rosto", "Placa", "Corpo", "Tela", "Documento", "Outro", "Modo", "Sólido", "Mosaico", "Desfoque", "Forma",
  "Retângulo", "Elipse", "Polígono", "Silhueta", "Intensidade", "Borda suave", "Margem", "Inverter", "Motivo", "Só neste clipe",
  "Cor", "Espessura", "Contorno", "Rótulo", "Direção", "Posição", "Mostrar", "Tempo do original", "Relógio da câmera", "Quadro", "Tempo do vídeo",
  "Estabilizar", "Lente", "Desentrelaçar", "Média de quadros", "Brilho", "Nitidez",
  // Título
  "Modelo", "Cartela", "Abertura", "Terço inferior", "Encerramento", "Subtítulo", "Rodapé", "Fundo", "Fonte", "Tamanho", "Até o fim do clipe", "Imagem…",
  // Áudio
  "Volume do original", "Narração", "Música", "Efeitos", "Importar…", "Gravar", "Parar", "Preso a", "Entrada (s)", "Saída (s)",
  // Saída
  "Automática", "Largura", "Altura", "Quadros/s",
  // Exportar, Fila
  "Exportar", "Predefinição", "Júri", "WhatsApp", "Preservação", "Personalizada", "Arquivo reprodutível", "Ciente", "Cancelar",
  "Fila", "Na fila", "Preparando", "Camadas", "Juntando", "Conferindo", "A conferir", "Relatório", "Pronto", "Falhou", "Cancelado",
  "Abrir pasta", "Ver registro", "Conferido", "Concluir sem conferir", "Pacote do júri", "Excluir", "Fechar",
  // Fase 4 (P4.4): ícone da exportação que veio inteira do cache (só tooltip)
  "Reaproveitada",
  // Fase 1 (§8.6 e contratos-fase1 (f)), já aqui: o pacote que liga a seção não edita este arquivo
  "Acompanhar", "Cortar", "Fecho",
  "Suavizar", "Borda", "Preta", "Ampliar", "Refazer estabilização", "Campo", "Contraste", "Gama", "Saturação",
  "Ampliação", "Janela", "Fixa", "Acompanha", "Ligação", "Calibração", "Comprimento (m)", "Divisões", "Cálculo",
  "Abaixar sob a narração", "Normalizar", "Limitador", "Silenciar trecho", "Tom", "Mudo", "Da degravação…",
  "Nova legenda", "Conferida", "Linhas",
  "Selo de velocidade", "Selo de ampliação", "Selo de realce",
  "Não", "Na imagem", "Arquivo", "As duas", "Incluir originais", "PDF sem anexos", "Revisado por", "Testar de novo",
  "Conferir ocultações", "Refazer e comparar", "Igual", "Conteúdo igual, arquivo diferente", "Não comparável",
  // Fase 2 (§8.6 "Rastreio" e contratos-fase2 (f)), já aqui: o pacote que liga a casca não edita este arquivo
  "Seguir para trás", "Incerto", "Fora de cena", "Oculto", "Perdido", "Retomar", "Aceitar ausência", "Próximo trecho fraco",
  "Apagar daqui em diante", "Simplificar", "Atualizar", "Dessaturar", "Quadro anterior", "Próximo quadro",
  // Fase 4 (§8.6 "sugestões (F4)", P4.5)
  "Sugestões", "Trechos fracos", "Confirmar", "Descartar",
  // amostras e unidades
  "Aa", "×", "%", "°", "#",
]);

/** Rótulo do §8.6, "Exportar com N avisos", "Vídeo f/t", ou dado: número, tempo, quadro (#n), grau, porcentagem. */
export function textoPermitido(t: string): boolean {
  if (ROTULOS.has(t)) return true;
  if (/^Exportar com \d+ avisos?$/.test(t)) return true;
  if (/^Vídeo \d+\/\d+$/.test(t)) return true;
  if (/^Diferente a partir de \d{2}:\d{2}:\d{2},\d{3}$/.test(t)) return true;
  if (/^Perdido em (\d{2}:)?\d{2}:\d{2},\d{3}$/.test(t)) return true;
  if (/^\d+ marcaç(ão|ões)$/.test(t)) return true;
  if (/^#\d+$/.test(t)) return true;
  if (/^-?\d+(,\d+)?\s?(%|°|×|px)?$/.test(t)) return true;
  // andamento da detecção (P4.5): quadros feitos/total
  if (/^\d+\/\d+$/.test(t)) return true;
  if (/^\d{2}:\d{2}(:\d{2})?(,\d{3})?$/.test(t)) return true;
  return false;
}
