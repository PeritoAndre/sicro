/**
 * Mudanças que o Inspetor faz no documento, todas pela store (`alterar`/`iniciarGesto`): um gesto ou uma
 * mudança de campo = um passo, com o rótulo do campo (§8.6). Valor inválido não chega aqui (os campos filtram).
 */

import { produce, type Draft } from "immer";
import { commands } from "@core/commands";
import type {
  Caixa,
  Chave,
  Lugar,
  PedidoMarcacao,
  Ponto,
  ClipePrincipal,
  ClipeCongelar,
  ClipeSobreposto,
  ClipeTrecho,
  Desconhecido,
  Efeito,
  EfEnquadramento,
  EfGirar,
  EfOcultar,
  Id,
  Intervalo,
  MidiaRef,
  Posicao,
  ProjetoEdicao,
  Pts,
  Seg,
  Transicao,
} from "@domain/edicao";
import { emSerie, useEdicaoStore, type Gesto } from "../store/edicaoStore";
import { acharClipe, acharEfeito, clipeConhecido, comoChegou, completar, efeitoConhecido, garantirMidia, porChave, sobrepostoConhecido, tirarChave } from "../modelo/projeto";
import { ehChaves } from "../modelo/curva";
import { exibido, ptsDaTabela, tempoDoQuadro } from "../modelo/tempo";
import { recursoDe } from "../palco/recursos";
import {
  CAIXA_CENTRO,
  novaTrilhaNarracao,
  novoCarimbo,
  novoClipeAudio,
  novoDestaque,
  novoEnquadramento,
  novoGirar,
  novoImagem,
  novoOcultar,
  novoRastreio,
  novoTexto,
} from "./novos";

const st = () => useEdicaoStore.getState();
type Mut = (d: Draft<ProjetoEdicao>) => void;

/** Uma mudança = um passo; `agrupar` junta tecla repetida e campo numérico (mesma chave em menos de 1 s). */
export function mudar(rotulo: string, f: Mut, agrupar?: string): void {
  st().alterar(rotulo, f, agrupar ? { agrupar } : undefined);
}

/** Muda pelo gesto quando há um (arrasto, digitação), senão um passo. */
export function mudarCom(rotulo: string, f: Mut, op?: { gesto?: Gesto | null; agrupar?: string }): void {
  if (op?.gesto) op.gesto.alterar(f);
  else mudar(rotulo, f, op?.agrupar);
}

export type ClipeQualquer = ClipePrincipal | ClipeSobreposto;

export function noClipe<C extends ClipeQualquer = ClipeQualquer>(id: Id, f: (c: Draft<C>, d: Draft<ProjetoEdicao>) => void): Mut {
  return (d) => {
    const a = acharClipe(d, id);
    if (a) f(a.clipe as Draft<C>, d);
  };
}

export function noEfeito<E extends Efeito = Efeito>(id: Id, f: (e: Draft<E>, d: Draft<ProjetoEdicao>) => void): Mut {
  return (d) => {
    const a = acharEfeito(d, id);
    if (a) f(a.efeito as Draft<E>, d);
  };
}

// ---- tempo do cursor no clipe ----

/** Tempo do efeito no cursor (fonte em trecho, local nos outros); fora do clipe, o começo dele. */
export function tempoNoClipe(c: ClipeQualquer): Seg {
  const cur = st().cursor;
  if (cur.modo === "clipe" && cur.clipe === c.id) return cur.t;
  return c.tipo === "trecho" ? c.inicio_s : c.tipo === "congelar" ? c.instante_s : 0;
}

/** Mídia de vídeo do clipe (trecho, congelar), quando há. */
export const midiaDoClipe = (c: ClipeQualquer): Id | null => (c.tipo === "trecho" || c.tipo === "congelar" ? c.midia : null);

// ---- efeitos ----

export type TipoAdicionar = "destaque" | "ocultar" | "texto" | "carimbo" | "imagem";

