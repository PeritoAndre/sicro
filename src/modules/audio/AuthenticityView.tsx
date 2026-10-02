/**
 * Relatório de autenticidade do áudio: estrutura do arquivo e detectores no
 * sinal, com instantes clicáveis e texto para o laudo. Só indícios — não conclui.
 */
import { useState } from "react";
import { ClipboardCopy, ShieldQuestion } from "lucide-react";
import type { AuthenticityReport } from "@domain/audio";
import styles from "./AuthenticityView.module.css";

const fmtTime = (s: number) => {
  const m = Math.floor(s / 60);
  return `${m}:${(s - m * 60).toFixed(2).padStart(5, "0")}`;
};
const fmtT = fmtTime;
const dec = (v: number, d: number) => v.toFixed(d).replace(".", ",");
const kbps = (b: number | null) => (b ? `${Math.round(b / 1000)} kb/s` : "—");

const bandSegments = (r: AuthenticityReport) =>
  r.bandwidth.segments
    .map(
      (g) =>
        `${dec(g.start_s, 0)}–${dec(g.end_s, 0)} s ${g.cutoff_hz != null ? `até ${dec(g.cutoff_hz / 1000, 1)} kHz` : "banda cheia"}`,
    )
    .join("; ");

export function reportToText(r: AuthenticityReport): string {
  // Texto para o laudo: vírgula decimal.
  const fmtT = (t: number) => fmtTime(t).replace(".", ",");
  const L: string[] = [];
  const s = r.structure;
  L.push(`Exame de autenticidade — ${r.source_kind}: ${r.source_file}`);
  if (s) {
    L.push(
      `Contêiner: ${s.format_long || s.format}; duração ${(s.duration_s != null ? dec(s.duration_s, 3) : "?")} s; taxa total ${kbps(s.bit_rate)}.`,
    );
    L.push(
      `Áudio: ${s.codec_long || s.codec}${s.profile ? ` (${s.profile})` : ""}; ${s.sample_rate ?? "?"} Hz; ${s.channels ?? "?"} canal(is)${s.bits_per_sample ? `; ${s.bits_per_sample} bits` : ""}; ${kbps(s.stream_bit_rate)}; ${s.lossless ? "sem perdas" : "com perdas"}.`,
    );
    const tags = { ...s.format_tags, ...s.stream_tags };
    const t = Object.entries(tags);
    if (t.length) L.push(`Metadados: ${t.map(([k, v]) => `${k}=${v}`).join("; ")}.`);
  }
  if (r.packets) {
    L.push(
      `Pacotes: ${r.packets.count}, tamanho ${r.packets.size_mode}; buracos na linha de tempo: ${r.packets.gaps.length}${r.packets.gaps.length ? ` (${r.packets.gaps.map(([a, d]) => `${fmtT(a)} +${dec(d, 3)} s`).join(", ")})` : ""}; sobreposições: ${r.packets.overlaps}.`,
    );
  }
  L.push(`Erros de decodificação: ${r.decode_errors}.`);
  const b = r.bandwidth;
  L.push(
    b.cutoff_hz != null
      ? `Banda: a energia termina em degrau em ${dec(b.cutoff_hz / 1000, 1)} kHz (queda de ${dec(b.drop_db, 0)} dB; limite do arquivo ${dec(b.nyquist_hz / 1000, 1)} kHz).`
      : `Banda: sem corte em degrau até o limite do arquivo (${dec(b.nyquist_hz / 1000, 1)} kHz).`,
  );
  if (b.varies) L.push(`Banda ao longo do arquivo: ${bandSegments(r)}.`);
  L.push(
    `Silêncio digital no meio do som: ${r.digital_silences_total}${r.digital_silences.length ? ` (${r.digital_silences.slice(0, 20).map(([a, d]) => `${fmtT(a)} por ${dec(d * 1000, 0)} ms`).join(", ")}${r.digital_silences_total > 20 ? "…" : ""})` : ""}.`,
  );
  L.push(
    `Mudanças bruscas no ruído de fundo (≥ 10 dB): ${r.noise_jumps.length}${r.noise_jumps.length ? ` (${r.noise_jumps.map((j) => `${fmtT(j.t_s)} ${j.delta_db > 0 ? "+" : ""}${dec(j.delta_db, 0)} dB no ${j.band}`).join(", ")})` : ""}.`,
  );
  L.push(
    `Impulsos isolados (cliques): ${r.clicks_total}${r.clicks.length ? ` (${r.clicks.slice(0, 20).map(fmtT).join(", ")}${r.clicks_total > 20 ? "…" : ""})` : ""}.`,
  );
  L.push("Observações:");
  r.notes.forEach((n) => L.push(`- ${n}`));
  L.push("Os detectores apontam indícios a conferir; não concluem sobre edição.");
  return L.join("\n");
}

