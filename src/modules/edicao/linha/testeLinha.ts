/** Tabelas e projetos sintéticos da linha do tempo (testes e a medida do Diagnóstico; prefixo `linha-`). Dono: P4.1. */

import type { ClipeTrecho, Id, MidiaRef, ProjetoEdicao, Racional, SobTitulo, TabelaQuadros, TrilhaSobreposicao } from "@domain/edicao";
import { novoId, novoProjeto, novoTrecho } from "../modelo/projeto";
import { ptsDaTabela, tempoDoQuadro, type ContextoTempo } from "../modelo/tempo";
import { novoTituloConteudo } from "../inspetor/novos";

/** Tabela de `n` quadros a `qps` (base 1/15360, começando em `pts0`). */
export function tabelaSintetica(n: number, qps = 30, pts0 = 0): TabelaQuadros {
  const passo = 15360 / qps;
  return { tb: { num: 1, den: 15360 }, pts0, passos: new Array(n - 1).fill(passo), passo_mediano: passo, chaves: Array.from({ length: Math.ceil(n / 30) }, (_, i) => i * 30), vfr: false, origem: "decodificador" };
}

export function midiaSintetica(id: Id, tab: TabelaQuadros | null, extra: Partial<MidiaRef> = {}): MidiaRef {
  const n = tab ? tab.passos.length + 1 : 300;
  return {
    id,
    tipo: "video",
    origem: "caso",
    media_id: null,
    sha256: id.padEnd(64, "0").slice(0, 64),
    nome: `${id}.mp4`,
    relativo: `${id}.mp4`,
    duracao_s: tab ? (n * tab.passo_mediano * tab.tb.num) / tab.tb.den : 10,
    inicio_s: tab ? tempoDoQuadro(tab, 0) : 0,
    largura: 1280,
    altura: 720,
    taxa: { num: 30, den: 1 },
    vfr: false,
    tem_audio: true,
    cor: null,
    derivacao: null,
    recebido: { nativo: null, obs: "" },
    relogio_desvio_s: null,
    gravador: null,
    decodificacao: null,
    ocultacoes: [],
    ...extra,
  };
}

/** Trecho nos quadros [a, b) da tabela, com os pts. */
export function trechoNosQuadros(midia: Id, tab: TabelaQuadros, a: number, b: number, fator = 1): ClipeTrecho {
  const pts = ptsDaTabela(tab);
  const c = novoTrecho(midia, tempoDoQuadro(tab, a), b < pts.length ? tempoDoQuadro(tab, b) : tempoDoQuadro(tab, pts.length - 1) + tab.passo_mediano / tab.tb.den);
  c.inicio_pts = pts[a] as number;
  c.fim_pts = b < pts.length ? (pts[b] as number) : null;
  c.velocidade = { tipo: "constante", fator };
  return c;
}

export function textoLivre(pos: SobTitulo["pos"], duracao_s: number | null, rotulo = ""): SobTitulo {
  return {
    tipo: "titulo",
    id: novoId(),
    rotulo,
    pos,
    duracao_s,
    transformacao: { x: 0.5, y: 0.5, escala: 1, rotacao: 0, opacidade: 1 },
    entrada_s: 0.3,
    saida_s: 0.3,
    efeitos: [],
    titulo: { ...novoTituloConteudo("cartela"), modelo: "livre" },
  };
}

export const trilhaS = (clipes: TrilhaSobreposicao["clipes"] = [], nome = ""): TrilhaSobreposicao => ({ id: novoId(), nome, oculta: false, clipes });

/** Projeto com a mídia "m1" (300 quadros a 30 qps) e saída a `fps`. */
export function projetoSintetico(fps: Racional = { num: 30, den: 1 }, n = 300): { doc: ProjetoEdicao; tab: TabelaQuadros; ctx: ContextoTempo } {
  const tab = tabelaSintetica(n);
  const doc = novoProjeto("linha");
  doc.saida.fps = fps;
  doc.midias.push(midiaSintetica("m1", tab));
  return { doc, tab, ctx: { tabela: (id) => (id === "m1" ? tab : null) } };
}
