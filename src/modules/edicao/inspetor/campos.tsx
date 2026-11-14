/**
 * Campos do Inspetor. Cada um grava pela store: mudança discreta = um passo (`alterar`); número e cor repetidos
 * em menos de 1 s = um passo (`agrupar`); arrasto e digitação = um passo (`iniciarGesto`). Valor que `valido`
 * recusa não entra no documento (o campo fica marcado). Rótulo de 11 px em cima; nenhum texto de exemplo.
 */

import { useEffect, useId, useRef, useState, type ButtonHTMLAttributes, type KeyboardEvent, type ReactNode } from "react";
import type { Draft } from "immer";
import type { Lugar, ProjetoEdicao, Pts, Seg, TabelaQuadros, Racional } from "@domain/edicao";
import { CampoNumero } from "@components/CampoNumero/CampoNumero";
import { CampoTempo } from "@components/CampoTempo/CampoTempo";
import type { BaseTempo } from "@components/CampoTempo/tempoTexto";
import { useEdicaoStore, type Gesto } from "../store/edicaoStore";
import { mudar } from "./acoes";
import styles from "./Inspetor.module.css";

type Mut = (d: Draft<ProjetoEdicao>) => void;

const useSo = () => useEdicaoStore((s) => s.soLeitura || !s.doc);

interface Base<T> {
  rotulo: string;
  valor: T;
  /** Mudança no documento para o valor novo. */
  aplicar: (v: T) => Mut;
  /** Id do item: chave do agrupamento por tempo. */
  item: string;
  valido?: (v: T) => boolean;
  larga?: boolean;
  desligado?: boolean;
}

export function Secao({ titulo, icone, acoes, children }: { titulo?: string; icone?: ReactNode; acoes?: ReactNode; children: ReactNode }) {
  return (
    <section className={styles.sec}>
      {(titulo || acoes) && (
        <header className={styles.secTopo}>
          {icone}
          {titulo && <h3 className={styles.secTitulo}>{titulo}</h3>}
          {acoes && <span className={styles.acoes}>{acoes}</span>}
        </header>
      )}
      {children}
    </section>
  );
}

export const Grade = ({ children }: { children: ReactNode }) => <div className={styles.grade}>{children}</div>;

/** Muda pelo gesto do campo, abrindo um se não há. Gesto encerrado por fora (outro gesto começou, como uma alça
 *  do palco que não tira o foco do campo) não muda nada: abre outro, senão o que se digita depois se perde. */
function alterarNoGesto(gesto: { current: Gesto | null }, rotulo: string, f: Mut): void {
  const doc = () => useEdicaoStore.getState().doc;
  const antes = doc();
  gesto.current ??= useEdicaoStore.getState().iniciarGesto(rotulo);
  gesto.current.alterar(f);
  if (doc() !== antes) return;
  gesto.current = useEdicaoStore.getState().iniciarGesto(rotulo);
  gesto.current.alterar(f);
}

/** Campo de texto: digitar é um gesto (prévia viva, um passo ao sair do campo; Esc desfaz). */
export function Texto({ rotulo, valor, aplicar, larga, desligado, linhas }: Omit<Base<string>, "item" | "valido"> & { linhas?: number }) {
  const so = useSo();
  const [texto, setTexto] = useState(valor);
  const gesto = useRef<Gesto | null>(null);
  useEffect(() => {
    if (!gesto.current) setTexto(valor);
  }, [valor]);
  const concluir = () => {
    gesto.current?.concluir();
    gesto.current = null;
  };
  useEffect(() => concluir, []);
  const mudou = (v: string) => {
    setTexto(v);
    alterarNoGesto(gesto, rotulo, aplicar(v));
  };
  const props = {
    className: styles.entrada,
    value: texto,
    disabled: so || desligado,
    spellCheck: true,
    onChange: (e: { target: { value: string } }) => mudou(e.target.value),
    onBlur: concluir,
    onKeyDown: (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        gesto.current?.cancelar();
        gesto.current = null;
        setTexto(valor);
      } else if (e.key === "Enter" && !linhas) concluir();
    },
  };
  return (
    <label className={`${styles.campo} ${larga ? styles.larga : ""}`}>
      <span className={styles.rotulo}>{rotulo}</span>
      {linhas ? <textarea {...props} rows={linhas} /> : <input type="text" {...props} />}
    </label>
  );
}

/** Número com vírgula decimal (`CampoNumero`); ± e digitação seguidos = um passo. */
export function Numero({
  rotulo,
  valor,
  aplicar,
  item,
  valido,
  larga,
  desligado,
  min,
  max,
  passo,
  casas,
  unidade,
}: Base<number> & { min?: number; max?: number; passo?: number; casas?: number; unidade?: string }) {
  const so = useSo();
  const [ruim, setRuim] = useState(false);
  useEffect(() => setRuim(false), [valor]);
  return (
    <div className={`${styles.campoNum} ${larga ? styles.larga : ""} ${ruim ? styles.ruim : ""}`} data-campo={rotulo}>
      <CampoNumero
        label={rotulo}
        value={valor}
        min={min}
        max={max}
        step={passo}
        casas={casas}
        unidade={unidade}
        disabled={so || desligado}
        onChange={(v) => {
          if (v === valor) return;
          if (valido && !valido(v)) return setRuim(true);
          setRuim(false);
          mudar(rotulo, aplicar(v), `${item}:${rotulo}`);
        }}
      />
    </div>
  );
}

