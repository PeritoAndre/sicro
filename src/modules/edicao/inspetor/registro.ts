/**
 * Registro do Inspetor (§8.3): tipo de efeito → descritor, e tipo de item → seções. Inspetor e palco só percorrem
 * o registro: efeito novo = um arquivo em `secoes/` e uma linha aqui. Tipo fora do registro (versão futura) e item
 * que o Rust acusa com `item_invalido` usam o descritor "desconhecido": linha apagada, só o tipo, sem editor.
 * Efeitos conhecidos sem edição nesta fase (realce, holofote, lupa, régua, velocidade) têm editor só de leitura.
 */

import { createElement, type FC } from "react";
import {
  Clock,
  Crop,
  FileQuestion,
  Focus,
  Image as ImageIcon,
  Lightbulb,
  Ruler,
  RotateCw,
  Search,
  ShieldOff,
  Square,
  Gauge,
  Sparkles,
  Type,
  type LucideIcon,
} from "lucide-react";
import type {
  Alca,
  Caixa,
  ClipePrincipal,
  ClipeSobreposto,
  ContextoClipe,
  Desconhecido,
  Efeito,
  Id,
  Lugar,
  MidiaInfo,
  Ponto,
  Primitiva,
  QuadroPalco,
  TabelaQuadros,
} from "@domain/edicao";
import { primitivas as primitivasDoEspelho } from "../palco/desenho/primitivas";
import { caixaNoCanvas, inversa, ponto, type Afim } from "../modelo/geometria";
import { efeitoConhecido, novoId } from "../modelo/projeto";
import type { Gesto } from "../store/edicaoStore";
import { novoCarimbo, novoDestaque, novoEnquadramento, novoGirar, novoImagem, novoOcultar, novoTexto, CAIXA_CENTRO } from "./novos";
import { DestaqueEditor } from "./secoes/DestaqueEditor";
import { OcultarEditor } from "./secoes/OcultarEditor";
import { CarimboEditor, ImagemEditor, TextoEditor } from "./secoes/AnotacoesEditor";
import { SoLeitura } from "./secoes/comum";
import { TrechoSecao } from "./secoes/TrechoSecao";
import { CongelarSecao, ImagemClipeSecao, SobImagemSecao } from "./secoes/ClipesSimples";
import { TituloSecao } from "./secoes/TituloSecao";
import { EntradaSecao } from "./secoes/EntradaSecao";
import { EnquadramentoSecao } from "./secoes/EnquadramentoSecao";
import { MarcacoesSecao, RealceSoLeitura } from "./secoes/MarcacoesSecao";
// Fase 1: cada pacote liga o seu (`*_PRONTA`/`*_PRONTO`) ao entregar, sem tocar aqui (contratos-fase1 (c))
import { adicionarLupa, LUPA_PRONTA, LupaEditor, novaLupa } from "./secoes/LupaEditor";
import { adicionarRegua, novaRegua, REGUA_PRONTA, ReguaEditor } from "./secoes/ReguaEditor";
import { adicionarVelocidade, novaVelocidade, VELOCIDADE_PRONTA, VelocidadeEditor } from "./secoes/VelocidadeEditor";
import { REALCE_PRONTO, RealceSecao } from "./secoes/RealceSecao";
import { AUDIO_CLIPE_PRONTO, AudioClipeSecao } from "./secoes/AudioClipeSecao";

// ---- efeitos ----

// CONTRATO: `op.rotulo` e `op.agrupar` são acréscimo ao §8.3 (o passo do histórico leva o rótulo do campo).
export interface EditorEfeitoProps<E extends Efeito> {
  valor: E;
  ctx: ContextoClipe;
  onChange(e: E, op?: { gesto?: Gesto; rotulo?: string; agrupar?: string }): void;
  onRemover(): void;
}

/** Tudo que a tela precisa saber de um tipo de efeito (§8.3). */
export interface DescritorEfeito<E extends Efeito> {
  tipo: E["tipo"];
  /** Rótulo do §8.6 (o do "Adicionar"). */
  rotulo: string;
  icone: LucideIcon;
  familia: "origem" | "enquadre" | "regiao" | "anotacao";
  /** Efeito com os padrões; destaque e ocultar apontam para um rastreio que `adicionarEfeito` cria junto. */
  novo(ctx: ContextoClipe): E;
  Editor: FC<EditorEfeitoProps<E>>;
  /** Espelho de `camada::formas::primitivas`. */
  primitivas(e: E, q: QuadroPalco): Primitiva[];
  alcas?(e: E, q: QuadroPalco): Alca[];
}

