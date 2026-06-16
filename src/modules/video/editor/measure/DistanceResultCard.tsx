/**
 * DistanceResultCard — apresenta UMA medição de distância como ESTIMATIVA.
 *
 * Distância de 2 pontos NÃO tem intervalo de confiança de regressão: a única
 * fonte de incerteza é o Monte Carlo. Sem σ informado, o card mostra só a
 * distância pontual e deixa EXPLÍCITO que não há intervalo (não esconde a
 * ausência de incerteza). As ressalvas vêm em destaque, e o enquadramento é
 * sempre "auxílio de medição — a conclusão é do perito".
 */

import type { VideoDistanceMeasurement } from "@domain/video_distance";
import styles from "../speed/SpeedPanel.module.css";

/** Metros com 2 casas e vírgula decimal (pt-BR): 2.347912 → "2,35 m". */
function mt(value: number): string {
  return `${value.toFixed(2).replace(".", ",")} m`;
}

export function DistanceResultCard({ m }: { m: VideoDistanceMeasurement }) {
  const hasMc = m.mc_p2_5_m != null && m.mc_p97_5_m != null;

  return (
    <div className={styles.resultCard}>
      <div className={styles.resultHead}>
        <div className={styles.resultBig}>
          {mt(m.distance_m)} <small>(estimativa)</small>
        </div>
        <div className={styles.resultSub}>
          p1 ({m.p1_px.toFixed(0)}, {m.p1_py.toFixed(0)}) → p2 ({m.p2_px.toFixed(0)},{" "}
          {m.p2_py.toFixed(0)}) px
        </div>
      </div>

      <p className={styles.disclaimer}>
        Estimativa de <strong>auxílio de medição</strong>. O valor não é
        definitivo — cabe ao perito confirmá-lo à luz das demais evidências.
      </p>

      <div className={styles.intervals}>
        <div className={styles.intervalBox}>
          <span className={styles.intervalLabel}>
            Intervalo Monte Carlo (p2,5–p97,5)
          </span>
          {hasMc ? (
            <>
              <strong>
                {mt(m.mc_p2_5_m!)} – {mt(m.mc_p97_5_m!)}
              </strong>
              <span className={styles.intervalMeta}>
                {m.mc_mean_m != null && <>média {mt(m.mc_mean_m)} · </>}
                {m.mc_n ?? 0} iterações
                {m.mc_failed ? ` · ${m.mc_failed} descartadas` : ""}
                {m.mc_seed != null && ` · seed ${m.mc_seed}`}
              </span>
            </>
          ) : (
            <span className={styles.intervalNone}>
              não executado — σ não informados; resultado é a distância pontual,
              SEM intervalo de incerteza
            </span>
          )}
        </div>
      </div>

      {m.limitations.length > 0 && (
        <div className={styles.limitations}>
          <span className={styles.limitationsTitle}>
            Ressalvas técnicas ({m.limitations.length})
          </span>
          <ul>
            {m.limitations.map((l, i) => (
              <li key={i}>{l}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
