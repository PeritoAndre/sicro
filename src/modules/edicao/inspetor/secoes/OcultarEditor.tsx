/**
 * Ocultar (§2.1, D17): da mídia por padrão (vale em todo uso dela, `ativo` em tempo de fonte); "Só neste clipe"
 * passa para o clipe (O5). Sólido opaco é o padrão; intensidade só em mosaico e desfoque.
 */

import type { Draft } from "immer";
import type { EfOcultar } from "@domain/edicao";
import { noEfeito, soNesteClipe } from "../acoes";
import { Cor, Deslizante, Escolha, Grade, Lista, Marca, Numero, Texto } from "../campos";
import { useEdicaoStore } from "../../store/edicaoStore";
import type { EditorEfeitoProps } from "../registro";
import { FormaIcone } from "./DestaqueEditor";
import { round } from "./comum";
import styles from "../Inspetor.module.css";

const CONTEUDOS: { v: EfOcultar["conteudo"]; r: string }[] = [
  { v: "rosto", r: "Rosto" },
  { v: "placa", r: "Placa" },
  { v: "corpo", r: "Corpo" },
  { v: "tela", r: "Tela" },
  { v: "documento", r: "Documento" },
  { v: "outro", r: "Outro" },
];

export function OcultarEditor({ valor: e, ctx }: EditorEfeitoProps<EfOcultar>) {
  const id = e.id;
  const em = (f: (x: Draft<EfOcultar>) => void) => noEfeito<EfOcultar>(id, f);
  const daMidia = ctx.projeto.midias.some((m) => m.ocultacoes.some((o) => o.id === id));
  const so = useEdicaoStore((s) => s.soLeitura);
  // polígono e silhueta (Fases 2 e 5) aparecem quando o documento já os tem
  const formas: EfOcultar["alvo"]["forma"][] = ["retangulo", "elipse"];
  if (!formas.includes(e.alvo.forma)) formas.push(e.alvo.forma);
  return (
    <Grade>
      <Lista rotulo="Conteúdo" valor={e.conteudo} opcoes={CONTEUDOS} aplicar={(v) => em((x) => void (x.conteudo = v))} />
      <Escolha
        rotulo="Forma"
        valor={e.alvo.forma}
        opcoes={formas.map((v) => ({ v, icone: <FormaIcone forma={v} /> }))}
        aplicar={(v) => em((x) => void (x.alvo.forma = v))}
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
