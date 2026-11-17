/**
 * Folha de contato de uma sugestão (§6.4, P4.5): os recortes do alvo em até `CELULAS` instantes, com os trechos
 * fracos sempre (`edicao_rastreio_tira`). O JPEG vem do cache do caso; cada célula leva ao instante dela. Só depois
 * de a imagem CARREGAR a sugestão pode ser confirmada (`onCarregada`).
 */

import { useEffect, useState } from "react";
import { Loader2, TriangleAlert } from "lucide-react";
import type { Rastreio, Seg, Tira } from "@domain/edicao";
import { commands } from "@core/commands";
import { formatarTempo } from "@components/CampoTempo/tempoTexto";
import { separarErro, useEdicaoStore } from "../store/edicaoStore";
import { urlDoCache } from "../palco/recursos";
import styles from "../inspetor/Inspetor.module.css";

/** Células pedidas (duas fileiras de seis). */
export const CELULAS = 12;

export interface FolhaContatoProps {
  rastreio: Rastreio;
  midiaSha: string;
  /** A imagem da folha deste denso apareceu na tela. */
  onCarregada(densoSha: string): void;
  onIrPara(t: Seg): void;
}

export function FolhaContato({ rastreio, midiaSha, onCarregada, onIrPara }: FolhaContatoProps) {
  const ws = useEdicaoStore((s) => s.ws);
  const projetoId = useEdicaoStore((s) => s.aberta?.projetoId ?? null);
  const raizCache = useEdicaoStore((s) => s.raizCache);
  const sha = rastreio.denso?.sha256 ?? null;
  const [tira, setTira] = useState<{ sha: string; t: Tira } | null>(null);
  const [erro, setErro] = useState<string | null>(null);

  useEffect(() => {
    if (!ws || !projetoId || !sha) return;
    let vivo = true;
    setErro(null);
    commands
      .edicaoRastreioTira(ws, projetoId, midiaSha, sha, CELULAS)
      .then((t) => vivo && setTira({ sha, t }))
      .catch((e) => {
        if (!vivo) return;
        const r = separarErro(e);
        setErro(r.frase || r.codigo || "");
      });
    return () => {
      vivo = false;
    };
  }, [ws, projetoId, midiaSha, sha]);

  if (erro)
    return (
      <span title={erro} aria-label={erro} data-folha="erro" style={{ display: "inline-flex", color: "var(--sicro-danger)" }}>
        <TriangleAlert size={13} aria-hidden />
      </span>
    );
  if (!tira || tira.sha !== sha || !raizCache) return <Loader2 size={13} className={styles.dim} aria-hidden data-folha="carregando" />;
  const { t } = tira;
  const fileiras = Math.ceil(t.tempos.length / Math.max(1, t.colunas));
  return (
    <div data-folha={tira.sha} style={{ position: "relative", width: "100%", lineHeight: 0 }}>
      <img src={urlDoCache(raizCache, t.relativo)} alt="" style={{ width: "100%", display: "block", borderRadius: 4 }} onLoad={() => onCarregada(tira.sha)} />
      <div style={{ position: "absolute", inset: 0, display: "grid", gridTemplateColumns: `repeat(${t.colunas}, 1fr)`, gridTemplateRows: `repeat(${fileiras}, 1fr)` }}>
        {t.tempos.map((s, i) => (
          <button
            key={i}
            type="button"
            title={formatarTempo(s)}
            aria-label={formatarTempo(s)}
            data-celula={i}
            onClick={() => onIrPara(s)}
            style={{ background: "transparent", border: 0, padding: 0, cursor: "pointer" }}
          />
        ))}
      </div>
    </div>
  );
}
