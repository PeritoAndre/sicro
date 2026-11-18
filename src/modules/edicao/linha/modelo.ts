/**
 * Linha do tempo (Fase 4, P4.1): OUTRA VISTA do mesmo `ProjetoEdicao` (contratos-fase4.md D1). Nada aqui é estado do
 * documento: as trilhas e os itens saem do documento e da linha resolvida (`modelo/tempo.ts::resolver`, a mesma da
 * Sequência), em slots do programa. Base de tempo de cada clipe (§3.1): fonte no trecho e na tela (o tempo do quadro
 * exibido no slot, pelo mapa do clipe), local nos demais. Puro; testado sem tela. Dono: P4.1.
 */

import type {
  ClipePrincipal,
  ClipeResolvido,
  ClipeSobreposto,
  Cor,
  Desconhecido,
  Efeito,
  Id,
  Intervalo,
  ItemSel,
  LinhaResolvida,
  ProjetoEdicao,
  Racional,
  Rastreio,
  Seg,
  TabelaQuadros,
} from "@domain/edicao";
import { clipeConhecido, efeitoConhecido, sobrepostoConhecido } from "../modelo/projeto";
import { mapaTrecho, tempoDoQuadro, type ContextoTempo, type MapaQuadros } from "../modelo/tempo";
import { tabelaDoClipe } from "../modelo/realce";
import { tabelasDaTela } from "./tabelas";

/** De cima para baixo: sobreposições (a de cima primeiro), principal (V1), faixas de efeito, som dos trechos,
 *  trilhas de áudio (na ordem do documento) e legendas (D2). */
export type TipoTrilha = "sobreposicao" | "principal" | "efeito" | "som" | "audio" | "legendas";

/** Ids das trilhas que não existem no documento. */
export const TRILHA_PRINCIPAL = "principal";
export const TRILHA_SOM = "som";
export const TRILHA_LEGENDAS = "legendas";

/** Um item numa trilha, em slots do programa `[de_q, ate_q)`. */
export interface ItemNaLinha {
  /** Id do clipe, livre, clipe de áudio, legenda ou efeito. */
  id: Id;
  k: ItemSel["k"] | "efeito";
  /** Trilha do documento (sobreposição ou áudio) ou uma das fixas. */
  trilha: string;
  de_q: number;
  ate_q: number;
  rotulo: string;
  /** Efeito: o clipe da principal (ou sobreposto) dono dele. */
  clipe?: Id;
  /** Slots dos quadros-chave (losangos), dentro de `[de_q, ate_q)`. */
  chaves?: number[];
  /** Item citado num problema da análise. */
  erro?: boolean;
  /** Item de versão futura (`Desconhecido`): aparece apagado, sem arrasto. */
  apagado?: boolean;
  // CONTRATO: acréscimos do P4.1 (campos opcionais do próprio pacote).
  /** Clipe ou efeito desligado: apagado, mas selecionável. */
  desligado?: boolean;
  /** Ocultação da mídia: `ativo` em tempo de fonte, vale em todo clipe da mídia (D2). */
  daMidia?: boolean;
  /** Tipo do item no documento ("trecho", "titulo", "video", "destaque"…). */
  tipo?: string;
  /** Mídia do item (`MidiaRef.id`): tira, onda, imagem. */
  midia?: Id;
  /** V1: transição de entrada (`sobrepoe_q` = slots misturados com o anterior; 0 em "escurecer"). */
  entrada?: { tipo: string; sobrepoe_q: number };
  /** Faixa de efeito: linha dentro da faixa (barras que se cruzam no tempo descem). */
  raia?: number;
  /** A barra é cortada pela borda do clipe nesse lado (o `ativo` passa dele): a borda desse lado não arrasta. */
  cortado?: { de: boolean; ate: boolean };
  /** Som dos trechos: o do clipe está mudo. */
  mudo?: boolean;
  /** Barra que atravessa emendas de clipe: os efeitos que a formam, em ordem (o primeiro dá a borda do começo, o
   *  último a do fim). Ausente = um efeito só. */
  partes?: ParteDaBarra[];
}

/** Um efeito dentro de uma barra emendada: o pedaço dele, no clipe dele. */
export interface ParteDaBarra {
  id: Id;
  clipe: Id;
  de_q: number;
  ate_q: number;
  daMidia?: boolean;
}

/** Faixa de efeito: chave = tipo + rastreio seguido (ou só o tipo), na ordem da primeira aparição (D2). */
export interface TrilhaNaLinha {
  id: string;
  tipo: TipoTrilha;
  /** Nome na cabeça: o da trilha do documento, ou o do tipo de efeito + rastreio ("PP1 · Seta"). */
  nome: string;
  itens: ItemNaLinha[];
  oculta?: boolean;
  muda?: boolean;
  // CONTRATO: acréscimos do P4.1.
  /** "S2", "V1", "A1" (sobreposição, principal, áudio). */
  sigla?: string;
  /** Faixa de efeito: cor da barra (a do rastreio, senão a da família). */
  cor?: Cor;
  /** Faixa de efeito: quantas raias. */
  raias?: number;
  /** Índice da trilha no documento (sobreposição, áudio). */
  indice?: number;
}

