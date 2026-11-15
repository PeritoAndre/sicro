/**
 * Configurações → ffmpeg (§5.6, contratos-fase2 (f)): caminho do ffmpeg escolhido pelo perito; vazio = automático
 * (o embutido, senão o do sistema). `SICRO_FFMPEG` definido vence sempre (no Rust).
 */
import { useEffect, useState } from "react";
import { FolderOpen, RotateCcw } from "lucide-react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { Button } from "@components/Button/Button";
import { toSicroError } from "@core/errors";
import { useSettingsStore } from "@stores/settingsStore";
import styles from "./CampoFfmpeg.module.css";

export function CampoFfmpeg() {
  const salvo = useSettingsStore((s) => s.settings.paths.ffmpeg_path);
  const [texto, setTexto] = useState(salvo);
  const [erro, setErro] = useState<string | null>(null);

  useEffect(() => setTexto(salvo), [salvo]);

  const gravar = async (caminho: string) => {
    const v = caminho.trim();
    setTexto(v);
    const st = useSettingsStore.getState();
    if (v === st.settings.paths.ffmpeg_path) return;
    setErro(null);
    try {
      await st.persist({ ...st.settings, paths: { ...st.settings.paths, ffmpeg_path: v } });
    } catch (e) {
      setErro(toSicroError(e).message);
    }
  };

  const escolher = async () => {
    const p = await openFileDialog({ multiple: false, directory: false });
    if (typeof p === "string") await gravar(p);
  };

  return (
    <div className={styles.linha}>
      <label className={styles.rotulo} htmlFor="campo-ffmpeg">
        ffmpeg
      </label>
      <input
        id="campo-ffmpeg"
        className={styles.campo}
        value={texto}
        spellCheck={false}
        onChange={(e) => setTexto(e.target.value)}
        onBlur={() => void gravar(texto)}
        onKeyDown={(e) => {
          if (e.key === "Enter") void gravar(texto);
        }}
      />
      <Button variant="ghost" size="sm" leftIcon={<FolderOpen size={13} />} onClick={() => void escolher()}>
        Escolher…
      </Button>
      <Button variant="ghost" size="sm" leftIcon={<RotateCcw size={13} />} onClick={() => void gravar("")} disabled={!salvo}>
        Automático
      </Button>
      {erro && <div className={styles.erroLinha}>{erro}</div>}
    </div>
  );
}
