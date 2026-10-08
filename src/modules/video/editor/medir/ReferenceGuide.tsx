/**
 * "O que no chão tem medida conhecida?" — escolhe o tipo de referência, mostra o que
 * clicar e grava a calibração (a mesma de antes: plano, linha ou razão cruzada).
 * Compartilhada pelos modos Velocidade e Distância.
 */

import { useState } from "react";
import { Button } from "@components/Button/Button";
import { toSicroError } from "@core/errors";
import type { VideoMedia } from "@domain/video";
import type { VideoSpeedCalibration } from "@domain/video_speed";
import { useVideoStore } from "../../store/videoStore";
import { buildReference, filaPosicao, fmt, pontosPedidos, VEICULOS } from "./geo";
import { useMedirStore, type RefKind } from "./medirStore";
import styles from "./Medir.module.css";

const ICONS: Record<RefKind, JSX.Element> = {
  retangulo: (
    <svg viewBox="0 0 56 38" aria-hidden>
      <rect width="56" height="38" rx="4" fill="#232f3e" />
      <path d="M14 34 L42 34 L36 6 L20 6 Z" fill="#4b4f55" />
      {[0, 1, 2, 3, 4].map((i) => (
        <path key={i} d={`M${16.5 + i * 5.2} 26 L${19 + i * 5.2} 26 L${19.6 + i * 4.4} 18 L${17.3 + i * 4.4} 18 Z`} fill="#eee" />
      ))}
      <path d="M15.5 26 L41 26 L39 18 L17.4 18 Z" fill="none" stroke="#d7a84f" strokeWidth="1.3" />
    </svg>
  ),
  fila: (
    <svg viewBox="0 0 56 38" aria-hidden>
      <rect width="56" height="38" rx="4" fill="#232f3e" />
      <path d="M14 34 L42 34 L36 6 L20 6 Z" fill="#4b4f55" />
      <g fill="#e3b23c">
        <rect x="27" y="27" width="2" height="5" />
        <rect x="27.3" y="16" width="1.6" height="4" />
        <rect x="27.5" y="8" width="1.2" height="3" />
      </g>
      <g fill="#d7a84f">
        <circle cx="28" cy="32" r="1.5" />
        <circle cx="28" cy="27" r="1.5" />
        <circle cx="28" cy="20" r="1.5" />
      </g>
    </svg>
  ),
  medida: (
    <svg viewBox="0 0 56 38" aria-hidden>
      <rect width="56" height="38" rx="4" fill="#232f3e" />
      <path d="M14 34 L42 34 L36 6 L20 6 Z" fill="#4b4f55" />
      <line x1="18" y1="27" x2="38" y2="27" stroke="#d7a84f" strokeWidth="1.5" />
      <circle cx="18" cy="27" r="2" fill="#d7a84f" />
      <circle cx="38" cy="27" r="2" fill="#d7a84f" />
    </svg>
  ),
  veiculo: (
    <svg viewBox="0 0 56 38" aria-hidden>
      <rect width="56" height="38" rx="4" fill="#232f3e" />
      <rect x="11" y="12" width="34" height="13" rx="4" fill="#8c2230" />
      <rect x="15" y="23" width="7" height="5" rx="1.5" fill="#111" />
      <rect x="35" y="23" width="7" height="5" rx="1.5" fill="#111" />
      <circle cx="18.5" cy="28" r="1.6" fill="#d7a84f" />
      <circle cx="38.5" cy="28" r="1.6" fill="#d7a84f" />
    </svg>
  ),
};

const KINDS: { kind: RefKind; title: string; desc: string; tech: string }[] = [
  { kind: "retangulo", title: "Um retângulo no chão", desc: "Faixa de pedestres, vaga pintada, placa de piso. Você clica nos 4 cantos.", tech: "plano · homografia (DLT)" },
  { kind: "fila", title: "Marcas em fila", desc: "Tracejado da pista: começo e fim de cada traço.", tech: "razão cruzada" },
  { kind: "medida", title: "Uma distância que eu medi", desc: "Dois pontos no chão e a trena do local.", tech: "linha · 2 pontos" },
  { kind: "veiculo", title: "O próprio veículo", desc: "As duas rodas do mesmo lado e o entre-eixos do modelo.", tech: "linha · entre-eixos" },
];