export interface MarcadorNaLinha {
  id: Id;
  q: number;
  cor: Cor;
  rotulo: string;
}

// ---- rótulos e cores (o rótulo do tipo é o do registro do Inspetor; `linha.test` confere) ----

export const ROTULO_DO_EFEITO: Readonly<Partial<Record<Efeito["tipo"], string>>> = {
  destaque: "Destaque",
  ocultar: "Ocultar",
  holofote: "Holofote",
  lupa: "Lupa",
  regua: "Régua",
  velocidade_exibida: "Velocidade",
  texto: "Texto",
  carimbo: "Tempo",
  imagem: "Imagem",
  enquadramento: "Enquadramento",
};

/** Família de cada efeito que tem faixa (origem e girar valem no clipe inteiro: sem faixa). */
const FAMILIA: Readonly<Partial<Record<Efeito["tipo"], "regiao" | "anotacao" | "enquadre">>> = {
  destaque: "anotacao",
  lupa: "anotacao",
  regua: "anotacao",
  velocidade_exibida: "anotacao",
  texto: "anotacao",
  carimbo: "anotacao",
  imagem: "anotacao",
  ocultar: "regiao",
  holofote: "regiao",
  enquadramento: "enquadre",
};

export const COR_DA_FAMILIA = { regiao: "#ef6b6b", anotacao: "#5aa9e6", enquadre: "#d7a84f", desconhecido: "#718096" } as const;

export const PAPEL: Readonly<Record<"narracao" | "musica" | "efeitos", string>> = { narracao: "Narração", musica: "Música", efeitos: "Efeitos" };

/** O efeito tem barra na linha? (regiões, anotações e o enquadramento; desconhecido também, apagado) */
export const temFaixa = (e: Efeito | Desconhecido): boolean => !efeitoConhecido(e) || FAMILIA[e.tipo] !== undefined;

/** Rastreio que o efeito segue (destaque, lupa, ocultar, holofote, velocidade que acompanha, enquadramento que segue). */
export function rastreioSeguido(e: Efeito): Id | null {
  switch (e.tipo) {
    case "destaque":
    case "lupa":
      return e.rastreio || null;
    case "ocultar":
    case "holofote":
      return e.alvo?.rastreio || null;
    case "velocidade_exibida":
      return e.onde.modo === "segue" ? e.onde.rastreio || null : null;
    case "enquadramento":
      return e.segue?.rastreio ?? null;
    default:
      return null;
  }
}

/** Forma do destaque como a maquete escreve na faixa ("PP1 · seta"). */
export const FORMA_NA_FAIXA: Readonly<Record<string, string>> = { seta: "seta", circulo: "círculo", elipse: "elipse", retangulo: "retângulo", colchetes: "colchetes" };

/** Faixas só pelo tipo, como na maquete ("Lupa", "Zoom"): o rastreio não as separa. */
const SO_PELO_TIPO = new Set<string>(["lupa", "enquadramento"]);

/** Nome da faixa pelo tipo (sem pessoa): o do registro, com o enquadramento chamado "Zoom" (maquete). */
const nomeDoTipo = (tipo: string) => (tipo === "enquadramento" ? "Zoom" : (ROTULO_DO_EFEITO[tipo as Efeito["tipo"]] ?? tipo));

/** Chave da faixa (D2): tipo + rastreio seguido, ou só o tipo. Item de versão futura: o tipo cru. Com `doc` (a
 *  linha), a da maquete: a pessoa pelo NOME do rastreio (a mesma pessoa em duas câmeras fica numa faixa), o destaque
 *  também pela forma, lupa e zoom só pelo tipo. */
// CONTRATO: `doc` opcional (P4.1 rodada 2); sem ele, a chave de antes (pelo id do rastreio).
export function chaveDaFaixaDe(e: Efeito | Desconhecido, doc?: ProjetoEdicao): string {
  if (!efeitoConhecido(e)) return `?${String((e as { tipo?: unknown }).tipo ?? "")}`;
  const r = rastreioSeguido(e);
  if (!doc) return r ? `${e.tipo}:${r}` : e.tipo;
  if (SO_PELO_TIPO.has(e.tipo) || !r) return e.tipo;
  const quem = doc.rastreios.find((x) => x.id === r)?.nome || r;
  return e.tipo === "destaque" ? `destaque:${quem}:${e.forma}` : `${e.tipo}:${quem}`;
}

/** Nome da faixa como na maquete: "PP1 · seta", "PP1 · ocultar", "Lupa", "Zoom", "Texto". */
export function nomeDaFaixa(e: Efeito | Desconhecido, doc: ProjetoEdicao): string {
  if (!efeitoConhecido(e)) return tipoDe(e);
  const rid = SO_PELO_TIPO.has(e.tipo) ? null : rastreioSeguido(e);
  const quem = rid ? doc.rastreios.find((x) => x.id === rid)?.nome : "";
  const oque = e.tipo === "destaque" ? (FORMA_NA_FAIXA[e.forma] ?? e.forma) : nomeDoTipo(e.tipo).toLowerCase();
  if (quem) return `${quem} · ${oque}`;
  return e.tipo === "destaque" ? oque.charAt(0).toUpperCase() + oque.slice(1) : nomeDoTipo(e.tipo);
}

