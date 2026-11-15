/**
 * Só para testes do rastreio (P2.4): mídia de 300 quadros a 30 qps em 1/15360, um trecho com destaque sobre um
 * rastreio manual e densos montados à mão (o que o backend gravaria). Nada aqui roda no app.
 */

import type { ArquivoDenso, Caixa, ClipeTrecho, Corrida, Denso, EfDestaque, MidiaInfo, MidiaRef, ProjetoEdicao, Rastreio, TabelaQuadros } from "@domain/edicao";
import { novoProjeto, novoTrecho } from "../modelo/projeto";
import { tempoDoQuadro } from "../modelo/tempo";

export const WS = "/casos/rastreio";
export const SHA = "d".repeat(64);
export const PASSO = 512;
export const tab: TabelaQuadros = { tb: { num: 1, den: 15360 }, pts0: 0, passos: Array(299).fill(PASSO), passo_mediano: PASSO, chaves: [0], vfr: false, origem: "decodificador" };
export const info: MidiaInfo = {
  id: "v1", sha256: SHA, nome: "cam.mp4", exibido: { w: 640, h: 360 }, taxa: { num: 30, den: 1 }, vfr: false, passo: { min: 1 / 30, max: 1 / 30 },
  de_s: 0, ate_s: 10, n: 300, tem_audio: false, previa: { relativo: "cam.mp4", tipo: "original", desloc_s: 0, de_s: 0, ate_s: 10, largura: 640, altura: 360, pronta: true },
  tabela: "decodificador", erros: 0, marcavel: true, cor: { matriz: "bt709", faixa: "tv" },
};
/** Tempo e pts do quadro j. */
export const q = (j: number) => ({ t: tempoDoQuadro(tab, j), pts: j * PASSO });
export const CAIXA: Caixa = { x: 0.4, y: 0.4, w: 0.1, h: 0.2 };

/** Projeto: mídia "m", trecho "c" (quadros 0 a 199), destaque "e1" sobre o rastreio manual "r1" com âncora no 30. */
export function projeto(): ProjetoEdicao {
  const p = novoProjeto("Rastreio");
  p.id = "00000000-0000-4000-8000-0000000000a4";
  p.midias = [{ id: "m", sha256: SHA, tipo: "video", nome: "cam.mp4", relativo: "cam.mp4", largura: 640, altura: 360, vfr: false, ocultacoes: [] } as unknown as MidiaRef];
  const r: Rastreio = {
    id: "r1", nome: "", cor: "#ffcc00", midia: "m", base: "fonte", tipo: "caixa", espaco: "original",
    ancoras: [{ ...q(30), v: CAIXA }], denso: null, silhuetas: null, ausencias: [], origem: { tipo: "manual" }, confirmacao: null,
  };
  p.rastreios = [r];
  const c = { ...novoTrecho("m", q(0).t, q(200).t), id: "c", inicio_pts: 0, fim_pts: 200 * PASSO } as ClipeTrecho;
  const e1: EfDestaque = {
    id: "e1", ligado: true, nome: "", ativo: null, tipo: "destaque", forma: "retangulo", rastreio: "r1",
    traco: { cor: "#ffcc00", espessura: 0.006, contorno: true }, tamanho: 0.12, angulo_graus: 45, margem: 0.05, rotulo: null,
  };
  c.efeitos = [e1];
  p.principal = [c];
  return p;
}

export interface Linha {
  j: number;
  estado?: number;
  tipo?: number;
  caixa?: Caixa;
}

const corrida = (de: number, ate: number): Corrida => ({
  motor: "vittrack", modelo: { nome: "VitTrack", origem: "opencv", commit: "868e4c94", arquivos: [{ nome: "vittrack.onnx", sha256: "2".repeat(64) }], desvio: "" },
  runtime: "onnxruntime 1.31.1", provedor: "cpu", sentido: "frente", ancora_pts: de * PASSO, de_pts: de * PASSO, ate_pts: ate * PASSO,
  limiares: { ok: 0.45, perdido: 0.3, paciencia: 60 }, parou: "fim", feito_em: "2026-11-15T10:00:00Z",
});

/** Arquivo denso com uma linha por quadro listado (o que `edicao_rastreio_ler` devolveria). */
export function densoDe(rid: string, linhas: Linha[]): ArquivoDenso {
  const c = linhas.map((l) => l.caixa ?? CAIXA);
  return {
    esquema: 2, rastreio: rid, midia_sha256: SHA, espaco: "original", pixel: { w: 640, h: 360 },
    pts: linhas.map((l) => l.j * PASSO), x: c.map((b) => b.x), y: c.map((b) => b.y), w: c.map((b) => b.w), h: c.map((b) => b.h),
    conf: linhas.map((l) => (l.estado === 1 ? 0.35 : 0.9)), estado: linhas.map((l) => l.estado ?? 0), tipo: linhas.map((l) => l.tipo ?? 0),
    corrida: linhas.map(() => 0), corridas: [corrida(linhas[0]?.j ?? 0, linhas[linhas.length - 1]?.j ?? 0)],
  };
}

/** Quadros a..b (inclusive), com o estado dado. */
export const faixa = (a: number, b: number, estado = 0, tipo = 0): Linha[] => Array.from({ length: b - a + 1 }, (_, i) => ({ j: a + i, estado, tipo }));

/** Ponteiro do denso (o que o evento `fim` e o `Editado` trazem). */
export function ponteiro(sha: string, d: ArquivoDenso): Denso {
  const n = d.pts.length;
  return {
    sha256: sha, n, de_s: ((d.pts[0] ?? 0) * tab.tb.num) / tab.tb.den, ate_s: ((d.pts[n - 1] ?? 0) * tab.tb.num) / tab.tb.den,
    ok: d.estado.filter((e) => e === 0).length, incertos: d.estado.filter((e) => e === 1).length, ausentes: d.estado.filter((e) => e === 2).length,
  };
}
