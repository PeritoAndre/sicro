/**
 * Inspetor da Edição (§8.2, §8.3): abas "Clipe", "Áudio", "Legendas", "Saída". Lê tudo da store (documento,
 * seleção, análise); a aba "Clipe" mostra as seções do item em foco pelo registro. Sem props.
 */

import { useEffect, type FC, type ReactNode } from "react";
import { Captions, FileQuestion, MousePointerClick } from "lucide-react";
import type { ClipePrincipal, ClipeSobreposto, Desconhecido, ItemSel, ProjetoEdicao } from "@domain/edicao";
import { useEdicaoStore } from "../store/edicaoStore";
import { clipeConhecido, sobrepostoConhecido } from "../modelo/projeto";
import { midiaDoClipe, type ClipeQualquer } from "./acoes";
import { useContextoClipe, useInvalidos, useMidiaInfo, useShaDe, useTabela } from "./contexto";
import { secoesDe } from "./registro";
import { SoLeitura } from "./secoes/comum";
import { AudioSecao } from "./secoes/AudioSecao";
import { SaidaSecao } from "./secoes/SaidaSecao";
// Fase 1: cada pacote liga o seu ao entregar (contratos-fase1 (c))
import { FECHO_PRONTO, FechoSecao } from "./secoes/EntradaSecao";
import { SELOS_PRONTOS, SelosSecao } from "./secoes/SelosSecao";
import { PAINEL_AUDIO_PRONTO, PainelAudio } from "../audio/PainelAudio";
import { LEGENDAS_PRONTAS, PainelLegendas } from "../legendas/PainelLegendas";
import { useInspetorUi, type Aba } from "./ui";
import styles from "./Inspetor.module.css";

const ABAS: { v: Aba; r: string }[] = [
  { v: "clipe", r: "Clipe" },
  { v: "audio", r: "Áudio" },
  { v: "legendas", r: "Legendas" },
  { v: "saida", r: "Saída" },
];

export function Inspetor() {
  const aba = useInspetorUi((s) => s.aba);
  const abrirAba = useInspetorUi((s) => s.abrirAba);
  const foco = useEdicaoStore((s) => s.selecao.foco);
  const aberto = useEdicaoStore((s) => !!s.doc);
  // item novo em foco: a aba dele; a marcação pendente do item anterior se desliga
  useEffect(() => {
    useInspetorUi.getState().cancelarMarcacao();
    if (foco?.k === "audio") abrirAba("audio");
    else if (foco?.k === "clipe" || foco?.k === "livre") abrirAba("clipe");
  }, [foco?.k, foco?.id, abrirAba]);
  return (
    <div className={styles.raiz} data-inspetor>
      <div className={styles.abas} role="tablist">
        {ABAS.map((a) => (
          <button
            key={a.v}
            type="button"
            role="tab"
            aria-selected={aba === a.v}
            className={`${styles.aba} ${aba === a.v ? styles.abaOn : ""}`}
            data-aba={a.v}
            onClick={() => abrirAba(a.v)}
          >
            {a.r}
          </button>
        ))}
      </div>
      <div className={styles.corpo} role="tabpanel" data-painel={aba}>
        {aberto && aba === "clipe" && <AbaClipe />}
        {aberto && aba === "audio" && (PAINEL_AUDIO_PRONTO ? <PainelAudio /> : <AudioSecao />)}
        {aberto && aba === "legendas" && (LEGENDAS_PRONTAS ? <PainelLegendas /> : <Vazio icone={<Captions size={20} />} />)}
        {aberto && aba === "saida" && (
          <>
            <SaidaSecao />
            {SELOS_PRONTOS && <SelosSecao />}
            {FECHO_PRONTO && <FechoSecao />}
          </>
        )}
      </div>
    </div>
  );
}

function Vazio({ icone }: { icone: ReactNode }) {
  return (
    <div className={styles.vazio} aria-hidden>
      {icone}
    </div>
  );
}

type Achado = { onde: "principal" | "livre"; item: ClipeQualquer | Desconhecido };

export function itemEmFoco(doc: ProjetoEdicao, foco: ItemSel | null): Achado | null {
  if (!foco) return null;
  const tem = (x: unknown) => typeof x === "object" && x !== null && (x as { id?: unknown }).id === foco.id;
  if (foco.k === "clipe") {
    const c = doc.principal.find(tem) as ClipePrincipal | Desconhecido | undefined;
    return c ? { onde: "principal", item: c } : null;
  }
  if (foco.k === "livre") {
    const trilhas = foco.trilha ? doc.sobreposicoes.filter((t) => t.id === foco.trilha) : doc.sobreposicoes;
    for (const t of trilhas) {
      const c = t.clipes.find(tem) as ClipeSobreposto | Desconhecido | undefined;
      if (c) return { onde: "livre", item: c };
    }
  }
  return null;
}

function AbaClipe() {
  const doc = useEdicaoStore((s) => s.doc);
  const foco = useEdicaoStore((s) => s.selecao.foco);
  const invalidos = useInvalidos();
  const achado = doc ? itemEmFoco(doc, foco) : null;
  if (!achado) return <Vazio icone={<MousePointerClick size={20} />} />;
  const { onde, item } = achado;
  const tipo = typeof (item as { tipo?: unknown }).tipo === "string" ? (item as { tipo: string }).tipo : "";
  const conhecido = onde === "principal" ? clipeConhecido(item as ClipePrincipal | Desconhecido) : sobrepostoConhecido(item as ClipeSobreposto | Desconhecido);
  const secoes = conhecido && !invalidos.has((item as { id: string }).id) ? secoesDe(onde, tipo) : null;
  if (!secoes) return tipo ? <SoLeitura tipo={tipo} /> : <Vazio icone={<FileQuestion size={20} />} />;
  return <ClipeAberto clipe={item as ClipeQualquer} secoes={secoes} />;
}

function ClipeAberto({ clipe, secoes }: { clipe: ClipeQualquer; secoes: NonNullable<ReturnType<typeof secoesDe>> }) {
  const ws = useEdicaoStore((s) => s.ws);
  const midia = midiaDoClipe(clipe);
  const sha = useShaDe(midia);
  const info = useMidiaInfo(ws, sha);
  const tabela = useTabela(ws, sha);
  const ctx = useContextoClipe(clipe, info);
  return (
    <>
      {secoes.map((S, i) => {
        const Secao = S as unknown as FC<{ clipe: ClipeQualquer; ctx: typeof ctx; info: typeof info; tabela: typeof tabela }>;
        return <Secao key={`${clipe.id}:${i}`} clipe={clipe} ctx={ctx} info={info} tabela={tabela} />;
      })}
    </>
  );
}
