/**
 * LibreOfficeManagerCard — gerenciador da dependência LibreOffice nas
 * Configurações › Dependências, no mesmo molde de IA/OCR.
 *
 * O LibreOffice é OPCIONAL: habilita exportar um PDF com diagramação "estilo
 * Word" (numeração de página no lugar, dentro de tabela, cabeçalho que repete)
 * a partir do `.docx` que o SICRO já gera. Como é um programa do sistema
 * (~360MB), o card BAIXA o instalador oficial COM barra de progresso (para um
 * cache temporário, FORA do backup) e o ABRE; o perito conclui a instalação e
 * clica "Verificar". §13: opt-in, offline-after-download, nada silencioso.
 */

import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { CheckCircle2, Download, FileType2, RefreshCw } from "lucide-react";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type {
  LibreOfficeProgress,
  LibreOfficeStatus,
} from "@domain/libreoffice";
import styles from "./AiManagerCard.module.css";

function prettyMB(bytes: number): string {
  return `${(bytes / 1024 / 1024).toFixed(0)} MB`;
}

export function LibreOfficeManagerCard() {
  const [status, setStatus] = useState<LibreOfficeStatus | null>(null);
  const [progress, setProgress] = useState<LibreOfficeProgress | null>(null);
  const [busy, setBusy] = useState(false);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setStatus(await commands.getLibreofficeStatus());
    } catch (e) {
      setError(toSicroError(e).message);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    let un: (() => void) | undefined;
    void listen<LibreOfficeProgress>("libreoffice-download-progress", (e) => {
      setProgress(e.payload);
    }).then((u) => {
      un = u;
    });
    return () => un?.();
  }, []);

  const download = async () => {
    setBusy(true);
    setError(null);
    setInfo(null);
    setProgress({ id: "libreoffice", received: 0, total: 0 });
    try {
      await commands.downloadLibreofficeInstaller();
      setInfo(
        "Instalador aberto. Conclua a instalação do LibreOffice e clique em “Verificar”.",
      );
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const check = async () => {
    setChecking(true);
    setError(null);
    try {
      await reload();
    } finally {
      setChecking(false);
    }
  };

  const installed = !!status?.installed;
  const pct =
    progress && progress.total > 0
      ? Math.round((progress.received / progress.total) * 100)
      : null;

  return (
    <section className={styles.card}>
      <div className={styles.head}>
        <FileType2 size={18} aria-hidden />
        <h2 className={styles.title}>LibreOffice (PDF estilo Word)</h2>
      </div>
      <p className={styles.desc}>
        Motor de <strong>exportação em PDF</strong> do laudo. O SICRO gera o{" "}
        <code>.docx</code> e o LibreOffice o converte em PDF com diagramação fiel
        (paginação igual ao Word). Sem ele, a exportação em PDF fica indisponível
        — nesse caso exporte <strong>DOCX</strong> e finalize/gere o PDF no Word.
      </p>
      <ul
        style={{
          margin: "0 0 4px",
          paddingLeft: 18,
          fontSize: "var(--text-xs)",
          color: "var(--sicro-fg-muted)",
          lineHeight: 1.55,
        }}
      >
        <li>
          <strong>PDF estilo Word</strong> — numeração de página no lugar exato,
          dentro de tabela e em cabeçalho que se repete em todas as folhas.
        </li>
        <li>
          <strong>PDF/A</strong> — formato de arquivamento de longo prazo (ISO
          19005) exigido por tribunais/órgãos.
        </li>
        <li>
          <strong>Marcadores navegáveis</strong> no PDF — árvore de seções
          clicável na barra lateral do leitor.
        </li>
        <li>
          <strong>Sumário com números de página</strong> reais (índice).
        </li>
      </ul>

      <div className={styles.statusRow}>
        <span className={installed ? styles.ok : styles.off}>
          {installed && <CheckCircle2 size={13} aria-hidden />} LibreOffice:{" "}
          {installed
            ? status?.version
              ? status.version
              : "instalado"
            : "não instalado"}
        </span>
      </div>

      {error && <div className={styles.error}>{error}</div>}

      <div className={styles.row}>
        <div className={styles.rowMain}>
          <div className={styles.rowLabel}>
            Instalador oficial (LibreOffice {status?.download_version ?? ""})
            {installed && <span className={styles.tagOk}>instalado</span>}
          </div>
          <div className={styles.rowNote}>
            ≈ {status?.approx_mb ?? 360} MB · baixado para um cache temporário
            (não entra no backup). Após baixar, o instalador abre — conclua e
            clique em “Verificar”.
          </div>
          {busy && (
            <div className={styles.progress}>
              <div
                className={styles.progressFill}
                style={{ width: `${pct ?? 0}%` }}
              />
              <span className={styles.progressTxt}>
                {pct != null ? `${pct}%` : "baixando…"}
                {progress && progress.total > 0
                  ? ` (${prettyMB(progress.received)} / ${prettyMB(progress.total)})`
                  : ""}
              </span>
            </div>
          )}
          {info && <div className={styles.rowNote}>{info}</div>}
          {status?.site_url && (
            <div className={styles.rowNote}>
              Preferir instalar manualmente? Baixe em:{" "}
              <code>{status.site_url}</code>
            </div>
          )}
        </div>
        <div className={styles.rowActions}>
          <Button
            variant={installed ? "ghost" : "secondary"}
            size="sm"
            leftIcon={<Download size={13} />}
            onClick={() => void download()}
            disabled={busy}
          >
            {busy ? "Baixando…" : installed ? "Reinstalar / atualizar" : "Baixar"}
          </Button>
        </div>
      </div>

      <div className={styles.footer}>
        <Button
          variant="ghost"
          size="sm"
          leftIcon={<RefreshCw size={13} />}
          onClick={() => void check()}
          disabled={checking}
        >
          {checking ? "Verificando…" : "Verificar"}
        </Button>
        <span className={styles.updateInfo}>
          {installed
            ? "Pronto: a exportação em PDF e PDF/A do laudo está disponível."
            : "Necessário para exportar PDF/PDF-A. Sem ele, use DOCX e finalize no Word."}
        </span>
      </div>
    </section>
  );
}