// ---- coordenadas ----

/** Largura de um slot (px). */
export const pxDoSlot = (pxPorS: number, fps: Racional) => (pxPorS * fps.den) / fps.num;
/** x (px, desde o começo do programa) do início do slot q. */
export const xDoSlot = (q: number, pxPorS: number, fps: Racional) => q * pxDoSlot(pxPorS, fps);

/** Slot do programa sob um x da tela (px), com `pxPorS` e a rolagem em s. */
export function slotNoX(x: number, pxPorS: number, rolagem_s: number, fps: { num: number; den: number }): number {
  const q = Math.floor(((x / pxPorS + rolagem_s) * fps.num) / fps.den + 1e-9);
  return Number.isFinite(q) ? Math.max(0, q) : 0;
}

/** Fronteira de slot mais perto de um x da tela (bordas e ímã). */
export function bordaNoX(x: number, pxPorS: number, rolagem_s: number, fps: { num: number; den: number }): number {
  const q = Math.round(((x / pxPorS + rolagem_s) * fps.num) / fps.den);
  return Number.isFinite(q) ? Math.max(0, q) : 0;
}

// ---- dono de um tempo (clipe da principal ou livre): slots ↔ base do clipe ----

/** Mapa do trecho em slots com a tabela da MÍDIA (`campos` = 2 no desentrelace por campo: dois quadros do mapa
 *  por quadro da fonte). */
export interface MapaClipe {
  mapa: MapaQuadros;
  tab: TabelaQuadros;
  campos: 1 | 2;
}

const mapas = new WeakMap<object, { tab: TabelaQuadros; fps: string; j: string; r: MapaClipe | null }>();

const midiaDeFonte = (c: ClipePrincipal): Id | null => (c.tipo === "trecho" ? c.midia : c.tipo === "tela" ? (c.celulas[c.referencia]?.midia ?? null) : null);

/** Mapa do clipe (trecho e tela) na linha; null sem tabela. Guardado por clipe (o immer mantém o objeto que não mudou). */
export function mapaDoClipe(c: ClipePrincipal, rc: ClipeResolvido, ctx: ContextoTempo, fps: Racional): MapaClipe | null {
  const midia = midiaDeFonte(c);
  if (!midia || !rc.fonte) return null;
  const tab = ctx.tabela(midia);
  if (!tab) return null;
  const chaveFps = `${fps.num}/${fps.den}`;
  // a velocidade também (o objeto do clipe pode ter mudado no lugar fora do immer)
  const vel = (c as { velocidade?: { tipo: string; fator?: number; pontos?: unknown[] } }).velocidade;
  const chaveJ = `${rc.fonte.j_in}:${rc.fonte.j_out}:${vel?.tipo}:${vel?.tipo === "constante" ? vel.fator : JSON.stringify(vel?.pontos)}`;
  const g = mapas.get(c);
  if (g && g.tab === tab && g.fps === chaveFps && g.j === chaveJ) return g.r;
  let r: MapaClipe | null = null;
  try {
    const tabC = tabelaDoClipe(tab, c.efeitos);
    const mapa = mapaTrecho(tabC, rc.fonte.j_in, rc.fonte.j_out, (c as { velocidade: Parameters<typeof mapaTrecho>[3] }).velocidade, fps);
    r = { mapa, tab, campos: tabC === tab ? 1 : 2 };
  } catch {
    r = null;
  }
  mapas.set(c, { tab, fps: chaveFps, j: chaveJ, r });
  return r;
}

/** Quem dá a base de tempo de um intervalo na linha: um clipe da principal (fonte ou local) ou um livre (local). */
export interface Dono {
  id: Id;
  inicio_q: number;
  n_q: number;
  fps: Racional;
  base: "fonte" | "local";
  mapa: MapaClipe | null;
  /** Sem mapa (tabela ainda não chegou): conta linear pelos segundos. */
  inicio_s: Seg;
  fator: number;
  /** Congelado: o instante de fonte (ocultação da mídia vale no clipe inteiro ou em nada). */
  instante_s?: Seg;
  /** O clipe da principal, quando é um. */
  clipe: ClipePrincipal | null;
}

/** Fator constante do trecho; rampa e inválido = 1 (só a conta sem tabela usa). */
const fatorDe = (c: ClipePrincipal) => {
  const v = (c as { velocidade?: { tipo: string; fator?: number } }).velocidade;
  return v?.tipo === "constante" && Number.isFinite(v.fator) && (v.fator as number) > 0 ? (v.fator as number) : 1;
};

export function donoDoClipe(c: ClipePrincipal, rc: ClipeResolvido, ctx: ContextoTempo, fps: Racional): Dono {
  const fonte = c.tipo === "trecho" || c.tipo === "tela";
  return {
    id: c.id,
    inicio_q: rc.inicio_q,
    n_q: rc.n_q,
    fps,
    base: fonte ? "fonte" : "local",
    mapa: fonte ? mapaDoClipe(c, rc, ctx, fps) : null,
    inicio_s: fonte ? (c as { inicio_s: Seg }).inicio_s : 0,
    fator: fonte ? fatorDe(c) : 1,
    instante_s: c.tipo === "congelar" ? c.instante_s : undefined,
    clipe: c,
  };
}

