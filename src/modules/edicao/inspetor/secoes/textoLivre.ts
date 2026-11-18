/**
 * Texto livre (Fase 4; contratos-fase4.md D4): `SobTitulo` de modelo "livre" numa trilha de sobreposição, um bloco
 * centrado no `lugar` (0,5; 0,5) do título e posto pela `transformacao` (centro x, y; rotação; escala 1). Padrões
 * do estilo (fundo, contorno, sombra), item novo no cursor e as contas que a seção e as alças dividem. Sem tela
 * aqui (a Sequência e a linha importam sem puxar o Inspetor). Dono: pacote do texto livre.
 */

import type {
  AnimacaoSob,
  BlocoTexto,
  ClipeResolvido,
  Cursor,
  EstiloTexto,
  Id,
  ItemSel,
  LinhaResolvida,
  Ponto,
  ProjetoEdicao,
  Seg,
  SobTitulo,
  SombraTexto,
  Surgir,
  TrilhaSobreposicao,
} from "@domain/edicao";
import { acharMidia, novoId } from "../../modelo/projeto";
import { instanteDoSlot, programaDaFonte, resolver } from "../../modelo/tempo";
import { aDe, ponto } from "../../modelo/geometria";
import { useEdicaoStore } from "../../store/edicaoStore";
import { recursoDe } from "../../palco/recursos";
import { useAreaVisivel } from "../../palco/areaVisivel";

/** O pacote liga ao entregar (regra 3): botão "Texto livre", seção do Inspetor, alças no palco. */
export const TEXTO_LIVRE_PRONTO = true;

/** O sobreposto é um texto livre (título de modelo "livre" com um bloco)? */
export function ehTextoLivre(c: { tipo: string; titulo?: { modelo: string; blocos: unknown[] } }): boolean {
  return c.tipo === "titulo" && c.titulo?.modelo === "livre" && (c.titulo?.blocos.length ?? 0) === 1;
}

// ---------------------------------------------------------------- padrões (D4)

/** Maior deslocamento e desfoque da sombra (fração da altura), o `camada::texto::SOMBRA_MAX`. */
export const SOMBRA_MAX = 0.05;
export const TAMANHO_PADRAO = 0.052;
/** Largura da caixa do texto novo, em fração da largura do canvas. */
export const LARGURA_PADRAO = 0.6;
export const DURACAO_TEXTO = 5;
/** Entrada e saída (s). */
export const DURACAO_SURGIR = 0.3;
export const COR_CAIXA = "#000000";
export const FAIXA = { cor: "#000000a0", margem: 0.012, raio: 0, faixa: true } as const;
export const CAIXA = { margem: 0.012, raio: 0.006 } as const;
export const CONTORNO = { cor: "#000000", largura: 0.003 } as const;
export const SOMBRA: SombraTexto = { cor: "#000000b3", dx: 0, dy: 0.003, desfoque: 0.004 };
/** Cores à mão na seção (a do seletor fica para o resto). */
export const CORES = ["#ffffff", "#ffd84a", "#ff453a", "#30d158", "#64d2ff", "#000000"] as const;

export type Fundo = "nenhum" | "faixa" | "caixa";

export function fundoDe(e: EstiloTexto): Fundo {
  if (!e.caixa) return "nenhum";
  return e.caixa.faixa ? "faixa" : "caixa";
}

/** O estilo com o fundo `f`. Caixa: a cor `cor` (a da caixa que já havia, senão preta). */
export function comFundo(e: EstiloTexto, f: Fundo, cor?: string): EstiloTexto {
  if (f === "nenhum") return { ...e, caixa: null };
  if (f === "faixa") return { ...e, caixa: { ...FAIXA } };
  const atual = e.caixa && !e.caixa.faixa ? e.caixa.cor : null;
  return { ...e, caixa: { cor: cor ?? atual ?? COR_CAIXA, margem: CAIXA.margem, raio: CAIXA.raio } };
}

/** Estilo do texto novo: Inter branca, 5,2 % da altura, centrada, na faixa translúcida. */
export function estiloNovo(): EstiloTexto {
  return { fonte: "sans", negrito: false, italico: false, tamanho: TAMANHO_PADRAO, cor: "#ffffff", alinhamento: "centro", caixa: { ...FAIXA }, contorno: null };
}

/** `animacao` com um lado trocado; volta a ausente quando os dois são "aparecer" (o padrão: o arquivo segue aberto
 *  na versão anterior). */
