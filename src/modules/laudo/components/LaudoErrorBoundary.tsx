/**
 * LaudoErrorBoundary — boundary global do editor de laudo.
 *
 * F12.6 — Isola crashes do TipTap/ProseMirror, NodeViews, plugins, etc.
 * Em vez do app inteiro morrer (tela branca), o boundary exibe uma UI
 * de recuperação:
 *   - Mensagem amigável + stack trace colapsável.
 *   - Botão "Tentar novamente" — força remount do tree filho.
 *   - Botão "Recuperar do auto-backup" — abre seletor de backups
 *     IndexedDB (F12.4) caso o documento esteja corrompido.
 *   - Botão "Voltar para a lista" — sair do editor (callback opcional).
 *
 * Por que classe? React só permite `componentDidCatch`/
 * `getDerivedStateFromError` em class components (até hoje, jan/2026).
 */

import { Component, Fragment, type ErrorInfo, type ReactNode } from "react";
import { AlertTriangle, RotateCcw, ArrowLeft, Database } from "lucide-react";
import { listAutoBackups, type AutoBackupEntry } from "../services/autoBackup";
import styles from "./LaudoErrorBoundary.module.css";

interface LaudoErrorBoundaryProps {
  /** ID do laudo atual — usado para consultar auto-backups. */
  laudoId?: string | null;
  /** Callback para "Voltar à lista". Se omitido, oculta o botão. */
  onBack?: () => void;
  /**
   * Callback chamado quando o usuário escolhe restaurar um auto-backup.
   * Recebe o `JSONContent` salvo no IndexedDB. O view pai é responsável
   * por integrar com o store.
   */
  onRestoreBackup?: (entry: AutoBackupEntry & { id: number }) => void;
  children: ReactNode;
}

interface LaudoErrorBoundaryState {
  error: Error | null;
  errorInfo: ErrorInfo | null;
  /** Conta resets — usada como key do filho para forçar remount. */
  resetKey: number;
  showDetails: boolean;
  backups: Array<AutoBackupEntry & { id: number }>;
  loadingBackups: boolean;
}

export class LaudoErrorBoundary extends Component<
  LaudoErrorBoundaryProps,
  LaudoErrorBoundaryState
> {
  state: LaudoErrorBoundaryState = {
    error: null,
    errorInfo: null,
    resetKey: 0,
    showDetails: false,
    backups: [],
    loadingBackups: false,
  };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // eslint-disable-next-line no-console
    console.error("[LaudoErrorBoundary] editor crashed", error, info);
    this.setState({ errorInfo: info });
  }

  private handleReset = () => {
    this.setState((prev) => ({
      error: null,
      errorInfo: null,
      resetKey: prev.resetKey + 1,
      showDetails: false,
    }));
  };

  private handleLoadBackups = async () => {
    const { laudoId } = this.props;
    if (!laudoId) return;
    this.setState({ loadingBackups: true });
    try {
      const list = await listAutoBackups(laudoId);
      this.setState({ backups: list, loadingBackups: false });
    } catch {
      this.setState({ loadingBackups: false });
    }
  };

  private handleRestore = (entry: AutoBackupEntry & { id: number }) => {
    this.props.onRestoreBackup?.(entry);
    this.handleReset();
  };

  render() {
    const { error, errorInfo, resetKey, showDetails, backups, loadingBackups } =
      this.state;
    const { onBack, laudoId, children } = this.props;

    if (!error) {
      // Fragment ao invés de <div> para não quebrar layouts flex parent.
      // (Bug visual no LaudoEditorView: o `.root` é flex-column e o
      // `.body` precisa ser filho direto com `flex: 1`. Um div opaco no
      // meio colapsava a altura.) `key` no Fragment serve para forçar
      // remount do tree quando "Tentar novamente" é clicado.
      return <Fragment key={resetKey}>{children}</Fragment>;
    }

    return (
      <div className={styles.boundary} role="alert" aria-live="assertive">
        <div className={styles.card}>
          <div className={styles.icon} aria-hidden="true">
            <AlertTriangle size={28} />
          </div>
          <h2 className={styles.title}>O editor encontrou um problema</h2>
          <p className={styles.subtitle}>
            Algo inesperado aconteceu enquanto o laudo era renderizado.
            Seu conteúdo provavelmente está seguro nos auto-backups locais.
          </p>

          <div className={styles.actions}>
            <button
              type="button"
              className={styles.primaryBtn}
              onClick={this.handleReset}
            >
              <RotateCcw size={14} /> Tentar novamente
            </button>
            {laudoId && (
              <button
                type="button"
                className={styles.secondaryBtn}
                onClick={() => void this.handleLoadBackups()}
                disabled={loadingBackups}
              >
                <Database size={14} />{" "}
                {loadingBackups ? "Buscando…" : "Recuperar do auto-backup"}
              </button>
            )}
            {onBack && (
              <button
                type="button"
                className={styles.tertiaryBtn}
                onClick={onBack}
              >
                <ArrowLeft size={14} /> Voltar para a lista
              </button>
            )}
          </div>

          {backups.length > 0 && (
            <div className={styles.backupList}>
              <h3 className={styles.backupTitle}>
                Auto-backups disponíveis ({backups.length})
              </h3>
              <ul>
                {backups.map((b) => (
                  <li key={b.id}>
                    <button
                      type="button"
                      className={styles.backupItem}
                      onClick={() => this.handleRestore(b)}
                    >
                      <span className={styles.backupTime}>
                        {new Date(b.capturedAt).toLocaleString("pt-BR")}
                      </span>
                      <span className={styles.backupMeta}>
                        {b.wordCount ?? 0} palavras
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          )}

          <details
            className={styles.details}
            open={showDetails}
            onToggle={(e) =>
              this.setState({ showDetails: (e.target as HTMLDetailsElement).open })
            }
          >
            <summary>Detalhes técnicos</summary>
            <div className={styles.stack}>
              <strong>{error.name}:</strong> {error.message}
              {errorInfo?.componentStack && (
                <pre>{errorInfo.componentStack}</pre>
              )}
              {error.stack && <pre>{error.stack}</pre>}
            </div>
          </details>
        </div>
      </div>
    );
  }
}
