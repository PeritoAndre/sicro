/**
 * Clipe · Enquadramento: girar e espelhar (EfGirar) e a região (EfEnquadramento), fixa ou animada entre a marca do
 * início e a do fim. A região é marcada no palco ("regiao", na proporção da saída). Valem no clipe inteiro.
 */

import { Crop, FlipHorizontal2, FlipVertical2, Trash2, X } from "lucide-react";
import type { Caixa, Chave, EfEnquadramento, EfGirar } from "@domain/edicao";
import { formatarTempo } from "@components/CampoTempo/tempoTexto";
import { ehChaves } from "../../modelo/curva";
import { useEdicaoStore } from "../../store/edicaoStore";
import { enquadramentoDe, giroEm, girarDe, marcarRegiao, mudarGiro, noEfeito, removerEfeito, tirarFimRegiao, type ClipeQualquer } from "../acoes";
import { pedirMarcacao } from "../contexto";
import { Escolha, Grade, Icone, Mini, Secao } from "../campos";
import type { SecaoProps } from "../registro";
import { useInspetorUi } from "../ui";
import styles from "../Inspetor.module.css";

const GRAUS: EfGirar["graus"][] = [0, 90, 180, 270];

export function EnquadramentoSecao({ clipe: c }: SecaoProps<ClipeQualquer>) {
  const id = c.id;
  const g = girarDe(c);
  const e = enquadramentoDe(c);
  const proporcao = useEdicaoStore((s) => (s.doc ? s.doc.saida.largura / s.doc.saida.altura : 16 / 9));
  const chave = useInspetorUi((s) => s.chave);
  const marcar = (ponta: "inicio" | "fim") =>
    pedirMarcacao(
      { tipo: "regiao", efeito: e?.id, proporcao },
      (v, t) => {
        if ("w" in v) marcarRegiao(id, ponta, t, v as Caixa, ponta === "inicio" ? "Marcar início" : "Marcar fim");
      },
      `${id}:regiao:${ponta}`,
    );
  const marcando = (ponta: "inicio" | "fim") => chave === `${id}:regiao:${ponta}`;
  const ks = e && ehChaves(e.regiao) ? (e.regiao as Chave<Caixa>[]) : null;
  const espelho = (lado: "espelhar_h" | "espelhar_v") => () => mudarGiro(id, { [lado]: !(g?.[lado] ?? false) }, "Espelhar");
  return (
    <Secao
      titulo="Enquadramento"
      acoes={
        e && (
          <Icone rotulo="Remover" onClick={() => removerEfeito(e.id, "Remover")}>
            <Trash2 size={13} />
          </Icone>
        )
      }
    >
      <Grade>
        <Escolha
          rotulo="Girar"
          valor={g?.graus ?? 0}
          opcoes={GRAUS.map((v) => ({ v, r: `${v}°` }))}
          aplicar={(v) => giroEm(id, { graus: v })}
        />
        <div className={styles.campo}>
          <span className={styles.rotulo}>Espelhar</span>
          <span className={styles.seg} role="group" aria-label="Espelhar">
            <button type="button" className={`${styles.segBtn} ${g?.espelhar_h ? styles.segOn : ""}`} aria-pressed={!!g?.espelhar_h} aria-label="Espelhar" data-valor="h" onClick={espelho("espelhar_h")}>
              <FlipHorizontal2 size={14} />
            </button>
            <button type="button" className={`${styles.segBtn} ${g?.espelhar_v ? styles.segOn : ""}`} aria-pressed={!!g?.espelhar_v} aria-label="Espelhar" data-valor="v" onClick={espelho("espelhar_v")}>
              <FlipVertical2 size={14} />
            </button>
          </span>
        </div>
        <div className={`${styles.larga} ${styles.marcasLinha}`}>
          <span className={styles.marcas}>
            <Mini ativo={marcando("inicio")} onClick={() => marcar("inicio")} data-acao="marcar-inicio">
              <Crop size={12} /> Marcar início
            </Mini>
            {ks && <span className={styles.tempoMarca} data-dado>{formatarTempo((ks[0] as Chave<Caixa>).t)}</span>}
          </span>
          <span className={styles.marcas}>
            <Mini ativo={marcando("fim")} onClick={() => marcar("fim")} data-acao="marcar-fim">
              <Crop size={12} /> Marcar fim
            </Mini>
            {ks && ks.length > 1 && (
              <>
                <span className={styles.tempoMarca} data-dado>{formatarTempo((ks[ks.length - 1] as Chave<Caixa>).t)}</span>
                <Icone rotulo="Remover" onClick={() => tirarFimRegiao(id, "Marcar fim")}>
                  <X size={13} />
                </Icone>
              </>
            )}
          </span>
        </div>
        {e && (
          <>
            <Escolha
              rotulo="Ajuste"
              larga
              valor={e.ajuste}
              opcoes={[
                { v: "conter", r: "Inteiro" },
                { v: "cobrir", r: "Preencher" },
              ]}
              aplicar={(v) => noEfeito<EfEnquadramento>(e.id, (x) => void (x.ajuste = v as EfEnquadramento["ajuste"]))}
            />
            <Escolha
              rotulo="Interpolação"
              larga
              valor={e.interpolacao}
              opcoes={[
                // "vizinho" só com região fixa (validação)
                ...(ks ? [] : [{ v: "vizinho" as const, r: "Vizinho" }]),
                { v: "bilinear" as const, r: "Bilinear" },
                { v: "bicubica" as const, r: "Bicúbica" },
              ]}
              aplicar={(v) => noEfeito<EfEnquadramento>(e.id, (x) => void (x.interpolacao = v))}
            />
          </>
        )}
      </Grade>
    </Secao>
  );
}
