/**
 * O que o palco desenha num instante do clipe (§7.4, §8.3): `QuadroPalco` montado do documento e as primitivas
 * das ocultações da mídia e dos efeitos ligados e ativos do clipe, nessa ordem. Erro de um efeito (Fase 1,
 * `nao_suportado`; dado inválido) não derruba os outros: vai na lista `erros` (ícone de estado no inspetor).
 */

import type {
  AtlasCarimbo,
  Caixa,
  ClipePrincipal,
  Desconhecido,
  Efeito,
  EstiloTexto,
  Id,
  Intervalo,
  PedidoSprite,
  Primitiva,
  ProjetoEdicao,
  Pts,
  QuadroPalco,
  Rastreio,
  Seg,
  SpriteTexto,
} from "@domain/edicao";
import { acharMidia, acharRastreio, efeitoConhecido } from "../modelo/projeto";
import { aDe, type Afim, type Tamanho } from "../modelo/geometria";
import { avaliar, emAusencia } from "../modelo/rastreio";
import { densoEmCache } from "../store/rastreioStore";
import { primitivas } from "./desenho/primitivas";

/** O que vem do backend ou do disco; null = ainda não chegou. */
export interface FontesCena {
  sprite(p: PedidoSprite): SpriteTexto | null;
  atlas(e: EstiloTexto, alturaCanvas: number): AtlasCarimbo | null;
  /** Tamanho da imagem (px) de uma `MidiaRef`. */
  midia(id: Id): { w: number; h: number } | null;
}

/** Onde o cursor está: tempo na base do clipe (fonte ou local), quadro da tabela e slot do programa. */
export interface Instante {
  t: Seg;
  pts: Pts | null;
  indice: number | null;
  q: number;
  programa_s: Seg;
}

export interface Cena {
  quadro: QuadroPalco;
  primitivas: Primitiva[];
  erros: { efeito: Id; codigo: string }[];
}

/** `ativo` vale em [de, até] na base do clipe; campo nulo = borda do clipe. */
export function ativoEm(ativo: Intervalo | null, t: Seg, borda: [Seg, Seg]): boolean {
  const de = ativo?.de ?? borda[0];
  const ate = ativo?.ate ?? borda[1];
  return t >= de && t <= ate;
}

/** Bordas do clipe na base dele: fonte no trecho; local nos demais. */
export function bordasDoClipe(c: ClipePrincipal): [Seg, Seg] {
  switch (c.tipo) {
    case "trecho":
    case "tela":
      return [c.inicio_s, c.fim_s];
    case "congelar":
      return [0, c.duracao_s];
    default:
      return [0, c.duracao_s];
  }
}

/** Duração na base local (clipe sem fonte); trecho e tela pela faixa de fonte. */
export function duracaoLocal(c: ClipePrincipal): Seg {
  return c.tipo === "trecho" || c.tipo === "tela" ? c.fim_s - c.inicio_s : c.duracao_s;
}

/** Mídia de vídeo do clipe (trecho e congelar). */
export function midiaDoClipe(c: ClipePrincipal): Id | null {
  if (c.tipo === "trecho" || c.tipo === "congelar") return c.midia;
  return null;
}

/** Par (t, pts) que uma chave de EFEITO grava no instante: pts só no trecho (chave presa a quadro de mídia); nos
 *  clipes de base local (congelar, imagem, título) o tempo é local e todo instante teria o mesmo pts. */
export function instanteDaChave(c: ClipePrincipal, inst: Pick<Instante, "t" | "pts">): { t: Seg; pts: Pts | null } {
  return c.tipo === "trecho" || c.tipo === "tela" ? { t: inst.t, pts: inst.pts } : { t: inst.t, pts: null };
}

/** Par (t, pts) que uma ÂNCORA de rastreio grava no instante (§3.1): base "fonte" num congelado = o instante dele
 *  (é onde o rastreio é amostrado); base "local" = tempo local, sem pts. */
