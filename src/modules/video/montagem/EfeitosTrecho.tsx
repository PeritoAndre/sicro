/** Zoom, destaques e desfoques de um trecho: listas de quadros-chave e alvo de marcação. */

import { Crosshair, Trash2 } from "lucide-react";
import type { MontagemDesfoque, MontagemDestaque, MontagemQuadro, MontagemTrecho, Regiao } from "@domain/montagem";
import { regiaoInteira, novoDesfoque, novoDestaque } from "./montagemModel";
import { mesmoAlvo, type AlvoMarcacao } from "./marcacao";
import { NumInput } from "./campos";
import styles from "./MontagemView.module.css";

const NOME_FORMA = { circulo: "Círculo", seta: "Seta", retangulo: "Retângulo" } as const;

function Quadros({
  quadros,
  comTamanho,
  onChange,
}: {
  quadros: MontagemQuadro[];
  comTamanho: boolean;
  onChange: (q: MontagemQuadro[]) => void;
}) {
  const set = (i: number, p: Partial<MontagemQuadro>) => onChange(quadros.map((k, j) => (j === i ? { ...k, ...p } : k)));
  return (
    <ul className={styles.quadros}>
      {quadros.map((k, i) => (
        <li key={i}>
          <label>
            t
            <NumInput valor={k.t} min={0} passo={0.1} onChange={(v) => set(i, { t: v })} />
          </label>
          <label>
            x
            <NumInput valor={k.x} min={0} max={1} passo={0.01} onChange={(v) => set(i, { x: v })} />
          </label>
          <label>
            y
            <NumInput valor={k.y} min={0} max={1} passo={0.01} onChange={(v) => set(i, { y: v })} />
          </label>
          {comTamanho && (
            <>
              <label>
                w
                <NumInput valor={k.w ?? 0} min={0} max={1} passo={0.01} onChange={(v) => set(i, { w: v })} />
              </label>
              <label>
                h
                <NumInput valor={k.h ?? 0} min={0} max={1} passo={0.01} onChange={(v) => set(i, { h: v })} />
              </label>
            </>
          )}
          <button type="button" title="Remover quadro" aria-label="Remover quadro" onClick={() => onChange(quadros.filter((_, j) => j !== i))}>
            <Trash2 size={13} />
          </button>
        </li>
      ))}
    </ul>
  );
}

function RegiaoCampos({ r, onChange }: { r: Regiao; onChange: (r: Regiao) => void }) {
  return (
    <span className={styles.regiao}>
      {(["x", "y", "w", "h"] as const).map((c) => (
        <label key={c}>
          {c}
          <NumInput valor={r[c]} min={0} max={1} passo={0.01} onChange={(v) => onChange({ ...r, [c]: v })} />
        </label>
      ))}
    </span>
  );
}

