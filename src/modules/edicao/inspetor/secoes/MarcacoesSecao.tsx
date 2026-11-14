/**
 * Clipe · Marcações: efeitos de região e anotação do clipe, mais as ocultações da mídia que valem nele (trecho e
 * quadro congelado).
 * Cada linha: ícone, rótulo, ligar, remover; aberta (selecionada) traz "De", "Até", "Marcar" e o editor do tipo.
 * Item desconhecido ou inválido: linha apagada com o tipo, sem editor.
 */

import { ChevronDown, ChevronRight, Crosshair, Eye, EyeOff, Film, Plus, Trash2 } from "lucide-react";
import { useContextMenu } from "@components/ContextMenu/ContextMenu";
import { useEffect, useRef, type MouseEvent } from "react";
import type { ContextoClipe, Desconhecido, Efeito, Intervalo, TabelaQuadros } from "@domain/edicao";
import { useEdicaoStore } from "../../store/edicaoStore";
import { aplicarMudanca, efeitoConhecido } from "../../modelo/projeto";
import {
  adicionarEfeito,
  adicionarImagem,
  ligarEfeito,
  marcarAncora,
  mudarCom,
  noEfeito,
  rastreioDe,
  removerEfeito,
  selecionarEfeito,
  type ClipeQualquer,
} from "../acoes";
import { pedirMarcacao, useInvalidos } from "../contexto";
import { Icone, Mini, Secao, Tempo } from "../campos";
import { ADICIONAR, descritor, REGISTRO, type SecaoProps } from "../registro";
import { useInspetorUi } from "../ui";
import { escolherMidia } from "./importar";
import styles from "../Inspetor.module.css";

interface Item {
  e: Efeito | Desconhecido;
  daMidia: boolean;
}

/** A ocultação da mídia vale no clipe quando o `ativo` (tempo de fonte) cruza a faixa dele. */
const cruza = (a: Intervalo | null, de: number, ate: number) => !a || ((a.de ?? -Infinity) < ate && (a.ate ?? Infinity) > de);

export function MarcacoesSecao({ clipe: c, ctx, tabela }: SecaoProps<ClipeQualquer>) {
  const midias = useEdicaoStore((s) => s.doc?.midias);
  const aberto = useEdicaoStore((s) => s.selecao.efeito);
  const invalidos = useInvalidos();
  const menu = useContextMenu();
  // enquadre tem seção própria; realce (origem) também
  const itens: Item[] = c.efeitos.filter((e) => !(efeitoConhecido(e) && ["enquadre", "origem"].includes(REGISTRO[e.tipo].familia))).map((e) => ({ e, daMidia: false }));
  if (c.tipo === "trecho" || c.tipo === "congelar") {
    const m = midias?.find((x) => x.id === c.midia);
    // no quadro congelado vale a do instante dele (D17: a ocultação da mídia vale em todo uso)
    const vale = (a: Intervalo | null) => (c.tipo === "trecho" ? cruza(a, c.inicio_s, c.fim_s) : !a || ((a.de ?? -Infinity) <= c.instante_s && c.instante_s < (a.ate ?? Infinity)));
    for (const o of m?.ocultacoes ?? []) if (vale(o.ativo)) itens.push({ e: o, daMidia: true });
  }
  const adicionar = (ev: MouseEvent<HTMLButtonElement>) => {
    const r = ev.currentTarget.getBoundingClientRect();
    menu.open(
      { clientX: r.left, clientY: r.bottom + 2, preventDefault() {}, stopPropagation() {} },
      ADICIONAR.map((tipo) => {
        const d = REGISTRO[tipo];
        const Ic = d.icone;
        return {
          label: d.rotulo,
          icon: <Ic size={12} />,
          onSelect: () => {
            if (tipo === "imagem") void escolherMidia("imagem").then((m) => m && adicionarImagem(c.id, m, d.rotulo));
            else adicionarEfeito(c.id, tipo, d.rotulo);
          },
        };
      }),
    );
  };
  return (
    <Secao
      titulo="Marcações"
      acoes={
        <Mini onClick={adicionar} data-acao="adicionar">
          <Plus size={12} /> Adicionar
        </Mini>
      }
    >
      {menu.element}
      {itens.length > 0 && (
        <ul className={styles.efeitos}>
          {itens.map(({ e, daMidia }, i) => (
            <Linha key={(e as { id?: string }).id ?? `x${i}`} e={e} daMidia={daMidia} aberta={aberto === (e as { id?: string }).id} invalidos={invalidos} clipe={c} ctx={ctx} tabela={tabela} />
          ))}
        </ul>
      )}
    </Secao>
  );
}

