/**
 * Shell do módulo Croquis: lista ou o editor conforme o `kind`
 * (viario → Konva, corporal → Konva, planta → Pixi).
 */

import { Component, lazy, Suspense, useEffect } from "react";
import type { ErrorInfo, ReactNode } from "react";
import { ListChecks, AlertTriangle, ArrowLeft } from "lucide-react";
import {
  selectActiveOccurrence,
  selectActiveWorkspacePath,
  useWorkspaceStore,
} from "@stores/workspaceStore";
import { NoOccurrenceState } from "@components/NoOccurrenceState/NoOccurrenceState";
import { CroquiListView } from "./CroquiListView";
import { CroquiEditor } from "./editor/CroquiEditor";
import { CorpoEditor } from "./corpo/editor/CorpoEditor";
import { useCroquiStore } from "./store/croquiStore";
import styles from "./CroquiModule.module.css";

// Lazy: o Pixi (pesado) fica em chunk separado e só carrega ao abrir uma planta.
const PlantaEditor = lazy(() =>
  import("./planta/editor/PlantaEditor").then((m) => ({
    default: m.PlantaEditor,
  })),
);

interface PlantaBoundaryProps {
  onBack: () => void;
  children: ReactNode;
}
interface PlantaBoundaryState {
  error: Error | null;
}

/** Isola falhas do Pixi/chunk da planta para não derrubar o módulo inteiro. */
class PlantaBoundary extends Component<PlantaBoundaryProps, PlantaBoundaryState> {
  state: PlantaBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // eslint-disable-next-line no-console
    console.error("[PlantaBoundary] editor de planta falhou", error, info);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div
        role="alert"
        style={{
          height: "100%",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          gap: 12,
          padding: 24,
          textAlign: "center",
        }}
      >
        <AlertTriangle size={28} color="#b91c1c" />
        <h2 style={{ margin: 0, fontSize: 16 }}>
          O editor de planta encontrou um problema
        </h2>
        <p style={{ maxWidth: 520, color: "#475569", fontSize: 13 }}>
          O motor de planta (Pixi) não pôde ser carregado/renderizado. Isso não
          afeta os croquis viário e corporal.
        </p>
        <pre
          style={{
            maxWidth: 640,
            maxHeight: 180,
            overflow: "auto",
            background: "#0f172a",
            color: "#e2e8f0",
            padding: 12,
            borderRadius: 8,
            fontSize: 11,
            textAlign: "left",
          }}
        >
          {error.name}: {error.message}
          {error.stack ? `\n\n${error.stack}` : ""}
        </pre>
        <button
          type="button"
          onClick={this.props.onBack}
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 6,
            padding: "8px 14px",
            border: "1px solid #e2e8f0",
            borderRadius: 8,
            background: "#fff",
            cursor: "pointer",
          }}
        >
          <ArrowLeft size={14} /> Voltar para a lista
        </button>
      </div>
    );
  }
}

export function CroquiModule() {
  const occurrence = useWorkspaceStore(selectActiveOccurrence);
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const activeCroquiId = useCroquiStore((s) => s.activeCroquiId);
  const activeCroqui = useCroquiStore((s) => s.activeCroqui);
  const clearCurrent = useCroquiStore((s) => s.clearCurrent);

  // Solta o croqui aberto ao trocar de ocorrência, pra não mostrar o croqui de
  // um workspace sobre os dados de outro.
  useEffect(() => {
    return () => {
      clearCurrent();
    };
  }, [workspacePath, clearCurrent]);

  if (!workspacePath || !occurrence) {
    return (
      <NoOccurrenceState
        icon={<ListChecks size={36} strokeWidth={1.5} />}
        moduleName="Croqui"
      />
    );
  }

  const kind = activeCroqui?.kind;
  return (
    <div className={styles.wrap}>
      {activeCroquiId == null ? (
        <CroquiListView />
      ) : kind === "corporal" ? (
        <CorpoEditor />
      ) : kind === "planta" ? (
        <PlantaBoundary onBack={clearCurrent}>
          <Suspense
            fallback={
              <div className={styles.empty}>
                <p>Carregando editor de planta…</p>
              </div>
            }
          >
            <PlantaEditor />
          </Suspense>
        </PlantaBoundary>
      ) : (
        <CroquiEditor />
      )}
    </div>
  );
}