/** Resumo da referência salva, em palavras. */
export function describeCalibration(cal: VideoSpeedCalibration): string {
  const xs = cal.control_points.map((c) => c.world_x_m);
  const ys = cal.control_points.map((c) => c.world_y_m);
  const span = (v: number[]) => Math.max(...v) - Math.min(...v);
  if (cal.method === "plane") return `Retângulo de ${fmt(span(ys), 2)} × ${fmt(span(xs), 2)} m`;
  if (cal.method === "cross_ratio") return `Marcas em fila (${cal.control_points.length} pontos, ${fmt(span(xs), 1)} m)`;
  if (cal.reference_source === "entre_eixos") return `Entre-eixos de ${fmt(span(xs), 2)} m`;
  return `Distância medida de ${fmt(span(xs), 2)} m`;
}

export function ReferenceGuide({
  workspacePath,
  media,
  author,
  onContinue,
  continueLabel,
}: {
  workspacePath: string;
  media: VideoMedia;
  author: string;
  onContinue: () => void;
  continueLabel: string;
}) {
  const cals = useVideoStore((s) => s.speedCalibrations);
  const createCalibration = useVideoStore((s) => s.createCalibration);
  const busy = useVideoStore((s) => s.isMutating);
  const ref = useMedirStore((s) => s.ref);
  const patchRef = useMedirStore((s) => s.patchRef);
  const perito = useMedirStore((s) => s.perito);
  const [error, setError] = useState<string | null>(null);
  const active = cals[0] ?? null;
  const editing = !active || ref.editando;
  const need = pontosPedidos(ref.kind);
  const n = ref.points.length;

  if (!editing && active) {
    const exact = active.method !== "cross_ratio" || active.control_points.length <= 3;
    return (
      <>
        <h3 className={styles.title}>Referência no chão pronta</h3>
        <div className={styles.check}>
          <i className={styles.ok}>✓</i>
          <span>
            <b>{describeCalibration(active)}</b>
            {active.reference_source === "norma_viaria" ? " · medida pela norma (presumida)" : active.reference_source === "campo" ? " · medida no local" : ""}
          </span>
        </div>
        <p className={styles.text}>
          {exact
            ? active.method === "plane"
              ? "Confira com o olho: a grade dourada de 1 m deve deitar certinho no asfalto."
              : "Confira com o olho: os traços da régua dourada marcam cada metro ao longo da referência."
            : `A referência confere com as marcas: erro médio de ${fmt((active.residuals_px ?? 0) * 100, 0)} cm.`}
        </p>
        <div className={styles.row}>
          <Button variant="primary" onClick={onContinue}>
            {continueLabel}
          </Button>
          <Button onClick={() => patchRef({ editando: true, points: [] })}>Refazer a referência</Button>
        </div>
        {perito && (
          <div className={styles.techBox}>
            <div className={styles.kv}>
              <span>método</span>
              <b>{active.method}</b>
              <span>fonte</span>
              <b>{active.reference_source}</b>
              <span>RMS</span>
              <b>{active.residuals_px != null ? `${active.residuals_px.toFixed(4)} m` : "—"}</b>
            </div>
            <span>H = [{active.homography.map((v) => v.toExponential(3)).join(", ")}]</span>
          </div>
        )}
      </>
    );
  }

  const save = async () => {
    setError(null);
    const built = buildReference(ref);
    if (typeof built === "string") {
      setError(built);
      return;
    }
    try {
      await createCalibration(workspacePath, { media_hash: media.sha256, ...built, author });
      patchRef({ editando: false, points: [] });
    } catch (e) {
      setError(toSicroError(e).message);
    }
  };

  const fonteSeg = (
    <div className={styles.field}>
      De onde vem a medida?
      <div className={styles.seg}>
        {([
          ["campo", "Medi no local"],
          ["norma_viaria", "Norma (presumida)"],
        ] as const).map(([k, t]) => (
          <button key={k} type="button" className={ref.fonte === k ? styles.segOn : ""} onClick={() => patchRef({ fonte: k })}>
            {t}
          </button>
        ))}
      </div>
    </div>
  );

  return (
    <>
      <h3 className={styles.title}>O que no chão tem medida conhecida?</h3>
      <div className={styles.cards}>
        {KINDS.map((k) => (
          <button
            key={k.kind}
            type="button"
            className={`${styles.card} ${ref.kind === k.kind ? styles.cardOn : ""}`}
            aria-pressed={ref.kind === k.kind}
            onClick={() => patchRef({ kind: k.kind, points: [], filaPos: [] })}
          >
            {ICONS[k.kind]}
            <span>
              <b>{k.title}</b>
              <small>
                {k.desc}
                {perito && <i className={styles.tech}> · {k.tech}</i>}
              </small>
            </span>
          </button>
        ))}
      </div>

      {ref.kind === "retangulo" && (
        <>
          <p className={styles.text}>
            Clique nos <b>4 cantos</b> no quadro, começando pelo mais perto à esquerda e seguindo no sentido horário.
          </p>
          <div className={styles.fields}>
            <label className={styles.field}>
              Comprimento (ao longo da via, m)
              <input value={ref.comprimento} inputMode="decimal" placeholder="ex.: 4,0" onChange={(e) => patchRef({ comprimento: e.target.value })} />
            </label>
            <label className={styles.field}>
              Largura (de um lado ao outro, m)
              <input value={ref.largura} inputMode="decimal" placeholder="ex.: 6,6" onChange={(e) => patchRef({ largura: e.target.value })} />
            </label>
          </div>
          {fonteSeg}
        </>
      )}

      {ref.kind === "fila" && (
        <>
          <p className={styles.text}>
            Clique no <b>começo e no fim de cada traço</b>, do mais perto para o mais longe. Três cliques bastam; mais cliques deixam a conta mais firme.
          </p>
          <div className={styles.field}>
            Medida do tracejado
            <div className={styles.seg}>
              {([
                ["urbano", "Urbano 2 + 4 m"],
                ["rodovia", "Rodovia 4 + 12 m"],
                ["livre", "Eu informo"],
              ] as const).map(([k, t]) => (
                <button key={k} type="button" className={ref.fila === k ? styles.segOn : ""} onClick={() => patchRef({ fila: k })}>
                  {t}
                </button>
              ))}
            </div>
          </div>
          {ref.fila === "livre" ? (
            n > 0 && (
              <div className={styles.fields}>
                {ref.points.map((_, i) => (
                  <label key={i} className={styles.field}>
                    Ponto {i + 1} (m)
                    <input
                      value={ref.filaPos[i] ?? ""}
                      inputMode="decimal"
                      onChange={(e) => {
                        const next = [...ref.filaPos];
                        next[i] = e.target.value;
                        patchRef({ filaPos: next });
                      }}
                    />
                  </label>
                ))}
              </div>
            )
          ) : (
            n > 0 && (
              <span className={styles.hint}>
                Posições: {ref.points.map((_, i) => fmt(filaPosicao(ref.fila as "urbano" | "rodovia", i), 0)).join(" · ")} m
              </span>
            )
          )}
          {fonteSeg}
        </>
      )}

      {ref.kind === "medida" && (
        <>
          <p className={styles.text}>
            Clique nas <b>duas pontas</b> de uma distância que você mediu no chão.
          </p>
          <label className={styles.field}>
            Distância medida (m)
            <input value={ref.distancia} inputMode="decimal" placeholder="ex.: 10,00" onChange={(e) => patchRef({ distancia: e.target.value })} />
          </label>
          {fonteSeg}
        </>
      )}

      {ref.kind === "veiculo" && (
        <>
          <p className={styles.text}>
            Clique onde a <b>roda da frente</b> e a <b>roda de trás</b> do mesmo lado tocam o chão. Use um quadro em que o veículo aparece de lado.
          </p>
          <div className={styles.fields}>
            <label className={styles.field}>
              Tipo de veículo
              <select
                value={ref.veiculo}
                onChange={(e) => {
                  const v = VEICULOS.find(([k]) => k === e.target.value);
                  patchRef({ veiculo: e.target.value, entreEixos: v ? fmt(v[2], 2) : ref.entreEixos });
                }}
              >
                {VEICULOS.map(([k, t]) => (
                  <option key={k} value={k}>
                    {t}
                  </option>
                ))}
              </select>
            </label>
            <label className={styles.field}>
              Entre-eixos (m)
              <input value={ref.entreEixos} inputMode="decimal" onChange={(e) => patchRef({ entreEixos: e.target.value })} />
            </label>
          </div>
          <span className={styles.hint}>O valor do tipo é uma média; troque pelo do modelo exato quando souber.</span>
        </>
      )}

      <div className={styles.row}>
        <span className={styles.hint}>
          {n} de {need.min === need.max ? need.max : `${need.min}+`} pontos
        </span>
        <Button size="sm" disabled={n === 0} onClick={() => patchRef({ points: ref.points.slice(0, -1) })}>
          Desfazer ponto
        </Button>
        <Button size="sm" disabled={n === 0} onClick={() => patchRef({ points: [] })}>
          Limpar
        </Button>
      </div>
      {error && <span className={styles.error}>{error}</span>}
      <div className={styles.row}>
        <Button variant="primary" disabled={busy || n < need.min} onClick={() => void save()}>
          Salvar referência
        </Button>
        {active && <Button onClick={() => patchRef({ editando: false, points: [] })}>Cancelar</Button>}
      </div>
    </>
  );
}