type Registro = { [T in Efeito["tipo"]]: DescritorEfeito<Extract<Efeito, { tipo: T }>> };

const prim = <E extends Efeito>(e: E, q: QuadroPalco) => primitivasDoEspelho(e, q);

/** Editor dos tipos conhecidos sem edição nesta fase. */
const SoLeituraEditor: FC<EditorEfeitoProps<Efeito>> = ({ valor }) => createElement(SoLeitura, { tipo: valor.tipo });

function soLeitura<T extends Efeito["tipo"]>(tipo: T, rotulo: string, icone: LucideIcon, familia: DescritorEfeito<Efeito>["familia"]): DescritorEfeito<Extract<Efeito, { tipo: T }>> {
  const d: DescritorEfeito<Efeito> = {
    tipo,
    rotulo,
    icone,
    familia,
    novo: () => {
      throw new Error(`nao_suportado: ${tipo}`);
    },
    Editor: SoLeituraEditor,
    primitivas: prim,
  };
  return d as unknown as DescritorEfeito<Extract<Efeito, { tipo: T }>>;
}

// ---- alças (no canvas, 0..1): lugar de texto e imagem; caixa do rastreio no quadro ----

const limitar = (v: number) => Math.min(1, Math.max(0, v));

function alcaLugar<E extends Extract<Efeito, { lugar: Lugar }>>(e: E, _q: QuadroPalco): Alca[] {
  return [
    {
      id: `${e.id}:lugar`,
      tipo: "mover",
      em: { x: e.lugar.x, y: e.lugar.y },
      cursor: "move",
      arrastar: (de: Ponto, para: Ponto) => ({ efeito: { ...e, lugar: { ...e.lugar, x: limitar(e.lugar.x + para.x - de.x), y: limitar(e.lugar.y + para.y - de.y) } } }),
    },
  ];
}

/** Caixa do rastreio (espaço de origem) no canvas: mover pelo meio e redimensionar pelos cantos; a âncora volta
 *  ao espaço de origem pela inversa de A e entra no quadro exibido (t, pts). */
function alcasRastreio(rastreio: string, q: QuadroPalco): Alca[] {
  const cx = q.rastreio(rastreio);
  const inv = inversa(q.A as Afim);
  if (!cx || !inv) return [];
  const c = caixaNoCanvas(q.A as Afim, cx);
  const paraOrigem = (p: Ponto) => ponto(inv, p);
  const ancora = (v: Caixa) => ({ rastreio, ancora: { t: q.t, pts: q.pts, v } });
  const cantos: [string, Ponto, string][] = [
    ["no", { x: c.x, y: c.y }, "nwse-resize"],
    ["ne", { x: c.x + c.w, y: c.y }, "nesw-resize"],
    ["so", { x: c.x, y: c.y + c.h }, "nesw-resize"],
    ["se", { x: c.x + c.w, y: c.y + c.h }, "nwse-resize"],
  ];
  return [
    {
      id: `${rastreio}:mover`,
      tipo: "mover",
      em: { x: c.x + c.w / 2, y: c.y + c.h / 2 },
      cursor: "move",
      arrastar: (de, para) => {
        const a = paraOrigem(de);
        const b = paraOrigem(para);
        return ancora({ ...cx, x: cx.x + b.x - a.x, y: cx.y + b.y - a.y });
      },
    },
    ...cantos.map(
      ([nome, em, cursor]): Alca => ({
        id: `${rastreio}:${nome}`,
        tipo: "canto",
        em,
        cursor,
        arrastar: (_de, para) => {
          // o canto oposto fica; os dois cantos voltam ao espaço de origem
          const oposto: Ponto = { x: nome.endsWith("o") ? c.x + c.w : c.x, y: nome.startsWith("n") ? c.y + c.h : c.y };
          const p1 = paraOrigem(oposto);
          const p2 = paraOrigem({ x: limitar(para.x), y: limitar(para.y) });
          return ancora({ x: Math.min(p1.x, p2.x), y: Math.min(p1.y, p2.y), w: Math.abs(p2.x - p1.x), h: Math.abs(p2.y - p1.y) });
        },
      }),
    ),
  ];
}