export function comSurgir(a: AnimacaoSob | undefined, lado: "entrada" | "saida", s: Surgir): AnimacaoSob | undefined {
  const novo: AnimacaoSob = { entrada: a?.entrada ?? "aparecer", saida: a?.saida ?? "aparecer", lado: a?.lado ?? "baixo", [lado]: s };
  return novo.entrada === "aparecer" && novo.saida === "aparecer" ? undefined : novo;
}

// ---------------------------------------------------------------- cursor e item novo

const fpsDe = (l: LinhaResolvida) => l.fps;

/** Clipe da principal que contém o slot `q` (o último, se `q` passa do fim; o primeiro, se antes do começo); na
 *  sobreposição de uma transição, o que entra. */
export function clipeNoSlot(linha: LinhaResolvida, q: number): ClipeResolvido | null {
  const cs = linha.clipes;
  if (cs.length === 0) return null;
  const dentro = cs.filter((c) => q >= c.inicio_q && q < c.inicio_q + c.n_q);
  if (dentro.length > 0) return dentro[dentro.length - 1] as ClipeResolvido;
  return q < (cs[0] as ClipeResolvido).inicio_q ? (cs[0] as ClipeResolvido) : (cs[cs.length - 1] as ClipeResolvido);
}

/** Slot do programa no cursor: o do modo programa, ou o do instante do clipe (trecho: o tempo de fonte pela
 *  velocidade; os outros: o local), dentro do clipe. */
export function slotDoCursor(doc: ProjetoEdicao, linha: LinhaResolvida, cursor: Cursor): number {
  const total = Math.max(linha.total_q, 0);
  if (cursor.modo === "programa") return Math.min(Math.max(Math.round(cursor.q), 0), Math.max(total - 1, 0));
  const r = linha.clipes.find((c) => c.id === cursor.clipe);
  const c = doc.principal.find((x) => (x as { id?: string }).id === cursor.clipe) as { tipo?: string; inicio_s?: number; velocidade?: unknown } | undefined;
  if (!r || !c) return 0;
  let s: number | null = cursor.t;
  if (c.tipo === "trecho" || c.tipo === "tela") s = programaDaFonte(c.velocidade as Parameters<typeof programaDaFonte>[0], c.inicio_s ?? 0, cursor.t);
  // o slot que contém o instante (o do quadro mostrado): o Clipe põe o cursor no meio do quadro, e arredondar dava o
  // slot seguinte (o texto novo nascia um quadro depois do palco e não aparecia nele)
  const k = s === null || !Number.isFinite(s) ? 0 : Math.floor((s * fpsDe(linha).num) / fpsDe(linha).den + 1e-6);
  return r.inicio_q + Math.min(Math.max(k, 0), Math.max(r.n_q - 1, 0));
}

/** Trilha de sobreposição de cima (não oculta); null = nenhuma (criar). */
export function trilhaDeCima(doc: ProjetoEdicao): Id | null {
  for (let i = doc.sobreposicoes.length - 1; i >= 0; i--) {
    const t = doc.sobreposicoes[i] as TrilhaSobreposicao;
    if (!t.oculta) return t.id;
  }
  return null;
}

/** Centro arredondado ao pixel da saída (o texto sem giro sai do sprite sem reamostrar). */
function noPixel(p: Ponto, doc: ProjetoEdicao): Ponto {
  const W = Math.max(doc.saida.largura, 1);
  const H = Math.max(doc.saida.altura, 1);
  const lim = (v: number) => Math.min(Math.max(v, 0), 1);
  return { x: Math.round(lim(p.x) * W) / W, y: Math.round(lim(p.y) * H) / H };
}

/** Texto livre novo no slot `q`, centrado em `centro` (o centro da área visível do palco), preso ao clipe da principal
 *  sob `q` (D5); `trilha` = a de sobreposição de cima, ou null para criar uma. Conteúdo vazio (nenhum texto de exemplo). */
export function novoTextoLivre(doc: ProjetoEdicao, linha: LinhaResolvida, q: number, centro: Ponto): { trilha: Id | null; clipe: SobTitulo } | null {
  if (!(doc.saida.largura > 0 && doc.saida.altura > 0)) return null;
  const fps = fpsDe(linha);
  const ancora = clipeNoSlot(linha, q);
  const pos: SobTitulo["pos"] = ancora ? { ref: "clipe", clipe: ancora.id, desloc: instanteDoSlot(q - ancora.inicio_q, fps) } : { ref: "programa", t: instanteDoSlot(Math.max(q, 0), fps) };
  const c = noPixel(centro, doc);
  const bloco: BlocoTexto = { id: novoId(), papel: "corpo", conteudo: "", estilo: estiloNovo(), lugar: { x: 0.5, y: 0.5, h: "centro", v: "meio" }, largura_max: LARGURA_PADRAO };
  return {
    trilha: trilhaDeCima(doc),
    clipe: {
      tipo: "titulo",
      id: novoId(),
      rotulo: "",
      pos,
      duracao_s: DURACAO_TEXTO,
      transformacao: { x: c.x, y: c.y, escala: 1, rotacao: 0, opacidade: 1 },
      entrada_s: DURACAO_SURGIR,
      saida_s: DURACAO_SURGIR,
      efeitos: [],
      titulo: { modelo: "livre", fundo: { tipo: "transparente" }, blocos: [bloco] },
    },
  };
}

