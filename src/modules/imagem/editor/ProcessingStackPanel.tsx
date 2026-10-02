/**
 * Catálogo de filtros forenses (defs, palavras-chave, notas), galeria buscável
 * e barra de camadas da pipeline não-destrutiva; conversão de `ProcessingOp`
 * para o formato do backend.
 */

import { useMemo, useState, type CSSProperties } from "react";
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  Eye,
  EyeOff,
  Plus,
  Search,
  SlidersHorizontal,
  Trash2,
  X as XIcon,
} from "lucide-react";
import type {
  ProcessingOp,
  ProcessingOpKind,
  SicroImageSelection,
} from "../engine/schema";
import styles from "./ProcessingStackPanel.module.css";

interface FilterControl {
  key: string;
  label: string;
  type: "number" | "range" | "select" | "checkbox";
  min?: number;
  max?: number;
  step?: number;
  /** Para type "select". */
  options?: Array<{ value: string; label: string }>;
}

interface FilterDef {
  kind: ProcessingOpKind;
  label: string;
  defaults: Record<string, unknown>;
  controls: FilterControl[];
  /** Sobrepõe a nota de `FILTER_NOTES` quando presente. */
  note?: string;
}

const FILTER_CATALOG: Record<string, FilterDef[]> = {
  Bordas: [
    {
      kind: "edge_sobel",
      label: "Sobel",
      defaults: { strength: 1.0 },
      controls: [
        { key: "strength", label: "Intensidade", type: "range", min: 0, max: 4, step: 0.1 },
      ],
    },
    {
      kind: "edge_laplacian",
      label: "Laplaciano",
      defaults: { strength: 1.0 },
      controls: [
        { key: "strength", label: "Intensidade", type: "range", min: 0, max: 4, step: 0.1 },
      ],
    },
    {
      kind: "edge_canny",
      label: "Canny",
      defaults: { low_threshold: 50, high_threshold: 150 },
      controls: [
        { key: "low_threshold", label: "Limiar inferior", type: "range", min: 0, max: 254, step: 1 },
        { key: "high_threshold", label: "Limiar superior", type: "range", min: 1, max: 255, step: 1 },
      ],
    },
  ],
  Suavização: [
    {
      kind: "blur_gaussian",
      label: "Gaussian",
      defaults: { sigma: 1.5 },
      controls: [{ key: "sigma", label: "Sigma", type: "range", min: 0.1, max: 8, step: 0.1 }],
    },
    {
      kind: "blur_median",
      label: "Median",
      defaults: { radius: 1 },
      controls: [{ key: "radius", label: "Raio", type: "range", min: 1, max: 5, step: 1 }],
    },
    {
      kind: "blur_bilateral",
      label: "Bilateral",
      defaults: { sigma_space: 2.0, sigma_color: 25.0 },
      controls: [
        { key: "sigma_space", label: "σ espacial", type: "range", min: 0.5, max: 6, step: 0.1 },
        { key: "sigma_color", label: "σ cor", type: "range", min: 5, max: 80, step: 1 },
      ],
    },
    {
      kind: "unsharp_mask",
      label: "Unsharp mask",
      defaults: { sigma: 1.5, amount: 1.0 },
      controls: [
        { key: "sigma", label: "Sigma", type: "range", min: 0.5, max: 5, step: 0.1 },
        { key: "amount", label: "Quantidade", type: "range", min: 0, max: 4, step: 0.1 },
      ],
    },
  ],
  Realce: [
    {
      kind: "clahe",
      label: "CLAHE",
      defaults: { tile_size: 8, clip_limit: 2.0, bins: 256 },
      controls: [
        // Tile = "block size" do Fiji; max 256 alcança o block grande do Fiji.
        { key: "tile_size", label: "Tile (block size)", type: "range", min: 4, max: 256, step: 4 },
        // Clip limit = "maximum slope" do Fiji (1 = imagem original).
        { key: "clip_limit", label: "Clip limit (max slope)", type: "range", min: 1.0, max: 8.0, step: 0.1 },
        // bins = "histogram bins" do Fiji (≤256 em 8 bits).
        { key: "bins", label: "Histogram bins", type: "range", min: 16, max: 256, step: 16 },
      ],
    },
    {
      kind: "subtract_background",
      label: "Subtrair fundo (rolling ball)",
      defaults: { radius: 50, light_background: false, disable_smoothing: false },
      controls: [
        { key: "radius", label: "Raio da bola (px)", type: "range", min: 1, max: 300, step: 1 },
        { key: "light_background", label: "Fundo claro", type: "checkbox" },
        { key: "disable_smoothing", label: "Sem suavização", type: "checkbox" },
      ],
    },
    {
      kind: "histogram_equalize",
      label: "Equalização global",
      defaults: {},
      controls: [],
    },
    {
      kind: "auto_levels",
      label: "Auto-levels",
      defaults: { percentile_low: 1, percentile_high: 99 },
      controls: [
        { key: "percentile_low", label: "Percentil baixo", type: "range", min: 0, max: 49, step: 0.5 },
        { key: "percentile_high", label: "Percentil alto", type: "range", min: 51, max: 100, step: 0.5 },
      ],
    },
    {
      kind: "white_balance_gray_world",
      label: "White balance (gray-world)",
      defaults: {},
      controls: [],
    },
    {
      kind: "threshold",
      label: "Threshold",
      defaults: { value: 128 },
      controls: [{ key: "value", label: "Limiar", type: "range", min: 0, max: 255, step: 1 }],
    },
  ],
  Morfologia: [
    {
      kind: "dilate",
      label: "Dilatar",
      defaults: { radius: 1 },
      controls: [{ key: "radius", label: "Raio", type: "range", min: 1, max: 5, step: 1 }],
    },
    {
      kind: "erode",
      label: "Erodir",
      defaults: { radius: 1 },
      controls: [{ key: "radius", label: "Raio", type: "range", min: 1, max: 5, step: 1 }],
    },
    {
      kind: "open",
      label: "Abertura",
      defaults: { radius: 1 },
      controls: [{ key: "radius", label: "Raio", type: "range", min: 1, max: 5, step: 1 }],
    },
    {
      kind: "close",
      label: "Fechamento",
      defaults: { radius: 1 },
      controls: [{ key: "radius", label: "Raio", type: "range", min: 1, max: 5, step: 1 }],
    },
  ],
  Geometria: [
    {
      kind: "rotate_90_cw",
      label: "Girar 90° horário",
      defaults: {},
      controls: [],
    },
    {
      kind: "rotate_90_ccw",
      label: "Girar 90° anti-horário",
      defaults: {},
      controls: [],
    },
    {
      kind: "rotate_180",
      label: "Girar 180°",
      defaults: {},
      controls: [],
    },
    {
      kind: "flip_horizontal",
      label: "Espelhar horizontal",
      defaults: {},
      controls: [],
    },
    {
      kind: "flip_vertical",
      label: "Espelhar vertical",
      defaults: {},
      controls: [],
    },
    {
      kind: "rotate_arbitrary",
      label: "Girar ângulo livre",
      defaults: { degrees: 0, expand: true },
      controls: [
        { key: "degrees", label: "Ângulo (°)", type: "range", min: -180, max: 180, step: 0.5 },
        { key: "expand", label: "Expandir tela", type: "checkbox" },
      ],
    },
  ],
  // ---- Tonais, canais, forense, genérico ----
  Tonal: [
    {
      kind: "levels",
      label: "Níveis",
      defaults: {
        channel: "rgb",
        in_black: 0,
        in_white: 255,
        gamma: 1.0,
        out_black: 0,
        out_white: 255,
      },
      controls: [
        {
          key: "channel",
          label: "Canal",
          type: "select",
          options: [
            { value: "rgb", label: "RGB" },
            { value: "r", label: "R" },
            { value: "g", label: "G" },
            { value: "b", label: "B" },
          ],
        },
        { key: "in_black", label: "Entrada preto", type: "range", min: 0, max: 254, step: 1 },
        { key: "in_white", label: "Entrada branco", type: "range", min: 1, max: 255, step: 1 },
        { key: "gamma", label: "Gama", type: "range", min: 0.1, max: 5, step: 0.05 },
        { key: "out_black", label: "Saída preto", type: "range", min: 0, max: 255, step: 1 },
        { key: "out_white", label: "Saída branco", type: "range", min: 0, max: 255, step: 1 },
      ],
    },
    {
      kind: "curves",
      label: "Curvas (S de contraste)",
      // Preset de contraste em S; ainda sem editor de curva interativo.
      defaults: {
        channel: "rgb",
        points: [
          [0, 0],
          [64, 44],
          [192, 212],
          [255, 255],
        ],
      },
      controls: [
        {
          key: "channel",
          label: "Canal",
          type: "select",
          options: [
            { value: "rgb", label: "RGB" },
            { value: "r", label: "R" },
            { value: "g", label: "G" },
            { value: "b", label: "B" },
          ],
        },
      ],    },
    {
      kind: "posterize",
      label: "Posterizar",
      defaults: { levels: 4 },
      controls: [{ key: "levels", label: "Níveis", type: "range", min: 2, max: 32, step: 1 }],
    },
  ],
  Canais: [
    {
      kind: "extract_channel",
      label: "Extrair canal",
      defaults: { channel: "luma" },
      controls: [
        {
          key: "channel",
          label: "Canal",
          type: "select",
          options: [
            { value: "luma", label: "Luma (601)" },
            { value: "luminance", label: "Luminância (709)" },
            { value: "r", label: "R" },
            { value: "g", label: "G" },
            { value: "b", label: "B" },
            { value: "h", label: "Matiz (H)" },
            { value: "s", label: "Saturação (S)" },
            { value: "v", label: "Valor (V)" },
            { value: "y", label: "Y (YCbCr)" },
            { value: "cb", label: "Cb" },
            { value: "cr", label: "Cr" },
            { value: "l_lab", label: "L* (Lab)" },
            { value: "a_lab", label: "a* (Lab)" },
            { value: "b_lab", label: "b* (Lab)" },
          ],
        },
      ],    },
    {
      kind: "false_color",
      label: "Falsa-cor",
      defaults: { colormap: "viridis" },
      controls: [
        {
          key: "colormap",
          label: "Mapa",
          type: "select",
          options: [
            { value: "viridis", label: "Viridis (perceptual)" },
            { value: "jet", label: "Jet" },
            { value: "ironbow", label: "Ironbow (térmico)" },
            { value: "grayscale", label: "Tons de cinza" },
          ],
        },
      ],
    },
  ],
  Forense: [
    {
      kind: "ela",
      label: "ELA (Error Level Analysis)",
      defaults: { quality: 90, scale: 15 },
      controls: [
        { key: "quality", label: "Qualidade JPEG", type: "range", min: 50, max: 100, step: 1 },
        { key: "scale", label: "Amplificação", type: "range", min: 1, max: 40, step: 1 },
      ],    },
    {
      kind: "decorrelation_stretch",
      label: "Decorrelation stretch",
      defaults: { target_sigma: 50, target_mean: 128 },
      controls: [
        { key: "target_sigma", label: "Intensidade (σ)", type: "range", min: 10, max: 100, step: 1 },
        { key: "target_mean", label: "Centro", type: "range", min: 64, max: 192, step: 1 },
      ],    },
    {
      kind: "luminance_gradient",
      label: "Gradiente de luminância",
      defaults: { strength: 1.5 },
      controls: [
        { key: "strength", label: "Intensidade", type: "range", min: 0.2, max: 6, step: 0.1 },
      ],    },
    {
      kind: "difference_of_gaussians",
      label: "Difference of Gaussians",
      defaults: { sigma1: 1.0, sigma2: 3.0, gain: 5.0 },
      controls: [
        { key: "sigma1", label: "σ fino", type: "range", min: 0.3, max: 10, step: 0.1 },
        { key: "sigma2", label: "σ grosso", type: "range", min: 0.5, max: 20, step: 0.1 },
        { key: "gain", label: "Ganho", type: "range", min: 1, max: 15, step: 0.5 },
      ],
    },
  ],
  Genérico: [
    {
      kind: "convolve",
      label: "Convolução — nitidez (3×3)",
      // Kernel fixo de nitidez; ainda sem editor de matriz.
      defaults: {
        kernel: [0, -1, 0, -1, 5, -1, 0, -1, 0],
        size: 3,
        divisor: 1,
        offset: 0,
      },
      controls: [
        { key: "divisor", label: "Divisor", type: "number", min: 0, max: 64, step: 1 },
        { key: "offset", label: "Offset", type: "number", min: -255, max: 255, step: 1 },
      ],    },
  ],
};

