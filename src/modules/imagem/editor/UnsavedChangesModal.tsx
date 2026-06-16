/**
 * UnsavedChangesModal (Imagem) — diálogo "Salvar antes de sair?" levantado
 * pelo ImageEditor quando o perito tenta deixar o editor / trocar de módulo
 * com alterações não salvas (pilha de filtros, anotações, ajustes, título…).
 *
 * Três saídas:
 *   - **Salvar e sair**   → salva a análise e prossegue a navegação pendente.
 *   - **Sair sem salvar** → descarta as edições locais e prossegue.
 *   - **Cancelar**        → fecha o modal e permanece no editor.
 *
 * Autocontido (estilos inline) para não acoplar a outro módulo. §13: salvar
 * grava só o sidecar `.sicroimage` (operações não-destrutivas) — o arquivo
 * original da evidência nunca é alterado.
 */

interface UnsavedChangesModalProps {
  /** Se o pai está no meio de um salvamento. */
  saving: boolean;
  /** Texto contextual — para onde o usuário tentava ir. */
  destinationLabel?: string;
  onSaveAndLeave: () => void;
  onDiscardAndLeave: () => void;
  onCancel: () => void;
}

export function UnsavedChangesModal({
  saving,
  destinationLabel,
  onSaveAndLeave,
  onDiscardAndLeave,
  onCancel,
}: UnsavedChangesModalProps) {
  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="img-unsaved-title"
      onClick={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 1000,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0, 0, 0, 0.55)",
      }}
    >
      <div
        style={{
          width: "min(460px, calc(100vw - 32px))",
          background: "var(--sicro-surface-1, #1b2330)",
          border: "1px solid var(--sicro-border, #324054)",
          borderRadius: 10,
          boxShadow: "0 12px 40px rgba(0, 0, 0, 0.5)",
          padding: 16,
          color: "var(--sicro-fg, #e6edf3)",
        }}
      >
        <strong
          id="img-unsaved-title"
          style={{ fontSize: 14, display: "block", marginBottom: 8 }}
        >
          Alterações não salvas na análise
        </strong>
        <p style={{ margin: 0, fontSize: 13, lineHeight: 1.5 }}>
          Há edições nesta imagem (filtros, anotações ou ajustes) que ainda não
          foram salvas.
          {destinationLabel
            ? ` Deseja salvar antes de ir para ${destinationLabel}?`
            : " Deseja salvar antes de sair?"}
        </p>
        <div
          style={{
            display: "flex",
            gap: 8,
            justifyContent: "flex-end",
            marginTop: 16,
          }}
        >
          <button
            type="button"
            onClick={onCancel}
            disabled={saving}
            style={btnStyle("rgba(148,163,184,0.9)")}
          >
            Cancelar
          </button>
          <button
            type="button"
            onClick={onDiscardAndLeave}
            disabled={saving}
            style={btnStyle("#f87171")}
          >
            Sair sem salvar
          </button>
          <button
            type="button"
            onClick={onSaveAndLeave}
            disabled={saving}
            style={{ ...btnStyle("#5aa9e6"), fontWeight: 600 }}
          >
            {saving ? "Salvando…" : "Salvar e sair"}
          </button>
        </div>
      </div>
    </div>
  );
}

function btnStyle(color: string): React.CSSProperties {
  return {
    background: "transparent",
    border: "1px solid var(--sicro-border, #324054)",
    borderRadius: 6,
    color,
    fontFamily: "inherit",
    fontSize: 13,
    padding: "6px 12px",
    cursor: "pointer",
  };
}
