/**
 * Título (cartela na principal, ou título preso a um clipe): prévia (P1.7), modelo, os quatro textos por papel,
 * fundo e estilo. "Modelo" arranja os blocos e o fundo (`modelo/titulos.ts`); texto novo entra no arranjo do modelo.
 * "Fonte" e "Cor" valem para todos os blocos; "Tamanho" é o do título e escala os outros na mesma proporção.
 */

import { current, isDraft, type Draft } from "immer";
import type { BlocoTexto, ClipeTitulo, EstiloTexto, Id, ProjetoEdicao, SobTitulo, Titulo } from "@domain/edicao";
import { noClipe } from "../acoes";
import { Cor, Escolha, Grade, Lista, Marca, Numero, Secao, Texto } from "../campos";
import { novoBloco } from "../novos";
import { novoId } from "../../modelo/projeto";
import { aplicarModelo, blocoNoModelo } from "../../modelo/titulos";
import type { SecaoProps } from "../registro";
import { round } from "./comum";
import styles from "../Inspetor.module.css";
import { EDITOR_TITULO_PRONTO, EditorTitulo } from "../../titulos/EditorTitulo";

type Mut = (d: Draft<ProjetoEdicao>) => void;

const MODELOS: { v: Titulo["modelo"]; r: string }[] = [
  { v: "cartela", r: "Cartela" },
  { v: "abertura", r: "Abertura" },
  { v: "evento", r: "Evento" },
  { v: "terco_inferior", r: "Terço inferior" },
  { v: "encerramento", r: "Encerramento" },
];

const PAPEIS: { papel: BlocoTexto["papel"]; rotulo: string; linhas?: number }[] = [
  { papel: "titulo", rotulo: "Título" },
  { papel: "subtitulo", rotulo: "Subtítulo" },
  { papel: "corpo", rotulo: "Texto", linhas: 3 },
  { papel: "rodape", rotulo: "Rodapé" },
];

const FONTES: { v: EstiloTexto["fonte"]; familia: string }[] = [
  { v: "sans", familia: "Inter, system-ui, sans-serif" },
  { v: "serifa", familia: "Georgia, 'Times New Roman', serif" },
  { v: "mono", familia: "var(--font-mono)" },
];

type ComTitulo = ClipeTitulo | SobTitulo;

export function TituloSecao({ clipe: c }: SecaoProps<ComTitulo>) {
  const id = c.id;
  const t = c.titulo;
  const noTitulo = (f: (x: Draft<Titulo>, d: Draft<ProjetoEdicao>) => void): Mut => noClipe<ComTitulo>(id, (x, d) => f(x.titulo, d));
  const preso = "pos" in c;
  const principal = t.blocos.find((b) => b.papel === "titulo") ?? t.blocos[0] ?? null;
  const estilo = principal?.estilo ?? null;
  return (
    <Secao titulo="Título">
      {EDITOR_TITULO_PRONTO && <EditorTitulo key={id} clipe={c} />}
      <Grade>
        <Texto rotulo="Nome" larga valor={c.rotulo} aplicar={(v) => noClipe<ComTitulo>(id, (x) => void (x.rotulo = v))} />
        <Lista rotulo="Modelo" larga valor={t.modelo} opcoes={MODELOS} aplicar={(v) => noClipe<ComTitulo>(id, (x, d) => void (x.titulo = noModelo(x.titulo, v, preso, d)))} />
        {PAPEIS.map(({ papel, rotulo, linhas }) => (
          <Texto
            key={papel}
            rotulo={rotulo}
            larga
            linhas={linhas}
            valor={t.blocos.find((b) => b.papel === papel)?.conteudo ?? ""}
            aplicar={(v) => noTitulo((x, d) => escreverNoModelo(x, papel, v, canvasDe(d)))}
          />
        ))}
        <Fundo id={id} titulo={t} />
        {estilo && (
          <>
            <Escolha
              rotulo="Fonte"
              larga
              valor={estilo.fonte}
              opcoes={FONTES.map((f) => ({ v: f.v, icone: <span style={{ fontFamily: f.familia }} className={styles.amostra} aria-hidden>Aa</span> }))}
              aplicar={(v) => noTitulo((x) => x.blocos.forEach((b) => void (b.estilo.fonte = v)))}
            />
            <Numero
              rotulo="Tamanho"
              item={id}
              valor={round(estilo.tamanho * 100, 1)}
              min={1}
              max={30}
              passo={0.5}
              casas={1}
              unidade="%"
              aplicar={(v) => noTitulo((x) => escalar(x, principal!.id, v / 100))}
            />
            <Cor rotulo="Cor" item={id} valor={estilo.cor} aplicar={(v) => noTitulo((x) => x.blocos.forEach((b) => void (b.estilo.cor = v)))} />
          </>
        )}
        {!("pos" in c) ? (
          <Numero rotulo="Duração (s)" item={id} valor={c.duracao_s} min={0.1} max={3600} passo={0.5} casas={2} valido={(v) => v > 0} aplicar={(v) => noClipe<ClipeTitulo>(id, (x) => void (x.duracao_s = v))} />
        ) : (
          <>
            <Marca rotulo="Até o fim do clipe" larga valor={c.duracao_s === null} aplicar={(v) => noClipe<SobTitulo>(id, (x) => void (x.duracao_s = v ? null : 3))} />
            {c.duracao_s !== null && (
              <Numero rotulo="Duração (s)" item={id} valor={c.duracao_s} min={0.1} max={3600} passo={0.5} casas={2} valido={(v) => v > 0} aplicar={(v) => noClipe<SobTitulo>(id, (x) => void (x.duracao_s = v))} />
            )}
          </>
        )}
      </Grade>
    </Secao>
  );
}

