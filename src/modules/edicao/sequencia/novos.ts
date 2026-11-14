/**
 * Itens novos da Sequência: título, imagem, presos ao clipe (título, terço inferior, imagem, narração) e os
 * trechos de "Dos eventos…" (um por par de eventos, na ordem do tempo). Valores de partida; o inspetor ajusta.
 */

import type { BlocoTexto, ClipeAudio, ClipeImagem, ClipeTitulo, EstiloTexto, Id, Lugar, ProjetoEdicao, Seg, SobImagem, SobTitulo, Titulo, TrilhaAudio, TrilhaSobreposicao } from "@domain/edicao";
import { novoId } from "../modelo/projeto";
import { DURACAO_PADRAO } from "./contas";

export type ModeloNovo = Exclude<Titulo["modelo"], "livre">;
export const MODELOS: readonly { valor: ModeloNovo; rotulo: string }[] = [
  { valor: "cartela", rotulo: "Cartela" },
  { valor: "abertura", rotulo: "Abertura" },
  { valor: "evento", rotulo: "Evento" },
  { valor: "terco_inferior", rotulo: "Terço inferior" },
  { valor: "encerramento", rotulo: "Encerramento" },
];

/** Tamanhos do v1 (fração da altura do canvas): p 0,033 · m 0,052 · g 0,078. */
const estilo = (tamanho: number, negrito: boolean, alinhamento: EstiloTexto["alinhamento"], caixa: EstiloTexto["caixa"] = null): EstiloTexto => ({
  fonte: "sans",
  negrito,
  italico: false,
  tamanho,
  cor: "#ffffff",
  alinhamento,
  caixa,
  contorno: null,
});
const FAIXA = { cor: "#0000008c", margem: 0.013, raio: 0 };

const bloco = (papel: BlocoTexto["papel"], conteudo: string, e: EstiloTexto, lugar: Lugar, largura_max = 0.9): BlocoTexto => ({ id: novoId(), papel, conteudo, estilo: e, lugar, largura_max });

/** Título com os blocos que tiverem texto. Terço inferior: transparente, à esquerda, em faixa. */
export function novoTitulo(modelo: ModeloNovo, titulo: string, subtitulo: string): Titulo {
  const blocos: BlocoTexto[] = [];
  if (modelo === "terco_inferior") {
    if (titulo) blocos.push(bloco("titulo", titulo, estilo(0.052, true, "esq", FAIXA), { x: 0.06, y: 0.8, h: "esq", v: "base" }, 0.7));
    if (subtitulo) blocos.push(bloco("subtitulo", subtitulo, estilo(0.033, false, "esq", FAIXA), { x: 0.06, y: 0.815, h: "esq", v: "topo" }, 0.7));
    return { modelo, fundo: { tipo: "transparente" }, blocos };
  }
  if (titulo) blocos.push(bloco("titulo", titulo, estilo(0.078, true, "centro"), { x: 0.5, y: subtitulo ? 0.47 : 0.5, h: "centro", v: subtitulo ? "base" : "meio" }));
  if (subtitulo) blocos.push(bloco("subtitulo", subtitulo, estilo(0.052, false, "centro"), { x: 0.5, y: 0.53, h: "centro", v: "topo" }));
  return { modelo, fundo: { tipo: "cor", cor: "#000000" }, blocos };
}

export function novoClipeTitulo(t: Titulo, duracao_s: Seg = DURACAO_PADRAO): ClipeTitulo {
  return { tipo: "titulo", id: novoId(), rotulo: "", desligado: false, transicao_entrada: null, efeitos: [], duracao_s, titulo: t };
}

export function novoClipeImagem(midia: Id, duracao_s: Seg = DURACAO_PADRAO): ClipeImagem {
  return { tipo: "imagem", id: novoId(), rotulo: "", desligado: false, transicao_entrada: null, efeitos: [], midia, duracao_s };
}

const NEUTRA = { x: 0.5, y: 0.5, escala: 1, rotacao: 0, opacidade: 1 };

