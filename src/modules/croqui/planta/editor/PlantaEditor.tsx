/** Editor da planta 5.0: abre o .sicroplanta, junta paleta, tela e inspetor, salva e exporta. */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { revealExported } from "@core/reveal";
import { useImmersive } from "@stores/immersiveStore";
import { selectActiveOccurrence, selectActiveWorkspacePath, useWorkspaceStore } from "@stores/workspaceStore";
import { useNavGuard } from "@app/navGuard";
import { askText } from "@components/Dialog/ask";
import { useCroquiStore } from "../../store/croquiStore";
import { UnsavedChangesModal } from "../../editor/UnsavedChangesModal";
import { dist, parseM, type Pt } from "../model/geom";
import { builtArea, roomGeometry } from "../model/rooms";
import { evidenceMeasures, measureText } from "../model/evidence";
import { coercePlanta, EVIDENCE_TIPOS, isLegacyPlanta, type SicroPlantaDoc, type WallKind } from "../model/schema";
import { deleteSelection, sheetLabel } from "./actions";
import { PlantaCanvas, type PlantaCanvasHandle } from "./PlantaCanvas";
import { PlantaInspector } from "./PlantaInspector";
import { PlantaPalette } from "./PlantaPalette";
import { stampPlanta, type PlantaLegend } from "./exportPlanta";
import { usePlanta, type Tool } from "./store";
import styles from "./planta.module.css";

const isTyping = (e: KeyboardEvent) => {
  const t = e.target as HTMLElement | null;
  return !!t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT" || t.isContentEditable);
};

function buildLegend(doc: SicroPlantaDoc): PlantaLegend {
  const rooms = roomGeometry(doc);
  return {
    evidences: doc.evidences.map((e) => ({
      label: e.label,
      title: [EVIDENCE_TIPOS[e.tipo], e.descricao.trim()].filter(Boolean).join(", "),
      sub: measureText(evidenceMeasures(doc, e)),
    })),
    people: doc.people.map((p) => ({ title: p.label || "Pessoa", sub: p.descricao, caido: p.pose === "caido" })),
    trajs: doc.trajectories.map((t) => ({ title: `${t.label} · Trajetória`, sub: t.descricao, color: t.color })),
    rooms: rooms.map((r) => ({ name: r.room.name, area: r.area })),
    usable: rooms.reduce((a, r) => a + r.area, 0),
    built: builtArea(doc),
  };
}

