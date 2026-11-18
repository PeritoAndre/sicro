/** Lupa (P1.2): "Forma" · "Ampliação" · "Janela" · "Posição" (Fixa, Acompanha) · "Interpolação" · "Ligação" · "Cor" ·
 *  "Espessura" · "Contorno" (da moldura). "De", "Até" e "Marcar" (o alvo) são da linha da marcação. */

import { Crosshair } from "lucide-react";
import type { Draft } from "immer";
import type { ClipeCongelar, ContextoClipe, EfLupa, Id, Lugar, ProjetoEdicao, Pts, Seg } from "@domain/edicao";
import { useEdicaoStore } from "../../store/edicaoStore";
import { acharClipe, novoId } from "../../modelo/projeto";
import { exibido, ptsDaTabela, tempoDoQuadro } from "../../modelo/tempo";
import { recursoDe } from "../../palco/recursos";
import { FATOR_MAX, FATOR_MIN } from "../../palco/desenho/lupa";
import { midiaDoClipe, mudar, noEfeito, selecionarEfeito, tempoNoClipe, type ClipeQualquer } from "../acoes";
import { Cor, Escolha, Grade, Marca, Mini, Numero, Posicao } from "../campos";
import { pedirMarcacao } from "../contexto";
import { COR_DESENHO, novoRastreio } from "../novos";
import { caixaNova } from "../../palco/areaVisivel";
import type { EditorEfeitoProps } from "../registro";
import { useInspetorUi } from "../ui";
import { round } from "./comum";
import { FormaIcone } from "./DestaqueEditor";

/** O P1.2 liga ao entregar: "Adicionar → Lupa" aparece e o editor abre. */
export const LUPA_PRONTA = true;

const base = () => ({ id: novoId(), ligado: true, nome: "", ativo: null });

/** Lupa nova com os padrões, de olho no rastreio `rastreio` (criado junto por `adicionarLupa`). */
function lupaPadrao(rastreio: Id): EfLupa {
  return {
    ...base(),
    tipo: "lupa",
    rastreio,
    janela: 0.15,
    fator: 2,
    forma: "circulo",
    interpolacao: "bilinear",
    onde: { modo: "fixa", x: 0.8, y: 0.25 },
    moldura: { cor: COR_DESENHO, espessura: 0.004, contorno: true },
    ligacao: true,
  };
}

/** Lupa nova com os padrões (rastreio criado junto por `adicionarEfeito`). */
export function novaLupa(ctx: ContextoClipe): EfLupa {
  void ctx;
  return lupaPadrao("");
}

/** Quadro mostrado no cursor (tempo e pts pela tabela da mídia), onde nasce a âncora do alvo; o congelado
 *  amostra o rastreio no instante dele. Sem tabela: o tempo do clipe, sem pts. */
function quadroMostrado(d: ProjetoEdicao, c: ClipeQualquer, t: Seg): { t: Seg; pts: Pts | null } | null {
  if (c.tipo !== "trecho" && c.tipo !== "congelar") return null;
  const tab = recursoDe(d.midias.find((m) => m.id === c.midia)?.sha256).tabela;
  const alvo = c.tipo === "congelar" ? (c as ClipeCongelar).instante_s : t;
  if (!tab) return c.tipo === "congelar" ? { t: alvo, pts: (c as ClipeCongelar).instante_pts } : null;
  const j = exibido(tab, alvo);
  return { t: tempoDoQuadro(tab, j), pts: ptsDaTabela(tab)[j] as number };
}

/** "Adicionar → Lupa": rastreio novo (como o do destaque) e a lupa apontando para ele, um passo; devolve o id. */
export function adicionarLupa(clipeId: Id, rotulo: string): Promise<Id | null> {
  const d = useEdicaoStore.getState().doc;
  const c = d ? (acharClipe(d, clipeId)?.clipe as ClipeQualquer | undefined) : undefined;
  if (!d || !c) return Promise.resolve(null);
  const t = tempoNoClipe(c);
  const k = quadroMostrado(d, c, t);
  // Fase 4 (contratos-fase4.md D9): nasce na área visível do palco
  const caixa = caixaNova(clipeId);
  const r = novoRastreio(midiaDoClipe(c), k ? k.t : t, caixa);
  if (k) r.ancoras = [{ t: k.t, pts: k.pts, v: caixa }];
  const ef = lupaPadrao(r.id);
  mudar(rotulo, (dd) => {
    dd.rastreios.push(r);
    acharClipe(dd, clipeId)?.clipe.efeitos.push(ef);
  });
  selecionarEfeito(ef.id);
  return Promise.resolve(ef.id);
}

// ---- "Posição": grade 3×3 ----
// Fixa: o centro da lupa no canvas; Acompanha: o lado do alvo. A grade de `campos.tsx` marca a casa pelo `Lugar`
// dela (x 0,02 · 0,5 · 0,98; y 0,03 · 0,5 · 0,94): o valor da casa acesa sai daqui.

