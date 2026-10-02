/**
 * Wizard "Importar OSM": coordenadas + raio → Overpass → seleção de vias →
 * objetos parity. Nada pesado roda no render e nada busca sozinho; o Leaflet
 * fica isolado em `OsmMapPanel` + ErrorBoundary.
 * Privacidade: só a bbox geográfica sai para o Overpass, nunca dado pericial.
 */

import { useCallback, useMemo, useState, type ReactNode } from "react";
import {
  bboxFromCenterRadius,
  clearOverpassCache,
  coordinateParseErrorMessage,
  estimatePxPerMeter,
  fetchOverpassBBox,
  formatCoordinates,
  parseCoordinates,
  type CoordinateParseError,
  type LatLon,
  type OsmDataset,
  type OsmViewport,
  type OsmWay,
} from "../engine";
import {
  convertOsmDatasetToParityObjects,
  type SicroRoadObject_parity,
  type SicroRoundaboutObject_parity,
} from "../engine/road-parity";
import { OsmMapPanel } from "./OsmMapPanel";
import styles from "./CroquiEditor.module.css";

export interface OsmImportResult {
  /** Em coordenadas de mundo (metros). */
  parity_roads: SicroRoadObject_parity[];
  /** Em coordenadas de mundo (metros). */
  parity_roundabouts: SicroRoundaboutObject_parity[];
  /** Mensagens em português para o feedback do editor. */
  warnings: string[];
  session: {
    imported_at: string;
    source: string;
    center_lat: number;
    center_lon: number;
    radius_m: number;
    query_bbox: {
      min_lat: number;
      max_lat: number;
      min_lon: number;
      max_lon: number;
    };
    selected_way_ids: number[];
    suggested_px_per_m: number | null;
  };
}

interface OsmImportModalProps {
  canvasWidth: number;
  canvasHeight: number;
  dossieCoords?: LatLon | null;
  onConfirm: (result: OsmImportResult) => void;
  onCancel: () => void;
}

type Phase = "idle" | "loading" | "results" | "empty" | "error";

const RADIUS_PRESETS = [25, 50, 100, 200];
const DEFAULT_RADIUS = 100;

