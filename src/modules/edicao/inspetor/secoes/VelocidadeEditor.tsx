/** Velocidade exibida (P1.3): "Cálculo" · "Posição" (Fixa, Acompanha) · "Fonte" · "Tamanho" · "Cor". O retrato do
 *  cálculo vem do Rust (`edicaoRetratoCalculo`); no trecho, o efeito nasce ativo só no intervalo medido. "Acompanha"
 *  cria um rastreio da mídia (marcado pelo "Marcar" da linha) e põe o texto de um lado dele. */

import { ArrowDown, ArrowLeft, ArrowRight, ArrowUp, Crosshair, RefreshCw } from "lucide-react";
import { useState } from "react";
import type { Draft } from "immer";
import { commands } from "@core/commands";
import type { ClipePrincipal, ClipeSobreposto, ContextoClipe, Desconhecido, EfVelocidadeExibida, Efeito, EstiloTexto, Id, Intervalo, Lugar, ProjetoEdicao, Pts, RetratoCalculo, Seg } from "@domain/edicao";
import type { VideoSpeedCalculation } from "@domain/video_speed";
import { formatarTempo } from "@components/CampoTempo/tempoTexto";
import { useEdicaoStore } from "../../store/edicaoStore";
import { acharClipe, acharMidia, efeitoConhecido, novoId } from "../../modelo/projeto";
import { exibido, ptsDaTabela, tempoDoQuadro } from "../../modelo/tempo";
import { recursoDe } from "../../palco/recursos";
import { textoDaVelocidade } from "../../palco/desenho/medidas";
import { mudar, noClipe, noEfeito, rastreioDe, selecionarEfeito, tempoNoClipe } from "../acoes";
import { Escolha, Grade, Icone, Mini, Posicao } from "../campos";
import { pedirMarcacao } from "../contexto";
import { CAIXA_CENTRO, estiloPadrao, novoRastreio, TAMANHO_TEXTO } from "../novos";
import type { EditorEfeitoProps } from "../registro";
import { useInspetorUi } from "../ui";
import { EstiloCampos } from "./comum";
import { ListaDeDados, noEfeitoSeExiste, pedirRetrato, shaDoClipe, useLista } from "./ReguaEditor";
import styles from "../Inspetor.module.css";

export const VELOCIDADE_PRONTA = true;

type Clipe = ClipePrincipal | ClipeSobreposto;

const LUGAR_PADRAO: Lugar = { x: 0.5, y: 0.06, h: "centro", v: "topo" };
/** Lados do rastreio ("Acompanha"): deslocamento do centro do texto, em fração do canvas. */
const LADOS = [
  { v: "cima", dx: 0, dy: -0.1, icone: <ArrowUp size={13} /> },
  { v: "baixo", dx: 0, dy: 0.1, icone: <ArrowDown size={13} /> },
  { v: "esq", dx: -0.15, dy: 0, icone: <ArrowLeft size={13} /> },
  { v: "dir", dx: 0.15, dy: 0, icone: <ArrowRight size={13} /> },
] as const;

const estiloDaVelocidade = (): EstiloTexto => ({ ...estiloPadrao(TAMANHO_TEXTO.m, true), caixa: { cor: "#0000008c", margem: 0.008, raio: 0 }, contorno: null });

const CALCULO_VAZIO: RetratoCalculo = { id: "", kmh: 0, faixa: null, de_s: 0, ate_s: 0, metodo: "" };

function velocidadeCom(r: RetratoCalculo, ativo: Intervalo | null): EfVelocidadeExibida {
  return { id: novoId(), ligado: true, nome: "", ativo, tipo: "velocidade_exibida", calculo: r, onde: { modo: "fixa", lugar: LUGAR_PADRAO }, estilo: estiloDaVelocidade() };
}

/** Intervalo medido (tempo de fonte do primeiro e do último quadro marcados), como o retrato do Rust. */
function medido(c: VideoSpeedCalculation): [Seg, Seg] {
  const t = c.points.map((p) => p.actual_timestamp_s).filter((x) => Number.isFinite(x));
  return [Math.min(...t), Math.max(...t)];
}

/** Só para a lista: o texto que o retrato daria (faixa como na análise: Monte Carlo, senão o IC). */
function rotuloDoCalculo(c: VideoSpeedCalculation): string {
  const [de, ate] = medido(c);
  const faixa: [number, number] | null =
    c.mc_p2_5_kmh != null && c.mc_p97_5_kmh != null ? [c.mc_p2_5_kmh, c.mc_p97_5_kmh] : c.ci_low != null && c.ci_high != null ? [c.ci_low, c.ci_high] : null;
  return `${textoDaVelocidade({ ...CALCULO_VAZIO, kmh: c.velocity_kmh, faixa })} · ${formatarTempo(de)}–${formatarTempo(ate)}`;
}

