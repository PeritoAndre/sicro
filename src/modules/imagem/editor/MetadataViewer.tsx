/** Todos os metadados em tela cheia: resumo em cartões e a lista completa por grupo, em português. */

import { useEffect, useMemo, useRef, useState } from "react";
import { Copy, Loader2, X } from "lucide-react";
import { commands } from "@core/commands";
import type { ImageMetadata } from "@domain/image_analysis";
import { campoPt, grupoPt, valorPt } from "./metadataPt";
import styles from "./MetadataViewer.module.css";

type Entry = { group: string; tag: string; value: string };
type Row = Entry & { label: string; shown: string };

interface Props {
  workspacePath: string;
  relativePath: string;
  hashSet?: ImageMetadata["hash_set"];
  sizeBytes?: number;
  onClose: () => void;
}

export function MetadataViewer({ workspacePath, relativePath, hashSet, sizeBytes, onClose }: Props) {
  const [data, setData] = useState<{ source: string; entries: Entry[] } | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [q, setQ] = useState("");
  const [copied, setCopied] = useState<string | null>(null);
  const [active, setActive] = useState<string | null>(null);
  const sectionRefs = useRef<Record<string, HTMLElement | null>>({});
  const [own, setOwn] = useState<{ hash?: ImageMetadata["hash_set"]; size?: number }>({});
  useEffect(() => {
    if (hashSet) return;
    let cancelled = false;
    commands
      .getImageMetadata(workspacePath, relativePath, true)
      .then((m) => !cancelled && setOwn({ hash: m.hash_set, size: m.size_bytes }))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [hashSet, workspacePath, relativePath]);
  const hashes = hashSet ?? own.hash;
  const bytes = sizeBytes ?? own.size;
  const fileName = relativePath.split(/[\\/]/).pop() ?? relativePath;

  useEffect(() => {
    let cancelled = false;
    commands
      .readAllImageMetadata(workspacePath, relativePath)
      .then((d) => !cancelled && setData(d))
      .catch((e) => !cancelled && setErr(String((e as Error)?.message ?? e)));
    return () => {
      cancelled = true;
    };
  }, [workspacePath, relativePath]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const rows: Row[] = useMemo(
    () => (data?.entries ?? []).map((e) => ({ ...e, label: campoPt(e.tag, e.group), shown: valorPt(e.value) })),
    [data],
  );

  const groups = useMemo(() => {
    const f = q.trim().toLowerCase();
    const map = new Map<string, Row[]>();
    for (const r of rows) {
      if (f && !`${r.label} ${r.tag} ${r.shown}`.toLowerCase().includes(f)) continue;
      const g = grupoPt(r.group);
      map.set(g, [...(map.get(g) ?? []), r]);
    }
    return [...map.entries()];
  }, [rows, q]);

  /** Primeiro valor encontrado entre as tags (em qualquer grupo, na ordem dada). */
  const pick = (...tags: string[]): string | null => {
    for (const t of tags) {
      const hit = rows.find((r) => r.tag === t && r.shown.trim() !== "");
      if (hit) return hit.shown;
    }
    return null;
  };
  const dji = rows.some((r) => r.group === "XMP-drone-dji");

  const copy = (key: string, text: string) => {
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(key);
      window.setTimeout(() => setCopied(null), 1500);
    });
  };
  const copyAll = () =>
    copy(
      "all",
      `Metadados de ${fileName} (${data?.source ?? ""})\n\n` +
        groups.map(([g, items]) => `# ${g}\n${items.map((i) => `${i.label}: ${i.shown}`).join("\n")}`).join("\n\n"),
    );

  const cards: { title: string; lines: [string, string | null][] }[] = [
    {
      title: "Captura",
      lines: [
        ["Data e hora", pick("DateTimeOriginal", "CreateDate", "DateCreated", "ModifyDate")],
        ["Fuso", pick("OffsetTimeOriginal", "OffsetTime")],
        ["Dimensões", (() => {
          const w = pick("ImageWidth", "ExifImageWidth");
          const h = pick("ImageHeight", "ExifImageHeight");
          return w && h ? `${w} × ${h} px` : null;
        })()],
      ],
    },
    {
      title: "Câmera",
      lines: [
        ["Aparelho", [pick("Make"), pick("Model")].filter(Boolean).join(" ") || null],
        ["Exposição", pick("ExposureTime", "ShutterSpeed")],
        ["Abertura", pick("FNumber", "Aperture")],
        ["ISO", pick("ISO")],
        ["Focal", pick("FocalLength")],
      ],
    },
    {
      title: "Local",
      lines: [
        ["Latitude", pick("GPSLatitude")],
        ["Longitude", pick("GPSLongitude")],
        ["Altitude", pick("GPSAltitude")],
      ],
    },
    ...(dji
      ? [
          {
            title: "Voo (DJI)",
            lines: [
              ["Altura relativa", pick("RelativeAltitude")],
              ["Altitude absoluta", pick("AbsoluteAltitude")],
              ["Gimbal: inclinação", pick("GimbalPitchDegree")],
              ["Gimbal: rumo", pick("GimbalYawDegree")],
              ["Aeronave: rumo", pick("FlightYawDegree")],
            ] as [string, string | null][],
          },
        ]
      : []),
  ];

  return (
    <div className={styles.backdrop} onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <section className={styles.sheet} role="dialog" aria-modal="true" aria-label={`Metadados de ${fileName}`}>
        <header className={styles.head}>
          <div className={styles.titles}>
            <h2>Metadados · {fileName}</h2>
            <span>
              {data ? `${data.entries.length} campos · ${data.source}` : err ? "falha na leitura" : "lendo…"}
              {data?.source === "leitor interno" && " · com o exiftool instalado aparecem todos os campos"}
            </span>
          </div>
          <input
            className={styles.filter}
            placeholder="Filtrar por nome ou valor"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            autoFocus
          />
          <button type="button" className={styles.btn} onClick={copyAll} disabled={!data}>
            <Copy size={13} /> {copied === "all" ? "Copiado" : "Copiar tudo"}
          </button>
          <button type="button" className={styles.close} onClick={onClose} title="Fechar (Esc)" aria-label="Fechar">
            <X size={16} />
          </button>
        </header>

        {err && <p className={styles.error}>{err}</p>}
        {!data && !err && (
          <p className={styles.loading}>
            <Loader2 size={14} className={styles.spin} /> Lendo os metadados…
          </p>
        )}

        {data && (
          <>
            <div className={styles.cards}>
              {cards.map((c) => {
                const lines = c.lines.filter(([, v]) => v);
                if (lines.length === 0) return null;
                return (
                  <div key={c.title} className={styles.card}>
                    <h3>{c.title}</h3>
                    <dl>
                      {lines.map(([k, v]) => (
                        <div key={k}>
                          <dt>{k}</dt>
                          <dd>{v}</dd>
                        </div>
                      ))}
                    </dl>
                  </div>
                );
              })}
              {hashes && (
                <div className={styles.card}>
                  <h3>Integridade</h3>
                  <dl>
                    {(
                      [
                        ["MD5", hashes.md5],
                        ["SHA-1", hashes.sha1],
                        ["SHA-256", hashes.sha256],
                        ["SHA3-256", hashes.sha3_256],
                      ] as const
                    ).map(([k, v]) => (
                      <div key={k}>
                        <dt>{k}</dt>
                        <dd>
                          <button type="button" className={styles.hash} onClick={() => copy(k, v)} title="Copiar">
                            {copied === k ? "copiado" : `${v.slice(0, 12)}…`}
                          </button>
                        </dd>
                      </div>
                    ))}
                    {typeof bytes === "number" && (
                      <div>
                        <dt>Tamanho</dt>
                        <dd>{bytes.toLocaleString("pt-BR")} bytes</dd>
                      </div>
                    )}
                  </dl>
                </div>
              )}
            </div>

            <div className={styles.body}>
              <nav className={styles.nav} aria-label="Grupos">
                {groups.map(([g, items]) => (
                  <button
                    key={g}
                    type="button"
                    className={g === active ? styles.navActive : undefined}
                    onClick={() => {
                      setActive(g);
                      sectionRefs.current[g]?.scrollIntoView({ block: "start" });
                    }}
                  >
                    <span>{g}</span>
                    <small>{items.length}</small>
                  </button>
                ))}
              </nav>
              <div className={styles.list}>
                {groups.length === 0 && <p className={styles.loading}>Nada com “{q}”.</p>}
                {groups.map(([g, items]) => (
                  <section key={g} ref={(el) => (sectionRefs.current[g] = el)} className={styles.group}>
                    <h4>
                      {g} <small>{items.length}</small>
                    </h4>
                    <table>
                      <tbody>
                        {items.map((r, i) => (
                          <tr key={`${r.group}:${r.tag}:${i}`}>
                            <th>{r.label}</th>
                            <td>{r.shown}</td>
                            <td className={styles.orig}>{r.tag}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </section>
                ))}
              </div>
            </div>
          </>
        )}
      </section>
    </div>
  );
}