/**
 * Palavras-chave de intenção por filtro, para a busca da galeria: o perito
 * pesquisa pelo que quer fazer ("falsificação", "ruído"), não pelo nome técnico.
 */
const FILTER_KEYWORDS: Partial<Record<ProcessingOpKind, string>> = {
  edge_sobel: "borda contorno gradiente arestas",
  edge_laplacian: "borda contorno realce arestas",
  edge_canny: "borda contorno deteccao arestas",
  blur_gaussian: "suavizar desfoque ruido",
  blur_median: "ruido sal pimenta suavizar mediana",
  blur_bilateral: "ruido preserva borda suavizar",
  unsharp_mask: "nitidez agucar foco realcar definicao",
  clahe: "contraste local realce sombra detalhe equalizacao adaptativa fiji blocksize bins slope",
  subtract_background:
    "fundo iluminacao uniformizar rolling ball placa documento sombra gradiente fiji imagej remover",
  histogram_equalize: "contraste equalizacao brilho",
  auto_levels: "contraste automatico niveis brilho",
  white_balance_gray_world: "cor temperatura dominante balanco branco",
  threshold: "binarizar preto branco limiar segmentar",
  dilate: "morfologia expandir engrossar",
  erode: "morfologia encolher afinar",
  open: "morfologia ruido remover abertura",
  close: "morfologia buraco preencher fechamento",
  rotate_90_cw: "girar rotacionar orientacao",
  rotate_90_ccw: "girar rotacionar orientacao",
  rotate_180: "girar rotacionar inverter orientacao",
  flip_horizontal: "espelhar inverter",
  flip_vertical: "espelhar inverter",
  rotate_arbitrary: "girar rotacionar angulo inclinar endireitar nivelar",
  levels: "tonal contraste brilho gama niveis histograma",
  curves: "tonal contraste curva tom",
  posterize: "tonal reduzir cores bandas",
  extract_channel: "canal separar luma luminancia lab hsv ycbcr cinza",
  false_color: "mapa termico colorir realce paleta",
  ela: "adulteracao falsificacao montagem jpeg fraude error level forense",
  decorrelation_stretch:
    "dstretch tinta apagada latente cor sutil grafite forense realce",
  luminance_gradient: "iluminacao colagem montagem sombra relevo forense",
  difference_of_gaussians: "borda detalhe realce textura dog",
  convolve: "kernel matriz nitidez personalizado",
};