/** Dono do id na linha: clipe da principal (ligado e conhecido) ou livre resolvido; null fora da linha. */
export function donoNaLinha(doc: ProjetoEdicao, linha: LinhaResolvida, id: Id, ctx: ContextoTempo = tabelasDaTela(doc)): Dono | null {
  const rc = linha.clipes.find((x) => x.id === id);
  if (rc) {
    const c = doc.principal.find((x) => (x as { id?: Id }).id === id);
    return c && clipeConhecido(c) ? donoDoClipe(c, rc, ctx, linha.fps) : null;
  }
  const l = linha.livres.find((x) => x.id === id);
  return l ? { id, inicio_q: l.inicio_q, n_q: l.n_q, fps: linha.fps, base: "local", mapa: null, inicio_s: 0, fator: 1, clipe: null } : null;
}

const EPS = 1e-9;

/** Quadro da fonte (índice da tabela da mídia) exibido no slot local `s` do dono com mapa. */
export function quadroNoSlot(m: MapaClipe, s: number): number {
  const q = m.mapa.quadro_do_slot;
  const k = Math.min(Math.max(Math.trunc(s), 0), q.length - 1);
  return Math.floor((q[k] as number) / m.campos);
}

/** Tempo na base do dono no slot LOCAL `s` (fonte: o do quadro exibido; local: s·den/num). Fora do clipe, a conta
 *  continua pela velocidade (aparar para fora). */
export function tempoDoSlot(d: Dono, s: number): Seg {
  const passo = (d.fps.den / d.fps.num) * d.fator;
  if (d.base === "local") return (s * d.fps.den) / d.fps.num;
  if (d.mapa && s >= 0 && s < d.n_q) return tempoDoQuadro(d.mapa.tab, quadroNoSlot(d.mapa, s));
  if (d.mapa) {
    // fora: a partir da ponta do mapa, na velocidade do clipe
    const ponta = s < 0 ? 0 : d.n_q - 1;
    return tempoDoQuadro(d.mapa.tab, quadroNoSlot(d.mapa, ponta)) + (s - ponta) * passo;
  }
  return d.inicio_s + s * passo;
}

/** Primeiro slot local cuja base é ≥ t (pode ser `n_q`: depois do clipe). */
function primeiroSlotDesde(d: Dono, t: Seg): number {
  if (t === -Infinity) return 0;
  if (d.base === "local") return Math.max(0, Math.ceil((t * d.fps.num) / d.fps.den - EPS));
  if (!d.mapa) return Math.max(0, Math.ceil(((t - d.inicio_s) / d.fator) * (d.fps.num / d.fps.den) - EPS));
  let lo = 0;
  let hi = d.n_q;
  while (lo < hi) {
    const m = (lo + hi) >>> 1;
    if (tempoDoSlot(d, m) >= t) hi = m;
    else lo = m + 1;
  }
  return lo;
}

/** Último slot local cuja base é ≤ t, mais um (fim exclusivo). */
function fimAte(d: Dono, t: Seg): number {
  if (t === Infinity) return d.n_q;
  if (d.base === "local") return Math.floor((t * d.fps.num) / d.fps.den + EPS) + 1;
  if (!d.mapa) return Math.floor(((t - d.inicio_s) / d.fator) * (d.fps.num / d.fps.den) + EPS) + 1;
  let lo = 0;
  let hi = d.n_q;
  while (lo < hi) {
    const m = (lo + hi) >>> 1;
    if (tempoDoSlot(d, m) <= t) lo = m + 1;
    else hi = m;
  }
  return lo;
}

/** Slot local onde a base chega a `t` (o primeiro com base ≥ t); null antes do primeiro quadro ou depois do último. */
export function slotDoTempo(d: Dono, t: Seg): number | null {
  if (!Number.isFinite(t) || d.n_q <= 0 || t < tempoDoSlot(d, 0) - EPS) return null;
  const s = primeiroSlotDesde(d, t);
  return s < d.n_q ? s : null;
}

/**
 * Intervalo na base do dono → slots do programa `[de_q, ate_q)` em que vale (`ativo` vale em [de, até] na base do
 * tempo do quadro exibido em cada slot, como `palco/cena.ts::ativoEm`); null = não vale em nenhum slot do dono.
 * `base: "fonte"` num dono de base local (ocultação da mídia num congelado): vale no clipe inteiro se o instante dele
 * está no intervalo.
 */
export function slotsNaBase(d: Dono, ativo: Intervalo | null, base: "dono" | "fonte" = "dono"): [number, number] | null {
  const de = ativo?.de ?? -Infinity;
  const ate = ativo?.ate ?? Infinity;
  if (base === "fonte" && d.base === "local") {
    const t = d.instante_s;
    return t !== undefined && t >= de && t <= ate && d.n_q > 0 ? [d.inicio_q, d.inicio_q + d.n_q] : null;
  }
  const a = Math.min(Math.max(primeiroSlotDesde(d, de), 0), d.n_q);
  const b = Math.min(Math.max(fimAte(d, ate), 0), d.n_q);
  return a < b ? [d.inicio_q + a, d.inicio_q + b] : null;
}

