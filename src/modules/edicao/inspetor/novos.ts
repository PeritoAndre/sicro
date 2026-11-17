/**
 * Itens novos com os padrões da tela (o que "Adicionar" cria). Puros: quem grava é `alterar`.
 * Tamanhos em fração da altura do canvas (§3.2); cores de desenho as do v1.
 */

import type {
  BlocoTexto,
  Caixa,
  ClipeAudio,
  ClipeTitulo,
  EfCarimbo,
  EfDestaque,
  EfEnquadramento,
  EfGirar,
  EfImagem,
  EfOcultar,
  EfTexto,
  EstiloTexto,
  Id,
  Intervalo,
  Lugar,
  Posicao,
  Rastreio,
  Seg,
  Titulo,
  TrilhaAudio,
} from "@domain/edicao";
import { novoId } from "../modelo/projeto";
import { padraoDoConteudo } from "./secoes/ocultarPadrao";

export const COR_DESENHO = "#ffd84a";
/** v1: p 0,033 · m 0,052 · g 0,078. */
export const TAMANHO_TEXTO = { p: 0.033, m: 0.052, g: 0.078 } as const;

export const estiloPadrao = (tamanho: number = TAMANHO_TEXTO.m, negrito = false): EstiloTexto => ({
  fonte: "sans",
  negrito,
  italico: false,
  tamanho,
  cor: "#ffffff",
  alinhamento: "centro",
  caixa: null,
  contorno: { cor: "#000000", largura: 0.003 },
});

/** Caixa do meio da imagem: ponto de partida de um alvo antes de o perito ajustar. */
export const CAIXA_CENTRO: Caixa = { x: 0.4, y: 0.4, w: 0.2, h: 0.2 };

const base = (ativo: Intervalo | null = null) => ({ id: novoId(), ligado: true, nome: "", ativo });

/** Rastreio manual de uma âncora (`midia` null = coordenadas do canvas, clipe sem mídia). */
export function novoRastreio(midia: Id | null, t: Seg, caixa: Caixa = CAIXA_CENTRO): Rastreio {
  return {
    id: novoId(),
    nome: "",
    cor: COR_DESENHO,
    midia,
    base: midia ? "fonte" : "local",
    tipo: "caixa",
    espaco: "original",
    ancoras: [{ t, pts: null, v: caixa }],
    denso: null,
    silhuetas: null,
    ausencias: [],
    origem: { tipo: "manual" },
    confirmacao: null,
  };
}

/** Padrão do dono (17/11): seta, espessura 0,2 %, contorno, tamanho 2 %, direção 90°, sem rótulo. */
export const novoDestaque = (rastreio: Id): EfDestaque => ({
  ...base(),
  tipo: "destaque",
  forma: "seta",
  rastreio,
  traco: { cor: COR_DESENHO, espessura: 0.002, contorno: true },
  tamanho: 0.02,
  angulo_graus: 90,
  margem: 0,
  rotulo: null,
});

/** Ocultação: nasce "rosto", com o modo e a intensidade do padrão do conteúdo (B27). `ativo` em tempo de fonte quando é da mídia. */
export const novoOcultar = (rastreio: Id, ativo: Intervalo | null = null): EfOcultar => {
  const { modo, intensidade } = padraoDoConteudo("rosto");
  return {
    ...base(ativo),
    tipo: "ocultar",
    modo,
    alvo: { rastreio, forma: "retangulo", vertices: null, margem: 0.1, borda_suave: 0, inverter: false },
    intensidade,
    cor: "#000000",
    conteudo: "rosto",
    motivo: "",
  };
};

export const novoTexto = (): EfTexto => ({
  ...base(),
  tipo: "texto",
  conteudo: "",
  estilo: estiloPadrao(TAMANHO_TEXTO.m),
  lugar: { x: 0.5, y: 0.94, h: "centro", v: "base" },
  largura_max: 0.9,
});

export const novoCarimbo = (): EfCarimbo => ({
  ...base(),
  tipo: "carimbo",
  mostra: "tempo_fonte",
  formato: "hms_ms",
  relogio: null,
  estilo: { ...estiloPadrao(TAMANHO_TEXTO.p), caixa: { cor: "#0000008c", margem: 0.008, raio: 0 }, contorno: null },
  lugar: { x: 0.02, y: 0.03, h: "esq", v: "topo" },
});

export const novoImagem = (midia: Id): EfImagem => ({
  ...base(),
  tipo: "imagem",
  midia,
  lugar: { x: 0.98, y: 0.03, h: "dir", v: "topo" },
  largura: 0.15,
  opacidade: 1,
});

export const novoGirar = (graus: EfGirar["graus"] = 90): EfGirar => ({ ...base(), tipo: "girar", graus, espelhar_h: false, espelhar_v: false });

export const novoEnquadramento = (regiao: Caixa): EfEnquadramento => ({ ...base(), tipo: "enquadramento", regiao, ajuste: "conter", interpolacao: "bilinear" });

// ---- títulos ----

/** Onde cada papel cai na cartela, e com que tamanho. */
const PAPEL: Record<BlocoTexto["papel"], { lugar: Lugar; tamanho: number; negrito: boolean }> = {
  titulo: { lugar: { x: 0.5, y: 0.44, h: "centro", v: "base" }, tamanho: TAMANHO_TEXTO.g, negrito: true },
  subtitulo: { lugar: { x: 0.5, y: 0.5, h: "centro", v: "topo" }, tamanho: TAMANHO_TEXTO.m, negrito: false },
  corpo: { lugar: { x: 0.5, y: 0.62, h: "centro", v: "topo" }, tamanho: 0.042, negrito: false },
  rodape: { lugar: { x: 0.5, y: 0.93, h: "centro", v: "base" }, tamanho: TAMANHO_TEXTO.p, negrito: false },
};

export function novoBloco(papel: BlocoTexto["papel"], conteudo = "", modelo?: Pick<EstiloTexto, "fonte" | "cor">): BlocoTexto {
  const p = PAPEL[papel];
  const estilo = { ...estiloPadrao(p.tamanho, p.negrito), contorno: null };
  return { id: novoId(), papel, conteudo, estilo: modelo ? { ...estilo, fonte: modelo.fonte, cor: modelo.cor } : estilo, lugar: { ...p.lugar }, largura_max: 0.86 };
}

export const novoTituloConteudo = (modelo: Titulo["modelo"] = "cartela"): Titulo => ({
  modelo,
  fundo: { tipo: "cor", cor: "#000000" },
  blocos: [novoBloco("titulo")],
});

export const novoTitulo = (duracao_s: Seg = 3): ClipeTitulo => ({
  tipo: "titulo",
  id: novoId(),
  rotulo: "",
  desligado: false,
  transicao_entrada: null,
  efeitos: [],
  duracao_s,
  titulo: novoTituloConteudo(),
});

// ---- áudio ----

export const novaTrilhaNarracao = (): TrilhaAudio => ({ id: novoId(), nome: "", papel: "narracao", volume: 1, mudo: false, abaixa_original: false, clipes: [] });

export const novoClipeAudio = (midia: Id, nome: string, pos: Posicao): ClipeAudio => ({
  id: novoId(),
  nome,
  midia,
  pos,
  de_s: 0,
  ate_s: null,
  volume: 1,
  entrada_s: 0,
  saida_s: 0,
});