/** Quadro mostrado no congelado: tempo e pts dele pela tabela da mídia (a do palco); sem tabela, o do clipe. */
function quadroDoCongelado(c: ClipeCongelar): { t: Seg; pts: Pts | null } {
  const doc = st().doc;
  const tab = doc ? recursoDe(doc.midias.find((m) => m.id === c.midia)?.sha256).tabela : null;
  if (!tab) return { t: c.instante_s, pts: c.instante_pts };
  const j = exibido(tab, c.instante_s);
  return { t: tempoDoQuadro(tab, j), pts: ptsDaTabela(tab)[j] as number };
}

/** Quadro mostrado no tempo `t` do trecho, com o pts: a âncora nasce igual à que o palco marca nesse quadro. */
function quadroDoTrecho(c: ClipeTrecho, t: Seg): { t: Seg; pts: Pts | null } | null {
  const doc = st().doc;
  const tab = doc ? recursoDe(doc.midias.find((m) => m.id === c.midia)?.sha256).tabela : null;
  if (!tab) return null;
  const j = exibido(tab, t);
  return { t: tempoDoQuadro(tab, j), pts: ptsDaTabela(tab)[j] as number };
}

/** Instante do 1º quadro do trecho (o `ativo` é intervalo fechado no instante do quadro). */
function inicioDoTrecho(c: ClipeTrecho): Seg {
  return quadroDoTrecho(c, c.inicio_s)?.t ?? c.inicio_s;
}

/** "Adicionar → …" no clipe. Ocultar de trecho vai para a mídia (vale em todo uso dela, D17), com `ativo` = faixa
 *  do clipe em tempo de fonte; de quadro congelado também, com `ativo` no instante do quadro (contratos (h) 23);
 *  nos outros clipes fica no clipe. Devolve o id do efeito, já selecionado. */
export function adicionarEfeito(clipeId: Id, tipo: Exclude<TipoAdicionar, "imagem">, rotulo: string): Id | null {
  const c = clipeAtual(clipeId);
  if (!c) return null;
  const t = tempoNoClipe(c);
  const midia = midiaDoClipe(c);
  let novoEf: Efeito;
  const extra: Mut[] = [];
  if (tipo === "destaque" || tipo === "ocultar") {
    // congelado: rastreio de base fonte amostrado no instante dele (não no tempo local do cursor)
    const q = c.tipo === "congelar" ? quadroDoCongelado(c) : null;
    const k = q ?? (c.tipo === "trecho" ? quadroDoTrecho(c, t) : null);
    const r = novoRastreio(midia, k ? k.t : t, CAIXA_CENTRO);
    if (k) r.ancoras = [{ t: k.t, pts: k.pts, v: CAIXA_CENTRO }];
    extra.push((d) => void d.rastreios.push(r));
    if (tipo === "destaque") novoEf = novoDestaque(r.id);
    else if (c.tipo === "trecho" || q) {
      const m = midia as Id;
      const ef = novoOcultar(r.id, q ? { de: q.t, ate: q.t } : { de: inicioDoTrecho(c as ClipeTrecho), ate: (c as ClipeTrecho).fim_s });
      mudar(rotulo, (d) => {
        d.rastreios.push(r);
        d.midias.find((x) => x.id === m)?.ocultacoes.push(ef);
      });
      selecionarEfeito(ef.id);
      return ef.id;
    } else novoEf = novoOcultar(r.id);
  } else novoEf = tipo === "texto" ? novoTexto() : novoCarimbo();
  const ef = novoEf;
  mudar(rotulo, (d) => {
    for (const f of extra) f(d);
    const a = acharClipe(d, clipeId);
    a?.clipe.efeitos.push(ef);
  });
  selecionarEfeito(ef.id);
  return ef.id;
}

/** "Imagem" sobre o vídeo: a mídia vem de `edicao_midia_importar`. */
export function adicionarImagem(clipeId: Id, midia: MidiaRef, rotulo: string): Id {
  const ef = novoImagem(midia.id);
  mudar(rotulo, (d) => {
    const id = garantirMidia(d, midia);
    ef.midia = id;
    acharClipe(d, clipeId)?.clipe.efeitos.push({ ...ef, midia: id });
  });
  selecionarEfeito(ef.id);
  return ef.id;
}

