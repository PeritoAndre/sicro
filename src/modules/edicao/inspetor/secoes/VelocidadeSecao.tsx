/**
 * Velocidade do trecho (P3.4): constante (um número) ou rampa (pontos de tempo de fonte e fator, linear entre eles,
 * §3.1). Sem editor de curvas (P4.2). Ligar a rampa deixa o som do clipe mudo (§5.4: mudo por padrão).
 */

import { Plus, Spline, X } from "lucide-react";
import type { Draft } from "immer";
import type { Chave, ClipeTrecho, ProjetoEdicao, Seg, TabelaQuadros } from "@domain/edicao";
import { noClipe } from "../acoes";
import { Icone, Numero, Tempo } from "../campos";
import { useEdicaoStore } from "../../store/edicaoStore";
import styles from "../Inspetor.module.css";
import css from "./VelocidadeSecao.module.css";

/** Velocidade aceita: 0,05× a 16×, com até 3 casas (vira racional exato no Rust). */
export const velocidadeValida = (v: number) => v >= 0.05 && v <= 16 && Math.abs(Math.round(v * 1000) - v * 1000) < 1e-6;

/** Fator do ponto do meio de uma rampa nova: a câmera lenta do aceite (1× → 0,25× → 1×). */
export const FATOR_LENTO = 0.25;
/** Folga (s) do "ponto dentro do trecho" da validação (`validar::EPS`). */
const EPS = 1e-9;

type Mut = (d: Draft<ProjetoEdicao>) => void;

/** Fator linear da rampa em `t` (a ponta fora dos pontos; `degrau` segura), em milésimos entre 0,05 e 16. */
export function fatorEm(pontos: Chave<number>[], t: Seg): number {
  const p = pontos;
  let v = p[0]?.v ?? 1;
  if (p.length > 0 && t >= p[p.length - 1]!.t) v = p[p.length - 1]!.v;
  else
    for (let i = 0; i + 1 < p.length; i++) {
      const a = p[i]!;
      const b = p[i + 1]!;
      if (t >= a.t && t < b.t) {
        v = a.interp === "degrau" || !(b.t > a.t) ? a.v : a.v + ((b.v - a.v) * (t - a.t)) / (b.t - a.t);
        break;
      }
    }
  return Math.min(16, Math.max(0.05, Math.round(v * 1000) / 1000));
}

/** Pontos de uma rampa nova: o fator de agora no início e no fim e 0,25× no cursor (ou no meio do trecho). */
export function rampaNova(c: Pick<ClipeTrecho, "inicio_s" | "fim_s" | "velocidade">, cursor: Seg | null): Chave<number>[] {
  const v = c.velocidade.tipo === "constante" ? c.velocidade.fator : 1;
  const meio = cursor !== null && cursor > c.inicio_s && cursor < c.fim_s ? cursor : (c.inicio_s + c.fim_s) / 2;
  return [
    { t: c.inicio_s, v },
    { t: meio, v: Math.min(v, FATOR_LENTO) },
    { t: c.fim_s, v },
  ];
}

/** Instante de um ponto novo: o cursor, se cai no trecho e não tem ponto; senão o meio do maior vão entre pontos. */
export function instanteNovo(pontos: Chave<number>[], c: Pick<ClipeTrecho, "inicio_s" | "fim_s">, cursor: Seg | null): Seg {
  if (cursor !== null && cursor > c.inicio_s - EPS && cursor < c.fim_s + EPS && !pontos.some((p) => Math.abs(p.t - cursor) < 1e-6)) return cursor;
  const ts = [c.inicio_s, ...pontos.map((p) => p.t), c.fim_s].sort((a, b) => a - b);
  let melhor = 0;
  for (let i = 1; i < ts.length; i++) if (ts[i]! - ts[i - 1]! > ts[melhor + 1]! - ts[melhor]!) melhor = i - 1;
  return (ts[melhor]! + ts[melhor + 1]!) / 2;
}

/** Ordena os pontos pelo tempo (a ordem do Rust: `t` crescente), mantendo a ordem dos iguais. */
const ordenar = (p: Draft<Chave<number>>[]) => p.sort((a, b) => a.t - b.t);

