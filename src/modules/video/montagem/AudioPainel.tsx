/** Áudio da montagem: volume do original e narrações (gravadas no microfone ou importadas). */

import { useEffect, useRef, useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { Mic, Square, Trash2, Upload } from "lucide-react";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { caminhoDoCaso } from "@core/caminhoDoCaso";
import { mediaSrc } from "@core/mediaSrc";
import type { Montagem } from "@domain/montagem";
import { NumInput, TempoInput } from "./campos";
import { novaNarracao } from "./montagemModel";
import { useMontagemStore } from "./montagemStore";
import styles from "./MontagemView.module.css";

const TIPOS = ["audio/webm;codecs=opus", "audio/ogg;codecs=opus", "audio/mp4", "audio/webm"];

export function AudioPainel({
  ws,
  doc,
  inicioAtual,
  onErro,
}: {
  ws: string;
  doc: Montagem;
  /** Posição na linha do tempo onde a narração começa (a prévia da montagem, se aberta). */
  inicioAtual: () => number;
  onErro: (m: string | null) => void;
}) {
  const m = useMontagemStore();
  const [gravando, setGravando] = useState(false);
  const rec = useRef<{ r: MediaRecorder; stream: MediaStream; inicio: number } | null>(null);

  useEffect(
    () => () => {
      rec.current?.stream.getTracks().forEach((t) => t.stop());
    },
    [],
  );

  const importar = async () => {
    onErro(null);
    try {
      const sel = await openFileDialog({
        multiple: false,
        filters: [{ name: "Áudio", extensions: ["mp3", "wav", "m4a", "ogg"] }],
      });
      if (typeof sel !== "string") return;
      const a = await commands.importarAudioMontagem(ws, sel);
      m.adicionarNarracao(novaNarracao(a.arquivo_relativo, a.nome, inicioAtual()));
    } catch (e) {
      onErro(toSicroError(e).message);
    }
  };

  const gravar = async () => {
    onErro(null);
    if (rec.current) {
      rec.current.r.stop();
      return;
    }
    if (typeof MediaRecorder === "undefined" || !navigator.mediaDevices?.getUserMedia) {
      onErro("Este sistema não oferece gravação de microfone ao app. Grave em outro programa e use Importar áudio.");
      return;
    }
    let stream: MediaStream;
    try {
      stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    } catch (e) {
      const nome = e instanceof DOMException ? e.name : "erro";
      onErro(`Microfone indisponível (${nome}). Verifique a permissão do sistema ou use Importar áudio.`);
      return;
    }
    const tipo = TIPOS.find((t) => MediaRecorder.isTypeSupported(t));
    const r = new MediaRecorder(stream, tipo ? { mimeType: tipo } : undefined);
    const partes: Blob[] = [];
    const inicio = inicioAtual();
    r.ondataavailable = (e) => e.data.size > 0 && partes.push(e.data);
    r.onstop = async () => {
      stream.getTracks().forEach((t) => t.stop());
      rec.current = null;
      setGravando(false);
      try {
        const mime = r.mimeType || tipo || "audio/webm";
        const ext = mime.includes("ogg") ? "ogg" : mime.includes("mp4") ? "m4a" : "webm";
        const bytes = new Uint8Array(await new Blob(partes, { type: mime }).arrayBuffer());
        const n = (doc.narracoes?.length ?? 0) + 1;
        const a = await commands.gravarAudioMontagem(ws, Array.from(bytes), ext, `Narração ${n}`);
        m.adicionarNarracao(novaNarracao(a.arquivo_relativo, a.nome, inicio));
      } catch (e) {
        onErro(toSicroError(e).message);
      }
    };
    rec.current = { r, stream, inicio };
    r.start();
    setGravando(true);
  };

  const narracoes = doc.narracoes ?? [];
  return (
    <div className={styles.audio}>
      <div className={styles.audioTopo}>
        <label>
          Volume do original
          <input
            type="range"
            min={0}
            max={2}
            step={0.05}
            value={doc.volume_original ?? 1}
            onChange={(e) => m.volumeOriginal(Number(e.target.value))}
          />
        </label>
        <span className={styles.dim}>{Math.round((doc.volume_original ?? 1) * 100)}%</span>
      </div>
      <div className={styles.barra}>
        <Button leftIcon={gravando ? <Square size={14} /> : <Mic size={14} />} variant={gravando ? "primary" : "secondary"} onClick={() => void gravar()}>
          {gravando ? "Parar gravação" : "Gravar narração"}
        </Button>
        <Button leftIcon={<Upload size={14} />} onClick={() => void importar()}>
          Importar áudio…
        </Button>
      </div>
      <ul className={styles.narracoes}>
        {narracoes.map((n) => (
          <li key={n.id}>
            <strong title={n.nome}>{n.nome}</strong>
            <label>
              Início
              <TempoInput valor={n.inicio_s} onChange={(v) => m.alterarNarracao(n.id, { inicio_s: Math.max(0, v) })} />
            </label>
            <label>
              Volume
              <NumInput valor={n.volume} min={0} max={2} passo={0.1} onChange={(v) => m.alterarNarracao(n.id, { volume: v })} />
            </label>
            <button type="button" title="Remover narração" aria-label="Remover narração" onClick={() => m.removerNarracao(n.id)}>
              <Trash2 size={13} />
            </button>
            <audio controls preload="none" src={mediaSrc(caminhoDoCaso(ws, n.arquivo_relativo))} />
          </li>
        ))}
      </ul>
    </div>
  );
}