const ctxTempo = (doc: ProjetoEdicao) => ({ tabela: (id: Id) => recursoDe(acharMidia(doc, id)?.sha256).tabela });

/** Centro (0..1 do canvas) da área do palco que o perito vê no clipe sob o cursor: o centro da área publicada pelo
 *  palco (na origem do clipe; o mesmo de `caixaNova`) levado ao canvas por A(τ); sem área publicada, o meio do canvas. */
export function centroVisivel(doc: ProjetoEdicao, linha: LinhaResolvida, q: number): Ponto {
  const meio = { x: 0.5, y: 0.5 };
  const r = clipeNoSlot(linha, q);
  const area = useAreaVisivel.getState();
  if (!r || area.clipe !== r.id || !area.caixa) return meio;
  const c = doc.principal.find((x) => (x as { id?: string }).id === r.id) as
    | { tipo: string; efeitos: Parameters<typeof aDe>[0]; midia?: Id; inicio_s?: number; velocidade?: { tipo: string; fator?: number }; instante_s?: number }
    | undefined;
  const sha = c?.midia ? acharMidia(doc, c.midia)?.sha256 : null;
  const info = sha ? recursoDe(sha).info : null;
  if (!c || !info) return meio;
  const local = instanteDoSlot(q - r.inicio_q, linha.fps);
  const tau = c.tipo === "trecho" ? (c.inicio_s ?? 0) + local * (c.velocidade?.tipo === "constante" ? (c.velocidade.fator ?? 1) : 1) : c.tipo === "congelar" ? (c.instante_s ?? 0) : local;
  const k = area.caixa;
  const p = ponto(aDe(c.efeitos, tau, info.exibido, { w: doc.saida.largura, h: doc.saida.altura }), { x: k.x + k.w / 2, y: k.y + k.h / 2 });
  return Number.isFinite(p.x) && Number.isFinite(p.y) ? { x: Math.min(Math.max(p.x, 0), 1), y: Math.min(Math.max(p.y, 0), 1) } : meio;
}

// ---- foco no campo "Texto" do item novo (a seção pega ao montar) ----

let focoPendente: Id | null = null;

/** A seção do texto `id` pede o foco no "Texto" ao montar (uma vez). */
export function tomarFoco(id: Id): boolean {
  if (focoPendente !== id) return false;
  focoPendente = null;
  return true;
}

/** "Texto livre": cria no cursor (um passo de histórico), seleciona e abre o Inspetor no campo "Texto". */
export function adicionarTextoLivre(): Id | null {
  const s = useEdicaoStore.getState();
  const doc = s.doc;
  if (!doc || s.soLeitura) return null;
  const linha = resolver(doc, ctxTempo(doc));
  const q = slotDoCursor(doc, linha, s.cursor);
  const novo = novoTextoLivre(doc, linha, q, centroVisivel(doc, linha, q));
  if (!novo) return null;
  let sel: ItemSel | null = null;
  s.alterar("Texto livre", (d) => {
    let t = novo.trilha ? d.sobreposicoes.find((x) => x.id === novo.trilha) : undefined;
    if (!t) {
      t = { id: novoId(), nome: "", oculta: false, clipes: [] };
      d.sobreposicoes.push(t);
    }
    t.clipes.push(novo.clipe);
    sel = { k: "livre", id: novo.clipe.id, trilha: t.id };
  });
  if (!sel) return null;
  focoPendente = novo.clipe.id;
  useEdicaoStore.getState().selecionar({ itens: [sel], foco: sel, efeito: null });
  return novo.clipe.id;
}

/** Instante local do texto no cursor (para gravar chave), dentro do clipe; null fora da linha. */
export function tauNoCursor(doc: ProjetoEdicao, linha: LinhaResolvida, cursor: Cursor, livre: Id): Seg | null {
  const l = linha.livres.find((x) => x.id === livre);
  if (!l) return null;
  const q = slotDoCursor(doc, linha, cursor);
  return instanteDoSlot(Math.min(Math.max(q - l.inicio_q, 0), Math.max(l.n_q - 1, 0)), linha.fps);
}
