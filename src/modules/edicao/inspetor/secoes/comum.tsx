/** Peças comuns às seções: nome da mídia (só leitura), item de versão futura e o estilo de texto. */

import { FileQuestion, RotateCcw } from "lucide-react";
import type { Draft } from "immer";
import type { EstiloTexto, Id, MidiaInfo, MidiaRef, ProjetoEdicao } from "@domain/edicao";
import { useEdicaoStore } from "../../store/edicaoStore";
import { acharMidia } from "../../modelo/projeto";
import { Cor, Escolha, Icone, Lista, Numero, Secao } from "../campos";
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

type CorDaFonte = NonNullable<MidiaRef["cor"]>;
const CORES: { v: string; r: string; cor: CorDaFonte }[] = [
  { v: "bt601:tv", r: "BT.601 · Limitada", cor: { matriz: "bt601", faixa: "tv" } },
  { v: "bt601:pc", r: "BT.601 · Cheia", cor: { matriz: "bt601", faixa: "pc" } },
  { v: "bt709:tv", r: "BT.709 · Limitada", cor: { matriz: "bt709", faixa: "tv" } },
  { v: "bt709:pc", r: "BT.709 · Cheia", cor: { matriz: "bt709", faixa: "pc" } },
];
const chaveDaCor = (c: CorDaFonte) => `${c.matriz}:${c.faixa}`;

/** "Cor" da mídia (§3.3, O4/F5): é do projeto (`MidiaRef.cor`, vale em todo clipe da mídia). Sem escolha mostra a da
 *  fonte (`MidiaInfo.cor`, etiqueta ou presunção); escolhida, o ícone "Automática" volta a ela. */
export function CorDaMidia({ midia, info }: { midia: Id; info: MidiaInfo | null }) {
  const escolhida = useEdicaoStore((s) => (s.doc ? (acharMidia(s.doc, midia)?.cor ?? null) : null));
  const atual = escolhida ?? info?.cor ?? null;
  const naMidia = (cor: CorDaFonte | null) => (d: Draft<ProjetoEdicao>) => {
    const m = d.midias.find((x) => x.id === midia);
    if (m) m.cor = cor;
  };
  return (
    <div className={`${styles.larga} ${styles.linhaCor}`} data-cor-midia={escolhida ? "escolhida" : "fonte"}>
      <Lista rotulo="Cor" larga valor={atual ? chaveDaCor(atual) : ""} opcoes={CORES.map(({ v, r }) => ({ v, r }))} aplicar={(v) => naMidia(CORES.find((c) => c.v === v)?.cor ?? null)} />
      {escolhida && (
        <Icone rotulo="Automática" onClick={() => useEdicaoStore.getState().alterar("Cor", naMidia(null))}>
          <RotateCcw size={13} />
        </Icone>
      )}
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