/** Fundo de cor; transparente e imagem aparecem como estão até o perito escolher uma cor. */
function Fundo({ id, titulo }: { id: Id; titulo: Titulo }) {
  const cor = titulo.fundo.tipo === "cor" ? titulo.fundo.cor : null;
  return (
    <div className={styles.fundoCampo}>
      <Cor rotulo="Fundo" item={id} valor={cor ?? "#000000"} aplicar={(v) => noClipe<ComTitulo>(id, (x) => void (x.titulo.fundo = { tipo: "cor", cor: v }))} />
      {cor === null && <span className={styles.xadrez} aria-hidden />}
    </div>
  );
}

const canvasDe = (d: Draft<ProjetoEdicao>) => ({ w: d.saida.largura, h: d.saida.altura });

/** O título no modelo `m` (arranjo dos blocos e fundo), para o canvas da saída. */
function noModelo(t: Draft<Titulo>, m: Titulo["modelo"], preso: boolean, d: Draft<ProjetoEdicao>): Titulo {
  return aplicarModelo((isDraft(t) ? current(t) : t) as Titulo, m, preso, canvasDe(d));
}

/** Texto do papel na tela: cria o bloco no primeiro caractere, no arranjo do modelo, e tira quando fica vazio. */
export function escreverNoModelo(t: Draft<Titulo>, papel: BlocoTexto["papel"], v: string, canvas: { w: number; h: number }): void {
  const i = t.blocos.findIndex((b) => b.papel === papel);
  if (i >= 0) {
    if (v === "" && t.blocos.length > 1) t.blocos.splice(i, 1);
    else (t.blocos[i] as Draft<BlocoTexto>).conteudo = v;
    return;
  }
  if (v === "") return;
  t.blocos.push(blocoNoModelo(t as Titulo, papel, novoId(), v, canvas));
}

/** Texto do papel: cria o bloco no primeiro caractere (no lugar da cartela da Fase 0), tira quando fica vazio.
 *  A tela usa `escreverNoModelo`; esta fica para os testes da Fase 0, que montam o projeto por ela. */
export function escreverBloco(t: Draft<Titulo>, papel: BlocoTexto["papel"], v: string): void {
  const i = t.blocos.findIndex((b) => b.papel === papel);
  if (i >= 0) {
    if (v === "" && t.blocos.length > 1) t.blocos.splice(i, 1);
    else (t.blocos[i] as Draft<BlocoTexto>).conteudo = v;
    return;
  }
  if (v === "") return;
  const ref = t.blocos[0]?.estilo;
  t.blocos.push(novoBloco(papel, v, ref ? { fonte: ref.fonte, cor: ref.cor } : undefined));
}

/** Tamanho do bloco principal; os outros acompanham na mesma proporção. */
function escalar(t: Draft<Titulo>, principal: Id, tamanho: number): void {
  const p = t.blocos.find((b) => b.id === principal);
  if (!p || p.estilo.tamanho <= 0) return;
  const k = tamanho / p.estilo.tamanho;
  for (const b of t.blocos) b.estilo.tamanho = b.id === principal ? tamanho : b.estilo.tamanho * k;
}