export function EfeitosTrecho({
  it,
  alvo,
  onAlvo,
  onChange,
}: {
  it: MontagemTrecho;
  alvo: AlvoMarcacao | null;
  onAlvo: (a: AlvoMarcacao) => void;
  onChange: (p: Partial<MontagemTrecho>) => void;
}) {
  const destaques = it.destaques ?? [];
  const desfoques = it.desfoques ?? [];
  const ativo = (a: AlvoMarcacao) => (mesmoAlvo(alvo, a) ? styles.marcando : "");
  const setDestaque = (id: string, p: Partial<MontagemDestaque>) =>
    onChange({ destaques: destaques.map((d) => (d.id === id ? { ...d, ...p } : d)) });
  const setDesfoque = (id: string, p: Partial<MontagemDesfoque>) =>
    onChange({ desfoques: desfoques.map((d) => (d.id === id ? { ...d, ...p } : d)) });
  const n = (it.zoom ? 1 : 0) + destaques.length + desfoques.length;

  return (
    <details className={styles.efeitos} open={n > 0}>
      <summary>Efeitos{n > 0 ? ` (${n})` : ""}</summary>
      <div className={styles.efeitosBarra}>
        {!it.zoom && (
          <button type="button" className={styles.mini} onClick={() => onChange({ zoom: { inicio: regiaoInteira(), fim: regiaoInteira() } })}>
            Zoom
          </button>
        )}
        {(["circulo", "seta", "retangulo"] as const).map((f) => (
          <button
            key={f}
            type="button"
            className={styles.mini}
            onClick={() => {
              const d = novoDestaque(f);
              onChange({ destaques: [...destaques, d] });
              onAlvo({ item: it.id, tipo: "destaque", id: d.id, ponto: f !== "retangulo" });
            }}
          >
            {NOME_FORMA[f]}
          </button>
        ))}
        <button
          type="button"
          className={styles.mini}
          onClick={() => {
            const d = novoDesfoque();
            onChange({ desfoques: [...desfoques, d] });
            onAlvo({ item: it.id, tipo: "desfoque", id: d.id });
          }}
        >
          Desfoque
        </button>
      </div>

      {it.zoom && (
        <div className={styles.efeito}>
          <div className={styles.efeitoTopo}>
            <strong>Zoom</strong>
            <button type="button" className={styles.mini} onClick={() => onChange({ zoom: null })} title="Remover zoom" aria-label="Remover zoom">
              <Trash2 size={13} />
            </button>
          </div>
          {(["inicio", "fim"] as const).map((ponta) => {
            const a: AlvoMarcacao = { item: it.id, tipo: "zoom", ponta };
            return (
              <div key={ponta} className={styles.efeitoLinha}>
                <button type="button" className={`${styles.mini} ${ativo(a)}`} onClick={() => onAlvo(a)}>
                  <Crosshair size={12} /> {ponta === "inicio" ? "Início" : "Fim"}
                </button>
                <RegiaoCampos
                  r={it.zoom![ponta]}
                  onChange={(r) => onChange({ zoom: { ...it.zoom!, [ponta]: r } })}
                />
              </div>
            );
          })}
        </div>
      )}

      {destaques.map((d) => {
        const a: AlvoMarcacao = { item: it.id, tipo: "destaque", id: d.id, ponto: d.forma !== "retangulo" };
        return (
          <div key={d.id} className={styles.efeito}>
            <div className={styles.efeitoTopo}>
              <strong>{NOME_FORMA[d.forma]}</strong>
              <input className={styles.cor} style={{ width: 40 }} type="color" value={d.cor} onChange={(e) => setDestaque(d.id, { cor: e.target.value })} />
              <button type="button" className={`${styles.mini} ${ativo(a)}`} onClick={() => onAlvo(a)}>
                <Crosshair size={12} /> Marcar
              </button>
              <button type="button" className={styles.mini} onClick={() => onChange({ destaques: destaques.filter((x) => x.id !== d.id) })} title="Remover" aria-label="Remover">
                <Trash2 size={13} />
              </button>
            </div>
            <Quadros quadros={d.quadros} comTamanho={d.forma === "retangulo"} onChange={(q) => setDestaque(d.id, { quadros: q })} />
          </div>
        );
      })}

      {desfoques.map((d) => {
        const a: AlvoMarcacao = { item: it.id, tipo: "desfoque", id: d.id };
        return (
          <div key={d.id} className={styles.efeito}>
            <div className={styles.efeitoTopo}>
              <strong>Desfoque</strong>
              <button type="button" className={`${styles.mini} ${ativo(a)}`} onClick={() => onAlvo(a)}>
                <Crosshair size={12} /> Marcar
              </button>
              <button type="button" className={styles.mini} onClick={() => onChange({ desfoques: desfoques.filter((x) => x.id !== d.id) })} title="Remover" aria-label="Remover">
                <Trash2 size={13} />
              </button>
            </div>
            <Quadros quadros={d.quadros} comTamanho onChange={(q) => setDesfoque(d.id, { quadros: q })} />
          </div>
        );
      })}
    </details>
  );
}
