/**
 * Clipe · Enquadramento: girar e espelhar (EfGirar) e o zoom (EfEnquadramento). O zoom tem quantas chaves o perito
 * quiser no mesmo trecho ("Chave no cursor"): aproximar, segurar e afastar sem dividir. Com o efeito selecionado, o
 * palco mostra o quadro inteiro com a caixa da região por cima (vista de origem); as alças gravam a chave do cursor.
 * Regiões na proporção da saída; zoom animado sempre em "Preencher". "Acompanhar rastreio": o centro da região
 * segue um rastreio do trecho, suavizado (D8, D9 de contratos-fase4.md).
 */

import { Crop, Crosshair, Diamond, DiamondMinus, DiamondPlus, FlipHorizontal2, FlipVertical2, Trash2 } from "lucide-react";
import type { Draft } from "immer";
import type { Caixa, Chave, ClipeTrecho, EfEnquadramento, EfGirar, Id, Pts, Seg } from "@domain/edicao";
import { formatarTempo } from "@components/CampoTempo/tempoTexto";
import { ehChaves } from "../../modelo/curva";
import { acharClipe } from "../../modelo/projeto";
import { useEdicaoStore } from "../../store/edicaoStore";
import { enquadramentoDe, giroEm, girarDe, mudar, noClipe, noEfeito, quadroDoTrecho, removerEfeito, selecionarEfeito, tempoNoClipe, CAIXA_INTEIRA, type ClipeQualquer } from "../acoes";
import { novoEnquadramento } from "../novos";
import { pedirMarcacao } from "../contexto";
import { useOrigemPendente } from "../../palco/origem";
import { comRegiao, proporcaoDaSaida, regiaoAnimada, regiaoNaSaida } from "../../palco/alcas";
import {
  candidatosDoSegue,
  comChaveNoCursor,
  comSegue,
  nomeDoCandidato,
  regiaoGravada,
  regiaoNoInstante,
  rotulosDasChaves,
  semChave,
  semSegue,
  SUAVIZAR_MAX_S,
} from "../../modelo/segue";
import { Escolha, Grade, Icone, Mini, Numero, Secao } from "../campos";
import type { SecaoProps } from "../registro";
import { useInspetorUi } from "../ui";
import styles from "../Inspetor.module.css";
import st from "./EnquadramentoSecao.module.css";

const GRAUS: EfGirar["graus"][] = [0, 90, 180, 270];

/** Par (t, pts) da chave no cursor: o quadro mostrado no trecho; tempo local nos outros. */
function noCursor(c: ClipeQualquer): { t: Seg; pts: Pts | null } {
  const t = tempoNoClipe(c);
  return c.tipo === "trecho" ? (quadroDoTrecho(c as ClipeTrecho, t) ?? { t, pts: null }) : { t, pts: null };
}

/** Troca o enquadramento do clipe (ou põe o novo). */
const comEnquadramento = (clipe: Id, novo: EfEnquadramento) =>
  noClipe(clipe, (c) => {
    const i = c.efeitos.findIndex((e) => (e as { id?: string }).id === novo.id);
    if (i >= 0) c.efeitos[i] = novo as Draft<EfEnquadramento>;
    else c.efeitos.push(novo as Draft<EfEnquadramento>);
  });

/** O enquadramento do clipe como está agora no documento (o palco pode ter mudado depois do último desenho). */
function atualDe(clipe: Id): EfEnquadramento | null {
  const d = useEdicaoStore.getState().doc;
  const a = d ? acharClipe(d, clipe) : null;
  return a ? enquadramentoDe(a.clipe) : null;
}

const fator = (f: number) => `${(Math.round(f * 10) / 10).toFixed(1).replace(".", ",")}×`;

