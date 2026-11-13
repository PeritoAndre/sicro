/** Rodapé da Ajuda: versão instalada e "Verificar atualizações". */

import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { RefreshCw } from "lucide-react";
import { useUpdaterStore, verificarAtualizacao } from "@/core/updater/updaterStore";
import styles from "./AjudaModule.module.css";

export function VersaoAtualizacao() {
  const [versao, setVersao] = useState("");
  const status = useUpdaterStore((s) => s.status);
  useEffect(() => {
    getVersion().then(setVersao).catch(() => setVersao(""));
  }, []);
  const ocupado = status === "checking" || status === "downloading";
  return (
    <div className={styles.versao}>
      {versao && <span>SICRO {versao}</span>}
      <button type="button" className={styles.versaoBtn} onClick={() => void verificarAtualizacao(false)} disabled={ocupado}>
        <RefreshCw size={12} className={ocupado ? styles.girando : undefined} />
        {ocupado ? "Verificando…" : "Verificar atualizações"}
      </button>
    </div>
  );
}