/** Intervalo de fonte que o clipe mostra (trecho: a faixa; congelado: o instante); null nos clipes sem vídeo. */
function janela(c: Clipe): [Seg, Seg] | null {
  return c.tipo === "trecho" ? [c.inicio_s, c.fim_s] : c.tipo === "congelar" ? [c.instante_s, c.instante_s] : null;
}

/** Rastreio novo no quadro mostrado (o mesmo par t/pts que o palco marca), como o do destaque. */
function rastreioNoQuadro(d: ProjetoEdicao, c: Clipe) {
  const midia = c.tipo === "trecho" || c.tipo === "congelar" ? c.midia : null;
  const tab = recursoDe(midia ? acharMidia(d, midia)?.sha256 : null).tabela;
  const t = c.tipo === "congelar" ? c.instante_s : tempoNoClipe(c);
  let k: { t: Seg; pts: Pts | null } | null = c.tipo === "congelar" ? { t: c.instante_s, pts: c.instante_pts } : null;
  if (tab && midia) {
    const j = exibido(tab, t);
    k = { t: tempoDoQuadro(tab, j), pts: ptsDaTabela(tab)[j] as number };
  }
  const r = novoRastreio(midia, k ? k.t : t, CAIXA_CENTRO);
  if (k) r.ancoras = [{ t: k.t, pts: k.pts, v: CAIXA_CENTRO }];
  return r;
}

/** Algum efeito do projeto ainda segue o rastreio? */
function usado(d: Draft<ProjetoEdicao>, rastreio: Id): boolean {
  const em = (l: readonly (Efeito | Desconhecido)[] | undefined) => (l ?? []).some((e) => efeitoConhecido(e) && rastreioDe(e) === rastreio);
  for (const c of d.principal) {
    const x = c as { efeitos?: (Efeito | Desconhecido)[]; celulas?: { efeitos: (Efeito | Desconhecido)[] }[] };
    if (em(x.efeitos) || (x.celulas ?? []).some((cel) => em(cel.efeitos))) return true;
  }
  for (const t of d.sobreposicoes) for (const c of t.clipes) if (em((c as { efeitos?: (Efeito | Desconhecido)[] }).efeitos)) return true;
  return d.midias.some((m) => em(m.ocultacoes));
}

