/**
 * Camada global da prévia viva (pendência da Fase 1, D10): legendas queimadas (`motor/legendas.rs::queimadas`, pelo
 * espelho `modelo/legendas.ts`) e selos de edição (`camada/selos.rs`: mesmos textos, mesmo estilo, mesmas emendas na
 * transição). Os dois viram `EfTexto` e saem pelas primitivas do palco (o sprite do Rust).
 */

import type { ClipePrincipal, ClipeSobreposto, Desconhecido, EfTexto, Efeito, EstiloTexto, Id, LinhaResolvida, Lugar, ProjetoEdicao, Velocidade } from "@domain/edicao";
import { clipeConhecido, efeitoConhecido, sobrepostoConhecido } from "../../modelo/projeto";
import { exibidas, LARGURA, type Exibida } from "../../modelo/legendas";

export const IMAGEM_PARADA = "Imagem parada";
export const VELOCIDADE_VARIAVEL = "Velocidade variável";
export const IMAGEM_AMPLIADA = "Imagem ampliada";
export const IMAGEM_TRATADA = "Imagem tratada";
export const IMAGEM_IA = "Imagem ampliada por IA";
const SEPARADOR = " · ";
const FOLGA = 1e-6;
const LUGAR_SELO: Lugar = { x: 0.98, y: 0.03, h: "dir", v: "topo" };
const ESTILO_SELO: EstiloTexto = {
  fonte: "sans",
  negrito: false,
  italico: false,
  tamanho: 0.03,
  cor: "#ffffff",
  alinhamento: "dir",
  caixa: { cor: "#000000b3", margem: 0.008, raio: 0.004, faixa: false },
  contorno: null,
};

type Ef = Efeito | Desconhecido;
const ligados = (es: readonly Ef[]) => es.filter((e): e is Efeito => efeitoConhecido(e) && e.ligado);

/** "Velocidade 0,5×" (três casas, sem zeros à direita). */
export function textoDaVelocidade(fator: number): string {
  const s = fator.toFixed(3).replace(/0+$/, "").replace(/\.$/, "");
  return `Velocidade ${s.replace(".", ",")}×`;
}

function daVelocidade(v: Velocidade): string | null {
  if (v.tipo === "rampa") return VELOCIDADE_VARIAVEL;
  return Math.abs(v.fator - 1) > 1e-9 ? textoDaVelocidade(v.fator) : null;
}

function ampliado(es: readonly Ef[]): boolean {
  const parte = (c: { w: number; h: number }) => c.w < 1 - FOLGA || c.h < 1 - FOLGA;
  const q = ligados(es).find((e) => e.tipo === "enquadramento");
  const enquadre = q?.tipo === "enquadramento" ? (Array.isArray(q.regiao) ? q.regiao.some((k) => parte(k.v)) : parte(q.regiao)) : false;
  return enquadre || ligados(es).some((e) => e.tipo === "lupa");
}

const realcado = (es: readonly Ef[]) => ligados(es).some((e) => ["estabilizar", "lente", "desentrelacar", "ajustes", "nitidez"].includes(e.tipo));

function porIa(p: ProjetoEdicao, midia: Id): boolean {
  return p.midias.some((m) => m.id === midia && m.derivacao?.tipo === "superres");
}

function midiasDoClipe(c: ClipePrincipal): Id[] {
  switch (c.tipo) {
    case "trecho":
    case "congelar":
    case "imagem":
      return [c.midia];
    case "titulo":
      return c.titulo.fundo.tipo === "imagem" ? [c.titulo.fundo.midia] : [];
    case "tela":
      return c.celulas.map((k) => k.midia);
  }
}

/** Textos dos selos do clipe (`selos::do_clipe`). */
export function selosDoClipe(p: ProjetoEdicao, c: ClipePrincipal): string[] {
  const es = c.efeitos;
  const s = p.saida.selos;
  const v: string[] = [];
  if (s.velocidade) {
    if (c.tipo === "congelar") v.push(IMAGEM_PARADA);
    else if (c.tipo === "trecho" || c.tipo === "tela") {
      const x = daVelocidade(c.velocidade);
      if (x) v.push(x);
    }
  }
  if (s.ampliacao && ampliado(es)) v.push(IMAGEM_AMPLIADA);
  if (s.realce && realcado(es)) v.push(IMAGEM_TRATADA);
  const media = ligados(es).find((e) => e.tipo === "media_temporal");
  if (media?.tipo === "media_temporal") v.push(`Média de ${media.quadros} imagens`);
  const sobreposta = ligados(es).some((e) => e.tipo === "imagem" && porIa(p, e.midia));
  if (sobreposta || midiasDoClipe(c).some((m) => porIa(p, m))) v.push(IMAGEM_IA);
  return v;
}

function doSobreposto(p: ProjetoEdicao, c: ClipeSobreposto): string[] {
  const m = c.tipo === "imagem" || c.tipo === "video" ? c.midia : c.titulo.fundo.tipo === "imagem" ? c.titulo.fundo.midia : null;
  return m && porIa(p, m) ? [IMAGEM_IA] : [];
}