/**
 * Explicação de cada filtro voltada à aplicação prática na perícia, em
 * linguagem leiga. Exibida no card do catálogo e no popover da pilha.
 */
const FILTER_NOTES: Partial<Record<ProcessingOpKind, string>> = {
  // Bordas
  edge_sobel:
    "Realça os contornos (onde a imagem muda de cor/brilho). Na perícia, ajuda a destacar o limite de objetos, arranhões, letras de placa ou bordas de uma peça — para medir e comparar formas.",
  edge_laplacian:
    "Detector de bordas sensível a detalhes finos. Na perícia, ajuda a revelar microdetalhe e relevo (marcas de ferramenta, gravações rasas, textura) que passam despercebidos a olho nu.",
  edge_canny:
    "Extrai contornos finos e contínuos, separando o objeto do fundo. Na perícia, ajuda a isolar o formato de algo (arma, placa, mancha) para medir, contar ou comparar com um padrão. Ajuste os limiares conforme o ruído.",
  // Suavização
  blur_gaussian:
    "Suaviza a imagem reduzindo ruído/granulação. Na perícia, serve de preparo: limpa a foto antes de medir ou detectar bordas, para o 'chuvisco' não atrapalhar a análise.",
  blur_median:
    "Remove pontos isolados de ruído ('sal e pimenta') preservando as bordas. Na perícia, ajuda a limpar fotos ruidosas (pouca luz, ISO alto) sem borrar o conteúdo que importa.",
  blur_bilateral:
    "Suaviza áreas planas mas mantém as bordas nítidas. Na perícia, ajuda a limpar a imagem antes de ler um texto/placa, sem 'derreter' os caracteres.",
  unsharp_mask:
    "Aumenta a nitidez do que está levemente desfocado. Na perícia, ajuda a tornar legível um texto, número de série ou detalhe fora de foco. Em excesso cria halos artificiais — não exagerar.",
  // Realce
  clahe:
    "Realça o contraste local, revelando detalhe em sombras e em áreas estouradas de luz. Na perícia, ajuda a 'enxergar dentro' de regiões muito escuras ou muito claras (um vão escuro, um documento com reflexo). Parâmetros equivalentes ao CLAHE do Fiji: Tile = block size, Clip limit = max slope, Histogram bins = nº de níveis.",
  subtract_background:
    "Remove fundo desigual 'rolando uma bola' sob a imagem (algoritmo Subtract Background do ImageJ/Fiji). Na perícia, uniformiza iluminação irregular e realça o que está sobre o fundo — clássico para destacar caracteres de placa/documento antes de ler. Marque 'Fundo claro' quando o objeto é escuro sobre fundo claro (placa, papel). O raio deve ser maior que as feições a preservar.",
  histogram_equalize:
    "Espalha os tons para maximizar o contraste geral. Na perícia, recupera imagens 'lavadas'/sem contraste; pode realçar ruído, então use com critério.",
  auto_levels:
    "Estica automaticamente o contraste entre os tons mais escuros e mais claros. Na perícia, recupera rápido fotos esmaecidas, com neblina ou subexpostas.",
  white_balance_gray_world:
    "Corrige dominância de cor (foto amarelada/azulada pela luz do ambiente). Na perícia, recupera a cor real do objeto — importante quando a cor é o que se analisa (tinta, hematoma, fiação).",
  threshold:
    "Transforma a imagem em preto-e-branco por um limiar. Na perícia, ajuda a isolar e medir formas/manchas/texto — por exemplo, separar uma mancha do piso para estimar a área.",
  // Morfologia
  dilate:
    "Engrossa/expande as regiões claras e conecta traços partidos. Na perícia, ajuda a reconstituir um traço ou contorno fragmentado (uma letra apagada, uma linha interrompida).",
  erode:
    "Afina/encolhe regiões claras e remove respingos pequenos. Na perícia, ajuda a separar objetos colados e a limpar ruído antes de contar ou medir.",
  open:
    "Remove pontinhos de ruído sem mudar o tamanho das formas maiores. Na perícia, limpa a imagem preservando os vestígios relevantes.",
  close:
    "Preenche buracos e une falhas finas. Na perícia, ajuda a fechar lacunas de um contorno ou traço para medir/contar com fidelidade.",
  // Geometria
  rotate_90_cw:
    "Gira a imagem 90° no sentido horário. Na perícia, só reorienta a foto para análise/leitura — não altera o conteúdo (sem perda).",
  rotate_90_ccw:
    "Gira a imagem 90° no sentido anti-horário. Na perícia, só reorienta a foto para análise/leitura — não altera o conteúdo (sem perda).",
  rotate_180:
    "Gira a imagem 180° (ex.: foto de cabeça para baixo). Na perícia, só reorienta — não altera o conteúdo (sem perda).",
  flip_horizontal:
    "Espelha a imagem na horizontal (esquerda↔direita). Na perícia, ajuda a comparar com um reflexo (espelho, vidro, imagem invertida de câmera). Sem perda de dados.",
  flip_vertical:
    "Espelha a imagem na vertical (cima↔baixo). Na perícia, reorienta ou compara reflexos. Sem perda de dados.",
  rotate_arbitrary:
    "Gira por um ângulo livre para nivelar o horizonte ou um objeto. Na perícia, ajuda a 'endireitar' uma placa/documento fotografado torto antes de medir ou ler. 'Expandir tela' evita cortar os cantos.",
  // Tonal
  levels:
    "Remapeia preto, branco e meio-tom (gama), por canal. Na perícia, é o ajuste fino de contraste — realça o que interessa sem 'queimar' o resto, de forma controlada e reversível.",
  curves:
    "Ajuste tonal por curva (aqui, um preset de contraste em S). Na perícia, realça diferenças sutis de brilho/cor com controle — útil para destacar uma marca apagada do fundo. (Editor de curva interativo virá depois.)",
  posterize:
    "Reduz a imagem a poucos níveis de cor, agrupando tons em faixas. Na perícia, evidencia degradês e regiões 'pintadas'/retocadas, que viram blocos chapados e se destacam.",
  // Canais
  extract_channel:
    "Mostra um único canal (R/G/B, luminância e, sobretudo, L*a*b*). Na perícia, os canais a*/b* (Lab) revelam recoloração e tinta sutil — ex.: uma escrita apagada ou um retoque que o RGB esconde.",
  false_color:
    "Pinta a luminância com uma paleta de cores. Na perícia, o olho distingue muito mais tons em cores do que em cinza — ajuda a ver diferenças mínimas de brilho (relevo, manchas latentes).",
  // Forense
  ela:
    "Error Level Analysis: destaca regiões recomprimidas de um JPEG. Na perícia, levanta suspeita de montagem/edição (uma parte colada 'brilha' diferente). Atenção: re-salvamentos achatam o efeito — é indício, não prova.",
  decorrelation_stretch:
    "DStretch: amplifica diferenças de cor sutis. Na perícia, é clássico para revelar tinta/grafite apagado, marcas latentes e pinturas desbotadas que somem a olho nu.",
  luminance_gradient:
    "Mostra a direção da iluminação por cores. Na perícia, ajuda a flagrar colagens (sombras vindas de lados diferentes) e áreas 'pintadas'/clonadas (onde o relevo fica artificialmente liso).",
  difference_of_gaussians:
    "Isola bordas e textura numa faixa de detalhe (subtrai dois desfoques). Na perícia, aproxima o realce de microvestígios e texturas finas — ex.: trama de tecido, marcas superficiais.",
  // Genérico
  convolve:
    "Aplica um filtro de matriz (aqui, nitidez 3×3). Na perícia, é uma ferramenta genérica para realçar ou atenuar detalhes conforme a necessidade.",
};

