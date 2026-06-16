/**
 * OsmMapPanel — the Leaflet map for the OSM import wizard.
 *
 * Round 4 (MVP 10) split this out of `OsmImportModal` so that:
 *
 *   1. The modal can mount and become visible WITHOUT touching
 *      Leaflet. Only when the perito clicks "Carregar mapa" does
 *      this component get rendered.
 *   2. If Leaflet crashes (any reason — missing CSS, tile server
 *      down, react-leaflet bug), the `<LazyMapBoundary>` in the
 *      parent catches it and the modal stays usable in
 *      coordinate-only mode.
 *
 * Round 3 had two infinite-loop sources that locked the WebView
 * thread on modal open:
 *
 *   - `MapInvalidator.onReady` was an inline closure created every
 *     parent render. Its identity changed each render → effect deps
 *     `[map, refreshKey, onReady]` re-fired → onReady called
 *     setState → parent re-rendered → new onReady → loop.
 *   - `TileLayer.eventHandlers` was a fresh object literal every
 *     render whose `tileload`/`tileerror` handlers called setState
 *     on every tile (dozens per second).
 *
 * This file fixes both by:
 *
 *   - storing the few diagnostic counters in **refs** (not state),
 *     so they don't trigger re-renders;
 *   - using a `useRef` for the "is map ready" boolean and updating
 *     a single `<div>` text node via `useEffect(() => …, [])`
 *     ONLY once;
 *   - giving `MapInvalidator` a stable empty-dep effect — schedule
 *     the four `invalidateSize` calls on mount and never again
 *     (a separate `refreshKey` prop wires up the "Recarregar mapa"
 *     button via a controlled remount).
 */

import { useEffect, useMemo, useRef, useState } from "react";
import {
  Circle,
  CircleMarker,
  MapContainer,
  Marker,
  Polyline,
  Popup,
  TileLayer,
  useMap,
  useMapEvents,
} from "react-leaflet";
import L from "leaflet";
import type { LatLon, OsmDataset } from "../engine";

// One-shot synthetic pin so we don't depend on Leaflet's PNG sprites
// (which can 404 in Vite/Tauri builds and never render).
const SITE_PIN_ICON = L.divIcon({
  className: "sicro-osm-pin",
  html: `<div style="
    width: 16px; height: 16px; border-radius: 50%;
    background: #ef4444; border: 2px solid #fff;
    box-shadow: 0 0 6px rgba(239, 68, 68, 0.8);
  "></div>`,
  iconSize: [16, 16],
  iconAnchor: [8, 8],
});

const DEFAULT_CENTRE: LatLon = { lat: -0.0345, lon: -51.0694 }; // Macapá

interface Props {
  centre: LatLon | null;
  radius: number;
  dataset: OsmDataset | null;
  selectedIds: Set<number>;
  onMapPick: (pt: LatLon) => void;
  onToggleWay: (id: number) => void;
  onUnload: () => void;
}