export function OsmImportModal({
  canvasWidth,
  canvasHeight,
  dossieCoords,
  onConfirm,
  onCancel,
}: OsmImportModalProps) {
  if (typeof console !== "undefined") {
    console.info("[OSM] modal mounted (safe mode)");
  }

  const [coordInput, setCoordInput] = useState(
    dossieCoords ? formatCoordinates(dossieCoords) : "",
  );
  const [centre, setCentre] = useState<LatLon | null>(
    dossieCoords ?? null,
  );
  const [parseError, setParseError] = useState<CoordinateParseError | null>(
    null,
  );
  const [radius, setRadius] = useState<number>(DEFAULT_RADIUS);
  const [customRadius, setCustomRadius] = useState<string>("");
  const [phase, setPhase] = useState<Phase>("idle");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [dataset, setDataset] = useState<OsmDataset | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState(false);
  // Mapa já aberto por padrão: o placeholder "Carregar mapa" parecia
  // indisponibilidade. Falha do Leaflet ainda cai no placeholder.
  const [mapEnabled, setMapEnabled] = useState(true);

  const viewport: OsmViewport | null = useMemo(() => {
    if (!centre) return null;
    return bboxFromCenterRadius(centre, radius, canvasWidth, canvasHeight);
  }, [centre, radius, canvasWidth, canvasHeight]);

  const suggestedPxPerM = useMemo(
    () => (viewport ? estimatePxPerMeter(viewport) : null),
    [viewport],
  );

  const applyCoordString = useCallback((raw: string) => {
    const parsed = parseCoordinates(raw);
    if (!parsed.ok) {
      setParseError(parsed.error);
      return;
    }
    setParseError(null);
    setCentre(parsed.value);
  }, []);

  const handleSearch = useCallback(async () => {
    if (!centre || !viewport) return;
    console.info("[OSM] handleSearch start", { centre, radius });
    setPhase("loading");
    setErrorMsg(null);
    setDataset(null);
    setSelectedIds(new Set());
    try {
      const r = await fetchOverpassBBox({
        min_lat: viewport.min_lat,
        max_lat: viewport.max_lat,
        min_lon: viewport.min_lon,
        max_lon: viewport.max_lon,
      });
      const drivable = r.ways.filter((w) => w.tags && w.tags.highway);
      const next = { ...r, ways: drivable };
      setDataset(next);
      console.info("[OSM] handleSearch ok", {
        nodes: r.nodes.length,
        ways: drivable.length,
        from_cache: r.from_cache,
      });
      if (drivable.length === 0) {
        setPhase("empty");
        return;
      }
      setSelectedIds(new Set(drivable.map((w) => w.id)));
      setPhase("results");
    } catch (e) {
      console.warn("[OSM] handleSearch error", e);
      setPhase("error");
      setErrorMsg((e as Error).message);
    }
  }, [centre, viewport, radius]);

  const toggleWay = useCallback((id: number) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);
  const selectAll = useCallback(() => {
    if (!dataset) return;
    setSelectedIds(new Set(dataset.ways.map((w) => w.id)));
  }, [dataset]);
  const selectNone = useCallback(() => setSelectedIds(new Set()), []);

  const handleConfirm = useCallback(() => {
    if (!dataset || !centre || !viewport) return;
    if (selectedIds.size === 0) return;
    console.info("[OSM] handleConfirm start", {
      selected: selectedIds.size,
    });
    setBusy(true);
    try {
      const chosen: OsmWay[] = dataset.ways.filter((w) =>
        selectedIds.has(w.id),
      );
      const session_base = {
        imported_at: new Date().toISOString(),
        source: "osm:overpass",
        center_lat: centre.lat,
        center_lon: centre.lon,
        radius_m: radius,
        query_bbox: {
          min_lat: viewport.min_lat,
          max_lat: viewport.max_lat,
          min_lon: viewport.min_lon,
          max_lon: viewport.max_lon,
        },
        selected_way_ids: Array.from(selectedIds).sort((a, b) => a - b),
      };

      const result = convertOsmDatasetToParityObjects({
        ways: chosen,
        nodes: dataset.nodes,
        center: centre,
        radius_m: radius,
        canvas: { width: canvasWidth, height: canvasHeight },
        options: {
          margin: 0.1,
          simplify_tolerance_m: 0.6,
          min_way_length_m: 4,
          preserve_roundabouts: true,
          ignore_non_vehicle: true,
        },
      });
      console.info("[OSM] handleConfirm parity ok", {
        parity_roads: result.roads.length,
        parity_roundabouts: result.roundabouts.length,
        warnings: result.warnings.length,
        skipped: result.stats.skipped_count,
      });
      onConfirm({
        parity_roads: result.roads,
        parity_roundabouts: result.roundabouts,
        warnings: result.warnings,
        session: {
          ...session_base,
          suggested_px_per_m: result.stats.px_per_m,
        },
      });
    } finally {
      setBusy(false);
    }
  }, [
    dataset,
    centre,
    viewport,
    selectedIds,
    radius,
    canvasWidth,
    canvasHeight,
    onConfirm,
  ]);

  const hasDossie = !!dossieCoords;
  const applyDossie = useCallback(() => {
    if (!dossieCoords) return;
    setCoordInput(formatCoordinates(dossieCoords));
    setCentre(dossieCoords);
    setParseError(null);
  }, [dossieCoords]);

  return (
    <div
      className={styles.overlay}
      role="dialog"
      aria-modal="true"
      aria-labelledby="osm-modal-title"
      onClick={(e) => {
        if (e.target === e.currentTarget && !busy) onCancel();
      }}
    >
      <div
        className={styles.dialog}
        // `.dialog` fixa width: 520px no CSS; o inline sobrepõe.
        style={{ width: 1080, maxWidth: "min(1080px, 95vw)", minHeight: 520 }}
      >
        <header className={styles.dialogHeader}>
          <strong id="osm-modal-title">Importar vias do OSM</strong>
          <button
            type="button"
            onClick={onCancel}
            className={styles.dialogClose}
            disabled={busy}
          >
            Fechar
          </button>
        </header>

        <div
          style={{
            display: "grid",
            // Sem mapa, a coluna central encolhe e os painéis laterais crescem.
            gridTemplateColumns: mapEnabled
              ? "260px 1fr 300px"
              : "320px 1fr 360px",
            gap: 10,
            minHeight: 440,
          }}
        >
          <LeftPanel
            coordInput={coordInput}
            onCoordInputChange={setCoordInput}
            onApplyCoordString={applyCoordString}
            parseError={parseError}
            hasDossie={hasDossie}
            onApplyDossie={applyDossie}
            radius={radius}
            onRadiusChange={(r) => {
              setRadius(r);
              setCustomRadius("");
            }}
            customRadius={customRadius}
            onCustomRadiusChange={setCustomRadius}
            phase={phase}
            canSearch={!!centre}
            onSearch={() => void handleSearch()}
            onClearCache={() => {
              clearOverpassCache();
              void handleSearch();
            }}
            hasDataset={!!dataset}
          />

          <CentrePanel
            mapEnabled={mapEnabled}
            onLoadMap={() => {
              console.info("[OSM] user requested map");
              setMapEnabled(true);
            }}
            onUnloadMap={() => {
              console.info("[OSM] user dismissed map");
              setMapEnabled(false);
            }}
            centre={centre}
            radius={radius}
            dataset={dataset}
            selectedIds={selectedIds}
            onMapPick={(pt) => {
              setCentre(pt);
              setCoordInput(formatCoordinates(pt));
              setParseError(null);
            }}
            onToggleWay={toggleWay}
          />

          <RightPanel
            phase={phase}
            dataset={dataset}
            errorMsg={errorMsg}
            selectedIds={selectedIds}
            onToggleWay={toggleWay}
            onSelectAll={selectAll}
            onSelectNone={selectNone}
            suggestedPxPerM={suggestedPxPerM}
          />
        </div>

        <div
          style={{
            marginTop: 10,
            padding: "6px 8px",
            fontSize: 11,
            color: "var(--sicro-fg-dim)",
            background: "rgba(124, 58, 237, 0.08)",
            border: "1px solid var(--sicro-border)",
            borderRadius: 4,
            lineHeight: 1.4,
          }}
        >
          {/* Texto neutro de propósito (não cita o motor). */}
          <strong style={{ color: "#7c3aed" }}>
            Importação do OpenStreetMap — referência geográfica
          </strong>{" "}
          — O mapa acima serve apenas de referência. As vias importadas viram
          objetos vetoriais editáveis (traçado suave e rotatórias), prontos
          para ajuste no croqui.
        </div>
        <div
          style={{
            display: "flex",
            gap: 8,
            justifyContent: "flex-end",
            marginTop: 8,
          }}
        >
          <button
            type="button"
            className={styles.dialogClose}
            onClick={onCancel}
            disabled={busy}
          >
            Cancelar
          </button>
          <button
            type="button"
            className={styles.dialogClose}
            style={{
              color: selectedIds.size > 0 ? "#5aa9e6" : undefined,
              fontWeight: 600,
              opacity:
                phase === "results" && selectedIds.size > 0 && !busy ? 1 : 0.5,
            }}
            disabled={phase !== "results" || selectedIds.size === 0 || busy}
            onClick={handleConfirm}
          >
            Importar selecionadas ({selectedIds.size})
          </button>
        </div>
      </div>
    </div>
  );
}