// ---- nomes ----

const tipoDe = (x: unknown) => String((x as { tipo?: unknown })?.tipo ?? "");

const semExtensao = (s: string) => s.replace(/\.[^.]+$/, "");

/** Nome do clipe da principal: o "Nome", senão o da mídia, senão o primeiro texto do título (o da Sequência). */
export function nomeDoClipe(c: ClipePrincipal, doc: ProjetoEdicao): string {
  if (c.rotulo) return c.rotulo;
  if (c.tipo === "trecho" || c.tipo === "congelar" || c.tipo === "imagem") {
    const m = doc.midias.find((x) => x.id === c.midia);
    return m ? semExtensao(m.nome) : "";
  }
  if (c.tipo === "titulo") return c.titulo.blocos[0]?.conteudo ?? "";
  if (c.tipo === "tela") return c.celulas.map((x) => x.rotulo).filter(Boolean).join(" · ");
  return "";
}

function nomeDoSobreposto(c: ClipeSobreposto, doc: ProjetoEdicao): string {
  if (c.rotulo) return c.rotulo;
  if (c.tipo === "titulo") return c.titulo.blocos.map((b) => b.conteudo).find((t) => t.trim() !== "") ?? "";
  const m = doc.midias.find((x) => x.id === c.midia);
  return m ? semExtensao(m.nome) : "";
}

const nomeDoRastreio = (rs: readonly Rastreio[], id: Id | null) => (id ? (rs.find((r) => r.id === id) ?? null) : null);

// ---- trilhas ----

/** Raias das barras de uma faixa: a primeira raia livre na ordem do tempo. */
function porRaias(itens: ItemNaLinha[]): number {
  const fins: number[] = [];
  const ordem = [...itens].sort((a, b) => a.de_q - b.de_q || a.ate_q - b.ate_q);
  for (const it of ordem) {
    let r = fins.findIndex((f) => f <= it.de_q);
    if (r < 0) {
      r = fins.length;
      fins.push(0);
    }
    fins[r] = it.ate_q;
    it.raia = r;
  }
  return Math.max(1, fins.length);
}

/** Posição de um sobreposto de versão futura, quando o `pos` dele se lê (para aparecer apagado onde está). */
function lugarDesconhecido(c: Desconhecido, linha: LinhaResolvida): [number, number] | null {
  const pos = c.pos as { ref?: unknown; t?: unknown; clipe?: unknown; desloc?: unknown } | undefined;
  const dur = typeof c.duracao_s === "number" && Number.isFinite(c.duracao_s) ? Math.max(1, Math.round((c.duracao_s * linha.fps.num) / linha.fps.den)) : null;
  let ini: number | null = null;
  let fim = linha.total_q;
  if (pos?.ref === "programa" && typeof pos.t === "number") ini = Math.round((pos.t * linha.fps.num) / linha.fps.den);
  else if (pos?.ref === "clipe" && typeof pos.desloc === "number") {
    const a = linha.clipes.find((x) => x.id === pos.clipe);
    if (a) {
      ini = a.inicio_q + Math.round((pos.desloc * linha.fps.num) / linha.fps.den);
      fim = a.inicio_q + a.n_q;
    }
  }
  if (ini === null || !Number.isFinite(ini)) return null;
  ini = Math.max(0, ini);
  return [ini, dur !== null ? ini + dur : Math.max(fim, ini + 1)];
}