export function EnquadramentoSecao({ clipe: c, info }: SecaoProps<ClipeQualquer>) {
  const id = c.id;
  const g = girarDe(c);
  const e = enquadramentoDe(c);
  const saida = useEdicaoStore((s) => (s.doc ? { w: s.doc.saida.largura, h: s.doc.saida.altura } : { w: 16, h: 9 }));
  const editando = useEdicaoStore((s) => !!e && s.selecao.efeito === e.id);
  // a chave do quadro mostrado fica acesa
  const cursor = useEdicaoStore((s) => (s.cursor.modo === "clipe" && s.cursor.clipe === id ? s.cursor.t : null));
  const chave = useInspetorUi((s) => s.chave);
  const doc = useEdicaoStore((s) => s.doc);
  const soLeitura = useEdicaoStore((s) => s.soLeitura);
  // a região é marcada no espaço de origem: espera o intermediário (§7.5)
  const parada = useOrigemPendente(c);
  const origem = info?.exibido ?? saida;
  const proporcao = proporcaoDaSaida(c.efeitos, origem, saida);
  const ks = e && ehChaves(e.regiao) ? (e.regiao as Chave<Caixa>[]) : null;
  const rotulos = ks && e ? rotulosDasChaves(ks, e.ajuste, proporcao) : [];
  const atual = cursor === null ? null : noCursor(c);
  const naChave = (k: Chave<Caixa>) => !!atual && (atual.pts != null && k.pts != null ? atual.pts === k.pts : atual.t === k.t);
  const candidatos = doc ? candidatosDoSegue(doc, c) : [];
  const segue = e?.segue ?? null;
  const animada = !!e && (regiaoAnimada(e.regiao) || !!segue);

  const chaveNoCursor = () => {
    const d = useEdicaoStore.getState().doc;
    if (!d) return;
    const { t, pts } = noCursor(c);
    const e = atualDe(id);
    const base = e ?? novoEnquadramento(CAIXA_INTEIRA);
    // no zoom que acompanha, a caixa da chave fica onde o rastreio está (é onde o palco a mostra)
    const v = (e?.segue ? regiaoNoInstante(d, c.efeitos, t) : null) ?? regiaoGravada(base, t);
    mudar("Chave no cursor", comEnquadramento(id, comChaveNoCursor(base, t, pts, v)));
    selecionarEfeito(base.id);
  };
  const marcar = () =>
    pedirMarcacao(
      { tipo: "regiao", efeito: e?.id, proporcao: saida.w / saida.h },
      (v, t, pts) => {
        if (!("w" in v)) return;
        const r = regiaoNaSaida(v as Caixa, proporcao);
        const atual = atualDe(id);
        mudar("Marcar", comEnquadramento(id, atual ? comRegiao(atual, r, t, pts) : novoEnquadramento(r)));
      },
      `${id}:regiao`,
    );
  const irPara = (k: Chave<Caixa>) => {
    useEdicaoStore.getState().posicionar({ modo: "clipe", clipe: id, t: k.t });
    if (e) selecionarEfeito(e.id);
  };
  const acompanhar = () => {
    const e = atualDe(id);
    if (!e?.segue && candidatos[0]) mudar("Acompanhar rastreio", comEnquadramento(id, comSegue(e ?? novoEnquadramento(CAIXA_INTEIRA), candidatos[0].id, proporcao)));
    else if (e?.segue) mudar("Acompanhar rastreio", comEnquadramento(id, semSegue(e)));
  };
  const espelho = (lado: "espelhar_h" | "espelhar_v") => () => mudar("Espelhar", giroEm(id, { [lado]: !(g?.[lado] ?? false) }));

  return (
    <Secao
      titulo="Enquadramento"
      acoes={
        e && (
          <>
            <Icone rotulo="Enquadramento" aria-pressed={editando} className={editando ? st.ligado : ""} onClick={() => selecionarEfeito(editando ? null : e.id)} data-acao="editar-regiao">
              <Crop size={13} />
            </Icone>
            <Icone rotulo="Remover" onClick={() => removerEfeito(e.id, "Remover")}>
              <Trash2 size={13} />
            </Icone>
          </>
        )
      }
    >
      <Grade>
        <Escolha rotulo="Girar" valor={g?.graus ?? 0} opcoes={GRAUS.map((v) => ({ v, r: `${v}°` }))} aplicar={(v) => giroEm(id, { graus: v })} />
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
        <div className={`${styles.larga} ${st.barra}`}>
          <Mini disabled={parada} onClick={chaveNoCursor} data-acao="chave-no-cursor">
            <DiamondPlus size={12} /> Chave no cursor
          </Mini>
          <Mini ativo={chave === `${id}:regiao`} disabled={parada} onClick={marcar} data-acao="marcar-regiao">
            <Crop size={12} /> Marcar
          </Mini>
        </div>
        {ks && ks.length > 0 && (
          <ul className={`${styles.larga} ${st.chaves}`}>
            {ks.map((k, i) => {
              const r = rotulos[i]!;
              const aqui = naChave(k);
              return (
                <li key={`${k.t}:${k.pts ?? ""}`} className={`${st.chave} ${aqui ? st.chaveAqui : ""}`} data-chave={i}>
                  <button type="button" className={st.chaveIr} onClick={() => irPara(k)} aria-current={aqui || undefined}>
                    <Diamond size={10} className={st.losango} />
                    <span className={st.rotuloChave}>{r.rotulo}</span>
                    <span className={st.dado} data-dado>
                      {fator(r.fator)}
                    </span>
                    <span className={st.tempo} data-dado>
                      {formatarTempo(k.t)}
                    </span>
                  </button>
                  <Icone rotulo="Tirar chave" onClick={() => e && mudar("Tirar chave", comEnquadramento(id, semChave(atualDe(id) ?? e, i)))}>
                    <DiamondMinus size={12} />
                  </Icone>
                </li>
              );
            })}
          </ul>
        )}
        {c.tipo === "trecho" && (
          <div className={`${styles.larga} ${st.segue}`}>
            <Mini ativo={!!segue} aria-pressed={!!segue} disabled={!segue && candidatos.length === 0} onClick={acompanhar} data-acao="acompanhar">
              <Crosshair size={12} /> Acompanhar rastreio
            </Mini>
            {segue && e && (
              <select
                className={`${styles.entrada} ${st.rastreio}`}
                aria-label="Acompanhar rastreio"
                value={segue.rastreio}
                disabled={soLeitura}
                data-dado
                onChange={(ev) => ev.target.value !== segue.rastreio && mudar("Acompanhar rastreio", noEfeito<EfEnquadramento>(e.id, (x) => void (x.segue = { ...segue, rastreio: ev.target.value })))}
              >
                {!candidatos.some((r) => r.id === segue.rastreio) && <option value={segue.rastreio} />}
                {candidatos.map((r, i) => (
                  <option key={r.id} value={r.id}>
                    {nomeDoCandidato(c, r, i)}
                  </option>
                ))}
              </select>
            )}
          </div>
        )}
        {segue && e && (
          <Numero
            rotulo="Suavizar"
            item={e.id}
            valor={segue.suavizar_s}
            min={0}
            max={SUAVIZAR_MAX_S}
            passo={0.1}
            casas={1}
            valido={(v) => Number.isFinite(v) && v >= 0 && v <= SUAVIZAR_MAX_S}
            aplicar={(v) => noEfeito<EfEnquadramento>(e.id, (x) => void (x.segue = { rastreio: x.segue?.rastreio ?? segue.rastreio, suavizar_s: v }))}
          />
        )}
        {e && (!animada || e.ajuste === "conter") && (
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
        )}
        {e && (
          <Escolha
            rotulo="Interpolação"
            larga
            valor={e.interpolacao}
            opcoes={[
              // "vizinho" só com região fixa (validação)
              ...(animada || ks ? [] : [{ v: "vizinho" as const, r: "Vizinho" }]),
              { v: "bilinear" as const, r: "Bilinear" },
              { v: "bicubica" as const, r: "Bicúbica" },
            ]}
            aplicar={(v) => noEfeito<EfEnquadramento>(e.id, (x) => void (x.interpolacao = v))}
          />
        )}
      </Grade>
    </Secao>
  );
}