/** Lista e posição do efeito `id`, também de versão futura (que `acharEfeito` não devolve). */
function acharQualquerEfeito(d: Draft<ProjetoEdicao>, id: Id): { lista: unknown[]; indice: number } | null {
  const em = (l: unknown) => {
    if (!Array.isArray(l)) return null;
    const i = l.findIndex((e) => typeof e === "object" && e !== null && (e as { id?: unknown }).id === id);
    return i >= 0 ? { lista: l as unknown[], indice: i } : null;
  };
  for (const c of d.principal) {
    if (!clipeConhecido(c)) continue;
    const r = em(c.efeitos) ?? (c.tipo === "tela" ? c.celulas.map((x) => em(x.efeitos)).find((x) => x) : null);
    if (r) return r;
  }
  for (const t of d.sobreposicoes) for (const c of t.clipes) if (sobrepostoConhecido(c)) {
    const r = em(c.efeitos);
    if (r) return r;
  }
  for (const m of d.midias) {
    const r = em(m.ocultacoes);
    if (r) return r;
  }
  return null;
}

/** Tira o efeito (do clipe ou da mídia) e o rastreio que só ele usava. Efeito de versão futura também sai:
 *  ligado, ele é `item_desconhecido` e barra a exportação. */
export function removerEfeito(efeitoId: Id, rotulo: string): void {
  mudar(rotulo, (d) => {
    const a = acharEfeito(d, efeitoId);
    if (!a) {
      const x = acharQualquerEfeito(d, efeitoId);
      x?.lista.splice(x.indice, 1);
      return;
    }
    a.lista.splice(a.indice, 1);
    const r = rastreioDe(a.efeito);
    if (r && !usaRastreio(d, r)) d.rastreios = d.rastreios.filter((x) => x.id !== r);
  });
  const s = st().selecao;
  if (s.efeito === efeitoId) st().selecionar({ ...s, efeito: null });
}

// `?.`: efeito de tipo conhecido com a forma errada (item_invalido) também passa por aqui
export const rastreioDe = (e: Efeito): Id | null =>
  e.tipo === "destaque" || e.tipo === "lupa"
    ? (e.rastreio ?? null)
    : e.tipo === "ocultar" || e.tipo === "holofote"
      ? (e.alvo?.rastreio ?? null)
      : e.tipo === "velocidade_exibida" && e.onde?.modo === "segue"
        ? (e.onde.rastreio ?? null)
        : null;

function usaRastreio(d: ProjetoEdicao, r: Id): boolean {
  const em = (l: (Efeito | Desconhecido)[]) => l.some((e) => efeitoConhecido(e) && rastreioDe(e) === r);
  for (const c of d.principal) if ("efeitos" in c && Array.isArray(c.efeitos) && em(c.efeitos as (Efeito | Desconhecido)[])) return true;
  for (const t of d.sobreposicoes) for (const c of t.clipes) if (Array.isArray(c.efeitos) && em(c.efeitos as (Efeito | Desconhecido)[])) return true;
  return d.midias.some((m) => em(m.ocultacoes));
}

/** "Só neste clipe": a ocultação passa da mídia para o clipe (tempo de fonte nos dois, trecho) ou volta. */
export function soNesteClipe(efeitoId: Id, clipeId: Id, sim: boolean, rotulo: string): void {
  mudar(rotulo, (d) => {
    const a = acharEfeito(d, efeitoId);
    const c = acharClipe(d, clipeId)?.clipe;
    if (!a || !c || a.efeito.tipo !== "ocultar" || c.tipo !== "trecho") return;
    const m = d.midias.find((x) => x.id === c.midia);
    if (!m) return;
    const ef = a.efeito as Draft<EfOcultar>;
    if (sim && a.midia) {
      a.lista.splice(a.indice, 1);
      c.efeitos.push(ef);
    } else if (!sim && a.clipe) {
      a.lista.splice(a.indice, 1);
      // da mídia: `ativo` obrigatório, em tempo de fonte
      ef.ativo = { de: ef.ativo?.de ?? inicioDoTrecho(c as ClipeTrecho), ate: ef.ativo?.ate ?? c.fim_s };
      m.ocultacoes.push(ef);
    }
  });
}