// ===========================================================================
// Esquerda: coordenadas, raio e busca

function LeftPanel({
  coordInput,
  onCoordInputChange,
  onApplyCoordString,
  parseError,
  hasDossie,
  onApplyDossie,
  radius,
  onRadiusChange,
  customRadius,
  onCustomRadiusChange,
  phase,
  canSearch,
  onSearch,
  onClearCache,
  hasDataset,
}: {
  coordInput: string;
  onCoordInputChange: (v: string) => void;
  onApplyCoordString: (raw: string) => void;
  parseError: CoordinateParseError | null;
  hasDossie: boolean;
  onApplyDossie: () => void;
  radius: number;
  onRadiusChange: (r: number) => void;
  customRadius: string;
  onCustomRadiusChange: (v: string) => void;
  phase: Phase;
  canSearch: boolean;
  onSearch: () => void;
  onClearCache: () => void;
  hasDataset: boolean;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        <span style={{ fontSize: 12 }}>Coordenadas (lat, lon)</span>
        <input
          type="text"
          value={coordInput}
          onChange={(e) => onCoordInputChange(e.target.value)}
          onBlur={() => coordInput && onApplyCoordString(coordInput)}
          placeholder="-0.0345, -51.0694"
          style={inputStyle}
        />
      </label>
      <div style={{ display: "flex", gap: 6 }}>
        <button
          type="button"
          className={styles.dialogClose}
          onClick={() => onApplyCoordString(coordInput)}
          title="Centraliza o mapa nas coordenadas digitadas"
        >
          Usar coordenadas
        </button>
        {hasDossie && (
          <button
            type="button"
            className={styles.dialogClose}
            onClick={onApplyDossie}
            title="Usa as coordenadas registradas na ocorrência"
          >
            Da ocorrência
          </button>
        )}
      </div>
      {parseError && (
        <small style={{ color: "#dc2626" }}>
          {coordinateParseErrorMessage(parseError)}
        </small>
      )}

      <div style={{ marginTop: 8 }}>
        <span style={{ fontSize: 12, display: "block", marginBottom: 4 }}>
          Raio de importação
        </span>
        <div style={{ display: "flex", gap: 4, flexWrap: "wrap" }}>
          {RADIUS_PRESETS.map((r) => (
            <button
              key={r}
              type="button"
              className={styles.dialogClose}
              style={{
                fontWeight: r === radius ? 700 : 400,
                color: r === radius ? "#5aa9e6" : undefined,
              }}
              onClick={() => onRadiusChange(r)}
            >
              {r} m
            </button>
          ))}
        </div>
        <label
          style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
            marginTop: 6,
            fontSize: 12,
          }}
        >
          Custom
          <input
            type="number"
            min={10}
            max={2000}
            step={5}
            value={customRadius}
            onChange={(e) => onCustomRadiusChange(e.target.value)}
            onBlur={() => {
              const n = Number.parseInt(customRadius, 10);
              if (Number.isFinite(n) && n >= 10) onRadiusChange(n);
            }}
            placeholder="m"
            style={{ ...inputStyle, width: 80 }}
          />
        </label>
      </div>

      <button
        type="button"
        className={styles.dialogClose}
        style={{
          marginTop: 12,
          color: canSearch ? "#5aa9e6" : undefined,
          fontWeight: 600,
          opacity: canSearch && phase !== "loading" ? 1 : 0.5,
        }}
        disabled={!canSearch || phase === "loading"}
        onClick={onSearch}
      >
        {phase === "loading" ? "Buscando…" : "Buscar vias"}
      </button>
      {hasDataset && (
        <button
          type="button"
          className={styles.dialogClose}
          style={{ fontSize: 11 }}
          onClick={onClearCache}
          title="Limpa o cache em memória e refaz a consulta Overpass"
        >
          Recarregar (sem cache)
        </button>
      )}

      <hr style={{ borderColor: "var(--sicro-border)", margin: "8px 0" }} />
      <small style={{ color: "var(--sicro-fg-dim)" }}>
        Privacidade: a consulta ao OSM envia apenas o retângulo
        geográfico — nenhum dado pericial sai do SICRO.
      </small>
    </div>
  );
}