export function instanteDaAncora(c: ClipePrincipal, base: Rastreio["base"], inst: Pick<Instante, "t" | "pts">): { t: Seg; pts: Pts | null } {
  if (c.tipo === "trecho" || c.tipo === "tela") return { t: inst.t, pts: inst.pts };
  if (c.tipo === "congelar" && base === "fonte") return { t: c.instante_s, pts: c.instante_pts };
  return { t: inst.t, pts: null };
}

/** `fonte` = onde amostrar rastreio de base "fonte" (no congelado, o instante dele); sem ele, o próprio instante. */
export function quadroDaCena(
  doc: ProjetoEdicao,
  efeitos: readonly (Efeito | Desconhecido)[],
  inst: Instante,
  origem: Tamanho,
  fontes: FontesCena,
  A?: Afim,
  fonte?: { t: Seg; pts: Pts | null },
): QuadroPalco {
  const canvas = { w: doc.saida.largura, h: doc.saida.altura };
  return {
    q: inst.q,
    t: inst.t,
    pts: inst.pts,
    indice: inst.indice,
    programa_s: inst.programa_s,
    A: A ?? aDe(efeitos, inst.t, origem, canvas),
    canvas,
    rastreio: (id: Id): Caixa | null => {
      const r = acharRastreio(doc, id);
      if (!r) return null;
      const em = r.base === "fonte" && fonte ? fonte : { t: inst.t, pts: inst.pts };
      return avaliar(r, em, densoEmCache(r))?.caixa ?? null;
    },
    ausente: (id: Id): boolean => {
      const r = acharRastreio(doc, id);
      if (!r) return false;
      const em = r.base === "fonte" && fonte ? fonte : { t: inst.t, pts: inst.pts };
      return emAusencia(r, em.t) && !avaliar(r, em, densoEmCache(r))?.caixa;
    },
    sprite: (p) => fontes.sprite(p),
    atlas: (e, h) => fontes.atlas(e, h),
    midia: (id) => fontes.midia(id),
  };
}

const codigoDe = (e: unknown) => String((e as Error)?.message ?? e).split(":")[0]!.trim();

/** Primitivas de uma lista de efeitos no quadro (só ligados e ativos). */
function juntar(lista: readonly (Efeito | Desconhecido)[], q: QuadroPalco, borda: [Seg, Seg], saida: Primitiva[], erros: Cena["erros"]) {
  for (const e of lista) {
    if (!efeitoConhecido(e) || !e.ligado || !ativoEm(e.ativo, q.t, borda)) continue;
    try {
      saida.push(...primitivas(e, q));
    } catch (x) {
      erros.push({ efeito: e.id, codigo: codigoDe(x) });
    }
  }
}

/** Cena de um clipe da principal no instante. `origem` = tamanho exibido da mídia (ou do canvas, sem mídia). */
export function cenaDoClipe(doc: ProjetoEdicao, c: ClipePrincipal, inst: Instante, origem: Tamanho, fontes: FontesCena, A?: Afim): Cena {
  const fonte = c.tipo === "congelar" ? { t: c.instante_s, pts: c.instante_pts } : undefined;
  const quadro = quadroDaCena(doc, c.efeitos, inst, origem, fontes, A, fonte);
  const saida: Primitiva[] = [];
  const erros: Cena["erros"] = [];
  const m = midiaDoClipe(c);
  const ref = m ? acharMidia(doc, m) : null;
  // ocultações da mídia: tempo de fonte, valem em todo uso dela (D17); no congelado, o instante dele
  if (ref) {
    const qf = fonte ? { ...quadro, t: fonte.t, pts: fonte.pts } : quadro;
    juntar(ref.ocultacoes, qf, [-Infinity, Infinity], saida, erros);
  }
  juntar(c.efeitos, quadro, bordasDoClipe(c), saida, erros);
  return { quadro, primitivas: saida, erros };
}