/** Efeitos que mudam o espaço de origem do clipe (§3.2): ligar, desligar ou tirar invalida o que foi marcado nele. */
export const MUDA_ESPACO = new Set(["estabilizar", "lente"]);

/** Marcações feitas no espaço de origem do clipe: âncoras dos rastreios das anotações e regiões (holofote, destaque,
 *  lupa, régua, velocidade) e as chaves da região do enquadramento. A ocultação mora no espaço original: fica fora. */
export function marcacoesNoEspaco(d: ProjetoEdicao, c: ClipeQualquer): number {
  let n = 0;
  const vistos = new Set<Id>();
  for (const e of c.efeitos) {
    if (!efeitoConhecido(e) || e.tipo === "ocultar") continue;
    if (e.tipo === "enquadramento") {
      const r = e.regiao as Caixa | Chave<Caixa>[];
      n += ehChaves(r) ? r.length : r.x === 0 && r.y === 0 && r.w === 1 && r.h === 1 ? 0 : 1;
      continue;
    }
    const rid = rastreioDe(e);
    if (!rid || vistos.has(rid)) continue;
    vistos.add(rid);
    n += d.rastreios.find((x) => x.id === rid)?.ancoras.length ?? 0;
  }
  return n;
}

export function ligarEfeito(efeitoId: Id, ligado: boolean, rotulo: string): void {
  mudar(rotulo, noEfeito(efeitoId, (e) => void (e.ligado = ligado)));
}

/** "De"/"Até" na base do clipe. Os dois nulos = o clipe inteiro (`ativo` nulo), salvo na mídia, onde é obrigatório. */
export function mudarAtivo(efeitoId: Id, campo: "de" | "ate", t: Seg | null, rotulo: string): void {
  mudar(rotulo, (d) => {
    const a = acharEfeito(d, efeitoId);
    if (!a) return;
    const at: Intervalo = { de: a.efeito.ativo?.de ?? null, ate: a.efeito.ativo?.ate ?? null };
    at[campo] = t;
    a.efeito.ativo = at.de == null && at.ate == null && !a.midia ? null : at;
  });
}

/** Âncora marcada no palco ("Marcar"): substitui a do mesmo quadro ou instante. */
export function marcarAncora(rastreioId: Id, ancora: Chave<Caixa>, rotulo: string): void {
  mudar(rotulo, (d) => {
    const r = d.rastreios.find((x) => x.id === rastreioId);
    if (r) r.ancoras = porChave<Caixa>(r.ancoras, ancora);
  });
}

// ---- enquadramento e girar (valem no clipe inteiro; vale o primeiro ligado) ----

export function enquadramentoDe(c: ClipeQualquer): EfEnquadramento | null {
  return (c.efeitos.find((e) => efeitoConhecido(e) && e.tipo === "enquadramento") as EfEnquadramento | undefined) ?? null;
}
export function girarDe(c: ClipeQualquer): EfGirar | null {
  return (c.efeitos.find((e) => efeitoConhecido(e) && e.tipo === "girar") as EfGirar | undefined) ?? null;
}

/** Giro e espelhos; 0° sem espelho tira o efeito. */
export const giroEm = (clipeId: Id, campos: Partial<Pick<EfGirar, "graus" | "espelhar_h" | "espelhar_v">>): Mut =>
  noClipe(clipeId, (c) => {
    const i = c.efeitos.findIndex((e) => efeitoConhecido(e as Efeito) && (e as Efeito).tipo === "girar");
    const atual = i >= 0 ? (c.efeitos[i] as EfGirar) : novoGirar(0);
    const novo = { ...atual, ...campos };
    if (novo.graus === 0 && !novo.espelhar_h && !novo.espelhar_v) {
      if (i >= 0) c.efeitos.splice(i, 1);
    } else if (i >= 0) Object.assign(c.efeitos[i] as EfGirar, campos);
    else c.efeitos.push(novo);
  });