export const REGISTRO: Registro = {
  destaque: {
    tipo: "destaque",
    rotulo: "Destaque",
    icone: Square,
    familia: "anotacao",
    novo: () => novoDestaque(novoId()),
    Editor: DestaqueEditor,
    primitivas: prim,
    alcas: (e, q) => alcasRastreio(e.rastreio, q),
  },
  ocultar: {
    tipo: "ocultar",
    rotulo: "Ocultar",
    icone: ShieldOff,
    familia: "regiao",
    novo: () => novoOcultar(novoId()),
    Editor: OcultarEditor,
    primitivas: prim,
    alcas: (e, q) => alcasRastreio(e.alvo.rastreio, q),
  },
  texto: { tipo: "texto", rotulo: "Texto", icone: Type, familia: "anotacao", novo: novoTexto, Editor: TextoEditor, primitivas: prim, alcas: alcaLugar },
  carimbo: { tipo: "carimbo", rotulo: "Tempo", icone: Clock, familia: "anotacao", novo: novoCarimbo, Editor: CarimboEditor, primitivas: prim, alcas: alcaLugar },
  imagem: { tipo: "imagem", rotulo: "Imagem", icone: ImageIcon, familia: "anotacao", novo: () => novoImagem(""), Editor: ImagemEditor, primitivas: prim, alcas: alcaLugar },
  // enquadre: editados na seção Enquadramento do clipe, não na lista de marcações
  girar: { ...soLeitura("girar", "Girar", RotateCw, "enquadre"), novo: () => novoGirar(90) },
  enquadramento: { ...soLeitura("enquadramento", "Enquadramento", Crop, "enquadre"), novo: () => novoEnquadramento(CAIXA_CENTRO) },
  // Fase 1: só leitura até o pacote dono ligar
  lupa: LUPA_PRONTA
    ? { tipo: "lupa", rotulo: "Lupa", icone: Search, familia: "anotacao", novo: novaLupa, Editor: LupaEditor, primitivas: prim, alcas: (e, q) => alcasRastreio(e.rastreio, q) }
    : soLeitura("lupa", "Lupa", Search, "anotacao"),
  regua: REGUA_PRONTA ? { tipo: "regua", rotulo: "Régua", icone: Ruler, familia: "anotacao", novo: novaRegua, Editor: ReguaEditor, primitivas: prim } : soLeitura("regua", "Régua", Ruler, "anotacao"),
  velocidade_exibida: VELOCIDADE_PRONTA
    ? { tipo: "velocidade_exibida", rotulo: "Velocidade", icone: Gauge, familia: "anotacao", novo: novaVelocidade, Editor: VelocidadeEditor, primitivas: prim }
    : soLeitura("velocidade_exibida", "Velocidade", Gauge, "anotacao"),
  // fases seguintes
  holofote: soLeitura("holofote", "Holofote", Lightbulb, "regiao"),
  estabilizar: soLeitura("estabilizar", "Estabilizar", Sparkles, "origem"),
  lente: soLeitura("lente", "Lente", Sparkles, "origem"),
  desentrelacar: soLeitura("desentrelacar", "Desentrelaçar", Sparkles, "origem"),
  media_temporal: soLeitura("media_temporal", "Média de quadros", Sparkles, "origem"),
  ajustes: soLeitura("ajustes", "Brilho", Sparkles, "origem"),
  nitidez: soLeitura("nitidez", "Nitidez", Focus, "origem"),
};