/** Trilha de sobreposição de baixo (cria se não houver). */
function trilhaDeSobreposicao(d: ProjetoEdicao): TrilhaSobreposicao {
  let t = d.sobreposicoes[0];
  if (!t) {
    t = { id: novoId(), nome: "", oculta: false, clipes: [] };
    d.sobreposicoes.push(t);
  }
  return d.sobreposicoes[0] as TrilhaSobreposicao;
}

/** Título preso ao clipe: o terço inferior vai até o fim do clipe (duração nula). */
export function prenderTitulo(d: ProjetoEdicao, clipe: Id, modelo: "evento" | "terco_inferior", texto = ""): Id {
  const t = trilhaDeSobreposicao(d);
  const c: SobTitulo = {
    tipo: "titulo",
    id: novoId(),
    rotulo: "",
    pos: { ref: "clipe", clipe, desloc: 0 },
    duracao_s: modelo === "terco_inferior" ? null : DURACAO_PADRAO,
    transformacao: { ...NEUTRA },
    entrada_s: 0,
    saida_s: 0,
    efeitos: [],
    titulo: modelo === "terco_inferior" ? novoTitulo("terco_inferior", texto, "") : { ...novoTitulo("evento", texto, ""), fundo: { tipo: "transparente" } },
  };
  t.clipes.push(c);
  return c.id;
}

export function prenderImagem(d: ProjetoEdicao, clipe: Id, midia: Id): Id {
  const t = trilhaDeSobreposicao(d);
  const c: SobImagem = { tipo: "imagem", id: novoId(), rotulo: "", pos: { ref: "clipe", clipe, desloc: 0 }, duracao_s: null, transformacao: { ...NEUTRA, escala: 0.25 }, entrada_s: 0, saida_s: 0, efeitos: [], midia };
  t.clipes.push(c);
  return c.id;
}

/** Narração presa ao clipe, na primeira trilha de narração (criada se não houver). */
export function prenderNarracao(d: ProjetoEdicao, clipe: Id, midia: Id, nome: string): Id {
  let t = d.audios.find((x) => x.papel === "narracao");
  if (!t) {
    const nova: TrilhaAudio = { id: novoId(), nome: "", papel: "narracao", volume: 1, mudo: false, abaixa_original: true, clipes: [] };
    d.audios.push(nova);
    t = d.audios[d.audios.length - 1] as TrilhaAudio;
  }
  const c: ClipeAudio = { id: novoId(), nome, midia, pos: { ref: "clipe", clipe, desloc: 0 }, de_s: 0, ate_s: null, volume: 1, entrada_s: 0, saida_s: 0 };
  t.clipes.push(c);
  return c.id;
}

/** Pares de eventos escolhidos (ordem do tempo): [1º, 2º], [3º, 4º]…; o último sem par fica de fora. */
export function paresDeEventos(tempos: readonly Seg[]): [Seg, Seg][] {
  const o = [...tempos].filter(Number.isFinite).sort((a, b) => a - b);
  const pares: [Seg, Seg][] = [];
  for (let i = 0; i + 1 < o.length; i += 2) if ((o[i + 1] as Seg) > (o[i] as Seg)) pares.push([o[i] as Seg, o[i + 1] as Seg]);
  return pares;
}

/** Tempo de fonte do evento da análise: `pts × time_base` quando o evento os tem; senão `timestamp_s`, que a análise
 *  grava com o tempo do reprodutor (o técnico, com o início do arquivo; contratos (h) 27). */
export function tempoDoEvento(ev: { timestamp_s: number; pts: number | null; time_base: string | null }): Seg {
  const m = ev.pts !== null && ev.time_base ? /^\s*(\d+)\s*\/\s*(\d+)\s*$/.exec(ev.time_base) : null;
  if (m && Number(m[2]) > 0) return (ev.pts! * Number(m[1])) / Number(m[2]);
  return ev.timestamp_s;
}
