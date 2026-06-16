/**
 * DigitalSignatureDialog — assinatura digital do laudo.
 *
 * Beta — dois fluxos institucionais reais:
 *   - **SIGDOCS** (default): sistema oficial de tramitação do Estado
 *     do Amapá. Exporta PDF, abre o portal embutido para login + assinatura,
 *     e arquiva o PDF assinado via `import_signed_pdf`.
 *   - **gov.br**: exporta PDF, abre o portal `assinador.iti.gov.br` no
 *     browser do SO, perito assina lá, depois volta e clica
 *     "Importar PDF assinado". O backend Tauri grava em
 *     `laudos/<id>/assinados/` e devolve hash + caminho.
 *
 * Os fluxos A1 (.pfx) / A3 (token) foram removidos da UI beta — a
 * integração real PKCS#12 / PKCS#11 fica para um MVP futuro. O tipo
 * `"mock"` permanece no schema apenas para compatibilidade com laudos
 * antigos que já tenham sido assinados em modo demonstração.
 */

import { useState } from "react";
import {
  ShieldCheck,
  X,
  AlertTriangle,
  Globe,
  Landmark,
  Copy,
  ExternalLink,
  Upload,
  CheckCircle2,
  Loader2,
} from "lucide-react";
import { useSigdocsStore } from "@stores/sigdocsStore";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { open as openShell } from "@tauri-apps/plugin-shell";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type {
  SicroDocFinalization,
  SicroDocSignature,
} from "../document-engine/schema";
import styles from "./DigitalSignatureDialog.module.css";

interface DigitalSignatureDialogProps {
  open: boolean;
  finalization: SicroDocFinalization;
  /** H — context for gov.br flow. */
  workspacePath: string;
  laudoId: string;
  /**
   * H — Disparado quando o user clica "Exportar PDF e abrir gov.br".
   * O componente pai (StatusPanel) chama o exportador (PDF do laudo)
   * e devolve o caminho relativo do PDF gerado. Aqui usamos pra
   * mostrar o caminho + copiar pro clipboard.
   */
  onExportPdfForSigning?: () => Promise<{
    relative_path: string;
    absolute_path: string;
  } | null>;
  onClose: () => void;
  onSigned: (signature: SicroDocSignature) => void;
}

type Tab = "gov_br" | "sigdocs";