/** Clipe · Realce: os efeitos de origem do clipe, sem editor nesta fase (o editor vem com o P1.4, `RealceSecao`). */
export function RealceSoLeitura({ clipe: c, ctx, tabela }: SecaoProps<ClipeQualquer>) {
  const invalidos = useInvalidos();
  const itens = c.efeitos.filter((e) => efeitoConhecido(e) && REGISTRO[e.tipo].familia === "origem");
  if (itens.length === 0) return null;
  return (
    <Secao titulo="Realce">
      <ul className={styles.efeitos}>
        {itens.map((e, i) => (
          <Linha key={(e as { id?: string }).id ?? `r${i}`} e={e} daMidia={false} aberta={false} invalidos={invalidos} clipe={c} ctx={ctx} tabela={tabela} />
        ))}
      </ul>
    </Secao>
  );
}

function Linha({ e, daMidia, aberta, invalidos, clipe, ctx, tabela }: { e: Efeito | Desconhecido; daMidia: boolean; aberta: boolean; invalidos: Set<string>; clipe: ClipeQualquer; ctx: ContextoClipe | null; tabela: TabelaQuadros | null }) {
  const { d, conhecido, editavel } = descritor(e, invalidos);
  const id = (e as { id?: string }).id;
  // só o editável abre; o conhecido de outra fase liga, desliga e sai, sem editor
  const ef = efeitoConhecido(e) && editavel ? e : null;
  const Ic = d.icone;
  const chave = useInspetorUi((s) => s.chave);
  const rastreio = ef ? rastreioDe(ef) : null;
  const ligado = efeitoConhecido(e) ? e.ligado : true;
  // ocultação da mídia: `ativo` sempre em tempo de fonte
  const fonte = clipe.tipo === "trecho" || daMidia;
  const li = useRef<HTMLLIElement>(null);
  // efeito aberto de fora (palco, diálogo de exportar): rola até ele
  useEffect(() => {
    if (aberta && ef) li.current?.scrollIntoView?.({ block: "nearest" });
  }, [aberta, ef]);
  return (
    <li ref={li} className={`${styles.efeito} ${aberta && ef ? styles.efeitoAberto : ""} ${!conhecido ? styles.efeitoApagado : ""}`} data-efeito={d.tipo}>
      <div className={styles.efeitoTopo}>
        <button type="button" className={styles.efeitoNome} disabled={!ef || !id} onClick={() => id && selecionarEfeito(aberta ? null : id)} aria-expanded={!!ef && aberta}>
          {ef ? aberta ? <ChevronDown size={12} aria-hidden /> : <ChevronRight size={12} aria-hidden /> : <span className={styles.semSeta} aria-hidden />}
          <Ic size={13} aria-hidden />
          {conhecido ? <span>{d.rotulo}</span> : <code>{d.rotulo}</code>}
          {daMidia && <Film size={11} className={styles.dim} aria-hidden data-da-midia />}
        </button>
        <span className={styles.acoes}>
          {conhecido && efeitoConhecido(e) && (
            <Icone rotulo={d.rotulo} aria-pressed={ligado} onClick={() => ligarEfeito(e.id, !ligado, d.rotulo)}>
              {ligado ? <Eye size={13} /> : <EyeOff size={13} />}
            </Icone>
          )}
          {id && (
            <Icone rotulo="Remover" onClick={() => removerEfeito(id, "Remover")}>
              <Trash2 size={13} />
            </Icone>
          )}
        </span>
      </div>
      {aberta && ef && ctx && (
        <div className={styles.efeitoCorpo}>
          <div className={styles.grade}>
            <Tempo
              rotulo="De"
              item={ef.id}
              valor={ef.ativo?.de ?? null}
              base={fonte ? "fonte" : "programa"}
              tabela={fonte ? tabela : null}
              valido={(t) => ef.ativo?.ate == null || t < ef.ativo.ate}
              aplicar={(t) => noEfeito(ef.id, (x) => void (x.ativo = { de: t, ate: x.ativo?.ate ?? null }))}
            />
            <Tempo
              rotulo="Até"
              item={ef.id}
              valor={ef.ativo?.ate ?? null}
              base={fonte ? "fonte" : "programa"}
              tabela={fonte ? tabela : null}
              valido={(t) => ef.ativo?.de == null || t > ef.ativo.de}
              aplicar={(t) => noEfeito(ef.id, (x) => void (x.ativo = { de: x.ativo?.de ?? null, ate: t }))}
            />
            {rastreio && (
              <div className={styles.larga}>
                <Mini
                  ativo={chave === `${ef.id}:caixa`}
                  data-acao="marcar"
                  onClick={() =>
                    pedirMarcacao({ tipo: "caixa", rastreio, efeito: ef.id }, (v, t, pts) => "w" in v && marcarAncora(rastreio, { t, pts, v }, "Marcar"), `${ef.id}:caixa`)
                  }
                >
                  <Crosshair size={12} /> Marcar
                </Mini>
              </div>
            )}
          </div>
          <d.Editor
            valor={ef}
            ctx={ctx}
            onChange={(novo, op) => mudarCom(op?.rotulo ?? d.rotulo, (dd) => void aplicarMudanca(dd, { efeito: novo }), op)}
            onRemover={() => removerEfeito(ef.id, "Remover")}
          />
        </div>
      )}
    </li>
  );
}
