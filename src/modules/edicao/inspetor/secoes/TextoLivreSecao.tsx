/**
 * Seção do texto livre no Inspetor (contratos-fase4.md (h)): "Texto" · "Fonte" · "Tamanho" · "Cor" · "Fundo" (Nenhum,
 * Faixa, Caixa) · "Contorno" · "Sombra" · "Posição" · "Giro" · "Largura" · "Entrada" / "Saída" (Corte, Aparecer,
 * Deslizar) · "Duração (s)" · "Lado"; com a linha do tempo (P4.1), "De" · "Até" · "Começar no cursor" · "Terminar no
 * cursor". Posição e largura em % da saída (a posição gravada no pixel inteiro), giro em graus; com chaves (P4.2), o
 * campo grava a chave no cursor; "Escala" e "Opacidade" e as raias das curvas (`linha/curvas/CurvasDoItem`) são do P4.2.
 * Montada pelo registro (`livre:texto`) só com `TEXTO_LIVRE_PRONTO`. Dono: pacote do texto livre.
 */

import { useEffect, useMemo, useRef } from "react";
import { current, isDraft, type Draft } from "immer";
import { ArrowDown, ArrowLeft, ArrowRight, ArrowUp } from "lucide-react";
import type { Animavel, AnimacaoSob, BlocoTexto, EstiloTexto, ProjetoEdicao, SobTitulo, Surgir } from "@domain/edicao";
import { CampoNumero } from "@components/CampoNumero/CampoNumero";
import { useEdicaoStore } from "../../store/edicaoStore";
import { acharMidia } from "../../modelo/projeto";
import { avaliar, ehChaves } from "../../modelo/curva";
import { resolver } from "../../modelo/tempo";
import { recursoDe, useRecursos } from "../../palco/recursos";
import { mudar, noClipe } from "../acoes";
import { Cor, Escolha, Grade, Marca, Mini, Numero, Secao, Tempo, Texto } from "../campos";
import type { SecaoProps } from "../registro";
import { round } from "./comum";
import { CONTORNO, CORES, DURACAO_SURGIR, SOMBRA, comFundo, comSurgir, fundoDe, tauNoCursor, tomarFoco, type Fundo } from "./textoLivre";
import { LINHA_PRONTA } from "../../linha/LinhaDoTempo";
import { apararLivre } from "../../linha/arrastar";
import { gravarEm } from "../../linha/curvas/chaves";
import { CurvasDoItem } from "../../linha/curvas/CurvasDoItem";
import styles from "../Inspetor.module.css";
import meu from "./TextoLivre.module.css";

type Mut = (d: Draft<ProjetoEdicao>) => void;

const FONTES: { v: EstiloTexto["fonte"]; familia: string }[] = [
  { v: "sans", familia: "Inter, system-ui, sans-serif" },
  { v: "serifa", familia: "Georgia, 'Times New Roman', serif" },
  { v: "mono", familia: "var(--font-mono)" },
];
const FUNDOS: { v: Fundo; r: string }[] = [
  { v: "nenhum", r: "Nenhum" },
  { v: "faixa", r: "Faixa" },
  { v: "caixa", r: "Caixa" },
];
const SURGIR: { v: Surgir; r: string }[] = [
  { v: "corte", r: "Corte" },
  { v: "aparecer", r: "Aparecer" },
  { v: "deslizar", r: "Deslizar" },
];
const LADOS: { v: AnimacaoSob["lado"]; r: string; I: typeof ArrowUp }[] = [
  { v: "cima", r: "Cima", I: ArrowUp },
  { v: "baixo", r: "Baixo", I: ArrowDown },
  { v: "esq", r: "Esquerda", I: ArrowLeft },
  { v: "dir", r: "Direita", I: ArrowRight },
];

const so = () => {
  const s = useEdicaoStore.getState();
  return s.soLeitura || !s.doc;
};

