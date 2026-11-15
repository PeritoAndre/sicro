/**
 * Ocultar (§2.1, D17): da mídia por padrão (vale em todo uso dela, `ativo` em tempo de fonte); "Só neste clipe"
 * passa para o clipe (O5). "Conteúdo" põe o modo e a intensidade do padrão dele (§6.6 (1), `ocultarPadrao.ts`) quando
 * ainda estavam no padrão do conteúdo anterior; intensidade só em mosaico e desfoque. "Polígono": vértices no palco
 * (`alvoRegiao.ts`).
 */

import type { Draft } from "immer";
import type { EfOcultar } from "@domain/edicao";
import { noEfeito, soNesteClipe } from "../acoes";
import { Cor, Deslizante, Escolha, Grade, Lista, Marca, Numero, Texto } from "../campos";
import { useEdicaoStore } from "../../store/edicaoStore";
import type { EditorEfeitoProps } from "../registro";
import { FormaIcone } from "./DestaqueEditor";
import { VERTICES_PADRAO } from "./alvoRegiao";
import { round } from "./comum";
import { padraoDoConteudo } from "./ocultarPadrao";
import styles from "../Inspetor.module.css";

const CONTEUDOS: { v: EfOcultar["conteudo"]; r: string }[] = [
  { v: "rosto", r: "Rosto" },
  { v: "placa", r: "Placa" },
  { v: "corpo", r: "Corpo" },
  { v: "tela", r: "Tela" },
  { v: "documento", r: "Documento" },
  { v: "outro", r: "Outro" },
];

/** Modo e intensidade ainda no padrão do conteúdo (no sólido a intensidade não conta). */
export function noPadrao(o: Pick<EfOcultar, "conteudo" | "modo" | "intensidade">): boolean {
  const p = padraoDoConteudo(o.conteudo);
  return o.modo === p.modo && (o.modo === "solido" || o.intensidade === p.intensidade);
}

export function OcultarEditor({ valor: e, ctx }: EditorEfeitoProps<EfOcultar>) {
  const id = e.id;
  const em = (f: (x: Draft<EfOcultar>) => void) => noEfeito<EfOcultar>(id, f);
  const daMidia = ctx.projeto.midias.some((m) => m.ocultacoes.some((o) => o.id === id));
  const so = useEdicaoStore((s) => s.soLeitura);
  // silhueta (Fase 5) aparece só quando o documento já a tem
  const formas: EfOcultar["alvo"]["forma"][] = ["retangulo", "elipse", "poligono"];
  if (!formas.includes(e.alvo.forma)) formas.push(e.alvo.forma);
  return (
    <Grade>
      <Lista
        rotulo="Conteúdo"
        valor={e.conteudo}
        opcoes={CONTEUDOS}
        aplicar={(v) =>
          em((x) => {
            // o padrão do novo conteúdo entra só se o modo ainda era o do anterior (escolha do perito fica)
            const segue = noPadrao(x);
            x.conteudo = v;
            if (segue) Object.assign(x, padraoDoConteudo(v));
          })
        }
      />
      <Escolha
        rotulo="Forma"
        valor={e.alvo.forma}
        opcoes={formas.map((v) => ({ v, icone: <FormaIcone forma={v} /> }))}
        aplicar={(v) =>
          em((x) => {
            x.alvo.forma = v;
            if (v === "poligono" && !(x.alvo.vertices && x.alvo.vertices.length >= 3)) x.alvo.vertices = VERTICES_PADRAO.map((p) => ({ ...p }));
          })
        }
      />
      <Escolha
        rotulo="Modo"
        larga
        valor={e.modo}
        opcoes={[
          { v: "solido", r: "Sólido" },
          { v: "mosaico", r: "Mosaico" },
          { v: "desfoque", r: "Desfoque" },
        ]}
        aplicar={(v) => em((x) => void (x.modo = v))}
      />
      {e.modo === "solido" ? (
        <Cor rotulo="Cor" item={id} valor={e.cor} aplicar={(v) => em((x) => void (x.cor = v))} />
      ) : (
        <Deslizante rotulo="Intensidade" item={id} valor={e.intensidade} min={0} max={1} passo={0.01} mostrar={(v) => `${Math.round(v * 100)} %`} aplicar={(v) => em((x) => void (x.intensidade = v))} />
      )}
      <Deslizante rotulo="Borda suave" item={id} valor={e.alvo.borda_suave} min={0} max={0.5} passo={0.01} mostrar={(v) => `${Math.round(v * 100)} %`} aplicar={(v) => em((x) => void (x.alvo.borda_suave = v))} />
      <Numero
        rotulo="Margem"
        item={id}
        valor={round(e.alvo.margem * 100, 1)}
        min={0}
        max={100}
        passo={1}
        casas={1}
        unidade="%"
        // em `ocultar` a margem nunca é negativa
        valido={(v) => v >= 0}
        aplicar={(v) => em((x) => void (x.alvo.margem = v / 100))}
      />
      <Marca rotulo="Inverter" valor={e.alvo.inverter} aplicar={(v) => em((x) => void (x.alvo.inverter = v))} />
      <Texto rotulo="Motivo" larga linhas={2} valor={e.motivo} aplicar={(v) => em((x) => void (x.motivo = v))} />
      <label className={`${styles.check} ${styles.larga}`}>
        <input
          type="checkbox"
          checked={!daMidia}
          disabled={so || ctx.clipe.tipo !== "trecho"}
          onChange={(ev) => soNesteClipe(id, ctx.clipe.id, ev.target.checked, "Só neste clipe")}
        />
        <span>Só neste clipe</span>
      </label>
    </Grade>
  );
}