/** As trilhas da linha, na ordem da tela, do documento e da linha resolvida (lista e linha sempre iguais). */
// CONTRATO: `ctx` opcional (tabelas das mídias); sem ele, as da tela (`palco/recursos`, as da Sequência).
export function trilhasDaLinha(doc: ProjetoEdicao, linha: LinhaResolvida, ctx: ContextoTempo = tabelasDaTela(doc)): TrilhaNaLinha[] {
  const fps = linha.fps;
  const saida: TrilhaNaLinha[] = [];
  const livres = new Map(linha.livres.map((l) => [l.id, l] as const));

  // ---- sobreposições: a de cima primeiro ----
  for (let i = doc.sobreposicoes.length - 1; i >= 0; i--) {
    const t = doc.sobreposicoes[i]!;
    const itens: ItemNaLinha[] = [];
    t.clipes.forEach((c, j) => {
      if (sobrepostoConhecido(c)) {
        const l = livres.get(c.id);
        if (!l) return;
        itens.push({ id: c.id, k: "livre", trilha: t.id, de_q: l.inicio_q, ate_q: l.inicio_q + l.n_q, rotulo: nomeDoSobreposto(c, doc), tipo: c.tipo, midia: c.tipo === "titulo" ? undefined : c.midia });
        return;
      }
      const lugar = lugarDesconhecido(c, linha);
      if (lugar) itens.push({ id: c.id ?? `?${t.id}:${j}`, k: "livre", trilha: t.id, de_q: lugar[0], ate_q: lugar[1], rotulo: tipoDe(c), tipo: tipoDe(c), apagado: true });
    });
    // maquete: "T1 Textos" (a trilha dos textos, títulos e imagens por cima do vídeo)
    saida.push({ id: t.id, tipo: "sobreposicao", nome: t.nome || "Textos", sigla: `T${i + 1}`, itens, oculta: t.oculta, indice: i });
  }

  // ---- principal (V1) ----
  const resolvidos = new Map(linha.clipes.map((c) => [c.id, c] as const));
  const donos = new Map<Id, Dono>();
  const v1: ItemNaLinha[] = [];
  let fim = 0;
  doc.principal.forEach((c, i) => {
    const id = (c as { id?: unknown }).id;
    if (!clipeConhecido(c)) {
      v1.push({ id: typeof id === "string" ? id : `?${i}`, k: "clipe", trilha: TRILHA_PRINCIPAL, de_q: fim, ate_q: fim, rotulo: tipoDe(c), tipo: tipoDe(c), apagado: true });
      return;
    }
    const rc = resolvidos.get(c.id);
    if (!rc) {
      v1.push({ id: c.id, k: "clipe", trilha: TRILHA_PRINCIPAL, de_q: fim, ate_q: fim, rotulo: nomeDoClipe(c, doc), tipo: c.tipo, desligado: true });
      return;
    }
    donos.set(c.id, donoDoClipe(c, rc, ctx, fps));
    const t = c.transicao_entrada;
    v1.push({
      id: c.id,
      k: "clipe",
      trilha: TRILHA_PRINCIPAL,
      de_q: rc.inicio_q,
      ate_q: rc.inicio_q + rc.n_q,
      rotulo: nomeDoClipe(c, doc),
      tipo: c.tipo,
      midia: c.tipo === "trecho" || c.tipo === "congelar" || c.tipo === "imagem" ? c.midia : (midiaDeFonte(c) ?? undefined),
      ...(t ? { entrada: { tipo: t.tipo, sobrepoe_q: rc.sobrepoe_q } } : {}),
    });
    fim = rc.inicio_q + rc.n_q;
  });

  // ---- faixas de efeito (ordem da primeira aparição na linha) ----
  const faixas = new Map<string, TrilhaNaLinha>();
  const naFaixa = (e: Efeito | Desconhecido, item: Omit<ItemNaLinha, "trilha">) => {
    const chave = chaveDaFaixaDe(e, doc);
    let f = faixas.get(chave);
    if (!f) {
      const conhecido = efeitoConhecido(e);
      const r = conhecido && !SO_PELO_TIPO.has(e.tipo) ? nomeDoRastreio(doc.rastreios, rastreioSeguido(e)) : null;
      const cor = r?.cor ?? (conhecido ? COR_DA_FAMILIA[FAMILIA[e.tipo] ?? "anotacao"] : COR_DA_FAMILIA.desconhecido);
      f = { id: `faixa:${chave}`, tipo: "efeito", nome: nomeDaFaixa(e, doc), cor, itens: [] };
      faixas.set(chave, f);
    }
    f.itens.push({ ...item, trilha: f.id });
  };
  const barra = (e: Efeito | Desconhecido, d: Dono, base: "dono" | "fonte", extra: Partial<ItemNaLinha>) => {
    if (!temFaixa(e)) return;
    const conhecido = efeitoConhecido(e);
    const ativo = conhecido ? e.ativo : ((e as { ativo?: Intervalo | null }).ativo ?? null);
    const at = ativo && typeof ativo === "object" ? ativo : null;
    const s = slotsNaBase(d, at, base);
    if (!s) return;
    // ocultação da mídia que passa da borda do clipe (vale também nos vizinhos da mesma mídia): a barra está cortada
    const cortado = base === "fonte" ? { de: s[0] === d.inicio_q && passaAntes(d, at?.de ?? null), ate: s[1] === d.inicio_q + d.n_q && passaDepois(d, at?.ate ?? null) } : null;
    const chaves = conhecido ? chavesEmSlots(doc, e, d) : [];
    naFaixa(e, {
      id: typeof (e as { id?: unknown }).id === "string" ? (e as { id: Id }).id : `?${d.id}:${tipoDe(e)}`,
      k: "efeito",
      de_q: s[0],
      ate_q: s[1],
      rotulo: conhecido ? e.nome || nomeDoRastreio(doc.rastreios, rastreioSeguido(e))?.nome || nomeDoTipo(e.tipo) : tipoDe(e),
      clipe: d.id,
      tipo: tipoDe(e),
      ...(chaves.length ? { chaves } : {}),
      ...(conhecido ? (e.ligado ? {} : { desligado: true }) : { apagado: true }),
      ...(cortado && (cortado.de || cortado.ate) ? { cortado } : {}),
      ...extra,
    });
  };
  for (const rc of linha.clipes) {
    const d = donos.get(rc.id);
    const c = d?.clipe;
    if (!d || !c) continue;
    for (const e of c.efeitos) barra(e, d, "dono", {});
    if (c.tipo === "trecho" || c.tipo === "congelar") {
      const m = doc.midias.find((x) => x.id === c.midia);
      for (const o of m?.ocultacoes ?? []) barra(o, d, "fonte", { daMidia: true });
    }
  }
  for (const t of doc.sobreposicoes)
    for (const c of t.clipes) {
      if (!sobrepostoConhecido(c) || c.efeitos.length === 0) continue;
      const l = livres.get(c.id);
      if (!l) continue;
      const d: Dono = { id: c.id, inicio_q: l.inicio_q, n_q: l.n_q, fps, base: "local", mapa: null, inicio_s: 0, fator: 1, clipe: null };
      for (const e of c.efeitos) barra(e, d, "dono", {});
    }
  // a barra de um efeito que segue no clipe seguinte (o corte duplicou o efeito; a mesma pessoa na outra câmera) é uma
  // barra só, como na maquete
  const limites = new Map<Id, [number, number]>([...donos.values()].map((d) => [d.id, [d.inicio_q, d.inicio_q + d.n_q]] as const));
  for (const f of faixas.values()) {
    f.itens = emendarBarras(f.itens, limites);
    f.raias = porRaias(f.itens);
  }

  saida.push({ id: TRILHA_PRINCIPAL, tipo: "principal", nome: "Vídeo", sigla: "V1", itens: v1 });
  saida.push(...faixas.values());

  // ---- som dos trechos, na posição deles ----
  const som: ItemNaLinha[] = [];
  for (const rc of linha.clipes) {
    const c = donos.get(rc.id)?.clipe;
    if (!c || (c.tipo !== "trecho" && c.tipo !== "tela")) continue;
    const midia = midiaDeFonte(c);
    const m = midia ? doc.midias.find((x) => x.id === midia) : null;
    if (!m?.tem_audio) continue;
    som.push({ id: c.id, k: "clipe", trilha: TRILHA_SOM, de_q: rc.inicio_q, ate_q: rc.inicio_q + rc.n_q, rotulo: nomeDoClipe(c, doc), tipo: c.tipo, midia: m.id, ...(c.audio.mudo ? { mudo: true } : {}) });
  }
  // maquete: "A1 Som" (o som dos trechos); as trilhas de áudio do documento seguem como A2, A3…
  saida.push({ id: TRILHA_SOM, tipo: "som", nome: "Som", sigla: "A1", itens: som });

  // ---- áudio, na ordem do documento ----
  doc.audios.forEach((t, i) => {
    const itens: ItemNaLinha[] = [];
    for (const c of t.clipes) {
      const l = livres.get(c.id);
      if (!l) continue;
      const m = doc.midias.find((x) => x.id === c.midia);
      itens.push({ id: c.id, k: "audio", trilha: t.id, de_q: l.inicio_q, ate_q: l.inicio_q + l.n_q, rotulo: c.nome || (m ? semExtensao(m.nome) : ""), tipo: t.papel, midia: c.midia });
    }
    saida.push({ id: t.id, tipo: "audio", nome: PAPEL[t.papel] ?? t.papel, sigla: `A${i + 2}`, itens, muda: t.mudo, indice: i });
  });

  // ---- legendas ----
  const legendas: ItemNaLinha[] = [];
  for (const l of doc.legendas.itens) {
    if (l.ref.tipo === "programa") {
      const a = Math.round((l.ref.de * fps.num) / fps.den);
      const b = Math.max(a + 1, Math.round((l.ref.ate * fps.num) / fps.den));
      if (Number.isFinite(a) && Number.isFinite(b)) legendas.push({ id: l.id, k: "legenda", trilha: TRILHA_LEGENDAS, de_q: Math.max(0, a), ate_q: b, rotulo: l.texto });
      continue;
    }
    const ref = l.ref;
    for (const rc of linha.clipes) {
      const d = donos.get(rc.id);
      const c = d?.clipe;
      if (!d || !c || c.tipo !== "trecho" || c.midia !== ref.midia) continue;
      const s = slotsNaBase(d, { de: ref.de, ate: ref.ate });
      if (s) legendas.push({ id: l.id, k: "legenda", trilha: TRILHA_LEGENDAS, de_q: s[0], ate_q: s[1], rotulo: l.texto, clipe: c.id });
    }
  }
  // sem legenda no projeto, a trilha não ocupa a altura do painel (a maquete não tem)
  if (doc.legendas.itens.length > 0) saida.push({ id: TRILHA_LEGENDAS, tipo: "legendas", nome: "Legendas", itens: legendas });
  return saida;
}

