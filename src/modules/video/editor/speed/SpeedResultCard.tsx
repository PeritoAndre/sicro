/**
 * SpeedResultCard — apresenta o resultado de um cálculo de velocidade.
 *
 * Mostra a velocidade de destaque e, QUANDO existirem, os DOIS intervalos:
 *   - IC do ajuste por regressão (Student's t, 95%);
 *   - intervalo Monte Carlo (p2,5–p97,5);
 * além do R², semente e a lista de ressalvas. Deixa explícito que o número
 * é um auxílio de medição — o perito confirma.
 */

import type { VideoSpeedCalculation } from "@domain/video_speed";
import styles from "./SpeedPanel.module.css";

export function SpeedResultCard({ calc }: { calc: VideoSpeedCalculation }) {
  const ms = calc.velocity_kmh / 3.6;
  const hasCi = calc.ci_low != null && calc.ci_high != null;
  const hasMc = calc.mc_p2_5_kmh != null && calc.mc_p97_5_kmh != null;

  return (
    <div className={styles.resultCard}>
      <div className={styles.resultHead}>
        <div className={styles.resultBig}>
          {calc.velocity_kmh.toFixed(1)} <small>km/h</small>
        </div>
        <div className={styles.resultSub}>
          {ms.toFixed(2)} m/s · v<sub>x</sub> {calc.vx_m_per_s.toFixed(2)} · v
          <sub>y</sub> {calc.vy_m_per_s.toFixed(2)} m/s · {calc.points.length} pts
        </div>
      </div>

      <p className={styles.disclaimer}>
        Estimativa de <strong>auxílio de medição</strong>. O valor não é
        definitivo — cabe ao perito confirmá-lo à luz das demais evidências.
      </p>

      <div className={styles.intervals}>
        <div className={styles.intervalBox}>
          <span className={styles.intervalLabel}>IC do ajuste (regressão)</span>
          {hasCi ? (
            <>
              <strong>
                {calc.ci_low!.toFixed(1)} – {calc.ci_high!.toFixed(1)} km/h
              </strong>
              <span className={styles.intervalMeta}>
                confiança {Math.round((calc.confidence ?? 0.95) * 100)}%
                {calc.r_squared != null && <> · R² {calc.r_squared.toFixed(4)}</>}
                {calc.se_m_per_s != null && (
                  <> · EP {calc.se_m_per_s.toFixed(2)} m/s</>
                )}
              </span>
            </>
          ) : (
            <span className={styles.intervalNone}>
              indisponível — 2 pontos não permitem IC estatístico
            </span>
          )}
        </div>

        <div className={styles.intervalBox}>
          <span className={styles.intervalLabel}>
            Intervalo Monte Carlo (p2,5–p97,5)
          </span>
          {hasMc ? (
            <>
              <strong>
                {calc.mc_p2_5_kmh!.toFixed(1)} – {calc.mc_p97_5_kmh!.toFixed(1)} km/h
              </strong>
              <span className={styles.intervalMeta}>
                média {calc.mc_mean_kmh!.toFixed(1)} km/h · {calc.mc_n ?? 0}{" "}
                iterações
                {calc.mc_failed ? ` · ${calc.mc_failed} descartadas` : ""}
                {calc.mc_seed != null && ` · seed ${calc.mc_seed}`}
              </span>
            </>
          ) : (
            <span className={styles.intervalNone}>
              não executado — σ não informados ou calibração por linha
            </span>
          )}
        </div>
      </div>

      {calc.limitations.length > 0 && (
        <div className={styles.limitations}>
          <span className={styles.limitationsTitle}>
            Ressalvas técnicas ({calc.limitations.length})
          </span>
          <ul>
            {calc.limitations.map((l, i) => (
              <li key={i}>{l}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