export function mudarGiro(clipeId: Id, campos: Partial<Pick<EfGirar, "graus" | "espelhar_h" | "espelhar_v">>, rotulo: string): void {
  mudar(rotulo, giroEm(clipeId, campos));
}

/** Chave com o pts do quadro quando há (o palco entrega o par de `instanteDaChave`, contratos (h) 22). */
const chaveEm = <T>(t: Seg, pts: Pts | null, v: T): Chave<T> => (pts === null ? { t, v } : { t, pts, v });

/** Região marcada no início (valor fixo, ou a primeira chave) ou no fim (vira animação de duas chaves). */
export function marcarRegiao(clipeId: Id, ponta: "inicio" | "fim", t: Seg, pts: Pts | null, v: Caixa, rotulo: string): void {
  mudar(
    rotulo,
    noClipe(clipeId, (c) => {
      const i = c.efeitos.findIndex((e) => efeitoConhecido(e as Efeito) && (e as Efeito).tipo === "enquadramento");
      // a chave do começo do clipe sai sem pts (o Rust prende)
      const k0 = (): Chave<Caixa> => ({ t: (c as ClipeQualquer).tipo === "trecho" ? (c as ClipeTrecho).inicio_s : 0, v: CAIXA_INTEIRA });
      if (i < 0) {
        if (ponta === "inicio") c.efeitos.push(novoEnquadramento(v));
        else {
          // o cursor pode estar antes do começo do clipe (aparar): as chaves vão em ordem de tempo
          const a = k0();
          c.efeitos.push({ ...novoEnquadramento(v), regiao: t >= a.t ? [a, chaveEm(t, pts, v)] : [chaveEm(t, pts, v), a] });
        }
        return;
      }
      const e = c.efeitos[i] as Draft<EfEnquadramento>;
      const r = e.regiao as Caixa | Chave<Caixa>[];
      if (ehChaves(r)) {
        const ks = r.slice();
        if (ponta === "inicio") ks.shift();
        else if (ks.length >= 2) ks.pop();
        e.regiao = porChave<Caixa>(ks, chaveEm(t, pts, v));
      } else if (ponta === "inicio") e.regiao = v;
      else {
        const a = { ...k0(), v: r };
        e.regiao = t > a.t ? [a, chaveEm(t, pts, v)] : [chaveEm(t, pts, v), a];
        // "vizinho" só com região fixa
        if (e.interpolacao === "vizinho") e.interpolacao = "bilinear";
      }
    }),
  );
}

/** Tira a chave do fim: volta a valor fixo (a lista nunca fica vazia). */
export function tirarFimRegiao(clipeId: Id, rotulo: string): void {
  mudar(
    rotulo,
    noClipe(clipeId, (c) => {
      const e = enquadramentoDe(c as ClipeQualquer);
      if (!e || !ehChaves(e.regiao)) return;
      const i = c.efeitos.findIndex((x) => (x as Efeito).id === e.id);
      const ks = e.regiao;
      (c.efeitos[i] as Draft<EfEnquadramento>).regiao = ks.length <= 2 ? (ks[0] as Chave<Caixa>).v : (tirarChave(ks, ks.length - 1) as Chave<Caixa>[]);
    }),
  );
}

export const CAIXA_INTEIRA: Caixa = { x: 0, y: 0, w: 1, h: 1 };
// ---- transição de entrada ----

/** Catálogo da Fase 0 (`juncao::TRANSICOES`): só "escurecer"; null = corte. */
export const transicaoEm = (clipeId: Id, tipo: string | null): Mut =>
  noClipe<ClipePrincipal>(clipeId, (c) => {
    if (tipo === null) c.transicao_entrada = null;
    else if (c.transicao_entrada) c.transicao_entrada.tipo = tipo;
    else c.transicao_entrada = { tipo, duracao_s: 0.5, audio: "acompanhar" } satisfies Transicao;
  });

