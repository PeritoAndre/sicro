/** Peças comuns às seções: nome da mídia (só leitura), item de versão futura e o estilo de texto. */

import { FileQuestion } from "lucide-react";
import type { Draft } from "immer";
import type { EstiloTexto, Id, ProjetoEdicao } from "@domain/edicao";
import { useEdicaoStore } from "../../store/edicaoStore";
import { acharMidia } from "../../modelo/projeto";
import { Cor, Escolha, Numero, Secao } from "../campos";
import styles from "../Inspetor.module.css";

type Mut = (d: Draft<ProjetoEdicao>) => void;

export function NomeMidia({ rotulo, midia }: { rotulo: string; midia: Id }) {
  const nome = useEdicaoStore((s) => (s.doc ? (acharMidia(s.doc, midia)?.nome ?? null) : null));
  return (
    <div className={`${styles.campo} ${styles.larga}`}>
      <span className={styles.rotulo}>{rotulo}</span>
      <output className={`${styles.leitura} ${nome === null ? styles.leituraRuim : ""}`} title={nome ?? undefined}>
        {nome ?? midia}
      </output>
    </div>
  );
}

/** Item de versão futura, ou de tipo conhecido com forma que o Rust recusa (`item_invalido`): só o tipo, sem editor. */
export function SoLeitura({ tipo }: { tipo: string }) {
  return (
    <Secao>
      <div className={styles.desconhecido} data-desconhecido={tipo}>
        <FileQuestion size={14} aria-hidden />
        <code>{tipo}</code>
      </div>
    </Secao>
  );
}

/** Amostra "Aa" em cada família: as opções de fonte não têm nome na tela. */
const FONTES: { v: EstiloTexto["fonte"]; familia: string }[] = [
  { v: "sans", familia: "Inter, system-ui, sans-serif" },
  { v: "serifa", familia: "Georgia, 'Times New Roman', serif" },
  { v: "mono", familia: "var(--font-mono)" },
];

/** "Fonte", "Tamanho" (fração da altura do canvas, em %), "Cor". */
export function EstiloCampos({ item, estilo, aplicar }: { item: Id; estilo: EstiloTexto; aplicar: (f: (e: Draft<EstiloTexto>) => void) => Mut }) {
  return (
    <>
      <Escolha
        rotulo="Fonte"
        larga
        valor={estilo.fonte}
        opcoes={FONTES.map((f) => ({ v: f.v, icone: <span style={{ fontFamily: f.familia }} className={styles.amostra} aria-hidden>Aa</span> }))}
        aplicar={(v) => aplicar((e) => void (e.fonte = v))}
      />
      <Numero
        rotulo="Tamanho"
        item={item}
        valor={round(estilo.tamanho * 100, 1)}
        min={1}
        max={30}
        passo={0.5}
        casas={1}
        unidade="%"
        aplicar={(v) => aplicar((e) => void (e.tamanho = v / 100))}
      />
      <Cor rotulo="Cor" item={item} valor={estilo.cor} aplicar={(v) => aplicar((e) => void (e.cor = v))} />
    </>
  );
}

export const round = (v: number, casas: number) => Math.round(v * 10 ** casas) / 10 ** casas;