/** O que "Adicionar" oferece, na ordem do §8.6 (lupa, régua e velocidade quando o pacote dono liga). */
export const ADICIONAR: readonly TipoAdicionavel[] = [
  "destaque",
  "ocultar",
  ...(LUPA_PRONTA ? (["lupa"] as const) : []),
  ...(REGUA_PRONTA ? (["regua"] as const) : []),
  ...(VELOCIDADE_PRONTA ? (["velocidade_exibida"] as const) : []),
  "texto",
  "carimbo",
  "imagem",
];
export type TipoAdicionavel = "destaque" | "ocultar" | "lupa" | "regua" | "velocidade_exibida" | "texto" | "carimbo" | "imagem";
/** "Adicionar" dos tipos que criam o efeito por conta própria (rastreio, retrato da análise): a função do pacote. */
export const ADICIONAR_PROPRIO: Partial<Record<TipoAdicionavel, (clipeId: Id, rotulo: string) => Promise<Id | null>>> = {
  lupa: adicionarLupa,
  regua: adicionarRegua,
  velocidade_exibida: adicionarVelocidade,
};
/** Tipos com editor nesta fase (o resto aparece só para leitura). */
export const EDITAVEIS = new Set<Efeito["tipo"]>([...ADICIONAR, "girar", "enquadramento"]);

/** Descritor de tipo desconhecido: o rótulo é o próprio tipo (dado, não frase). */
export function descritorDesconhecido(tipo: string): DescritorEfeito<Efeito> {
  return {
    tipo: tipo as Efeito["tipo"],
    rotulo: tipo,
    icone: FileQuestion,
    familia: "anotacao",
    novo: () => {
      throw new Error(`item_desconhecido: ${tipo}`);
    },
    Editor: () => createElement(SoLeitura, { tipo }),
    primitivas: () => [],
  };
}

/** Descritor do efeito. `conhecido`: tipo do registro e não acusado com `item_invalido` (`invalidos`); senão é o
 *  descritor "desconhecido". `editavel`: tem editor nesta fase. */
export function descritor(e: Efeito | Desconhecido, invalidos?: ReadonlySet<string>): { d: DescritorEfeito<Efeito>; conhecido: boolean; editavel: boolean } {
  const tipo = typeof (e as { tipo?: unknown })?.tipo === "string" ? (e as { tipo: string }).tipo : "";
  const id = (e as { id?: unknown })?.id;
  if (!efeitoConhecido(e) || (typeof id === "string" && invalidos?.has(id))) return { d: descritorDesconhecido(tipo), conhecido: false, editavel: false };
  return { d: REGISTRO[e.tipo] as unknown as DescritorEfeito<Efeito>, conhecido: true, editavel: EDITAVEIS.has(e.tipo) };
}

// ---- itens → seções ----

export interface SecaoProps<C> {
  clipe: C;
  ctx: ContextoClipe | null;
  info: MidiaInfo | null;
  tabela: TabelaQuadros | null;
}

type TipoItem = `principal:${ClipePrincipal["tipo"]}` | `livre:${ClipeSobreposto["tipo"]}`;

/** Realce: o do P1.4 quando ele liga; até lá, os efeitos de origem só para leitura. */
const Realce = REALCE_PRONTO ? RealceSecao : RealceSoLeitura;

/** Seções do Inspetor, por tipo de item, na ordem em que aparecem. `tela` e vídeo sobreposto: Fase 3. */
export const SECOES: Partial<Record<TipoItem, FC<SecaoProps<never>>[]>> = {
  "principal:trecho": [TrechoSecao, ...(AUDIO_CLIPE_PRONTO ? [AudioClipeSecao] : []), EntradaSecao, EnquadramentoSecao, Realce, MarcacoesSecao] as FC<SecaoProps<never>>[],
  "principal:congelar": [CongelarSecao, EntradaSecao, EnquadramentoSecao, Realce, MarcacoesSecao] as FC<SecaoProps<never>>[],
  "principal:imagem": [ImagemClipeSecao, EntradaSecao, EnquadramentoSecao, Realce, MarcacoesSecao] as FC<SecaoProps<never>>[],
  "principal:titulo": [TituloSecao, EntradaSecao, MarcacoesSecao] as FC<SecaoProps<never>>[],
  "livre:titulo": [TituloSecao] as FC<SecaoProps<never>>[],
  "livre:imagem": [SobImagemSecao] as FC<SecaoProps<never>>[],
};

export function secoesDe(onde: "principal" | "livre", tipo: string): FC<SecaoProps<never>>[] | null {
  return SECOES[`${onde}:${tipo}` as TipoItem] ?? null;
}