/** Tempo `HH:MM:SS,mmm` (`CampoTempo`); `#n` e as setas dão o pts do quadro pela tabela. */
export function Tempo({
  rotulo,
  valor,
  aplicar,
  item,
  valido,
  desligado,
  base,
  tabela,
  fps,
  min,
  max,
}: Omit<Base<Seg | null>, "aplicar" | "valido"> & {
  aplicar: (t: Seg, pts: Pts | null) => Mut;
  valido?: (t: Seg) => boolean;
  base: BaseTempo;
  tabela?: TabelaQuadros | null;
  fps?: Racional | null;
  min?: Seg;
  max?: Seg;
}) {
  const so = useSo();
  const [ruim, setRuim] = useState(false);
  useEffect(() => setRuim(false), [valor]);
  return (
    <div className={`${styles.campoNum} ${styles.larga} ${ruim ? styles.ruim : ""}`} data-campo={rotulo}>
      <CampoTempo
        label={rotulo}
        value={valor}
        base={base}
        tabela={tabela}
        fps={fps}
        min={min}
        max={max}
        disabled={so || desligado}
        onChange={(t, pts) => {
          if (valido && !valido(t)) return setRuim(true);
          setRuim(false);
          mudar(rotulo, aplicar(t, pts), `${item}:${rotulo}`);
        }}
      />
    </div>
  );
}

export interface Opcao<T> {
  v: T;
  r?: string;
  icone?: ReactNode;
}

/** Botões lado a lado (uma escolha). Opção só de ícone leva o rótulo do campo como nome acessível. */
export function Escolha<T extends string | number>({ rotulo, valor, aplicar, opcoes, larga, desligado, semRotulo }: Omit<Base<T>, "item" | "valido"> & { opcoes: Opcao<T>[]; semRotulo?: boolean }) {
  const so = useSo();
  const id = useId();
  return (
    <div className={`${styles.campo} ${larga ? styles.larga : ""}`} data-campo={rotulo}>
      {!semRotulo && (
        <span className={styles.rotulo} id={id}>
          {rotulo}
        </span>
      )}
      <div className={styles.seg} role="group" aria-labelledby={semRotulo ? undefined : id} aria-label={semRotulo ? rotulo : undefined}>
        {opcoes.map((o) => (
          <button
            key={String(o.v)}
            type="button"
            className={`${styles.segBtn} ${o.v === valor ? styles.segOn : ""}`}
            aria-pressed={o.v === valor}
            aria-label={o.r ? undefined : rotulo}
            data-valor={String(o.v)}
            disabled={so || desligado}
            onClick={() => o.v !== valor && mudar(rotulo, aplicar(o.v))}
          >
            {o.icone}
            {o.r}
          </button>
        ))}
      </div>
    </div>
  );
}

/** Lista de opções (mais de quatro, ou texto longo). */
export function Lista<T extends string>({ rotulo, valor, aplicar, opcoes, larga, desligado }: Omit<Base<T>, "item" | "valido"> & { opcoes: { v: T; r: string }[] }) {
  const so = useSo();
  return (
    <label className={`${styles.campo} ${larga ? styles.larga : ""}`}>
      <span className={styles.rotulo}>{rotulo}</span>
      <select className={styles.entrada} value={valor} disabled={so || desligado} onChange={(e) => e.target.value !== valor && mudar(rotulo, aplicar(e.target.value as T))}>
        {!opcoes.some((o) => o.v === valor) && <option value={valor} />}
        {opcoes.map((o) => (
          <option key={o.v} value={o.v}>
            {o.r}
          </option>
        ))}
      </select>
    </label>
  );
}

export function Marca({ rotulo, valor, aplicar, larga, desligado }: Omit<Base<boolean>, "item" | "valido">) {
  const so = useSo();
  return (
    <label className={`${styles.check} ${larga ? styles.larga : ""}`}>
      <input type="checkbox" checked={valor} disabled={so || desligado} onChange={(e) => mudar(rotulo, aplicar(e.target.checked))} />
      <span>{rotulo}</span>
    </label>
  );
}