export function definirTransicao(clipeId: Id, tipo: string | null, rotulo: string): void {
  mudar(rotulo, transicaoEm(clipeId, tipo));
}

// ---- narração ----

/** Clipe de narração preso ao clipe da principal (desloc 0), ou no início do programa. Cria a trilha se não há. */
export function adicionarNarracao(midia: MidiaRef, presoA: Id | null, rotulo: string, trilha?: Id): Id {
  let id = "";
  mudar(rotulo, (d) => {
    id = juntarNarracao(d, midia, presoA, trilha);
  });
  return id;
}

/** A mudança de `adicionarNarracao` sobre um rascunho. */
export function juntarNarracao(d: Draft<ProjetoEdicao>, midia: MidiaRef, presoA: Id | null, trilha?: Id): Id {
  const pos: Posicao = presoA ? { ref: "clipe", clipe: presoA, desloc: 0 } : { ref: "programa", t: 0 };
  const clipe = novoClipeAudio(midia.id, midia.nome, pos);
  const id = garantirMidia(d, midia);
  let t = trilha ? d.audios.find((x) => x.id === trilha) : d.audios.find((x) => x.papel === "narracao");
  if (!t) {
    d.audios.push(novaTrilhaNarracao());
    t = d.audios[d.audios.length - 1];
  }
  t!.clipes.push({ ...clipe, midia: id });
  return clipe.id;
}

/** Narração gravada no projeto `id` do caso `ws`: pela store quando ele está aberto; senão no arquivo dele (o perito
 *  trocou de edição ou de caso gravando: a gravação não se perde nem cai no projeto que abriu depois). */
export function narracaoNoProjeto(ws: string, id: Id, midia: MidiaRef, presoA: Id | null, rotulo: string, trilha?: Id): Promise<void> {
  const aberto = () => st().ws === ws && st().aberta?.projetoId === id && !!st().doc;
  if (aberto()) {
    adicionarNarracao(midia, presoA, rotulo, trilha);
    return Promise.resolve();
  }
  return emSerie(async () => {
    if (aberto()) {
      adicionarNarracao(midia, presoA, rotulo, trilha);
      return;
    }
    const r = await commands.edicaoAbrir(ws, id);
    // versão mais nova do arquivo: não se regrava (o áudio fica na pasta do projeto)
    if (r.so_leitura) return;
    const base = completar(r.projeto);
    const novo = produce(base, (d) => void juntarNarracao(d, midia, presoA, trilha));
    await commands.edicaoSalvar(ws, comoChegou(novo, base, r.projeto));
  });
}

/** O que o palco marcou para um pedido sem dono próprio (`ContextoClipe.marcar`): âncora do rastreio, lugar do
 *  efeito ou região do enquadramento (fixa). */
export function aplicarMarcado(p: PedidoMarcacao, v: Caixa | Ponto | Lugar, t: Seg, pts: Pts | null): void {
  if (p.rastreio && "x" in v && !("h" in v && typeof v.h === "string")) {
    const c = v as Caixa;
    marcarAncora(p.rastreio, { t, pts, v: { x: c.x, y: c.y, w: c.w ?? 0, h: c.h ?? 0 } }, "Marcar");
  } else if (p.efeito && p.tipo === "lugar" && "h" in v && typeof v.h === "string") {
    const l = v as Lugar;
    mudar("Marcar", noEfeito(p.efeito, (e) => void ("lugar" in e && (e.lugar = l))));
  } else if (p.efeito && p.tipo === "regiao" && "w" in v) {
    mudar("Marcar início", noEfeito<EfEnquadramento>(p.efeito, (e) => void (e.regiao = v as Caixa)));
  }
}

// ---- seleção ----

export function selecionarEfeito(id: Id | null): void {
  const s = st().selecao;
  st().selecionar({ ...s, efeito: id });
}

function clipeAtual(id: Id): ClipeQualquer | null {
  const d = st().doc;
  return d ? (acharClipe(d, id)?.clipe ?? null) : null;
}

export type { Pts };