// ===========================================================================
// Centro: mapa (ou placeholder)

function CentrePanel({
  mapEnabled,
  onLoadMap,
  onUnloadMap,
  centre,
  radius,
  dataset,
  selectedIds,
  onMapPick,
  onToggleWay,
}: {
  mapEnabled: boolean;
  onLoadMap: () => void;
  onUnloadMap: () => void;
  centre: LatLon | null;
  radius: number;
  dataset: OsmDataset | null;
  selectedIds: Set<number>;
  onMapPick: (pt: LatLon) => void;
  onToggleWay: (id: number) => void;
}) {
  return (
    <div
      style={{
        border: "1px solid var(--sicro-border)",
        borderRadius: 6,
        overflow: "hidden",
        position: "relative",
        height: 440,
        width: "100%",
        background: "#0f172a",
      }}
    >
      {!mapEnabled && (
        <PlaceholderMap onLoadMap={onLoadMap} />
      )}
      {mapEnabled && (
        <LazyMapBoundary onUnloadMap={onUnloadMap}>
          <OsmMapPanel
            centre={centre}
            radius={radius}
            dataset={dataset}
            selectedIds={selectedIds}
            onMapPick={onMapPick}
            onToggleWay={onToggleWay}
            onUnload={onUnloadMap}
          />
        </LazyMapBoundary>
      )}
    </div>
  );
}

