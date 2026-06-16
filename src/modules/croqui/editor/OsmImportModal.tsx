/**
 * OsmImportModal — MVP 10 "Importar OSM" (Round 4 — safe-mode rewrite).
 *
 * Round 3 froze the app on open because the modal mounted Leaflet
 * eagerly with inline-closure callbacks whose identity changed on
 * every render. The effect that called `invalidateSize()` and the
 * `tileload` handler each triggered `setState`, which re-rendered
 * the parent, which produced new closure identities, which re-fired
 * the effect, which set state again — classic infinite update loop
 * that locked the WebView thread before the modal could paint.
 *
 * Round 4 (this file) takes the staged approach the user demanded:
 *
 *   1. Click "Importar OSM" → modal appears INSTANTLY with title,
 *      coords input, "Carregar mapa" button, close button. **No
 *      Leaflet** in the initial render. No Overpass call. No work.
 *   2. User clicks "Carregar mapa" → the heavy `OsmMapPanel` mounts
 *      (Leaflet, tiles, react-leaflet). Wrapped in an ErrorBoundary
 *      so any Leaflet error stays contained — the modal itself stays
 *      open.
 *   3. User clicks "Buscar vias" → Overpass POST + JSON parse +
 *      polylines on the map (or a list-only fallback if the map
 *      never loaded).
 *   4. User clicks "Importar selecionadas" → conversion via
 *      `osmDatasetToRoadsFit` + parent `onConfirm`.
 *
 * Nothing heavy runs in the render phase. Nothing fetches by
 * itself. The "Importar OSM" button is now safe to click.
 *
 * Privacy reminder: the only thing this modal ever sends out is the
 * geographic bbox to the Overpass endpoint — no BO, no occurrence,
 * no laudo content.
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
// Fase S — adapter parity é o ÚNICO motor de conversão OSM agora.
// `convertOsmDatasetToSicroObjects` (Road v2) foi descontinuado em S.
// Gera `SicroRoadObject_parity` + `SicroRoundaboutObject_parity` em
// coordenadas de mundo (metros).
import {
  convertOsmDatasetToParityObjects,
  type SicroRoadObject_parity,
  type SicroRoundaboutObject_parity,
} from "../engine/road-parity";
import { OsmMapPanel } from "./OsmMapPanel";
import styles from "./CroquiEditor.module.css";

// ---------------------------------------------------------------------------
// Public API — unchanged from previous rounds.

export interface OsmImportResult {
  /**
   * Vias Python Parity Engine (`SicroRoadObject_parity`) em coordenadas
   * de mundo (metros). Único produto do modal pós-Fase S.
   */
  parity_roads: SicroRoadObject_parity[];
  /**
   * Rotatórias Python Parity Engine (`SicroRoundaboutObject_parity`)
   * em coordenadas de mundo (metros).
   */
  parity_roundabouts: SicroRoundaboutObject_parity[];
  /**
   * Mensagens humanas (Português) — way ignorada, geometria
   * irregular, etc. Mostrar no feedback do editor.
   */
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

export interface OsmImportModalProps {
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
    // One-shot log on every modal mount so we can tell from the
    // console exactly when the React component appears.
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
  // MVP 10 Round 5 — the map now mounts automatically. The previous
  // "Click Carregar mapa" placeholder confused the perito (it suggests
  // the map is unavailable). Leaflet still lives inside the
  // `LazyMapBoundary`, so a Leaflet crash still falls back gracefully
  // — but the default UX is "the map just works".
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

      // Fase S — único adapter ativo é o parity. Devolve
      // `SicroRoadObject_parity` + `SicroRoundaboutObject_parity` em
      // coords de mundo (metros). O perito ajusta posição/escala
      // depois pela ferramenta "Definir escala".
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
        // Round 3 lesson: the `.dialog` CSS module hard-codes
        // `width: 520px`; inline `width: 1080` overrides it.
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
            // Map area collapses to a thin column when the map is OFF —
            // the layout still fits both side panels comfortably.
            gridTemplateColumns: mapEnabled
              ? "260px 1fr 300px"
              : "320px 1fr 360px",
            gap: 10,
            minHeight: 440,
          }}
        >
          {/* ============================== LEFT ============================== */}
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

          {/* ============================== CENTRE ============================= */}
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

          {/* ============================== RIGHT ============================== */}
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

        {/* ============================== FOOTER ============================== */}
        {/* Fase S — único motor de importação OSM é o Python Parity
            Engine. Antes existia toggle v2/parity; v1 e v2 foram
            removidos do app. */}
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
          {/* Implementação: as vias viram objetos do Road Parity Engine —
              cubic Bézier de 4 pontos + rotatórias dedicadas (paridade com o
              SICRO 1.0/Python). O texto da UI é neutro de propósito. */}
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
// LEFT — coordinate input + radius + search button.
// Pure controlled component, no Leaflet imports.

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
            title="Usa as coordenadas registradas no Dossiê da ocorrência"
          >
            Do Dossiê
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
// CENTRE — map placeholder OR the lazy-mounted OsmMapPanel.

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

/**
 * Placeholder shown only when the perito explicitly clicked "Esconder
 * mapa" (or when the ErrorBoundary above brought us back to this view
 * after a Leaflet crash). The default modal state mounts the map
 * directly — no manual step required.
 */
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

/**
 * ErrorBoundary that isolates Leaflet failures: if anything inside
 * OsmMapPanel throws (tile load, react-leaflet bug, missing CSS,
 * etc.), the modal stays open with the placeholder shown.
 */
import { Component, type ErrorInfo } from "react";

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
// RIGHT — way list with checkboxes. No Leaflet either.

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