/**
 * Barras da mesma faixa que se tocam numa emenda de clipe viram uma: a primeira acaba no fim do clipe dela e a
 * seguinte começa no começo do clipe seguinte (na transição com sobreposição, dentro dela). Mesma situação (ligado,
 * apagado) e nenhuma da versão futura. As partes ficam no item para as bordas e a seleção.
 */
export function emendarBarras(itens: ItemNaLinha[], limites: ReadonlyMap<Id, readonly [number, number]>): ItemNaLinha[] {
  const ordem = [...itens].sort((a, b) => a.de_q - b.de_q || a.ate_q - b.ate_q);
  const r: ItemNaLinha[] = [];
  const parte = (it: ItemNaLinha): ParteDaBarra => ({ id: it.id, clipe: it.clipe as Id, de_q: it.de_q, ate_q: it.ate_q, ...(it.daMidia ? { daMidia: true } : {}) });
  const li = (it: ItemNaLinha) => (it.clipe ? limites.get(it.clipe) : undefined);
  for (const it of ordem) {
    const lit = li(it);
    let ant: ItemNaLinha | null = null;
    // a barra que acaba no fim do clipe de antes, na emenda em que esta começa
    for (let k = r.length - 1; k >= 0 && lit && !it.apagado && it.de_q === lit[0]; k--) {
      const x = r[k]!;
      const u = x.partes ? x.partes[x.partes.length - 1]! : parte(x);
      const lu = limites.get(u.clipe);
      if (!x.apagado && x.tipo === it.tipo && !x.desligado === !it.desligado && u.clipe !== it.clipe && lu && u.ate_q === lu[1] && lit[0] <= lu[1] && lit[0] >= lu[0]) {
        ant = x;
        break;
      }
    }
    if (!ant) {
      r.push(it);
      continue;
    }
    const partes = [...(ant.partes ?? [parte(ant)]), parte(it)];
    const chaves = [...new Set([...(ant.chaves ?? []), ...(it.chaves ?? [])])];
    const cortado = { de: !!ant.cortado?.de, ate: !!it.cortado?.ate };
    const junta: ItemNaLinha = { ...ant, ate_q: Math.max(ant.ate_q, it.ate_q), partes, ...(chaves.length ? { chaves } : {}) };
    if (cortado.de || cortado.ate) junta.cortado = cortado;
    else delete junta.cortado;
    if (ant.erro || it.erro) junta.erro = true;
    r[r.indexOf(ant)] = junta;
  }
  return r;
}