export function PlantaEditor() {
  useImmersive();
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const occurrence = useWorkspaceStore(selectActiveOccurrence);
  const activeCroqui = useCroquiStore((s) => s.activeCroqui);
  const clearCurrent = useCroquiStore((s) => s.clearCurrent);
  const loadList = useCroquiStore((s) => s.loadList);
  const doc = usePlanta((s) => s.doc);
  const savedJson = usePlanta((s) => s.savedJson);
  const canUndo = usePlanta((s) => s.history.length > 0);
  const canRedo = usePlanta((s) => s.future.length > 0);
  const sel = usePlanta((s) => s.sel);
  const tool = usePlanta((s) => s.tool);
  const [legacy, setLegacy] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [newWall, setNewWall] = useState<{ thickness: number; kind: WallKind }>({ thickness: 0.15, kind: "parede" });
  const [leave, setLeave] = useState<null | { resolve: (ok: boolean) => void }>(null);
  const canvasRef = useRef<PlantaCanvasHandle | null>(null);
  const registerGuard = useNavGuard((s) => s.register);
  const unregisterGuard = useNavGuard((s) => s.unregister);

  const flash = useCallback((m: string) => {
    setMessage(m);
    window.setTimeout(() => setMessage((cur) => (cur === m ? null : cur)), 4000);
  }, []);

  // ---------- abrir ----------
  useEffect(() => {
    if (!workspacePath || !activeCroqui) return;
    let alive = true;
    setLegacy(false);
    setLoadError(null);
    void commands
      .readCroqui(workspacePath, activeCroqui.id)
      .then((payload) => {
        if (!alive) return;
        const raw = payload.doc as unknown;
        if (isLegacyPlanta(raw)) {
          setLegacy(true);
          return;
        }
        usePlanta.getState().load(coercePlanta(raw, { planta_id: activeCroqui.id, occurrence_id: activeCroqui.occurrence_id, title: activeCroqui.title }));
      })
      .catch((err) => alive && setLoadError(toSicroError(err).message));
    return () => {
      alive = false;
    };
  }, [workspacePath, activeCroqui]);

  const dirty = useMemo(() => !!doc && JSON.stringify(doc) !== savedJson, [doc, savedJson]);

  // ---------- salvar ----------
  const save = useCallback(async (): Promise<boolean> => {
    const d = usePlanta.getState().doc;
    if (!workspacePath || !activeCroqui || !d) return false;
    setSaving(true);
    try {
      const stamped = { ...d, updated_at: new Date().toISOString() };
      await commands.saveCroqui(workspacePath, activeCroqui.id, stamped);
      usePlanta.getState().markSaved(usePlanta.getState().doc ?? stamped);
      await loadList(workspacePath);
      flash("Planta salva.");
      return true;
    } catch (err) {
      flash(`Falha ao salvar: ${toSicroError(err).message}`);
      return false;
    } finally {
      setSaving(false);
    }
  }, [workspacePath, activeCroqui, loadList, flash]);

  // ---------- exportar ----------
  const exportPng = useCallback(
    async (variant: "tecnico" | "limpo") => {
      const S = usePlanta.getState();
      if (!workspacePath || !activeCroqui || !S.doc || exporting) return;
      setExporting(true);
      try {
        if (JSON.stringify(S.doc) !== S.savedJson && !(await save())) return;
        S.setSel([]);
        const d = usePlanta.getState().doc!;
        const png = canvasRef.current?.capture(variant);
        if (!png) throw new Error("não foi possível capturar a folha");
        const final =
          variant === "limpo"
            ? png
            : await stampPlanta(
                png,
                {
                  occurrence: occurrence ? { numero_bo: occurrence.numero_bo, tipo_pericia: occurrence.tipo_pericia, municipio: occurrence.municipio } : null,
                  sheetLabel: sheetLabel(d),
                  timestamp: new Date(),
                  pxPerM: Math.min(d.sheet.png_width, 8000) / d.sheet.w,
                },
                buildLegend(d),
              );
        const path = await commands.exportCroquiPng(workspacePath, activeCroqui.id, { png_base64: final.split(",")[1] ?? final });
        await loadList(workspacePath);
        revealExported(workspacePath, String(path));
        flash(variant === "limpo" ? "PNG limpo exportado (só o desenho)." : "PNG técnico exportado (desenho, legenda e áreas).");
      } catch (err) {
        flash(`Falha ao exportar: ${toSicroError(err).message}`);
      } finally {
        setExporting(false);
      }
    },
    [workspacePath, activeCroqui, occurrence, exporting, save, loadList, flash],
  );

  // ---------- sair ----------
  const confirmLeave = useCallback(() => new Promise<boolean>((resolve) => setLeave({ resolve })), []);
  const back = useCallback(async () => {
    if (dirty && !(await confirmLeave())) return;
    clearCurrent();
  }, [dirty, confirmLeave, clearCurrent]);
  useEffect(() => {
    if (!dirty) {
      unregisterGuard();
      return;
    }
    registerGuard(() => confirmLeave());
    return () => unregisterGuard();
  }, [dirty, registerGuard, unregisterGuard, confirmLeave]);

  // ---------- fundo ----------
  const importBackground = useCallback(async () => {
    try {
      const picked = await openFileDialog({ multiple: false, title: "Imagem de fundo da planta", filters: [{ name: "Imagens", extensions: ["png", "jpg", "jpeg", "webp"] }] });
      if (typeof picked !== "string") return;
      const S = usePlanta.getState();
      S.apply((d) => ({
        ...d,
        background: { asset: picked, x: d.sheet.x + 0.5, y: d.sheet.y + 0.5, width: d.sheet.w * 0.7, rot: 0, opacity: 0.5, locked: false },
      }));
      S.setTool("select");
      S.setSel([{ kind: "bg", id: "bg" }]);
      flash("Fundo importado. Arraste para posicionar e use a escala por dois pontos.");
    } catch (err) {
      flash(`Falha ao importar: ${toSicroError(err).message}`);
    }
  }, [flash]);
  const onBgScalePoints = useCallback(
    async (a: Pt, b: Pt) => {
      const v = await askText({ title: "Distância real entre os dois pontos (m)", numeric: true, confirmLabel: "Aplicar escala" });
      const real = v ? parseM(v) : null;
      if (!real || real <= 0) return;
      const k = real / dist(a, b);
      usePlanta.getState().apply((d) => {
        const bg = d.background;
        if (!bg) return d;
        return { ...d, background: { ...bg, x: a.x + (bg.x - a.x) * k, y: a.y + (bg.y - a.y) * k, width: bg.width * k } };
      });
      flash("Escala do fundo ajustada.");
    },
    [flash],
  );

  // ---------- teclado ----------
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isTyping(e) || leave) return;
      const S = usePlanta.getState();
      const mod = e.ctrlKey || e.metaKey;
      const k = e.key.toLowerCase();
      if (mod && k === "s") {
        e.preventDefault();
        void save();
      } else if (mod && k === "z" && !e.shiftKey) {
        e.preventDefault();
        S.undo();
      } else if (mod && (k === "y" || (k === "z" && e.shiftKey))) {
        e.preventDefault();
        S.redo();
      } else if (mod && k === "e") {
        e.preventDefault();
        void exportPng(e.shiftKey ? "limpo" : "tecnico");
      } else if (mod && k === "0") {
        e.preventDefault();
        canvasRef.current?.fit();
      } else if (!mod && (e.key === "Delete" || e.key === "Backspace") && S.sel.length) {
        e.preventDefault();
        S.apply((d) => deleteSelection(d, S.sel));
        S.setSel([]);
      } else if (!mod && !e.altKey && (k === "v" || k === "p" || k === "c")) {
        S.setTool(k === "v" ? "select" : k === "p" ? "wall" : "room");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [save, exportPng, leave]);

  if (legacy)
    return (
      <div className={styles.legacy}>
        <h2>Planta do editor antigo</h2>
        <p>Esta planta foi feita antes da 5.0, num formato que o editor novo não abre. O arquivo continua guardado no caso. Crie uma planta nova para desenhar no editor atual.</p>
        <button type="button" className={styles.btn} style={{ flex: "none" }} onClick={clearCurrent}>
          Voltar para a lista
        </button>
      </div>
    );
  if (loadError)
    return (
      <div className={styles.legacy}>
        <h2>Não foi possível abrir a planta</h2>
        <p>{loadError}</p>
        <button type="button" className={styles.btn} style={{ flex: "none" }} onClick={clearCurrent}>
          Voltar para a lista
        </button>
      </div>
    );
  if (!doc || !workspacePath) return <div className={styles.legacy}>Abrindo a planta…</div>;

  return (
    <div className={styles.wrap}>
      <PlantaPalette
        activeTool={tool}
        onSelectTool={(t: Tool) => usePlanta.getState().setTool(t)}
        onDropTool={(t, x, y) => canvasRef.current?.dropTool(t, x, y)}
        onBack={() => void back()}
        onSave={() => void save()}
        saving={saving}
        onExport={(v) => void exportPng(v)}
        exporting={exporting}
        canUndo={canUndo}
        onUndo={() => usePlanta.getState().undo()}
        canRedo={canRedo}
        onRedo={() => usePlanta.getState().redo()}
        canDelete={sel.length > 0}
        onDelete={() => {
          const S = usePlanta.getState();
          S.apply((d) => deleteSelection(d, S.sel));
          S.setSel([]);
        }}
        onFit={() => canvasRef.current?.fit()}
        hasBackground={!!doc.background}
        onImportBackground={() => void importBackground()}
        onRemoveBackground={() => usePlanta.getState().apply((d) => ({ ...d, background: null }))}
        bgOpacity={doc.background?.opacity ?? 0.5}
        onBgOpacity={(v) => usePlanta.getState().apply((d) => (d.background ? { ...d, background: { ...d.background, opacity: v } } : d))}
        onScaleBackground={() => usePlanta.getState().setTool("bgscale")}
        bgLocked={!!doc.background?.locked}
        onToggleBgLock={() => usePlanta.getState().apply((d) => (d.background ? { ...d, background: { ...d.background, locked: !d.background.locked } } : d))}
      />
      <PlantaCanvas
        ref={canvasRef}
        workspacePath={workspacePath}
        newWall={newWall}
        onBgScalePoints={(a, b) => void onBgScalePoints(a, b)}
        title={`${activeCroqui?.title ?? "Planta"}${dirty ? " · não salvo" : ""}`}
        message={message}
      />
      <PlantaInspector newWall={newWall} onNewWall={setNewWall} />
      {leave && (
        <UnsavedChangesModal
          saving={saving}
          exporting={exporting}
          destinationLabel="sair da planta"
          onSaveAndLeave={() => {
            void save().then((ok) => {
              leave.resolve(ok);
              setLeave(null);
            });
          }}
          onDiscardAndLeave={() => {
            leave.resolve(true);
            setLeave(null);
          }}
          onCancel={() => {
            leave.resolve(false);
            setLeave(null);
          }}
        />
      )}
    </div>
  );
}