const H: Lugar["h"][] = ["esq", "centro", "dir"];
const V: Lugar["v"][] = ["topo", "meio", "base"];
const GRADE_X = [0.02, 0.5, 0.98];
const GRADE_Y = [0.03, 0.5, 0.94];
const FIXA_X = [0.2, 0.5, 0.8];
const FIXA_Y = [0.25, 0.5, 0.75];
const SEGUE_X = [-0.25, 0, 0.25];
const SEGUE_Y = [-0.3, 0, 0.3];

const casa = (valores: number[], v: number) => valores.findIndex((x) => Math.abs(x - v) < 1e-9);

/** Casa acesa da grade para a posição atual (nenhuma, se não cai numa casa). */
function lugarDaGrade(onde: EfLupa["onde"]): Lugar {
  const [i, j] = onde.modo === "fixa" ? [casa(FIXA_X, onde.x), casa(FIXA_Y, onde.y)] : [casa(SEGUE_X, onde.dx), casa(SEGUE_Y, onde.dy)];
  return i < 0 || j < 0 ? { x: -1, y: -1, h: "centro", v: "meio" } : { x: GRADE_X[i]!, y: GRADE_Y[j]!, h: H[i]!, v: V[j]! };
}

function ondeDaGrade(modo: EfLupa["onde"]["modo"], l: Lugar): EfLupa["onde"] {
  const i = Math.max(0, H.indexOf(l.h));
  const j = Math.max(0, V.indexOf(l.v));
  return modo === "fixa" ? { modo, x: FIXA_X[i]!, y: FIXA_Y[j]! } : { modo, dx: SEGUE_X[i]!, dy: SEGUE_Y[j]! };
}

/** "Marcar" (fixa): o centro da lupa vem do palco. */
function MarcarCentro({ id }: { id: Id }) {
  const chave = useInspetorUi((s) => s.chave);
  const minha = `${id}:lugar`;
  return (
    <Mini
      ativo={chave === minha}
      data-acao="marcar-lupa"
      onClick={() => pedirMarcacao({ tipo: "lugar", efeito: id }, (v) => mudar("Marcar", noEfeito<EfLupa>(id, (x) => void (x.onde = { modo: "fixa", x: v.x, y: v.y }))), minha)}
    >
      <Crosshair size={12} /> Marcar
    </Mini>
  );
}

const FORMAS: EfLupa["forma"][] = ["circulo", "retangulo"];

export function LupaEditor({ valor: e }: EditorEfeitoProps<EfLupa>) {
  const id = e.id;
  const em = (f: (x: Draft<EfLupa>) => void) => noEfeito<EfLupa>(id, f);
  const modo = e.onde.modo;
  return (
    <Grade>
      <Escolha rotulo="Forma" larga valor={e.forma} opcoes={FORMAS.map((v) => ({ v, icone: <FormaIcone forma={v} /> }))} aplicar={(v) => em((x) => void (x.forma = v))} />
      <Numero rotulo="Ampliação" item={id} valor={e.fator} min={FATOR_MIN} max={FATOR_MAX} passo={0.5} casas={1} unidade="×" aplicar={(v) => em((x) => void (x.fator = v))} />
      <Numero rotulo="Janela" item={id} valor={round(e.janela * 100, 1)} min={5} max={100} passo={1} casas={1} unidade="%" aplicar={(v) => em((x) => void (x.janela = v / 100))} />
      <Posicao
        rotulo="Posição"
        larga
        valor={lugarDaGrade(e.onde)}
        aplicar={(l) => em((x) => void (x.onde = ondeDaGrade(x.onde.modo, l)))}
        extra={modo === "fixa" ? <MarcarCentro id={id} /> : undefined}
      />
      {/* linha própria: ao lado da grade "Acompanha" saía cortado (340 px de Inspetor) */}
      <Escolha
        rotulo="Posição"
        semRotulo
        larga
        valor={modo}
        opcoes={[
          { v: "fixa" as const, r: "Fixa" },
          { v: "segue" as const, r: "Acompanha" },
        ]}
        aplicar={(v) => em((x) => void (x.onde = ondeDaGrade(v, lugarDaGrade(x.onde))))}
      />
      <Escolha
        rotulo="Interpolação"
        larga
        valor={e.interpolacao}
        opcoes={[
          { v: "vizinho" as const, r: "Vizinho" },
          { v: "bilinear" as const, r: "Bilinear" },
          { v: "bicubica" as const, r: "Bicúbica" },
        ]}
        aplicar={(v) => em((x) => void (x.interpolacao = v))}
      />
      <Marca rotulo="Ligação" valor={e.ligacao} aplicar={(v) => em((x) => void (x.ligacao = v))} />
      <Cor rotulo="Cor" item={id} valor={e.moldura.cor} aplicar={(v) => em((x) => void (x.moldura.cor = v))} />
      <Numero
        rotulo="Espessura"
        item={id}
        valor={round(e.moldura.espessura * 100, 2)}
        min={0.1}
        max={5}
        passo={0.1}
        casas={1}
        unidade="%"
        aplicar={(v) => em((x) => void (x.moldura.espessura = v / 100))}
      />
      <Marca rotulo="Contorno" valor={e.moldura.contorno} aplicar={(v) => em((x) => void (x.moldura.contorno = v))} />
    </Grade>
  );
}