/** Controle deslizante: o arrasto inteiro é um passo; teclado agrupa por tempo. */
export function Deslizante({
  rotulo,
  valor,
  aplicar,
  item,
  larga,
  desligado,
  min,
  max,
  passo,
  mostrar,
  semRotulo,
}: Omit<Base<number>, "valido"> & { min: number; max: number; passo: number; mostrar?: (v: number) => string; semRotulo?: boolean }) {
  const so = useSo();
  const gesto = useRef<Gesto | null>(null);
  const concluir = () => {
    gesto.current?.concluir();
    gesto.current = null;
  };
  useEffect(() => concluir, []);
  const fmt = mostrar ?? ((v: number) => v.toFixed(2).replace(".", ","));
  return (
    <label className={`${styles.campo} ${larga ? styles.larga : ""}`} data-campo={rotulo}>
      {!semRotulo && <span className={styles.rotulo}>{rotulo}</span>}
      <span className={styles.deslizante}>
        <input
          type="range"
          min={min}
          max={max}
          step={passo}
          value={valor}
          disabled={so || desligado}
          aria-label={semRotulo ? rotulo : undefined}
          onPointerDown={() => {
            gesto.current?.concluir();
            gesto.current = useEdicaoStore.getState().iniciarGesto(rotulo);
          }}
          onPointerUp={concluir}
          onPointerCancel={() => {
            gesto.current?.cancelar();
            gesto.current = null;
          }}
          onBlur={concluir}
          onChange={(e) => {
            const v = Number(e.target.value);
            if (!Number.isFinite(v) || v < min || v > max || v === valor) return;
            if (gesto.current) alterarNoGesto(gesto, rotulo, aplicar(v));
            else mudar(rotulo, aplicar(v), `${item}:${rotulo}`);
          }}
        />
        <span className={styles.valor} data-dado>{fmt(valor)}</span>
      </span>
    </label>
  );
}

/** Cor `#rrggbb`; o seletor arrastado = um passo (agrupado). O seletor não tem alfa: o de `#rrggbbaa` fica. */
export function Cor({ rotulo, valor, aplicar, item, larga, desligado }: Omit<Base<string>, "valido">) {
  const so = useSo();
  const seis = /^#[0-9a-f]{6}/i.exec(valor)?.[0] ?? "#000000";
  const alfa = /^#[0-9a-f]{8}$/i.test(valor) ? valor.slice(7).toLowerCase() : "";
  return (
    <label className={`${styles.campo} ${larga ? styles.larga : ""}`}>
      <span className={styles.rotulo}>{rotulo}</span>
      <input
        type="color"
        className={styles.cor}
        value={seis.toLowerCase()}
        disabled={so || desligado}
        onChange={(e) => /^#[0-9a-f]{6}$/i.test(e.target.value) && e.target.value.toLowerCase() !== seis.toLowerCase() && mudar(rotulo, aplicar(e.target.value.toLowerCase() + alfa), `${item}:${rotulo}`)}
      />
    </label>
  );
}

// ---- posição no canvas (Lugar): grade 3×3, com os lugares que "Adicionar" usa (novos.ts) ----

const COLS: [number, Lugar["h"]][] = [
  [0.02, "esq"],
  [0.5, "centro"],
  [0.98, "dir"],
];
const LINHAS: [number, Lugar["v"]][] = [
  [0.03, "topo"],
  [0.5, "meio"],
  [0.94, "base"],
];

export function Posicao({ rotulo, valor, aplicar, larga, desligado, extra }: Omit<Base<Lugar>, "item" | "valido"> & { extra?: ReactNode }) {
  const so = useSo();
  return (
    <div className={`${styles.campo} ${larga ? styles.larga : ""}`} data-campo={rotulo}>
      <span className={styles.rotulo}>{rotulo}</span>
      <span className={styles.posLinha}>
        <span className={styles.posGrade} role="group" aria-label={rotulo}>
          {LINHAS.map(([y, v]) =>
            COLS.map(([x, h]) => {
              const on = valor.h === h && valor.v === v && Math.abs(valor.x - x) < 1e-6 && Math.abs(valor.y - y) < 1e-6;
              return (
                <button
                  key={`${h}${v}`}
                  type="button"
                  className={`${styles.posBtn} ${on ? styles.posOn : ""}`}
                  aria-pressed={on}
                  aria-label={rotulo}
                  data-valor={`${h}-${v}`}
                  disabled={so || desligado}
                  onClick={() => !on && mudar(rotulo, aplicar({ x, y, h, v }))}
                >
                  <span className={styles.posPonto} style={{ justifySelf: h === "esq" ? "start" : h === "dir" ? "end" : "center", alignSelf: v === "topo" ? "start" : v === "base" ? "end" : "center" }} />
                </button>
              );
            }),
          )}
        </span>
        {extra}
      </span>
    </div>
  );
}

/** Botão pequeno de ação (Marcar, Importar…); `ativo` = ferramenta ligada. */
export function Mini({ children, ativo, ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { ativo?: boolean }) {
  const so = useSo();
  return (
    <button type="button" {...rest} disabled={so || rest.disabled} className={`${styles.mini} ${ativo ? styles.miniAtivo : ""} ${rest.className ?? ""}`}>
      {children}
    </button>
  );
}

/** Ação só de ícone: o nome acessível é o rótulo do §8.6. */
export function Icone({ rotulo, children, ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { rotulo: string }) {
  const so = useSo();
  return (
    <button type="button" {...rest} disabled={so || rest.disabled} className={`${styles.icone} ${rest.className ?? ""}`} title={rotulo} aria-label={rotulo}>
      {children}
    </button>
  );
}