export function DigitalSignatureDialog({
  open,
  finalization,
  workspacePath: _workspacePath,
  laudoId,
  onExportPdfForSigning,
  onClose,
  onSigned,
}: DigitalSignatureDialogProps) {
  // I — Default = SIGDOCS porque é o fluxo institucional atual do
  // Estado do Amapá. gov.br + A1/A3 ficam como alternativas.
  const [tab, setTab] = useState<Tab>("sigdocs");
  // signerName / signerId são consumidos pelos fluxos gov.br e SIGDOCS
  // (caem em SicroDocSignature ao importar o PDF assinado). A UI beta
  // não expõe campo dedicado — usa `finalization.finalized_by` como
  // valor default. O state continua mutável para um futuro botão
  // "editar metadados do signatário", se vier a ser necessário.
  const [signerName] = useState(finalization.finalized_by);
  const [signerId] = useState("");

  // --- H — Estado do fluxo gov.br ---
  const [govExportedPath, setGovExportedPath] = useState<string | null>(
    null,
  );
  const [govExportedAbs, setGovExportedAbs] = useState<string | null>(null);
  const [govExporting, setGovExporting] = useState(false);
  const [govImporting, setGovImporting] = useState(false);
  const [govImportError, setGovImportError] = useState<string | null>(null);
  const [govPathCopied, setGovPathCopied] = useState(false);

  // --- I — Estado do fluxo SIGDOCS ---
  const [sigdocsExportedPath, setSigdocsExportedPath] = useState<string | null>(
    null,
  );
  const [sigdocsExportedAbs, setSigdocsExportedAbs] = useState<string | null>(
    null,
  );
  const [sigdocsExporting, setSigdocsExporting] = useState(false);
  const [sigdocsImporting, setSigdocsImporting] = useState(false);
  const [sigdocsImportError, setSigdocsImportError] = useState<string | null>(
    null,
  );
  const [sigdocsPathCopied, setSigdocsPathCopied] = useState(false);
  const [sigdocsFolder, setSigdocsFolder] = useState("");
  const [sigdocsProtocol, setSigdocsProtocol] = useState("");
  // J — Store agora expõe coverOpen em vez de splitOpen.
  const setSigdocsSplitOpen = useSigdocsStore((s) => s.setCoverOpen);
  const sigdocsSplitOpen = useSigdocsStore((s) => s.coverOpen);

  if (!open) return null;

  // --- H — gov.br flow handlers ---

  const handleGovExport = async () => {
    if (!onExportPdfForSigning) {
      setGovImportError(
        "Exportação de PDF não está configurada — peça suporte ao time SICRO.",
      );
      return;
    }
    setGovExporting(true);
    setGovImportError(null);
    try {
      const result = await onExportPdfForSigning();
      if (!result) {
        setGovImportError("Exportação cancelada ou falhou.");
      } else {
        setGovExportedPath(result.relative_path);
        setGovExportedAbs(result.absolute_path);
        // Copy absolute path for the user to paste in the portal upload.
        try {
          await writeText(result.absolute_path);
          setGovPathCopied(true);
        } catch {
          /* clipboard non-fatal */
        }
        // Open the gov.br portal in the OS default browser.
        try {
          await openShell("https://assinador.iti.gov.br/");
        } catch (err) {
          setGovImportError(
            `Falha ao abrir o portal: ${(err as Error)?.message ?? err}`,
          );
        }
      }
    } catch (err) {
      setGovImportError(toSicroError(err).message);
    } finally {
      setGovExporting(false);
    }
  };

  const handleGovImport = async () => {
    setGovImporting(true);
    setGovImportError(null);
    try {
      const selected = await openDialog({
        title: "Selecione o PDF assinado pelo gov.br",
        multiple: false,
        directory: false,
        filters: [{ name: "PDF assinado", extensions: ["pdf"] }],
      });
      if (!selected || typeof selected !== "string") {
        setGovImporting(false);
        return;
      }
      const filename = selected.split(/[\\/]/).pop() ?? "assinado.pdf";
      const result = await commands.importSignedPdf(_workspacePath, {
        laudo_id: laudoId,
        source_absolute_path: selected,
        preferred_filename: filename,
      });
      const sig: SicroDocSignature = {
        type: "gov_br",
        signer_name: signerName.trim() || finalization.finalized_by,
        signer_id: signerId.trim() || undefined,
        issuer: "ITI — gov.br",
        signed_at: new Date().toISOString(),
        signed_hash: finalization.content_hash,
        signature_blob: undefined,
        gov_br_signed_pdf_path: result.relative_path,
        gov_br_signed_pdf_hash: result.sha256,
        gov_br_signed_pdf_size: result.size_bytes,
        gov_br_verification_url: "https://validar.iti.gov.br/",
      };
      onSigned(sig);
      onClose();
    } catch (err) {
      setGovImportError(toSicroError(err).message);
    } finally {
      setGovImporting(false);
    }
  };

  const handleCopyPath = async () => {
    if (!govExportedAbs) return;
    try {
      await writeText(govExportedAbs);
      setGovPathCopied(true);
      window.setTimeout(() => setGovPathCopied(false), 1500);
    } catch {
      /* ignored */
    }
  };

  // --- I — SIGDOCS flow handlers ---

  const handleSigdocsExport = async () => {
    if (!onExportPdfForSigning) {
      setSigdocsImportError(
        "Exportação de PDF não está configurada — peça suporte ao time SICRO.",
      );
      return;
    }
    setSigdocsExporting(true);
    setSigdocsImportError(null);
    try {
      const result = await onExportPdfForSigning();
      if (!result) {
        setSigdocsImportError("Exportação cancelada ou falhou.");
        return;
      }
      setSigdocsExportedPath(result.relative_path);
      setSigdocsExportedAbs(result.absolute_path);

      // J — Como o SIGDOC bloqueia Ctrl+V no upload, NÃO adianta só
      // copiar pro clipboard. Em vez disso, abrimos o Explorer com
      // o PDF SELECIONADO — o perito arrasta direto pra dentro do
      // portal (ou clica "Anexar" no SIGDOC e o file picker já abre
      // na pasta certa, sendo o último arquivo "recente").
      try {
        await commands.revealPathInExplorer(result.absolute_path);
      } catch {
        /* non-fatal — o cover ainda abre */
      }
      // Mesmo assim, copia pro clipboard como segurança (caso o
      // perito queira colar no campo "Caminho" de algum modal).
      try {
        await writeText(result.absolute_path);
        setSigdocsPathCopied(true);
      } catch {
        /* clipboard non-fatal */
      }

      // J — Fecha o dialog ANTES de abrir o cover, para que o
      // SigdocsCoverHost no LaudoEditorView possa medir corretamente
      // a área. Em seguida, abre o cover.
      onClose();
      // Pequeno delay pra layout estabilizar.
      await new Promise((resolve) => setTimeout(resolve, 60));
      try {
        const cfg = await commands.getSigdocsUrl(_workspacePath);
        const bodyEl = document.querySelector<HTMLElement>(
          '[data-sigdocs-cover-body="1"]',
        );
        const rect = bodyEl?.getBoundingClientRect();
        const headerH = 36;
        const bounds = rect
          ? {
              x: rect.left,
              y: rect.top + headerH,
              width: rect.width,
              height: Math.max(50, rect.height - headerH),
            }
          : { x: 200, y: 120, width: 1000, height: 600 };
        await commands.openSigdocsCover(cfg.url, bounds);
        setSigdocsSplitOpen(true);
      } catch (err) {
        // Fallback Onda 1: janela secundária separada.
        try {
          const cfg = await commands.getSigdocsUrl(_workspacePath);
          await commands.openSigdocsWindow(cfg.url);
        } catch (err2) {
          setSigdocsImportError(
            `Falha ao abrir SIGDOC: ${(err2 as Error)?.message ?? err2}`,
          );
        }
      }
    } catch (err) {
      setSigdocsImportError(toSicroError(err).message);
    } finally {
      setSigdocsExporting(false);
    }
  };

  const handleSigdocsImport = async () => {
    setSigdocsImporting(true);
    setSigdocsImportError(null);
    try {
      const selected = await openDialog({
        title: "Selecione o PDF assinado pelo SIGDOCS",
        multiple: false,
        directory: false,
        filters: [{ name: "PDF assinado", extensions: ["pdf"] }],
      });
      if (!selected || typeof selected !== "string") {
        setSigdocsImporting(false);
        return;
      }
      const filename = selected.split(/[\\/]/).pop() ?? "assinado.pdf";
      const result = await commands.importSignedPdf(_workspacePath, {
        laudo_id: laudoId,
        source_absolute_path: selected,
        preferred_filename: filename,
      });
      const sig: SicroDocSignature = {
        type: "sigdocs",
        signer_name: signerName.trim() || finalization.finalized_by,
        signer_id: signerId.trim() || undefined,
        issuer: "SIGDOCS — Estado do Amapá",
        signed_at: new Date().toISOString(),
        signed_hash: finalization.content_hash,
        signature_blob: undefined,
        sigdocs_signed_pdf_path: result.relative_path,
        sigdocs_signed_pdf_hash: result.sha256,
        sigdocs_signed_pdf_size: result.size_bytes,
        sigdocs_folder: sigdocsFolder.trim() || undefined,
        sigdocs_protocol: sigdocsProtocol.trim() || undefined,
      };
      onSigned(sig);
      // Fecha o split SIGDOCS depois de importar (perito provavelmente
      // não precisa do portal aberto enquanto inspeciona o laudo
      // assinado).
      if (sigdocsSplitOpen) {
        await commands.closeSigdocsCover().catch(() => {
          /* ignored */
        });
        setSigdocsSplitOpen(false);
      }
      onClose();
    } catch (err) {
      setSigdocsImportError(toSicroError(err).message);
    } finally {
      setSigdocsImporting(false);
    }
  };

  const handleCopySigdocsPath = async () => {
    if (!sigdocsExportedAbs) return;
    try {
      await writeText(sigdocsExportedAbs);
      setSigdocsPathCopied(true);
      window.setTimeout(() => setSigdocsPathCopied(false), 1500);
    } catch {
      /* ignored */
    }
  };

  const handleReopenSigdocs = async () => {
    try {
      const cfg = await commands.getSigdocsUrl(_workspacePath);
      onClose();
      await new Promise((resolve) => setTimeout(resolve, 60));
      const bodyEl = document.querySelector<HTMLElement>(
        '[data-sigdocs-cover-body="1"]',
      );
      const rect = bodyEl?.getBoundingClientRect();
      const headerH = 36;
      const bounds = rect
        ? {
            x: rect.left,
            y: rect.top + headerH,
            width: rect.width,
            height: Math.max(50, rect.height - headerH),
          }
        : { x: 200, y: 120, width: 1000, height: 600 };
      await commands.openSigdocsCover(cfg.url, bounds);
      setSigdocsSplitOpen(true);
    } catch (err) {
      setSigdocsImportError(toSicroError(err).message);
    }
  };

  return (
    <div
      className={styles.overlay}
      role="dialog"
      aria-modal="true"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className={styles.dialog}>
        <header className={styles.header}>
          <strong>
            <ShieldCheck size={16} /> Assinatura digital
          </strong>
          <button
            type="button"
            className={styles.closeBtn}
            onClick={onClose}
            aria-label="Fechar"
          >
            <X size={14} />
          </button>
        </header>

        <div className={styles.tabs} role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={tab === "sigdocs"}
            className={`${styles.tab} ${tab === "sigdocs" ? styles.tabActive : ""}`}
            onClick={() => setTab("sigdocs")}
          >
            <Landmark size={13} /> SIGDOCS{" "}
            <span className={styles.tabBadgeInst}>institucional</span>
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={tab === "gov_br"}
            className={`${styles.tab} ${tab === "gov_br" ? styles.tabActive : ""}`}
            onClick={() => setTab("gov_br")}
          >
            <Globe size={13} /> gov.br
          </button>
        </div>

        <div className={styles.body}>
          {tab === "sigdocs" && (
            <div className={styles.govBrWrap}>
              <p className={styles.govIntro}>
                <Landmark size={12} /> Use o <strong>SIGDOCS</strong> — o sistema
                oficial de tramitação de documentos do Estado do Amapá.
                Assine no portal e o PDF assinado é arquivado de volta no
                SICRO. Continua o fluxo institucional já estabelecido
                (perito → secretaria → delegacia).
              </p>

              <ol className={styles.steps}>
                {/* Passo 1 — Exportar o PDF + abrir SIGDOCS */}
                <li className={sigdocsExportedPath ? styles.stepDone : ""}>
                  <div className={styles.stepNum}>1</div>
                  <div className={styles.stepBody}>
                    <strong>Exportar PDF e abrir o SIGDOCS</strong>
                    <p>
                      O SICRO vai gerar o PDF, copiar o caminho pro
                      clipboard, e abrir o SIGDOCS lado a lado (split
                      sincronizado).
                    </p>
                    {!sigdocsExportedPath && (
                      <button
                        type="button"
                        className={styles.primaryBtn}
                        onClick={() => void handleSigdocsExport()}
                        disabled={sigdocsExporting}
                      >
                        {sigdocsExporting ? (
                          <>
                            <Loader2 size={13} className={styles.spin} />{" "}
                            Exportando…
                          </>
                        ) : (
                          <>
                            <Landmark size={13} /> Exportar PDF e abrir SIGDOCS
                          </>
                        )}
                      </button>
                    )}
                    {sigdocsExportedPath && (
                      <div className={styles.stepResult}>
                        <CheckCircle2 size={13} /> PDF gerado em{" "}
                        <code>{sigdocsExportedPath}</code>
                        {sigdocsExportedAbs && (
                          <button
                            type="button"
                            className={styles.copyBtn}
                            onClick={() => void handleCopySigdocsPath()}
                            title="Copiar caminho absoluto"
                          >
                            <Copy size={11} />{" "}
                            {sigdocsPathCopied
                              ? "Copiado!"
                              : "Copiar caminho"}
                          </button>
                        )}
                      </div>
                    )}
                  </div>
                </li>

                {/* Passo 2 — Assinar no portal */}
                <li className={sigdocsExportedPath ? styles.stepActive : ""}>
                  <div className={styles.stepNum}>2</div>
                  <div className={styles.stepBody}>
                    <strong>Assinar no SIGDOCS</strong>
                    <p>No painel do SIGDOCS (à direita):</p>
                    <ol className={styles.subSteps}>
                      <li>Entre na sua pasta institucional.</li>
                      <li>
                        Faça upload do PDF (cole o caminho copiado, ou
                        navegue até <code>laudos/exports/</code>).
                      </li>
                      <li>Assine pelo SIGDOCS.</li>
                      <li>
                        Baixe o PDF assinado para reimportar no SICRO.
                      </li>
                    </ol>
                    <button
                      type="button"
                      className={styles.secondaryBtn}
                      onClick={() => void handleReopenSigdocs()}
                    >
                      <ExternalLink size={11} /> Reabrir SIGDOCS
                    </button>
                  </div>
                </li>

                {/* Passo 3 — Metadados + importar */}
                <li>
                  <div className={styles.stepNum}>3</div>
                  <div className={styles.stepBody}>
                    <strong>Importar PDF assinado + metadados</strong>
                    <p>
                      Informe (opcional) a pasta e protocolo SIGDOCS pra
                      registro institucional, depois selecione o PDF
                      assinado.
                    </p>
                    <div className={styles.fieldRow}>
                      <div className={styles.field}>
                        <label htmlFor="sig-folder">Pasta SIGDOCS</label>
                        <input
                          id="sig-folder"
                          type="text"
                          value={sigdocsFolder}
                          onChange={(e) => setSigdocsFolder(e.target.value)}
                          placeholder="Ex: Perícia Criminal — Macapá"
                        />
                      </div>
                      <div className={styles.field}>
                        <label htmlFor="sig-protocol">Protocolo</label>
                        <input
                          id="sig-protocol"
                          type="text"
                          value={sigdocsProtocol}
                          onChange={(e) =>
                            setSigdocsProtocol(e.target.value)
                          }
                          placeholder="Ex: 2026-001234"
                        />
                      </div>
                    </div>
                    <button
                      type="button"
                      className={styles.primaryBtn}
                      onClick={() => void handleSigdocsImport()}
                      disabled={sigdocsImporting}
                    >
                      {sigdocsImporting ? (
                        <>
                          <Loader2 size={13} className={styles.spin} />{" "}
                          Importando…
                        </>
                      ) : (
                        <>
                          <Upload size={13} /> Importar PDF assinado
                        </>
                      )}
                    </button>
                  </div>
                </li>
              </ol>

              {sigdocsImportError && (
                <div className={styles.errorBox}>
                  <AlertTriangle size={12} /> {sigdocsImportError}
                </div>
              )}
            </div>
          )}

          {tab === "gov_br" && (
            <div className={styles.govBrWrap}>
              <p className={styles.govIntro}>
                Use o assinador oficial do governo federal — gratuito, integrado
                ao seu login <strong>gov.br</strong>, e com validade jurídica
                plena (Lei 14.063/2020). O fluxo tem três passos:
              </p>

              <ol className={styles.steps}>
                {/* Passo 1 — Exportar o PDF */}
                <li className={govExportedPath ? styles.stepDone : ""}>
                  <div className={styles.stepNum}>1</div>
                  <div className={styles.stepBody}>
                    <strong>Exportar o laudo como PDF</strong>
                    <p>
                      O SICRO vai gerar o PDF, copiar o caminho pro
                      clipboard e abrir o portal do gov.br no seu navegador.
                    </p>
                    {!govExportedPath && (
                      <button
                        type="button"
                        className={styles.primaryBtn}
                        onClick={() => void handleGovExport()}
                        disabled={govExporting}
                      >
                        {govExporting ? (
                          <>
                            <Loader2 size={13} className={styles.spin} />{" "}
                            Exportando…
                          </>
                        ) : (
                          <>
                            <ExternalLink size={13} /> Exportar PDF e abrir gov.br
                          </>
                        )}
                      </button>
                    )}
                    {govExportedPath && (
                      <div className={styles.stepResult}>
                        <CheckCircle2 size={13} /> PDF gerado em{" "}
                        <code>{govExportedPath}</code>
                        {govExportedAbs && (
                          <button
                            type="button"
                            className={styles.copyBtn}
                            onClick={() => void handleCopyPath()}
                            title="Copiar caminho absoluto"
                          >
                            <Copy size={11} />{" "}
                            {govPathCopied ? "Copiado!" : "Copiar caminho"}
                          </button>
                        )}
                      </div>
                    )}
                  </div>
                </li>

                {/* Passo 2 — Assinar no portal */}
                <li className={govExportedPath ? styles.stepActive : ""}>
                  <div className={styles.stepNum}>2</div>
                  <div className={styles.stepBody}>
                    <strong>Assinar no portal do gov.br</strong>
                    <p>
                      Na aba do navegador que abriu:
                    </p>
                    <ol className={styles.subSteps}>
                      <li>Faça login com seu <strong>gov.br</strong> (selo prata ou ouro).</li>
                      <li>Clique em <strong>"Escolher arquivo"</strong> e cole o caminho copiado (ou navegue até o PDF).</li>
                      <li>Confirme com o 2FA (SMS ou app gov.br).</li>
                      <li>Baixe o PDF assinado.</li>
                    </ol>
                    <button
                      type="button"
                      className={styles.secondaryBtn}
                      onClick={() =>
                        void openShell("https://assinador.iti.gov.br/")
                      }
                    >
                      <ExternalLink size={11} /> Abrir portal novamente
                    </button>
                  </div>
                </li>

                {/* Passo 3 — Importar de volta */}
                <li>
                  <div className={styles.stepNum}>3</div>
                  <div className={styles.stepBody}>
                    <strong>Importar o PDF assinado</strong>
                    <p>
                      Selecione o arquivo baixado. O SICRO arquiva em{" "}
                      <code>laudos/{laudoId.slice(0, 8)}/assinados/</code>,
                      calcula o hash SHA-256 do arquivo assinado e marca o
                      laudo como <strong>Assinado gov.br</strong>.
                    </p>
                    <button
                      type="button"
                      className={styles.primaryBtn}
                      onClick={() => void handleGovImport()}
                      disabled={govImporting}
                    >
                      {govImporting ? (
                        <>
                          <Loader2 size={13} className={styles.spin} /> Importando…
                        </>
                      ) : (
                        <>
                          <Upload size={13} /> Importar PDF assinado
                        </>
                      )}
                    </button>
                  </div>
                </li>
              </ol>

              {govImportError && (
                <div className={styles.errorBox}>
                  <AlertTriangle size={12} /> {govImportError}
                </div>
              )}
            </div>
          )}

        </div>

        <footer className={styles.footer}>
          <button
            type="button"
            className={styles.secondaryBtn}
            onClick={onClose}
          >
            Fechar
          </button>
        </footer>
      </div>
    </div>
  );
}