/** Só aparece após "Esconder mapa" ou falha do Leaflet. */
function PlaceholderMap({ onLoadMap }: { onLoadMap: () => void }) {
  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 12,
        color: "var(--sicro-fg-dim)",
        textAlign: "center",
        padding: "0 24px",
      }}
    >
      <div style={{ fontSize: 32 }}>🗺️</div>
      <div style={{ fontSize: 13, lineHeight: 1.5, maxWidth: 320 }}>
        Mapa OSM oculto. Você ainda pode trabalhar inteiramente por
        coordenadas — ou voltar a exibir o mapa.
      </div>
      <button
        type="button"
        onClick={onLoadMap}
        style={{
          background: "rgba(90,169,230,0.15)",
          border: "1px solid #5aa9e6",
          color: "#fff",
          fontFamily: "inherit",
          padding: "6px 14px",
          borderRadius: 4,
          cursor: "pointer",
          fontSize: 13,
          fontWeight: 600,
        }}
      >
        Exibir mapa
      </button>
    </div>
  );
}

import { Component, type ErrorInfo } from "react";

/** Isola falhas do Leaflet: o modal continua aberto no modo sem mapa. */
class LazyMapBoundary extends Component<
  { children: ReactNode; onUnloadMap: () => void },
  { error: Error | null }