export function TextoLivreSecao({ clipe: c }: SecaoProps<SobTitulo>) {
  const id = c.id;
  const raiz = useRef<HTMLDivElement | null>(null);
  const doc = useEdicaoStore((s) => s.doc);
  const cursor = useEdicaoStore((s) => s.cursor);
  const soLeitura = useEdicaoStore((s) => s.soLeitura);
  // item novo: o foco vai para o "Texto" (nenhum texto de exemplo)
  useEffect(() => {
    if (tomarFoco(id)) raiz.current?.querySelector("textarea")?.focus();
  }, [id]);
  const b = c.titulo.blocos[0] as BlocoTexto | undefined;
  // P4.2: a linha resolvida só muda com o documento e as tabelas (o cursor muda a cada quadro tocando)
  const versao = useRecursos((s) => s.versao);
  const resolvida = useMemo(() => (doc ? resolver(doc, { tabela: (m) => recursoDe(acharMidia(doc, m)?.sha256).tabela }) : null), [doc, versao]);
  if (!doc || !b) return null;
  const e = b.estilo;
  const W = Math.max(doc.saida.largura, 1);
  const H = Math.max(doc.saida.altura, 1);
  const tr = c.transformacao;
  const animado = ehChaves(tr.x) || ehChaves(tr.y) || ehChaves(tr.rotacao) || ehChaves(tr.escala) || ehChaves(tr.opacidade);
  const linha = animado || LINHA_PRONTA ? resolvida : null;
  const tau = (linha && tauNoCursor(doc, linha, cursor, id)) ?? 0;
  // P4.2 (D7): com chaves, o campo grava a chave no cursor (a que já existe ali fica com a curva dela)
  const naChave = (a: Animavel<number>, v: number): Animavel<number> => gravarEm(a, tau, v);

  const noBloco = (f: (x: Draft<BlocoTexto>, s: Draft<SobTitulo>) => void): Mut =>
    noClipe<SobTitulo>(id, (x) => {
      const bl = x.titulo.blocos[0];
      if (bl) f(bl, x);
    });
  const noEstilo = (f: (x: Draft<EstiloTexto>) => void) => noBloco((bl) => f(bl.estilo));
  const fundo = fundoDe(e);
  const anima = c.animacao;
  const ladoEm = anima?.entrada === "deslizar" ? "entrada" : anima?.saida === "deslizar" ? "saida" : null;
  const livre = linha?.livres.find((l) => l.id === id) ?? null;
  const tempo = (q: number) => (linha ? (q * linha.fps.den) / linha.fps.num : 0);
  /** Borda do texto no slot `q` (D5, pela conta da linha do P4.1). */
  const naBorda = (lado: "inicio" | "fim", q: number | null, rotulo: string) => {
    if (!linha || q === null || so()) return;
    mudar(rotulo, (d) => apararLivre(d, linha, id, lado, q));
  };
  const qDoCursor = () => {
    const s = useEdicaoStore.getState().cursor;
    return s.modo === "programa" ? s.q : null;
  };

  const surgir = (lado: "entrada" | "saida") => {
    const s: Surgir = (lado === "entrada" ? anima?.entrada : anima?.saida) ?? "aparecer";
    const campo = lado === "entrada" ? "entrada_s" : "saida_s";
    return (
      <>
        <Escolha<Surgir>
          rotulo={lado === "entrada" ? "Entrada" : "Saída"}
          larga
          valor={s}
          opcoes={SURGIR}
          aplicar={(v) =>
            noClipe<SobTitulo>(id, (x) => {
              const a = comSurgir(x.animacao, lado, v);
              if (a) x.animacao = a;
              else delete x.animacao;
              // duração gasta pelo corte: volta ao padrão ao escolher outra
              if (v !== "corte" && !(x[campo] > 0)) x[campo] = DURACAO_SURGIR;
            })
          }
        />
        {s !== "corte" && (
          <Numero
            rotulo="Duração (s)"
            item={`${id}:${lado}`}
            valor={round(c[campo], 2)}
            min={0.05}
            max={10}
            passo={0.1}
            casas={2}
            valido={(v) => v > 0}
            aplicar={(v) => noClipe<SobTitulo>(id, (x) => void (x[campo] = v))}
          />
        )}
        {ladoEm === lado && <Lado valor={anima?.lado ?? "baixo"} aplicar={(v) => noClipe<SobTitulo>(id, (x) => void (x.animacao && (x.animacao.lado = v)))} />}
      </>
    );
  };

  return (
    <Secao titulo="Texto livre">
      <div ref={raiz} className={meu.raiz} data-texto-livre={id}>
        <Grade>
          <Texto rotulo="Texto" larga linhas={3} valor={b.conteudo} aplicar={(v) => noBloco((bl) => void (bl.conteudo = v))} />
          <Escolha
            rotulo="Fonte"
            larga
            valor={e.fonte}
            opcoes={FONTES.map((f) => ({ v: f.v, icone: <span style={{ fontFamily: f.familia }} className={styles.amostra} aria-hidden>Aa</span> }))}
            aplicar={(v) => noEstilo((x) => void (x.fonte = v))}
          />
          <Numero
            rotulo="Tamanho"
            item={id}
            valor={round(e.tamanho * 100, 1)}
            min={0.8}
            max={40}
            passo={0.5}
            casas={1}
            unidade="%"
            valido={(v) => v > 0}
            aplicar={(v) => noEstilo((x) => void (x.tamanho = v / 100))}
          />
          <Cor rotulo="Cor" item={id} valor={e.cor} aplicar={(v) => noEstilo((x) => void (x.cor = v))} />
          <div className={`${meu.cores} ${styles.larga}`} role="group" aria-label="Cor">
            {CORES.map((k) => (
              <button
                key={k}
                type="button"
                className={`${meu.amostraCor} ${e.cor.slice(0, 7).toLowerCase() === k ? meu.corOn : ""}`}
                style={{ background: k }}
                aria-label="Cor"
                aria-pressed={e.cor.slice(0, 7).toLowerCase() === k}
                data-cor={k}
                disabled={soLeitura}
                onClick={() => !so() && mudar("Cor", noEstilo((x) => void (x.cor = k)))}
              />
            ))}
          </div>
          <Escolha<Fundo> rotulo="Fundo" larga valor={fundo} opcoes={FUNDOS} aplicar={(v) => noBloco((bl) => void (bl.estilo = comFundo((isDraft(bl.estilo) ? current(bl.estilo) : bl.estilo) as EstiloTexto, v)))} />
          {fundo === "caixa" && e.caixa && <Cor rotulo="Caixa" item={`${id}:caixa`} valor={e.caixa.cor} aplicar={(v) => noEstilo((x) => void (x.caixa && (x.caixa.cor = v)))} />}
          {fundo === "caixa" && <span aria-hidden />}
          <Marca rotulo="Contorno" valor={!!e.contorno} aplicar={(v) => noEstilo((x) => void (x.contorno = v ? { ...CONTORNO } : null))} />
          <Marca
            rotulo="Sombra"
            valor={!!e.sombra}
            aplicar={(v) =>
              noEstilo((x) => {
                if (v) x.sombra = { ...SOMBRA };
                else delete x.sombra;
              })
            }
          />
          <div className={`${styles.campo} ${styles.larga}`} data-campo="Posição">
            <span className={styles.rotulo}>Posição</span>
            <span className={meu.par}>
              {(["x", "y"] as const).map((eixo) => {
                const lado = eixo === "x" ? W : H;
                const v = round(avaliar(tr[eixo], tau) * 100, 1);
                return (
                  <div key={eixo} className={styles.campoNum} data-eixo={eixo}>
                    <CampoNumero
                      value={v}
                      step={0.5}
                      casas={1}
                      min={0}
                      max={100}
                      unidade="%"
                      disabled={soLeitura}
                      // no pixel inteiro da saída: sem giro o texto sai do sprite sem reamostrar
                      onChange={(n) =>
                        n !== v &&
                        mudar(
                          "Posição",
                          noClipe<SobTitulo>(id, (x) => {
                            x.transformacao[eixo] = naChave(tr[eixo], Math.round((n / 100) * lado) / lado);
                            // posição com chaves: x e y ganham a chave do instante juntos (a raia é uma só)
                            const outro = eixo === "x" ? "y" : "x";
                            if (ehChaves(tr[eixo]) && ehChaves(tr[outro])) x.transformacao[outro] = naChave(tr[outro], avaliar(tr[outro], tau));
                          }),
                          `${id}:Posição:${eixo}`,
                        )
                      }
                    />
                  </div>
                );
              })}
            </span>
          </div>
          <Numero
            rotulo="Giro"
            item={id}
            valor={round(avaliar(tr.rotacao, tau), 1)}
            min={-180}
            max={180}
            passo={1}
            casas={1}
            unidade="°"
            aplicar={(v) => noClipe<SobTitulo>(id, (x) => void (x.transformacao.rotacao = naChave(tr.rotacao, v)))}
          />
          <Numero
            rotulo="Escala"
            item={id}
            valor={round(avaliar(tr.escala, tau) * 100, 1)}
            min={5}
            max={1000}
            passo={5}
            casas={1}
            unidade="%"
            valido={(v) => v > 0}
            aplicar={(v) => noClipe<SobTitulo>(id, (x) => void (x.transformacao.escala = naChave(tr.escala, v / 100)))}
          />
          <Numero
            rotulo="Opacidade"
            item={id}
            valor={round(avaliar(tr.opacidade, tau) * 100, 1)}
            min={0}
            max={100}
            passo={5}
            casas={1}
            unidade="%"
            valido={(v) => v >= 0 && v <= 100}
            aplicar={(v) => noClipe<SobTitulo>(id, (x) => void (x.transformacao.opacidade = naChave(tr.opacidade, v / 100)))}
          />
          <Numero
            rotulo="Largura"
            item={id}
            valor={round(b.largura_max * 100, 1)}
            min={2}
            max={200}
            passo={1}
            casas={1}
            unidade="%"
            valido={(v) => v > 0}
            aplicar={(v) => noBloco((bl) => void (bl.largura_max = v / 100))}
          />
          <CurvasDoItem item={{ k: "livre", id }} />
          {surgir("entrada")}
          {surgir("saida")}
          {LINHA_PRONTA && livre && linha && (
            <>
              {(["inicio", "fim"] as const).map((lado) => (
                <Tempo
                  key={lado}
                  rotulo={lado === "inicio" ? "De" : "Até"}
                  item={`${id}:${lado}`}
                  base="programa"
                  fps={linha.fps}
                  valor={tempo(lado === "inicio" ? livre.inicio_q : livre.inicio_q + livre.n_q)}
                  aplicar={(t) => (d) => apararLivre(d, linha, id, lado, Math.round((t * linha.fps.num) / linha.fps.den))}
                />
              ))}
              <Mini onClick={() => naBorda("inicio", qDoCursor(), "Começar no cursor")} data-acao="comecar-no-cursor">
                Começar no cursor
              </Mini>
              <Mini onClick={() => naBorda("fim", qDoCursor(), "Terminar no cursor")} data-acao="terminar-no-cursor">
                Terminar no cursor
              </Mini>
            </>
          )}
        </Grade>
      </div>
    </Secao>
  );
}

/** "Lado" do deslizar: quatro ícones com `title`. */
function Lado({ valor, aplicar }: { valor: AnimacaoSob["lado"]; aplicar: (v: AnimacaoSob["lado"]) => Mut }) {
  const soLeitura = useEdicaoStore((s) => s.soLeitura);
  return (
    <div className={styles.campo} data-campo="Lado">
      <span className={styles.rotulo}>Lado</span>
      <div className={styles.seg} role="group" aria-label="Lado">
        {LADOS.map(({ v, r, I }) => (
          <button
            key={v}
            type="button"
            className={`${styles.segBtn} ${v === valor ? styles.segOn : ""}`}
            aria-pressed={v === valor}
            aria-label={r}
            title={r}
            data-valor={v}
            disabled={soLeitura}
            onClick={() => v !== valor && mudar("Lado", aplicar(v))}
          >
            <I size={13} aria-hidden />
          </button>
        ))}
      </div>
    </div>
  );
}
