/** Clipe · Entrada: transição de entrada pelo catálogo `juncao::TRANSICOES` ("Corte" = nenhuma). `FechoSecao`: a do
 *  último clipe para o fundo (`fecho`), na aba Saída. As que sobrepõem os clipes não têm nome na tela: só o ícone. */

import type { Draft } from "immer";
import type { ClipePrincipal, ProjetoEdicao, Transicao } from "@domain/edicao";
import { useEdicaoStore } from "../../store/edicaoStore";
import { noClipe, transicaoEm } from "../acoes";
import { Escolha, Grade, Numero, Secao } from "../campos";
import type { SecaoProps } from "../registro";

type Mut = (d: Draft<ProjetoEdicao>) => void;

const CORTE = "corte";
const COM_NOME = [
  { v: CORTE, r: "Corte" },
  { v: "escurecer", r: "Escurecer" },
];
/** Espelho do catálogo do motor (`juncao-transicoes.json`), na ordem dele. */
const SOBREPOEM = ["fundir", "cortina_esq", "cortina_dir", "deslizar_esq", "deslizar_dir", "circulo"] as const;
type Sobrepoe = (typeof SOBREPOEM)[number];
const SONS: { v: Transicao["audio"]; r: string }[] = [
  { v: "acompanhar", r: "Acompanhar" },
  { v: "cortar", r: "Cortar" },
];

/** A seção "Fecho" aparece na aba Saída. */
export const FECHO_PRONTO = true;

/** Quadro em traço; o que entra, preenchido; seta = para onde ele anda. */
function TransicaoIcone({ tipo }: { tipo: Sobrepoe }) {
  const p = { fill: "none", stroke: "currentColor", strokeWidth: 1.4, strokeLinecap: "round" as const, strokeLinejoin: "round" as const };
  const cheio = { fill: "currentColor", fillOpacity: 0.55, stroke: "none" };
  return (
    <svg width={16} height={14} viewBox="0 0 16 14" aria-hidden data-icone={tipo}>
      <rect x={1.5} y={2.5} width={13} height={9} rx={0.5} {...p} />
      {tipo === "fundir" && <rect x={1.5} y={2.5} width={13} height={9} rx={0.5} {...cheio} fillOpacity={0.3} />}
      {(tipo === "cortina_esq" || tipo === "deslizar_esq") && <rect x={8} y={2.5} width={6.5} height={9} {...cheio} />}
      {(tipo === "cortina_dir" || tipo === "deslizar_dir") && <rect x={1.5} y={2.5} width={6.5} height={9} {...cheio} />}
      {tipo === "deslizar_esq" && <path d="M6 7 H3 M4.5 5.5 L3 7 L4.5 8.5" {...p} />}
      {tipo === "deslizar_dir" && <path d="M10 7 H13 M11.5 5.5 L13 7 L11.5 8.5" {...p} />}
      {tipo === "circulo" && <circle cx={8} cy={7} r={3} {...cheio} />}
    </svg>
  );
}

const ICONES = SOBREPOEM.map((v) => ({ v, icone: <TransicaoIcone tipo={v} /> }));

/** "Transição" (com nome e só ícone, uma escolha só), "Duração (s)", "Som". `tipo(null)` = corte. */
function CamposTransicao({ item, tr, tipo, muda }: { item: string; tr: Transicao | null; tipo: (v: string | null) => Mut; muda: (f: (t: Draft<Transicao>) => void) => Mut }) {
  const valor = tr?.tipo ?? CORTE;
  return (
    <Grade>
      {/* tipo de versão futura: nenhuma opção marcada até o perito escolher */}
      <Escolha rotulo="Transição" larga valor={valor} opcoes={COM_NOME} aplicar={(v) => tipo(v === CORTE ? null : v)} />
      <Escolha rotulo="Transição" semRotulo larga valor={valor} opcoes={ICONES} aplicar={(v) => tipo(v)} />
      {tr && (
        <Numero
          rotulo="Duração (s)"
          item={item}
          valor={tr.duracao_s}
          min={0.04}
          max={10}
          passo={0.1}
          casas={2}
          valido={(v) => v > 0}
          aplicar={(v) => muda((t) => void (t.duracao_s = v))}
        />
      )}
      {tr && <Escolha rotulo="Som" valor={tr.audio} opcoes={SONS} aplicar={(v) => muda((t) => void (t.audio = v))} />}
    </Grade>
  );
}

/** Saída · "Fecho": "Transição" · "Duração (s)" · "Som" (Acompanhar, Cortar). */
export function FechoSecao() {
  const aberto = useEdicaoStore((s) => !!s.doc);
  const fecho = useEdicaoStore((s) => s.doc?.fecho ?? null);
  if (!aberto) return null;
  const tipo = (v: string | null): Mut => (d) => {
    if (v === null) d.fecho = null;
    else if (d.fecho) d.fecho.tipo = v;
    else d.fecho = { tipo: v, duracao_s: 0.5, audio: "acompanhar" } satisfies Transicao;
  };
  const muda = (f: (t: Draft<Transicao>) => void): Mut => (d) => void (d.fecho && f(d.fecho));
  return (
    <Secao titulo="Fecho">
      <CamposTransicao item="fecho" tr={fecho} tipo={tipo} muda={muda} />
    </Secao>
  );
}

export function EntradaSecao({ clipe: c }: SecaoProps<ClipePrincipal>) {
  const id = c.id;
  const muda = (f: (t: Draft<Transicao>) => void) => noClipe<ClipePrincipal>(id, (x) => void (x.transicao_entrada && f(x.transicao_entrada)));
  return (
    <Secao titulo="Entrada">
      <CamposTransicao item={id} tr={c.transicao_entrada} tipo={(v) => transicaoEm(id, v)} muda={muda} />
    </Secao>
  );
}
