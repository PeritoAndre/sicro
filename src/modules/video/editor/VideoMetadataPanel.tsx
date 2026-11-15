/** Metadados técnicos do ffprobe + avisos do probe + proveniência da mídia (§8.6 Mídia; F6, P1.10). */

import { useEffect, useId, useRef, useState, type CSSProperties } from "react";
import { AlertTriangle, Check, Loader2, RotateCw } from "lucide-react";
import { commands } from "@core/commands";
import type { ProvenienciaMidia } from "@domain/edicao";
import type { VideoMedia } from "@domain/video";
import { selectActiveWorkspacePath, useWorkspaceStore } from "@stores/workspaceStore";
import { separarErro } from "@modules/edicao/store/edicaoStore";
import { Button } from "@components/Button/Button";
import { formatDuration, prettyBytes } from "./format";
import { conferirProveniencia, desvioDoTexto, MAX_GRAVADOR, MAX_OBS, textoDoDesvio } from "./proveniencia";
import styles from "./VideoMetadataPanel.module.css";

interface Props {
  media: VideoMedia;
  warnings: string[];
}

export function VideoMetadataPanel({ media, warnings }: Props) {
  return (
    <section className={styles.panel}>
      <h3 className={styles.title}>Metadados técnicos</h3>
      <dl className={styles.grid}>
        <Row label="Codec" value={media.codec ?? "—"} mono />
        <Row
          label="Resolução"
          value={media.width && media.height ? `${media.width}×${media.height}` : "—"}
        />
        <Row label="Pixel format" value={media.pixel_format ?? "—"} mono />
        <Row label="Duração" value={formatDuration(media.duration_s ?? 0)} mono />
        <Row
          label="FPS declarado"
          value={media.fps_declared ? media.fps_declared.toFixed(3) : "—"}
          mono
        />
        <Row
          label="avg_frame_rate"
          value={media.avg_frame_rate ?? "—"}
          mono
        />
        <Row
          label="r_frame_rate"
          value={media.r_frame_rate ?? "—"}
          mono
        />
        <Row label="time_base" value={media.time_base ?? "—"} mono />
        <Row
          label="Frames (nb_frames)"
          value={media.frame_count != null ? String(media.frame_count) : "—"}
          mono
        />
        <Row
          label="Bitrate"
          value={
            media.bitrate != null
              ? `${(media.bitrate / 1000).toFixed(0)} kbps`
              : "—"
          }
          mono
        />
        <Row label="Tamanho" value={prettyBytes(media.size_bytes)} />
        <Row
          label="SHA-256"
          value={media.sha256}
          mono
          title={media.sha256}
        />
      </dl>
      <ProvenienciaCampos mediaId={media.id} />
      {warnings.length > 0 && (
        <details className={styles.warnings}>
          <summary>{warnings.length} aviso(s) técnico(s)</summary>
          <ul>
            {warnings.map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        </details>
      )}
    </section>
  );
}

function Row({
  label,
  value,
  mono,
  title,
}: {
  label: string;
  value: string;
  mono?: boolean;
  title?: string;
}) {
  return (
    <>
      <dt>{label}</dt>
      <dd className={mono ? styles.mono : undefined} title={title}>
        {value}
      </dd>
    </>
  );
}

// ---- proveniência (Formato do gravador, Observação, Desvio do relógio (s), Gravador) ----

const CAMPO: CSSProperties = {
  width: "100%",
  boxSizing: "border-box",
  background: "var(--sicro-surface-2)",
  color: "var(--sicro-fg)",
  border: "1px solid var(--sicro-border)",
  borderRadius: 4,
  padding: "2px 6px",
  font: "inherit",
};

type Leitura = { tipo: "lendo" } | { tipo: "pronta"; p: ProvenienciaMidia } | { tipo: "erro"; frase: string };
type Gravacao = { estado: "parada" | "gravando" | "gravada" } | { estado: "erro"; frase: string };

/** Campos de texto: cada um com a sua revisão. */
type Texto = "recebido_obs" | "relogio_desvio_s" | "gravador";
const TEXTOS: Texto[] = ["recebido_obs", "relogio_desvio_s", "gravador"];

/** Frase do catálogo; sem ela, o código. */
function fraseDoErro(e: unknown): string {
  const { codigo, frase } = separarErro(e);
  return frase || codigo || "";
}

const iguais = (a: ProvenienciaMidia, b: ProvenienciaMidia) =>
  a.recebido_nativo === b.recebido_nativo && a.recebido_obs === b.recebido_obs && a.relogio_desvio_s === b.relogio_desvio_s && a.gravador === b.gravador;

/** Os campos mostram sempre o que está no banco: leitura que falhou desliga tudo (nunca "não informado"); gravação
 *  que falhou volta ao gravado. Uma gravação por vez. */
function ProvenienciaCampos({ mediaId }: { mediaId: string }) {
  const ws = useWorkspaceStore(selectActiveWorkspacePath);
  const [leitura, setLeitura] = useState<Leitura>({ tipo: "lendo" });
  const [gravacao, setGravacao] = useState<Gravacao>({ estado: "parada" });
  // sobe na resposta do PRÓPRIO campo (o rascunho volta ao gravado mesmo com o mesmo valor); a resposta de um campo
  // não mexe no que está sendo digitado em outro
  const [rev, setRev] = useState<Record<Texto, number>>({ recebido_obs: 0, relogio_desvio_s: 0, gravador: 0 });
  const subir = (campos: readonly string[]) =>
    setRev((r) => {
      const n = { ...r };
      for (const c of TEXTOS) if (campos.includes(c)) n[c] += 1;
      return n;
    });
  const [reler, setReler] = useState(0);
  const chave = `${ws ?? ""}|${mediaId}`;
  const chaveRef = useRef(chave);
  const gravado = useRef<ProvenienciaMidia | null>(null);
  const fila = useRef<Promise<void>>(Promise.resolve());
  const id = useId();

  useEffect(() => {
    chaveRef.current = chave;
    gravado.current = null;
    setLeitura({ tipo: "lendo" });
    setGravacao({ estado: "parada" });
    if (!ws) return;
    let vivo = true;
    commands
      .videoProveniencia(ws, mediaId)
      .then(conferirProveniencia)
      .then(
        (p) => {
          if (!vivo) return;
          gravado.current = p;
          setLeitura({ tipo: "pronta", p });
          subir(TEXTOS);
        },
        (e: unknown) => vivo && setLeitura({ tipo: "erro", frase: fraseDoErro(e) }),
      );
    return () => {
      vivo = false;
    };
  }, [chave, reler]); // eslint-disable-line react-hooks/exhaustive-deps

  const gravar = (m: Partial<ProvenienciaMidia>) => {
    if (!ws) return;
    const k = chave;
    fila.current = fila.current.then(async () => {
      const base = gravado.current;
      if (!base || chaveRef.current !== k) return;
      const novo = { ...base, ...m };
      if (iguais(novo, base)) {
        subir(Object.keys(m));
        return;
      }
      setGravacao({ estado: "gravando" });
      try {
        const p = conferirProveniencia(await commands.videoProvenienciaSalvar(ws, mediaId, novo));
        if (chaveRef.current !== k) return;
        gravado.current = p;
        setLeitura({ tipo: "pronta", p });
        setGravacao({ estado: "gravada" });
      } catch (e) {
        if (chaveRef.current !== k) return;
        setGravacao({ estado: "erro", frase: fraseDoErro(e) });
      }
      subir(Object.keys(m));
    });
  };

  if (!ws) return null;
  const p = leitura.tipo === "pronta" ? leitura.p : null;
  const fora = p === null;
  return (
    <div data-proveniencia={leitura.tipo}>
      {/* leitura que falhou: campos desligados também na aparência */}
      <dl className={`${styles.grid} ${styles.comoEscrito}`} style={{ marginTop: "var(--space-2)", alignItems: "center", opacity: leitura.tipo === "erro" ? 0.5 : 1 }}>
        <dt>
          <label htmlFor={`${id}-f`}>Formato do gravador</label>
        </dt>
        <dd>
          <select
            id={`${id}-f`}
            style={CAMPO}
            disabled={fora}
            value={p?.recebido_nativo == null ? "" : p.recebido_nativo ? "sim" : "nao"}
            onChange={(e) => gravar({ recebido_nativo: e.target.value === "" ? null : e.target.value === "sim" })}
          >
            <option value="" />
            <option value="sim">Sim</option>
            <option value="nao">Não</option>
          </select>
        </dd>
        <dt>
          <label htmlFor={`${id}-o`}>Observação</label>
        </dt>
        <dd>
          <Rascunho id={`${id}-o`} valor={p?.recebido_obs ?? ""} rev={rev.recebido_obs} max={MAX_OBS} disabled={fora} onConfirmar={(t) => (gravar({ recebido_obs: t }), true)} />
        </dd>
        <dt>
          <label htmlFor={`${id}-d`}>Desvio do relógio (s)</label>
        </dt>
        <dd>
          <Rascunho
            id={`${id}-d`}
            decimal
            valor={textoDoDesvio(p?.relogio_desvio_s ?? null)}
            rev={rev.relogio_desvio_s}
            disabled={fora}
            onConfirmar={(t) => {
              const d = desvioDoTexto(t);
              if (d === undefined) return false;
              gravar({ relogio_desvio_s: d });
              return true;
            }}
          />
        </dd>
        <dt>
          <label htmlFor={`${id}-g`}>Gravador</label>
        </dt>
        <dd>
          <Rascunho
            id={`${id}-g`}
            valor={p?.gravador ?? ""}
            rev={rev.gravador}
            max={MAX_GRAVADOR}
            disabled={fora}
            onConfirmar={(t) => (gravar({ gravador: t.trim() === "" ? null : t }), true)}
          />
        </dd>
      </dl>
      <div style={{ display: "flex", justifyContent: "flex-end", gap: 6, minHeight: 16, marginTop: 2 }}>
        {leitura.tipo === "lendo" && <Loader2 size={14} data-estado="lendo" />}
        {leitura.tipo === "erro" && (
          <>
            <span data-estado="erro-leitura" title={leitura.frase} style={{ color: "var(--sicro-danger)", display: "inline-flex" }}>
              <AlertTriangle size={14} />
            </span>
            <Button variant="ghost" size="sm" title="Testar de novo" aria-label="Testar de novo" onClick={() => setReler((n) => n + 1)}>
              <RotateCw size={12} />
            </Button>
          </>
        )}
        {gravacao.estado === "gravando" && <Loader2 size={14} data-estado="gravando" />}
        {gravacao.estado === "gravada" && (
          <span data-estado="gravada" style={{ color: "var(--sicro-success)", display: "inline-flex" }}>
            <Check size={14} />
          </span>
        )}
        {gravacao.estado === "erro" && (
          <span data-estado="erro-gravacao" title={gravacao.frase} style={{ color: "var(--sicro-danger)", display: "inline-flex" }}>
            <AlertTriangle size={14} />
          </span>
        )}
      </div>
    </div>
  );
}

/** Campo de texto que grava ao sair ou com Enter; Esc e recusa (`onConfirmar` falso) voltam ao valor gravado. */
function Rascunho({
  id,
  valor,
  rev,
  disabled,
  decimal,
  max,
  onConfirmar,
}: {
  id: string;
  valor: string;
  rev: number;
  disabled: boolean;
  decimal?: boolean;
  /** Teto do backend (caracteres). */
  max?: number;
  onConfirmar(t: string): boolean;
}) {
  const [t, setT] = useState(valor);
  useEffect(() => setT(valor), [valor, rev]);
  const confirmar = () => {
    if (t !== valor && !onConfirmar(t)) setT(valor);
  };
  return (
    <input
      id={id}
      type="text"
      inputMode={decimal ? "decimal" : undefined}
      maxLength={max}
      style={CAMPO}
      value={t}
      disabled={disabled}
      onChange={(e) => setT(e.target.value)}
      onBlur={confirmar}
      onKeyDown={(e) => {
        if (e.key === "Enter") confirmar();
        else if (e.key === "Escape") setT(valor);
      }}
    />
  );
}