const juntar = (a: readonly string[], b: readonly string[]) => [...a, ...b.filter((x) => !a.includes(x))];
const iguais = (a: readonly string[], b: readonly string[]) => a.length === b.length && a.every((x, i) => x === b[i]);

/** Selos por faixa de slots `[de, ate)` do programa inteiro (`selos::itens`, sem cortar em blocos). */
export function faixasDosSelos(p: ProjetoEdicao, linha: LinhaResolvida): { de: number; ate: number; texto: string }[] {
  const porId = new Map<Id, ClipePrincipal>();
  for (const c of p.principal) if (clipeConhecido(c)) porId.set(c.id, c);
  const cl = linha.clipes;
  const textos = cl.map((r) => {
    const c = porId.get(r.id);
    return c ? selosDoClipe(p, c) : [];
  });
  const fim = (i: number) => cl[i]!.inicio_q + cl[i]!.n_q;
  // (de, ate, textos, trecho de baixo)
  const trechos: [number, number, string[], number | null][] = [];
  const por = (de: number, ate: number, t: string[], i: number) => {
    if (de < ate && t.length > 0) trechos.push([de, ate, t, i]);
  };
  cl.forEach((c, i) => {
    const cabeca = i > 0 ? Math.max(Math.min(fim(i - 1), fim(i)), c.inicio_q) : c.inicio_q;
    const juntos = i > 0 ? juntar(textos[i - 1]!, textos[i]!) : [...textos[i]!];
    const prox = cl[i + 1];
    const ate = prox ? Math.min(prox.inicio_q, fim(i)) : fim(i);
    if (cabeca === c.inicio_q || iguais(juntos, textos[i]!)) por(c.inicio_q, ate, textos[i]!, i);
    else {
      por(c.inicio_q, Math.min(cabeca, ate), juntos, i);
      por(cabeca, ate, textos[i]!, i);
    }
  });
  // imagem por IA numa trilha de sobreposição soma o selo ao do clipe por baixo
  const extras: [number, number, string[]][] = [];
  for (const t of p.sobreposicoes) {
    if (t.oculta) continue;
    for (const c of t.clipes) {
      if (!sobrepostoConhecido(c)) continue;
      const s = doSobreposto(p, c);
      const l = linha.livres.find((x) => x.id === c.id && x.trilha === t.id);
      if (l && s.length > 0 && l.n_q > 0) extras.push([l.inicio_q, l.inicio_q + l.n_q, s]);
    }
  }
  let faixas = trechos.map(([de, ate, t]) => ({ de, ate, textos: t }));
  if (extras.length > 0) {
    const pontos = [...new Set([...trechos.flatMap((x) => [x[0], x[1]]), ...extras.flatMap((x) => [x[0], x[1]])])].sort((a, b) => a - b);
    faixas = [];
    for (let k = 0; k + 1 < pontos.length; k++) {
      const [a, z] = [pontos[k]!, pontos[k + 1]!];
      const base = trechos.find((x) => x[0] <= a && a < x[1]);
      let t = base ? [...base[2]] : [];
      for (const e of extras) if (e[0] <= a && a < e[1]) t = juntar(t, e[2]);
      if (t.length === 0) continue;
      const u = faixas.at(-1);
      if (u && u.ate === a && iguais(u.textos, t)) u.ate = z;
      else faixas.push({ de: a, ate: z, textos: t });
    }
  }
  return faixas.map((f) => ({ de: f.de, ate: f.ate, texto: f.textos.join(SEPARADOR) }));
}

const efeitoTexto = (id: string, conteudo: string, estilo: EstiloTexto, lugar: Lugar, largura_max: number): EfTexto => ({
  id,
  tipo: "texto",
  ligado: true,
  nome: "",
  ativo: null,
  conteudo,
  estilo,
  lugar,
  largura_max,
});

/** A camada global pronta para o slot: legenda queimada (se a última exportação queima) e selo. */
export interface Global {
  legendas: Exibida[];
  selos: { de: number; ate: number; texto: string }[];
}

export function globalDe(p: ProjetoEdicao, linha: LinhaResolvida): Global {
  const queima = p.saida.exportacao.legendas === "queimar" || p.saida.exportacao.legendas === "ambos";
  let legendas: Exibida[] = [];
  if (queima && p.legendas.itens.length > 0)
    try {
      legendas = exibidas(p, linha);
    } catch {
      legendas = [];
    }
  return { legendas, selos: faixasDosSelos(p, linha) };
}

/** Efeitos de texto da camada global no slot `q` (legenda por baixo, selo por cima, como no motor). */
export function textosNoSlot(p: ProjetoEdicao, g: Global, q: number): EfTexto[] {
  const v: EfTexto[] = [];
  for (const e of g.legendas)
    if (q >= e.de_q && q < e.ate_q) v.push(efeitoTexto("legenda", e.linhas.join("\n"), p.legendas.estilo, p.legendas.lugar, LARGURA));
  for (const s of g.selos) if (q >= s.de && q < s.ate) v.push(efeitoTexto("selo", s.texto, ESTILO_SELO, LUGAR_SELO, 0.5));
  return v;
}
