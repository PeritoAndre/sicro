/** Aviso de versão nova: notas, baixar e instalar com progresso. */

import { Button } from "@components/Button/Button";
import { Dialog } from "@components/Dialog/Dialog";
import { notasEmTexto } from "./notasTexto";
import { adiarAtualizacao, instalarAtualizacao, useUpdaterStore } from "./updaterStore";
import styles from "./UpdateDialog.module.css";

export function UpdateDialog() {
  const s = useUpdaterStore();
  const aberto = s.aviso && (s.status === "available" || s.status === "downloading" || s.status === "installed");
  if (!aberto) return null;
  const baixando = s.status === "downloading";
  const pct = Math.round(s.progresso * 100);
  return (
    <Dialog
      open
      title={`SICRO ${s.nova} disponível`}
      onClose={() => {
        if (!baixando) adiarAtualizacao();
      }}
      footer={
        s.status === "installed" ? (
          <span className={styles.aviso}>Instalado. Reiniciando o SICRO…</span>
        ) : baixando ? (
          <span className={styles.aviso}>Baixando… {pct}%</span>
        ) : (
          <>
            <Button variant="secondary" onClick={adiarAtualizacao}>
              Depois
            </Button>
            <Button variant="primary" onClick={() => void instalarAtualizacao()}>
              {s.instalavel ? "Atualizar agora" : "Abrir página de download"}
            </Button>
          </>
        )
      }
    >
      <p className={styles.linha}>
        Você usa a {s.atual}. {s.instalavel ? "O SICRO baixa, instala e reabre sozinho." : "Nesta instalação a atualização é manual: baixe o pacote da página e instale por cima."}
      </p>
      {baixando && (
        <div className={styles.barra} role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100}>
          <div className={styles.barraCheia} style={{ width: `${pct}%` }} />
        </div>
      )}
      {s.notas && <pre className={styles.notas}>{notasEmTexto(s.notas)}</pre>}
    </Dialog>
  );
}