export function OsmMapPanel({
  centre,
  radius,
  dataset,
  selectedIds,
  onMapPick,
  onToggleWay,
  onUnload,
}: Props) {
  console.info("[OSM] OsmMapPanel mounting (Leaflet starts now)");
  const [mapReady, setMapReady] = useState(false);
  // `refreshNonce` triggers a remount of `MapInvalidator` when the
  // user clicks "Recarregar mapa". We use `key={nonce}` so a stale
  // map measurement gets thrown out cleanly.
  const [refreshNonce, setRefreshNonce] = useState(0);

  // Build the polylines lazily and only re-derive when the dataset
  // or the selection truly changes — no per-tile re-renders.
  const wayPolylines = useMemo(() => {
    if (!dataset) return [];
    const nodeMap = new Map(dataset.nodes.map((n) => [n.id, n]));
    type WayPoly = {
      id: number;
      latlngs: [number, number][];
      tags: Record<string, string>;
      selected: boolean;
    };
    const out: WayPoly[] = [];
    for (const w of dataset.ways) {
      const latlngs: [number, number][] = [];
      for (const ref of w.node_refs) {
        const n = nodeMap.get(ref);
        if (n) latlngs.push([n.lat, n.lon]);
      }
      if (latlngs.length >= 2) {
        out.push({
          id: w.id,
          latlngs,
          tags: w.tags,
          selected: selectedIds.has(w.id),
        });
      }
    }
    return out;
  }, [dataset, selectedIds]);

  return (
    <div style={{ position: "absolute", inset: 0 }}>
      <MapContainer
        center={[
          centre?.lat ?? DEFAULT_CENTRE.lat,
          centre?.lon ?? DEFAULT_CENTRE.lon,
        ]}
        zoom={16}
        style={{ width: "100%", height: "100%" }}
        attributionControl
      >
        <TileLayer
          attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>'
          url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
        />
        <MapInvalidator
          key={refreshNonce}
          onReady={() => setMapReady(true)}
        />
        <CentreSync centre={centre} radius={radius} />
        <MapClickHandler
          onClick={(latlng) =>
            onMapPick({ lat: latlng.lat, lon: latlng.lng })
          }
        />
        {centre && (
          <>
            <Marker
              position={[centre.lat, centre.lon]}
              icon={SITE_PIN_ICON}
            >
              <Popup>Centro do sinistro</Popup>
            </Marker>
            {/* Círculo do raio — usa `Circle` (raio em METROS geográficos)
                ao invés do `CircleMarker` (que usa pixels). Assim o
                círculo cresce/diminui com o zoom do mapa e sempre
                representa visualmente o raio real selecionado.
                Hard-cap do clip de importação. */}
            <Circle
              center={[centre.lat, centre.lon]}
              radius={radius}
              pathOptions={{
                color: "#5aa9e6",
                fillColor: "#5aa9e6",
                fillOpacity: 0.12,
                weight: 2,
                dashArray: "6 4",
              }}
            />
            {/* Pino central pequeno (CircleMarker — pixels fixos) por
                cima do `Circle` pra deixar a posição exata visível
                mesmo em zoom alto, quando o `Circle` ocupa toda a
                tela. */}
            <CircleMarker
              center={[centre.lat, centre.lon]}
              radius={4}
              pathOptions={{
                color: "#5aa9e6",
                fillColor: "#5aa9e6",
                fillOpacity: 0.9,
                weight: 0,
              }}
            />
          </>
        )}
        {wayPolylines.map((w) => (
          <Polyline
            key={w.id}
            positions={w.latlngs}
            pathOptions={{
              color: w.selected ? "#22c55e" : "#94a3b8",
              weight: w.selected ? 4 : 2,
              opacity: w.selected ? 0.9 : 0.6,
            }}
            eventHandlers={{
              click: () => onToggleWay(w.id),
            }}
          />
        ))}
      </MapContainer>

      {/* Top-right note — emphasises that the map is for LOCATION
          only; the final shape on the croqui comes from Road Engine. */}
      <div
        style={{
          position: "absolute",
          top: 8,
          right: 8,
          maxWidth: 260,
          background: "rgba(15, 23, 42, 0.85)",
          color: "#cbd5e1",
          fontSize: 10,
          lineHeight: 1.35,
          padding: "5px 8px",
          borderRadius: 4,
          pointerEvents: "none",
          textAlign: "right",
        }}
      >
        O desenho final é gerado pelo <strong>Road Engine</strong> do
        SICRO — o mapa OSM serve apenas para localizar e selecionar
        as vias.
      </div>
      {/* Floating control strip — independent of Leaflet state. */}
      <div
        style={{
          position: "absolute",
          bottom: 6,
          left: 6,
          right: 6,
          display: "flex",
          gap: 8,
          fontSize: 10,
          color: "#cbd5e1",
          background: "rgba(15, 23, 42, 0.85)",
          padding: "3px 6px",
          borderRadius: 4,
          fontFamily: "var(--font-mono)",
          pointerEvents: "none",
        }}
      >
        <span
          style={{
            color: mapReady ? "#16a34a" : "#f59e0b",
            fontWeight: 600,
          }}
        >
          ● leaflet {mapReady ? "OK" : "carregando…"}
        </span>
        <button
          type="button"
          onClick={() => {
            console.info("[OSM] manual refresh requested");
            setRefreshNonce((n) => n + 1);
          }}
          style={{
            marginLeft: "auto",
            pointerEvents: "auto",
            background: "rgba(90, 169, 230, 0.2)",
            border: "1px solid #5aa9e6",
            color: "#fff",
            borderRadius: 3,
            padding: "1px 6px",
            fontSize: 10,
            cursor: "pointer",
            fontFamily: "inherit",
          }}
          title="Força Leaflet a recalcular dimensões"
        >
          Recarregar mapa
        </button>
        <button
          type="button"
          onClick={() => {
            console.info("[OSM] user closed map panel");
            onUnload();
          }}
          style={{
            pointerEvents: "auto",
            background: "rgba(239, 68, 68, 0.15)",
            border: "1px solid #ef4444",
            color: "#fff",
            borderRadius: 3,
            padding: "1px 6px",
            fontSize: 10,
            cursor: "pointer",
            fontFamily: "inherit",
          }}
          title="Volta para o modo sem mapa"
        >
          Esconder mapa
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Leaflet helpers — each one has an EMPTY dep array or a stable ref
// pattern so no parent re-render can re-fire them.

/**
 * Calls `invalidateSize` once when mounted (with a microtask + 50/200/500 ms
 * fallbacks). To force a re-measurement after the user clicks "Recarregar
 * mapa", remount the component by bumping its `key`.
 *
 * Uses a ref for `onReady` so the effect doesn't depend on the callback's
 * identity → no re-fire loop.
 */
function MapInvalidator({ onReady }: { onReady: () => void }) {
  const map = useMap();
  const onReadyRef = useRef(onReady);
  onReadyRef.current = onReady;

  useEffect(() => {
    console.info("[OSM] MapInvalidator effect — scheduling invalidateSize");
    let cancelled = false;
    const fire = () => {
      if (cancelled) return;
      map.invalidateSize();
    };
    queueMicrotask(() => {
      if (cancelled) return;
      fire();
      onReadyRef.current();
    });
    const t50 = setTimeout(fire, 50);
    const t200 = setTimeout(fire, 200);
    const t500 = setTimeout(() => {
      if (cancelled) return;
      fire();
      onReadyRef.current();
    }, 500);
    return () => {
      cancelled = true;
      clearTimeout(t50);
      clearTimeout(t200);
      clearTimeout(t500);
    };
    // Empty deps — mount-only. To force a fresh measurement,
    // the parent remounts this via `key={refreshNonce}`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return null;
}

function CentreSync({
  centre,
  radius,
}: {
  centre: LatLon | null;
  radius: number;
}) {
  const map = useMap();
  // Effect re-runs quando o centro ou o raio muda — fitBounds enquadra
  // o círculo do raio com uma pequena margem (1.4x para deixar
  // respiração visual).
  useEffect(() => {
    if (!centre) return;
    // Cria bounds com extensão de ~1.4x do raio em torno do centro.
    // L.LatLng.toBounds(meters) gera um quadrado de lado 2 * meters.
    const padded = radius * 1.4;
    const ll = L.latLng(centre.lat, centre.lon);
    const bounds = ll.toBounds(padded * 2);
    map.flyToBounds(bounds, { duration: 0.3, maxZoom: 19 });
  }, [centre, radius, map]);
  return null;
}

function MapClickHandler({
  onClick,
}: {
  onClick: (latlng: { lat: number; lng: number }) => void;
}) {
  const onClickRef = useRef(onClick);
  onClickRef.current = onClick;
  useMapEvents({
    click(e) {
      onClickRef.current(e.latlng);
    },
  });
  return null;
}