export function AuthenticityView({
  report: r,
  onSeek,
}: {
  report: AuthenticityReport;
  onSeek?: (t: number) => void;
}) {
  const [copied, setCopied] = useState(false);
  const s = r.structure;
  const T = ({ t, label }: { t: number; label?: string }) => (
    <button type="button" className={styles.time} onClick={() => onSeek?.(t)} title="Levar o player a este ponto">
      {label ?? fmtT(t)}
    </button>
  );
  const tags = s ? Object.entries({ ...s.format_tags, ...s.stream_tags }) : [];

  return (
    <div className={styles.box}>
      <div className={styles.head}>
        <ShieldQuestion size={14} aria-hidden />
        <strong>Autenticidade</strong>
        <span className={styles.src}>
          {r.source_kind}: <code>{r.source_file}</code>
        </span>
        <button
          type="button"
          className={styles.copy}
          onClick={() => {
            void navigator.clipboard.writeText(reportToText(r)).then(() => {
              setCopied(true);
              window.setTimeout(() => setCopied(false), 1500);
            });
          }}
        >
          <ClipboardCopy size={12} /> {copied ? "copiado!" : "Copiar como texto"}
        </button>
      </div>

      <ul className={styles.notes}>
        {r.notes.map((n, i) => (
          <li key={i}>{n}</li>
        ))}
      </ul>

      <div className={styles.grid}>
        <section>
          <h4>Arquivo</h4>
          {s ? (
            <dl>
              <dt>Contêiner</dt>
              <dd>{s.format_long || s.format}</dd>
              <dt>Codec</dt>
              <dd>
                {s.codec_long || s.codec}
                {s.profile ? ` (${s.profile})` : ""} · {s.lossless ? "sem perdas" : "com perdas"}
              </dd>
              <dt>Formato</dt>
              <dd>
                {s.sample_rate ?? "?"} Hz · {s.channels ?? "?"} canal(is)
                {s.channel_layout ? ` (${s.channel_layout})` : ""}
                {s.bits_per_sample ? ` · ${s.bits_per_sample} bits` : ""}
              </dd>
              <dt>Taxa</dt>
              <dd>
                {kbps(s.stream_bit_rate)} (total {kbps(s.bit_rate)})
              </dd>
              <dt>Duração</dt>
              <dd>
                {s.duration_s?.toFixed(3) ?? "—"} s
                {s.start_time_s ? ` · começa em ${s.start_time_s.toFixed(3)} s` : ""}
              </dd>
              {tags.map(([k, v]) => (
                <span key={k} className={styles.tagRow}>
                  <dt>{k}</dt>
                  <dd>{v}</dd>
                </span>
              ))}
            </dl>
          ) : (
            <p className={styles.dim}>O ffprobe não leu a estrutura do arquivo.</p>
          )}
        </section>

        <section>
          <h4>Pacotes e decodificação</h4>
          {r.packets ? (
            <dl>
              <dt>Pacotes</dt>
              <dd>
                {r.packets.count} · tamanho {r.packets.size_mode} ({r.packets.size_min}–{r.packets.size_max} bytes)
              </dd>
              <dt>Buracos no tempo</dt>
              <dd className={r.packets.gaps.length ? styles.warn : undefined}>
                {r.packets.gaps.length === 0
                  ? "nenhum"
                  : r.packets.gaps.slice(0, 12).map(([a, d], i) => (
                      <span key={i}>
                        <T t={a} /> +{d.toFixed(3)} s{" "}
                      </span>
                    ))}
              </dd>
              <dt>Sobreposições</dt>
              <dd className={r.packets.overlaps ? styles.warn : undefined}>{r.packets.overlaps}</dd>
            </dl>
          ) : (
            <p className={styles.dim}>Sem lista de pacotes.</p>
          )}
          <dl>
            <dt>Erros ao decodificar</dt>
            <dd className={r.decode_errors ? styles.warn : undefined}>
              {r.decode_errors}
              {r.decode_error_samples.length > 0 && (
                <code className={styles.errs}>{r.decode_error_samples.join("\n")}</code>
              )}
            </dd>
          </dl>
        </section>

        <section>
          <h4>Sinal</h4>
          <dl>
            <dt>Banda</dt>
            <dd className={r.bandwidth.cutoff_hz != null || r.bandwidth.varies ? styles.warn : undefined}>
              {r.bandwidth.cutoff_hz != null
                ? `termina em degrau em ${(r.bandwidth.cutoff_hz / 1000).toFixed(1)} kHz (−${r.bandwidth.drop_db.toFixed(0)} dB); o arquivo vai até ${(r.bandwidth.nyquist_hz / 1000).toFixed(1)} kHz`
                : `sem corte em degrau até ${(r.bandwidth.nyquist_hz / 1000).toFixed(1)} kHz (média do arquivo)`}
              {r.bandwidth.varies && (
                <span className={styles.segs}>
                  muda ao longo do arquivo:{" "}
                  {r.bandwidth.segments.map((g, i) => (
                    <span key={i}>
                      <T t={g.start_s} />
                      {g.cutoff_hz != null ? `até ${(g.cutoff_hz / 1000).toFixed(1)} kHz` : "banda cheia"}{" "}
                    </span>
                  ))}
                </span>
              )}
            </dd>
            <dt>Silêncio digital</dt>
            <dd className={r.digital_silences_total ? styles.warn : undefined}>
              {r.digital_silences_total === 0
                ? "nenhum no meio do som"
                : (
                  <>
                    {r.digital_silences_total > 1 && `${r.digital_silences_total}: `}
                    {r.digital_silences.slice(0, 12).map(([a, d], i) => (
                      <span key={i}>
                        <T t={a} /> {(d * 1000).toFixed(0)} ms{" "}
                      </span>
                    ))}
                    {r.digital_silences_total > 12 ? "…" : ""}
                  </>
                )}
            </dd>
            <dt>Ruído de fundo</dt>
            <dd className={r.noise_jumps.length ? styles.warn : undefined}>
              {r.noise_jumps.length === 0
                ? "sem mudança brusca"
                : r.noise_jumps.map((j, i) => (
                    <span key={i}>
                      <T t={j.t_s} /> {j.delta_db > 0 ? "+" : ""}
                      {j.delta_db.toFixed(0)} dB no {j.band}{" "}
                    </span>
                  ))}
            </dd>
            <dt>Cliques</dt>
            <dd className={r.clicks_total ? styles.warn : undefined}>
              {r.clicks_total === 0
                ? "nenhum"
                : (
                  <>
                    {r.clicks_total}:{" "}
                    {r.clicks.slice(0, 30).map((t, i) => (
                      <T key={i} t={t} />
                    ))}
                    {r.clicks_total > 30 ? " …" : ""}
                  </>
                )}
            </dd>
          </dl>
        </section>
      </div>
      <p className={styles.dim}>
        Indícios a conferir ouvindo e com o ENF — nenhum teste aqui conclui que houve ou não edição.
      </p>
    </div>
  );
}
