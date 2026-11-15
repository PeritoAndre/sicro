/**
 * Holofote (§2.1, §5.4): "Forma" (Retângulo, Elipse, Polígono) · "Margem" · "Borda suave" · "Inverter" ·
 * "Escurecer" · "Dessaturar". "De", "Até", "Marcar" e o rastreio são da linha da marcação. Dono: P2.6.
 */

import type { Draft } from "immer";
import type { ContextoClipe, EfHolofote, Id } from "@domain/edicao";
import { midiaDoClipe, mudar, noEfeito, quadroDoCongelado, quadroDoTrecho, selecionarEfeito, tempoNoClipe, type ClipeQualquer } from "../acoes";
import { Deslizante, Escolha, Grade, Marca, Numero } from "../campos";
import { CAIXA_CENTRO, novoRastreio } from "../novos";
import type { EditorEfeitoProps } from "../registro";
import { acharClipe, novoId } from "../../modelo/projeto";
import { useEdicaoStore } from "../../store/edicaoStore";
import { FormaIcone } from "./DestaqueEditor";
import { VERTICES_PADRAO } from "./alvoRegiao";
import { round } from "./comum";

/** Ligado: "Adicionar → Holofote" aparece e o editor abre. */
export const HOLOFOTE_PRONTO = true;

const FORMAS: EfHolofote["alvo"]["forma"][] = ["retangulo", "elipse", "poligono"];

/** Holofote com os padrões, de olho no rastreio `rastreio`. */
function holofotePadrao(rastreio: Id): EfHolofote {
  return {
    id: novoId(),
    ligado: true,
    nome: "",
    ativo: null,
    tipo: "holofote",
    alvo: { rastreio, forma: "elipse", vertices: null, margem: 0.1, borda_suave: 0.1, inverter: false },
    escurecer: 0.6,
    dessaturar: 0,
  };
}

/** Holofote novo com os padrões, de olho num rastreio criado junto por `adicionarHolofote`. */
export function novoHolofote(ctx: ContextoClipe): EfHolofote {
  void ctx;
  return holofotePadrao("");
}

/** "Adicionar → Holofote": rastreio novo com a âncora no quadro mostrado (como o destaque) e o holofote apontando
 *  para ele, um passo; devolve o id do efeito (null = nada criado). */
export function adicionarHolofote(clipeId: Id, rotulo: string): Promise<Id | null> {
  const d = useEdicaoStore.getState().doc;
  const c = d ? (acharClipe(d, clipeId)?.clipe as ClipeQualquer | undefined) : undefined;
  if (!d || !c) return Promise.resolve(null);
  const t = tempoNoClipe(c);
  // congelado: o rastreio de base fonte é amostrado no instante dele
  const k = c.tipo === "congelar" ? quadroDoCongelado(c) : c.tipo === "trecho" ? quadroDoTrecho(c, t) : null;
  const r = novoRastreio(midiaDoClipe(c), k ? k.t : t, CAIXA_CENTRO);
  if (k) r.ancoras = [{ t: k.t, pts: k.pts, v: CAIXA_CENTRO }];
  const ef = holofotePadrao(r.id);
  mudar(rotulo, (dd) => {
    dd.rastreios.push(r);
    acharClipe(dd, clipeId)?.clipe.efeitos.push(ef);
  });
  selecionarEfeito(ef.id);
  return Promise.resolve(ef.id);
}

const pct = (v: number) => `${Math.round(v * 100)} %`;

export function HolofoteEditor({ valor: e }: EditorEfeitoProps<EfHolofote>) {
  const id = e.id;
  const em = (f: (x: Draft<EfHolofote>) => void) => noEfeito<EfHolofote>(id, f);
  return (
    <Grade>
      <Escolha
        rotulo="Forma"
        larga
        valor={e.alvo.forma}
        opcoes={FORMAS.map((v) => ({ v, icone: <FormaIcone forma={v} /> }))}
        aplicar={(v) =>
          em((x) => {
            x.alvo.forma = v;
            if (v === "poligono" && !(x.alvo.vertices && x.alvo.vertices.length >= 3)) x.alvo.vertices = VERTICES_PADRAO.map((p) => ({ ...p }));
          })
        }
      />
      <Numero
        rotulo="Margem"
        item={id}
        valor={round(e.alvo.margem * 100, 1)}
        min={-50}
        max={100}
        passo={1}
        casas={1}
        unidade="%"
        aplicar={(v) => em((x) => void (x.alvo.margem = v / 100))}
      />
      <Deslizante rotulo="Borda suave" item={id} valor={e.alvo.borda_suave} min={0} max={0.5} passo={0.01} mostrar={pct} aplicar={(v) => em((x) => void (x.alvo.borda_suave = v))} />
      <Marca rotulo="Inverter" valor={e.alvo.inverter} aplicar={(v) => em((x) => void (x.alvo.inverter = v))} />
      <Deslizante rotulo="Escurecer" item={id} valor={e.escurecer} min={0} max={1} passo={0.01} mostrar={pct} aplicar={(v) => em((x) => void (x.escurecer = v))} />
      <Deslizante rotulo="Dessaturar" item={id} valor={e.dessaturar} min={0} max={1} passo={0.01} mostrar={pct} aplicar={(v) => em((x) => void (x.dessaturar = v))} />
    </Grade>
  );
}