/** Normaliza para busca: minúsculas + sem acentos. */
function normalizeSearch(s: string): string {
  return s
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();
}

function findDef(kind: ProcessingOpKind): FilterDef | null {
  for (const group of Object.values(FILTER_CATALOG)) {
    const found = group.find((d) => d.kind === kind);
    if (found) return found;
  }
  return null;
}

/** Catálogo achatado (uma entrada por filtro): fonte única para a galeria e a paleta de comandos. */
export const FILTER_INDEX: Array<{
  kind: ProcessingOpKind;
  label: string;
  group: string;
  note?: string;
  keywords?: string;
}> = Object.entries(FILTER_CATALOG).flatMap(([group, items]) =>
  items.map((d) => ({
    kind: d.kind,
    label: d.label,
    group,
    note: d.note ?? FILTER_NOTES[d.kind],
    keywords: FILTER_KEYWORDS[d.kind],
  })),
);

/** Cria um `ProcessingOp` novo (id + defaults do catálogo). */
export function makeProcessingOp(kind: ProcessingOpKind): ProcessingOp {
  const def = findDef(kind);
  return {
    id: `op-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
    kind,
    enabled: true,
    params: def ? { ...def.defaults } : {},
    created_at: new Date().toISOString(),
  };
}

function labelFor(kind: ProcessingOpKind): string {
  const def = findDef(kind);
  if (def) return def.label;
  return kind;
}

/** Galeria buscável do catálogo: só descobre e adiciona filtros (`onAdd`); a pilha ativa fica na `LayersBar`. */
export function FilterGallery({
  onAdd,
}: {
  onAdd: (kind: ProcessingOpKind) => void;
}) {
  const [query, setQuery] = useState("");

  // Busca acento-insensível em label + grupo + nota + palavras-chave + kind.
  const filteredGroups = useMemo(() => {
    const q = normalizeSearch(query.trim());
    return Object.entries(FILTER_CATALOG)
      .map(([group, items]) => {
        if (!q) return [group, items] as const;
        const gnorm = normalizeSearch(group);
        const matched = items.filter((d) =>
          normalizeSearch(
            `${d.label} ${gnorm} ${d.note ?? ""} ${FILTER_KEYWORDS[d.kind] ?? ""} ${d.kind}`,
          ).includes(q),
        );
        return [group, matched] as const;
      })
      .filter(([, items]) => items.length > 0);
  }, [query]);
  const resultCount = filteredGroups.reduce(
    (n, [, items]) => n + items.length,
    0,
  );

  return (
    <div className={styles.galleryWrap}>
      <div className={styles.galleryHead}>
        <strong>Catálogo de filtros forenses</strong>
        <span className={styles.galleryCount}>{FILTER_INDEX.length}</span>
      </div>
      <div className={styles.search}>
        <Search size={12} />
        <input
          type="text"
          placeholder="Buscar filtro… (ruído, borda, falsificação…)"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape" && query) setQuery("");
          }}
        />
        {query && (
          <button
            type="button"
            className={styles.searchClear}
            onClick={() => setQuery("")}
            title="Limpar busca"
          >
            <XIcon size={12} />
          </button>
        )}
      </div>

      <div className={styles.galleryScroll}>
        {resultCount === 0 ? (
          <p className={styles.galleryEmpty}>
            Nenhum filtro encontrado para “{query}”.
          </p>
        ) : (
          filteredGroups.map(([group, items]) => (
            <div key={group} className={styles.pickerGroup}>
              <h4>{group}</h4>
              <div className={styles.gallery}>
                {items.map((d) => {
                  const note = d.note ?? FILTER_NOTES[d.kind];
                  return (
                    <button
                      key={d.kind}
                      type="button"
                      className={styles.card}
                      onClick={() => onAdd(d.kind)}
                      title={note ?? d.label}
                    >
                      <span className={styles.cardLabel}>
                        <Plus size={10} /> {d.label}
                      </span>
                      {note && <span className={styles.cardNote}>{note}</span>}
                    </button>
                  );
                })}
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

/** Editor de parâmetros de uma operação (select / checkbox / range / number). */
function OpControls({
  op,
  onParam,
}: {
  op: ProcessingOp;
  onParam: (key: string, value: unknown) => void;
}) {
  const def = findDef(op.kind);
  if (!def || def.controls.length === 0) return null;
  return (
    <div className={styles.controls}>
      {def.controls.map((ctrl) => {
        if (ctrl.type === "select") {
          const val =
            (op.params[ctrl.key] as string | undefined) ??
            (def.defaults[ctrl.key] as string | undefined) ??
            "";
          return (
            <label key={ctrl.key} className={styles.control}>
              <span>{ctrl.label}</span>
              <select
                value={val}
                onChange={(e) => onParam(ctrl.key, e.target.value)}
              >
                {ctrl.options?.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
            </label>
          );
        }
        if (ctrl.type === "checkbox") {
          const val =
            (op.params[ctrl.key] as boolean | undefined) ??
            (def.defaults[ctrl.key] as boolean | undefined) ??
            false;
          return (
            <label
              key={ctrl.key}
              className={`${styles.control} ${styles.controlCheck ?? ""}`}
            >
              <span>{ctrl.label}</span>
              <input
                type="checkbox"
                checked={val}
                onChange={(e) => onParam(ctrl.key, e.target.checked)}
              />
            </label>
          );
        }
        const val =
          (op.params[ctrl.key] as number | undefined) ??
          (def.defaults[ctrl.key] as number | undefined) ??
          0;
        return (
          <label key={ctrl.key} className={styles.control}>
            <span>{ctrl.label}</span>
            <input
              type={ctrl.type}
              min={ctrl.min}
              max={ctrl.max}
              step={ctrl.step ?? 1}
              value={val}
              onChange={(e) => onParam(ctrl.key, parseFloat(e.target.value))}
            />
            <output>{Number(val).toFixed(2)}</output>
          </label>
        );
      })}
    </div>
  );
}

/** Estilo do par de botões de escopo (imagem × seleção). */
function scopeBtnStyle(active: boolean): CSSProperties {
  return {
    flex: 1,
    padding: "4px 6px",
    fontSize: 10.5,
    fontFamily: "inherit",
    cursor: "pointer",
    borderRadius: 4,
    border: active
      ? "1px solid rgba(34,211,238,0.6)"
      : "1px solid rgba(148,163,184,0.3)",
    background: active ? "rgba(34,211,238,0.18)" : "rgba(30,41,59,0.6)",
    color: active ? "#22d3ee" : "rgba(203,213,225,0.85)",
    fontWeight: active ? 600 : 400,
  };
}

/**
 * Barra horizontal de camadas da pipeline, embaixo do canvas (esquerda = base,
 * direita = topo = ordem de aplicação). Clicar numa camada abre o popover com
 * parâmetros, escopo, ordem e remoção.
 */
export function LayersBar({
  stack,
  onChange,
  collapsed = false,
  onToggleCollapsed,
  activeSelection = null,
}: {
  stack: ProcessingOp[];
  onChange: (stack: ProcessingOp[]) => void;
  collapsed?: boolean;
  onToggleCollapsed?: () => void;
  /** Seleção ativa no editor, para confinar um filtro a ela. */
  activeSelection?: SicroImageSelection | null;
}) {
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const updateOp = (id: string, patch: Partial<ProcessingOp>) => {
    onChange(stack.map((o) => (o.id === id ? { ...o, ...patch } : o)));
  };
  const updateParam = (id: string, key: string, value: unknown) => {
    onChange(
      stack.map((o) =>
        o.id === id ? { ...o, params: { ...o.params, [key]: value } } : o,
      ),
    );
  };
  const removeOp = (id: string) => {
    onChange(stack.filter((o) => o.id !== id));
    if (selectedId === id) setSelectedId(null);
  };
  const move = (id: string, delta: number) => {
    const i = stack.findIndex((o) => o.id === id);
    if (i < 0) return;
    const j = i + delta;
    if (j < 0 || j >= stack.length) return;
    const next = [...stack];
    const a = next[i];
    const b = next[j];
    if (!a || !b) return;
    next[i] = b;
    next[j] = a;
    onChange(next);
  };

  const selected = stack.find((o) => o.id === selectedId) ?? null;
  const selIndex = selected
    ? stack.findIndex((o) => o.id === selected.id)
    : -1;
  const selNote = selected
    ? (findDef(selected.kind)?.note ?? FILTER_NOTES[selected.kind])
    : null;

  return (
    <div className={styles.layersBar}>
      <header className={styles.layersBarHead}>
        <button
          type="button"
          className={styles.layersBarToggle}
          onClick={onToggleCollapsed}
          aria-expanded={!collapsed}
          title={collapsed ? "Expandir pipeline" : "Recolher pipeline"}
        >
          {collapsed ? <ChevronUp size={13} /> : <ChevronDown size={13} />}
          <SlidersHorizontal size={13} />
          <strong>Pipeline</strong>
          <span className={styles.dockCount}>{stack.length}</span>
        </button>
        {!collapsed && stack.length > 1 && (
          <span className={styles.layersHint}>ordem de aplicação →</span>
        )}
        {!collapsed && stack.length > 0 && (
          <button
            type="button"
            className={styles.dockClear}
            onClick={() => {
              onChange([]);
              setSelectedId(null);
            }}
            title="Remover todos os filtros da pipeline"
          >
            Limpar
          </button>
        )}
      </header>

      {!collapsed && (
        <div className={styles.layersTrack}>
          {stack.length === 0 ? (
            <span className={styles.layersEmpty}>
              Pipeline vazia. Adicione filtros pela aba <strong>Filtros</strong>{" "}
              (à direita) ou recorte com a ferramenta de corte — cada etapa
              aparece aqui, na ordem de aplicação.
            </span>
          ) : (
            stack.map((op, i) => (
              <div
                key={op.id}
                className={`${styles.layerCard} ${
                  selectedId === op.id ? styles.layerCardActive : ""
                } ${op.enabled === false ? styles.layerCardOff : ""}`}
              >
                <button
                  type="button"
                  className={styles.layerEye}
                  onClick={() =>
                    updateOp(op.id, { enabled: op.enabled === false })
                  }
                  title={op.enabled === false ? "Mostrar etapa" : "Ocultar etapa"}
                >
                  {op.enabled === false ? (
                    <EyeOff size={12} />
                  ) : (
                    <Eye size={12} />
                  )}
                </button>
                <button
                  type="button"
                  className={styles.layerCardBody}
                  onClick={() =>
                    setSelectedId((cur) => (cur === op.id ? null : op.id))
                  }
                  title="Parâmetros da etapa"
                >
                  <span className={styles.layerIndex}>{i + 1}</span>
                  <span className={styles.layerName}>{labelFor(op.kind)}</span>
                  {op.scope === "selection" && (
                    <span
                      title="Este filtro é aplicado só dentro da seleção"
                      style={{
                        fontSize: 8.5,
                        fontWeight: 700,
                        letterSpacing: 0.3,
                        color: "#22d3ee",
                        border: "1px solid rgba(34,211,238,0.55)",
                        borderRadius: 4,
                        padding: "0 4px",
                        marginLeft: 2,
                        whiteSpace: "nowrap",
                      }}
                    >
                      SELEÇÃO
                    </span>
                  )}
                </button>
              </div>
            ))
          )}
        </div>
      )}

      {!collapsed && selected && (
        <div className={styles.layerPopover}>
          <header className={styles.layerPopoverHead}>
            <strong>{labelFor(selected.kind)}</strong>
            <div className={styles.layerPopoverActions}>
              <button
                type="button"
                onClick={() => move(selected.id, -1)}
                disabled={selIndex <= 0}
                title="Aplicar antes (←)"
              >
                <ChevronLeft size={13} />
              </button>
              <button
                type="button"
                onClick={() => move(selected.id, 1)}
                disabled={selIndex >= stack.length - 1}
                title="Aplicar depois (→)"
              >
                <ChevronRight size={13} />
              </button>
              <button
                type="button"
                onClick={() => removeOp(selected.id)}
                title="Remover etapa"
                className={styles.itemDanger}
              >
                <Trash2 size={12} />
              </button>
              <button
                type="button"
                onClick={() => setSelectedId(null)}
                title="Fechar"
              >
                <XIcon size={13} />
              </button>
            </div>
          </header>
          <OpControls
            op={selected}
            onParam={(k, v) => updateParam(selected.id, k, v)}
          />
          {/* Escopo da etapa: imagem inteira × só na seleção. */}
          {isMaskableOpKind(selected.kind) && (
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                gap: 5,
                marginTop: 8,
                paddingTop: 8,
                borderTop: "1px solid rgba(148,163,184,0.15)",
              }}
            >
              <span
                style={{ fontSize: 10.5, color: "rgba(148,163,184,0.85)" }}
              >
                Onde aplicar
              </span>
              <div style={{ display: "flex", gap: 6 }}>
                <button
                  type="button"
                  onClick={() =>
                    updateOp(selected.id, { scope: "image", mask: null })
                  }
                  style={scopeBtnStyle(selected.scope !== "selection")}
                >
                  Imagem inteira
                </button>
                <button
                  type="button"
                  disabled={
                    selected.scope !== "selection" && !activeSelection
                  }
                  onClick={() => {
                    if (selected.scope === "selection") return;
                    if (activeSelection)
                      updateOp(selected.id, {
                        scope: "selection",
                        mask: activeSelection,
                      });
                  }}
                  title={
                    !activeSelection && selected.scope !== "selection"
                      ? "Faça uma seleção no canvas primeiro"
                      : "Confinar este filtro à seleção"
                  }
                  style={scopeBtnStyle(selected.scope === "selection")}
                >
                  Só na seleção
                </button>
              </div>
              {selected.scope !== "selection" && !activeSelection && (
                <span
                  style={{ fontSize: 10, color: "rgba(148,163,184,0.7)" }}
                >
                  Desenhe uma seleção no canvas para poder confinar este filtro
                  a ela.
                </span>
              )}
              {selected.scope === "selection" && !selected.mask && (
                <span style={{ fontSize: 10, color: "#f59e0b" }}>
                  Máscara ausente — reaplique com uma seleção ativa.
                </span>
              )}
            </div>
          )}
          {selNote && <p className={styles.opNote}>{selNote}</p>}
        </div>
      )}
    </div>
  );
}

/**
 * Converte a seleção (px da imagem) numa `MaskSpec` normalizada [0,1]: assim a
 * mesma máscara vale no preview reduzido e no export em resolução cheia.
 * Null se faltam dimensões ou a geometria é inválida.
 */
export function selectionToMaskSpec(
  sel: SicroImageSelection,
  sourceWidth?: number,
  sourceHeight?: number,
): Record<string, unknown> | null {
  const W = sourceWidth && sourceWidth > 0 ? sourceWidth : 0;
  const H = sourceHeight && sourceHeight > 0 ? sourceHeight : 0;
  if (W <= 0 || H <= 0) return null;
  const inverted = !!sel.inverted;
  if (sel.kind === "polygon") {
    const pts = (sel.points ?? []).map((p) => [p.x / W, p.y / H]);
    if (pts.length < 3) return null;
    return { shape: "polygon", points: pts, inverted };
  }
  if (
    sel.x === undefined ||
    sel.y === undefined ||
    sel.width === undefined ||
    sel.height === undefined
  ) {
    return null;
  }
  return {
    shape: sel.kind, // "rect" | "ellipse"
    x: sel.x / W,
    y: sel.y / H,
    width: sel.width / W,
    height: sel.height / H,
    inverted,
  };
}

/** Kinds geométricos mudam a dimensão da imagem, então a máscara não alinharia: não são mascaráveis. */
const NON_MASKABLE_KINDS: ReadonlySet<ProcessingOpKind> =
  new Set<ProcessingOpKind>([
    "crop",
    "resize",
    "rotate_90_cw",
    "rotate_90_ccw",
    "rotate_180",
    "flip_horizontal",
    "flip_vertical",
    "perspective",
    "rotate_arbitrary",
  ]);

function isMaskableOpKind(kind: ProcessingOpKind): boolean {
  return !NON_MASKABLE_KINDS.has(kind);
}

/** Converte um `ProcessingOp` no `BackendOperation` aceito por `apply_operation_stack`. */
export function processingOpToBackendOperation(
  op: ProcessingOp,
  sourceWidth?: number,
  sourceHeight?: number,
): Record<string, unknown> {
  const inner = { kind: op.kind, ...op.params };
  // Escopo "seleção": embrulha o op em `masked` com a máscara normalizada (o backend
  // rasteriza no tamanho corrente). Sem dimensões/geometria, cai para imagem inteira.
  if (op.scope === "selection" && op.mask) {
    const spec = selectionToMaskSpec(op.mask, sourceWidth, sourceHeight);
    if (spec) return { kind: "masked", op: inner, mask: spec };
  }
  return inner;
}