/** O `ativo` (tempo de fonte) começa antes do primeiro quadro do clipe? Nulo = desde o começo da mídia. No congelado,
 *  a barra é o clipe inteiro: os dois lados cortados. */
function passaAntes(d: Dono, de: Seg | null): boolean {
  if (d.base === "local") return true;
  return de === null || de < tempoDoSlot(d, 0) - EPS;
}

function passaDepois(d: Dono, ate: Seg | null): boolean {
  if (d.base === "local") return true;
  return ate === null || ate > tempoDoSlot(d, d.n_q - 1) + EPS;
}

/** Instantes (base do clipe) dos quadros-chave: chaves da região do enquadramento e âncoras do rastreio seguido. */
export function instantesDasChaves(doc: ProjetoEdicao, e: Efeito): Seg[] {
  const ts: Seg[] = [];
  if (e.tipo === "enquadramento" && Array.isArray(e.regiao)) for (const k of e.regiao) ts.push(k.t);
  const rid = rastreioSeguido(e);
  const r = rid ? doc.rastreios.find((x) => x.id === rid) : null;
  if (r) for (const a of r.ancoras) ts.push(a.t);
  return [...new Set(ts.filter((t) => Number.isFinite(t)))].sort((a, b) => a - b);
}

/** Losangos da barra em slots do programa, só os que caem no clipe. Rastreio de base "fonte" num congelado: nenhum. */
function chavesEmSlots(doc: ProjetoEdicao, e: Efeito, d: Dono): number[] {
  if (d.instante_s !== undefined) return [];
  const qs: number[] = [];
  for (const t of instantesDasChaves(doc, e)) {
    const s = slotDoTempo(d, t);
    if (s !== null) qs.push(d.inicio_q + s);
  }
  return [...new Set(qs)];
}

/** Marcadores na régua (slot do programa): do programa, presos a clipe, ou num instante de fonte (um por trecho
 *  que mostra esse instante). */
export function marcadoresNaLinha(doc: ProjetoEdicao, linha: LinhaResolvida, ctx: ContextoTempo = tabelasDaTela(doc)): MarcadorNaLinha[] {
  const fps = linha.fps;
  const r: MarcadorNaLinha[] = [];
  const arred = (s: Seg) => {
    const v = (s * fps.num) / fps.den;
    return v < 0 ? -Math.round(-v) : Math.round(v);
  };
  for (const m of doc.marcadores) {
    const p = m.pos;
    if (p.ref === "programa") r.push({ id: m.id, q: Math.max(0, arred(p.t)), cor: m.cor, rotulo: m.rotulo });
    else if (p.ref === "clipe") {
      const a = linha.clipes.find((x) => x.id === p.clipe);
      if (a) r.push({ id: m.id, q: Math.max(0, a.inicio_q + arred(p.desloc)), cor: m.cor, rotulo: m.rotulo });
    } else {
      for (const rc of linha.clipes) {
        const c = doc.principal.find((x) => (x as { id?: Id }).id === rc.id);
        if (!c || !clipeConhecido(c) || c.tipo !== "trecho" || c.midia !== p.midia) continue;
        const s = slotDoTempo(donoDoClipe(c, rc, ctx, fps), p.t);
        if (s !== null) r.push({ id: m.id, q: rc.inicio_q + s, cor: m.cor, rotulo: m.rotulo });
      }
    }
  }
  return r;
}

/** Item das trilhas por id (o primeiro; a ocultação da mídia aparece uma vez por clipe: `clipe` escolhe). */
export function acharItem(trilhas: readonly TrilhaNaLinha[], id: Id, clipe?: Id): ItemNaLinha | null {
  for (const t of trilhas) for (const it of t.itens) if (it.id === id && (clipe === undefined || it.clipe === clipe)) return it;
  return null;
}