export function VelocidadeEditor({ valor: e, ctx }: EditorEfeitoProps<EfVelocidadeExibida>) {
  const id = e.id;
  const em = (f: (x: Draft<EfVelocidadeExibida>) => void) => noEfeito<EfVelocidadeExibida>(id, f);
  const sha = shaDoClipe(ctx.projeto, ctx.clipe);
  const [vez, setVez] = useState(0);
  const calcs = useLista<VideoSpeedCalculation>(sha ? `${ctx.ws}|${sha}` : null, () => commands.listSpeedCalculations(ctx.ws, sha!), vez);
  const chave = useInspetorUi((s) => s.chave);
  const noTrecho = ctx.clipe.tipo === "trecho";
  const trocar = async (calcId: string) => {
    const r = await pedirRetrato((ws) => commands.edicaoRetratoCalculo(ws, calcId));
    if (!r) return;
    noEfeitoSeExiste<EfVelocidadeExibida>(id, "Cálculo", (x) => {
      x.calculo = r;
      // só dentro do intervalo medido (O5); no congelado o `ativo` é local
      if (noTrecho) x.ativo = { de: r.de_s, ate: r.ate_s };
    });
  };
  const modo = (v: "fixa" | "segue") => (d: Draft<ProjetoEdicao>) => {
    if (v === e.onde.modo) return;
    if (v === "segue") {
      const r = rastreioNoQuadro(d as ProjetoEdicao, ctx.clipe);
      d.rastreios.push(r);
      em((x) => void (x.onde = { modo: "segue", rastreio: r.id, dx: 0, dy: -0.1 }))(d);
    } else {
      const antigo = e.onde.modo === "segue" ? e.onde.rastreio : null;
      em((x) => void (x.onde = { modo: "fixa", lugar: LUGAR_PADRAO }))(d);
      if (antigo && !usado(d, antigo)) d.rastreios = d.rastreios.filter((r) => r.id !== antigo);
    }
  };
  const lugar = (l: Lugar) => em((x) => void (x.onde = { modo: "fixa", lugar: l }));
  const onde = e.onde;
  return (
    <Grade>
      <div className={`${styles.larga} ${styles.linhaCor}`}>
        <ListaDeDados rotulo="Cálculo" valor={e.calculo.id} opcoes={(calcs ?? []).map((c) => ({ v: c.id, r: rotuloDoCalculo(c) }))} aplicar={(v) => void trocar(v)} />
        <Icone
          rotulo="Cálculo"
          data-acao="retrato"
          onClick={() => {
            setVez((v) => v + 1);
            void trocar(e.calculo.id);
          }}
        >
          <RefreshCw size={13} />
        </Icone>
      </div>
      <output className={`${styles.leitura} ${styles.larga}`} data-dado data-texto-velocidade>
        {textoDaVelocidade(e.calculo)}
      </output>
      <Escolha
        rotulo="Posição"
        semRotulo
        larga
        valor={onde.modo}
        opcoes={[
          { v: "fixa", r: "Fixa" },
          { v: "segue", r: "Acompanha" },
        ]}
        aplicar={modo}
      />
      {onde.modo === "fixa" ? (
        <Posicao
          rotulo="Posição"
          larga
          valor={onde.lugar}
          aplicar={lugar}
          extra={
            <Mini
              ativo={chave === `${id}:lugar`}
              data-acao="marcar-lugar"
              onClick={() =>
                pedirMarcacao(
                  { tipo: "lugar", efeito: id },
                  (v) => mudar("Marcar", lugar("h" in v && typeof v.h === "string" ? (v as Lugar) : { ...onde.lugar, x: v.x, y: v.y })),
                  `${id}:lugar`,
                )
              }
            >
              <Crosshair size={12} /> Marcar
            </Mini>
          }
        />
      ) : (
        <Escolha
          rotulo="Posição"
          larga
          valor={LADOS.find((l) => l.dx === onde.dx && l.dy === onde.dy)?.v ?? ""}
          opcoes={LADOS.map((l) => ({ v: l.v, icone: l.icone }))}
          aplicar={(v) => {
            const l = LADOS.find((x) => x.v === v)!;
            return em((x) => void (x.onde.modo === "segue" && Object.assign(x.onde, { dx: l.dx, dy: l.dy })));
          }}
        />
      )}
      <EstiloCampos item={id} estilo={e.estilo} aplicar={(f) => em((x) => f(x.estilo))} />
    </Grade>
  );
}

/** Velocidade nova com os padrões, sem cálculo: quem cria pelo "Adicionar" põe o retrato (`adicionarVelocidade`). */
export function novaVelocidade(ctx: ContextoClipe): EfVelocidadeExibida {
  void ctx;
  return velocidadeCom(CALCULO_VAZIO, null);
}

/** "Adicionar → Velocidade": o cálculo da mídia do clipe cujo intervalo medido cruza o que o clipe mostra (senão o
 *  mais novo); no trecho, ativo só no intervalo medido. Sem cálculo (ou clipe sem vídeo), o aviso do catálogo e nada
 *  criado. Devolve o id do efeito, já selecionado. */
export async function adicionarVelocidade(clipeId: Id, rotulo: string): Promise<Id | null> {
  const s = useEdicaoStore.getState();
  const c = s.doc ? acharClipe(s.doc, clipeId)?.clipe : null;
  if (!s.ws || !s.doc || !c) return null;
  const projeto = s.doc.id;
  const sha = shaDoClipe(s.doc, c);
  const j = janela(c);
  const calcs = sha ? await commands.listSpeedCalculations(s.ws, sha).catch(() => [] as VideoSpeedCalculation[]) : [];
  const cruza = (x: VideoSpeedCalculation) => {
    const [de, ate] = medido(x);
    return j !== null && de <= j[1] && ate >= j[0];
  };
  // id vazio: o backend responde `calculo_ausente` (a frase do catálogo vai para o aviso)
  const r = await pedirRetrato((w) => commands.edicaoRetratoCalculo(w, (calcs.find(cruza) ?? calcs[0])?.id ?? ""));
  const d = useEdicaoStore.getState().doc;
  if (!r || !d || d.id !== projeto || !acharClipe(d, clipeId)) return null;
  const ef = velocidadeCom(r, c.tipo === "trecho" ? { de: r.de_s, ate: r.ate_s } : null);
  mudar(rotulo, noClipe(clipeId, (x) => void x.efeitos.push(ef)));
  selecionarEfeito(ef.id);
  return ef.id;
}