export function VelocidadeCampos({ clipe: c, tabela, travado }: { clipe: ClipeTrecho; tabela: TabelaQuadros | null; travado: boolean }) {
  const id = c.id;
  const so = useEdicaoStore((s) => s.soLeitura || !s.doc);
  const cursor = useEdicaoStore((s) => (s.cursor.modo === "clipe" && s.cursor.clipe === id ? s.cursor.t : null));
  const alterar = (rotulo: string, m: Mut) => useEdicaoStore.getState().alterar(rotulo, m);
  const v = c.velocidade;
  const noTrecho = (t: Seg) => t >= c.inicio_s - EPS && t <= c.fim_s + EPS;

  if (v.tipo === "constante")
    return (
      <div className={css.constante}>
        <Numero
          rotulo="Velocidade"
          item={id}
          valor={v.fator}
          min={0.05}
          max={16}
          passo={0.25}
          casas={!Number.isInteger(v.fator * 100) ? 3 : 2}
          unidade="×"
          valido={velocidadeValida}
          aplicar={(f) => noClipe<ClipeTrecho>(id, (x) => void (x.velocidade = { tipo: "constante", fator: f }))}
        />
        <Icone
          rotulo="Rampa"
          aria-pressed={false}
          disabled={so || travado}
          onClick={() =>
            alterar(
              "Rampa",
              noClipe<ClipeTrecho>(id, (x) => {
                x.velocidade = { tipo: "rampa", pontos: rampaNova(x, cursor) };
                x.audio.mudo = true;
              }),
            )
          }
        >
          <Spline size={14} />
        </Icone>
      </div>
    );

  const pontos = v.pontos;
  const noPonto = (i: number, f: (p: Draft<Chave<number>>, todos: Draft<Chave<number>>[]) => void) =>
    noClipe<ClipeTrecho>(id, (x) => {
      if (x.velocidade.tipo !== "rampa") return;
      const p = x.velocidade.pontos[i];
      if (p) f(p, x.velocidade.pontos);
    });
  return (
    <div className={`${styles.larga} ${css.rampa}`} data-campo="Rampa">
      <div className={css.topo}>
        <span className={styles.rotulo}>Velocidade</span>
        <span className={css.acoes}>
          <Icone
            rotulo="Rampa"
            aria-pressed
            className={styles.iconeOn}
            disabled={so}
            onClick={() => alterar("Rampa", noClipe<ClipeTrecho>(id, (x) => void (x.velocidade = { tipo: "constante", fator: pontos[0] && velocidadeValida(pontos[0].v) ? pontos[0].v : 1 })))}
          >
            <Spline size={14} />
          </Icone>
          <Icone
            rotulo="Adicionar"
            disabled={so || pontos.length >= 64}
            onClick={() => {
              const t = instanteNovo(pontos, c, cursor);
              alterar(
                "Adicionar",
                noClipe<ClipeTrecho>(id, (x) => {
                  if (x.velocidade.tipo !== "rampa") return;
                  x.velocidade.pontos.push({ t, v: fatorEm(pontos, t) });
                  ordenar(x.velocidade.pontos);
                }),
              );
            }}
          >
            <Plus size={14} />
          </Icone>
        </span>
      </div>
      {pontos.map((p, i) => (
        <div key={i} className={css.ponto} data-ponto={i}>
          <Tempo
            rotulo="Tempo"
            item={`${id}:rampa:${i}`}
            valor={p.t}
            base="fonte"
            tabela={tabela}
            desligado={travado}
            valido={noTrecho}
            aplicar={(t) =>
              noPonto(i, (x, todos) => {
                x.t = t;
                ordenar(todos);
              })
            }
          />
          <Numero
            rotulo="Velocidade"
            item={`${id}:rampa:${i}`}
            valor={p.v}
            min={0.05}
            max={16}
            passo={0.25}
            casas={!Number.isInteger(p.v * 100) ? 3 : 2}
            unidade="×"
            valido={velocidadeValida}
            aplicar={(f) => noPonto(i, (x) => void (x.v = f))}
          />
          <Icone
            rotulo="Remover"
            disabled={so || pontos.length <= 1}
            onClick={() =>
              alterar(
                "Remover",
                noClipe<ClipeTrecho>(id, (x) => void (x.velocidade.tipo === "rampa" && x.velocidade.pontos.splice(i, 1))),
              )
            }
          >
            <X size={13} />
          </Icone>
        </div>
      ))}
    </div>
  );
}