> {
  state = { error: null as Error | null };
  static getDerivedStateFromError(error: Error) {
    return { error };
  }
  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[OSM] map panel crashed", error, info);
  }
  render() {
    if (this.state.error) {
      return (
        <div
          style={{
            position: "absolute",
            inset: 0,
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            gap: 8,
            padding: 24,
            color: "var(--sicro-fg-dim)",
            textAlign: "center",
          }}
        >
          <div style={{ color: "#dc2626", fontWeight: 600, fontSize: 13 }}>
            Falha ao carregar o mapa
          </div>
          <small
            style={{
              fontFamily: "var(--font-mono)",
              fontSize: 10,
              maxWidth: 420,
              wordBreak: "break-word",
            }}
          >
            {this.state.error.message ?? String(this.state.error)}
          </small>
          <button
            type="button"
            onClick={() => {
              this.setState({ error: null });
              this.props.onUnloadMap();
            }}
            style={{
              marginTop: 4,
              background: "rgba(90,169,230,0.15)",
              border: "1px solid #5aa9e6",
              color: "#fff",
              fontFamily: "inherit",
              padding: "4px 10px",
              borderRadius: 4,
              cursor: "pointer",
              fontSize: 12,
            }}
          >
            Voltar para o modo sem mapa
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}

// ===========================================================================
// Direita: lista de vias

function RightPanel({
  phase,
  dataset,
  errorMsg,
  selectedIds,
  onToggleWay,
  onSelectAll,
  onSelectNone,
  suggestedPxPerM,
}: {
  phase: Phase;
  dataset: OsmDataset | null;
  errorMsg: string | null;
  selectedIds: Set<number>;
  onToggleWay: (id: number) => void;
  onSelectAll: () => void;
  onSelectNone: () => void;
  suggestedPxPerM: number | null;
}) {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 6,
        minHeight: 420,
      }}
    >
      <div style={{ fontSize: 12, fontWeight: 600 }}>
        Vias encontradas{" "}
        {dataset && (
          <span style={{ color: "var(--sicro-fg-dim)" }}>
            ({dataset.ways.length})
          </span>
        )}
      </div>

      {phase === "idle" && (
        <p style={{ color: "var(--sicro-fg-dim)", fontSize: 12, margin: 0 }}>
          Defina um ponto (coordenadas ou clique no mapa) e clique em
          "Buscar vias".
        </p>
      )}
      {phase === "loading" && (
        <p style={{ fontSize: 12, color: "var(--sicro-accent)" }}>
          Consultando Overpass…
        </p>
      )}
      {phase === "empty" && (
        <p style={{ fontSize: 12, color: "var(--sicro-fg-dim)" }}>
          Nenhuma via tag <code>highway</code> encontrada dentro do
          raio. Aumente o raio ou ajuste o ponto.
        </p>
      )}
      {phase === "error" && (
        <p style={{ fontSize: 12, color: "#dc2626" }}>
          {errorMsg ?? "Falha desconhecida ao consultar OSM."}
        </p>
      )}

      {phase === "results" && dataset && (
        <>
          <div style={{ display: "flex", gap: 4 }}>
            <button
              type="button"
              className={styles.dialogClose}
              style={{ fontSize: 11 }}
              onClick={onSelectAll}
            >
              Todas
            </button>
            <button
              type="button"
              className={styles.dialogClose}
              style={{ fontSize: 11 }}
              onClick={onSelectNone}
            >
              Nenhuma
            </button>
          </div>
          <div
            style={{
              overflowY: "auto",
              flex: 1,
              border: "1px solid var(--sicro-border)",
              borderRadius: 4,
              padding: 4,
            }}
          >
            {dataset.ways.map((w) => (
              <label
                key={w.id}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  fontSize: 11,
                  padding: "3px 4px",
                  borderRadius: 3,
                  cursor: "pointer",
                }}
              >
                <input
                  type="checkbox"
                  checked={selectedIds.has(w.id)}
                  onChange={() => onToggleWay(w.id)}
                />
                <span style={{ flex: 1, minWidth: 0 }}>
                  <strong>
                    {w.tags.name ?? w.tags.ref ?? `way ${w.id}`}
                  </strong>
                  <span
                    style={{
                      color: "var(--sicro-fg-dim)",
                      marginLeft: 4,
                    }}
                  >
                    · {w.tags.highway}
                    {w.tags.oneway === "yes" && " · ↓"}
                    {w.tags.junction === "roundabout" && " · ⊙"}
                    {w.tags.lanes && ` · ${w.tags.lanes}f`}
                  </span>
                </span>
                <span
                  style={{
                    fontVariantNumeric: "tabular-nums",
                    color: "var(--sicro-fg-dim)",
                  }}
                >
                  {w.node_refs.length}p
                </span>
              </label>
            ))}
          </div>
        </>
      )}

      {suggestedPxPerM && phase === "results" && (
        <small style={{ color: "var(--sicro-fg-dim)" }}>
          Escala sugerida: ≈ {suggestedPxPerM.toFixed(2)} px/m
          <br />
          Informada apenas — não alteramos a escala do croqui
          automaticamente.
        </small>
      )}
    </div>
  );
}

const inputStyle: React.CSSProperties = {
  background: "var(--sicro-surface-2)",
  border: "1px solid var(--sicro-border)",
  color: "var(--sicro-fg)",
  borderRadius: 4,
  padding: "4px 6px",
  fontSize: 12,
  fontFamily: "inherit",
};
