/**
 * Tela de entrada do módulo Imagem: lista de análises da ocorrência + picker
 * de origem (foto do caso, frame de vídeo, arquivo local). Com análise aberta
 * no store, delega para `ImageEditor`.
 */

import { useCallback, useEffect, useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { ImageIcon, ImagePlus } from "lucide-react";
import { Central, CentralMini, type CentralItem, type CentralLamina } from "@components/Central/Central";
import { NoOccurrenceState } from "@components/NoOccurrenceState/NoOccurrenceState";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import {
  selectActiveOccurrence,
  selectActiveWorkspacePath,
  useWorkspaceStore,
} from "@stores/workspaceStore";
import type { ImageAnalysis, ImageMetadata } from "@domain/image_analysis";
import type { VideoStoryboardFrame } from "@domain/video";
import { useImagemStore } from "./store/imagemStore";
import { ImageEditor } from "./editor/ImageEditor";
import { assetUrl } from "./editor/shared";
import imagemImg from "./assets/central/imagem.webp";
import framesImg from "./assets/central/frames.webp";
import styles from "./ImagemModule.module.css";

const ORIGEM: Record<string, string> = {
  local_import: "arquivo",
  video_frame: "frame de vídeo",
  photo: "foto do caso",
  evidence: "evidência",
};

/** "4000×3000 · arquivo" a partir da linha da análise. */
function resumoDe(a: ImageAnalysis): string {
  let dim = "";
  try {
    const m = JSON.parse(a.metadata_json) as Partial<ImageMetadata>;
    if (m.width && m.height) dim = `${m.width}×${m.height} · `;
  } catch {
    /* sem dimensões */
  }
  return dim + (ORIGEM[a.source_kind] ?? a.source_kind);
}

export function ImagemModule() {
  const occurrence = useWorkspaceStore(selectActiveOccurrence);
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const list = useImagemStore((s) => s.list);
  const isLoadingList = useImagemStore((s) => s.isLoadingList);
  const loadList = useImagemStore((s) => s.loadList);
  const activeAnalysis = useImagemStore((s) => s.activeAnalysis);
  const activeDoc = useImagemStore((s) => s.activeDoc);
  const openAnalysis = useImagemStore((s) => s.openAnalysis);
  const createFromEvidence = useImagemStore((s) => s.createFromEvidence);
  const createFromFile = useImagemStore((s) => s.createFromFile);
  const clearActive = useImagemStore((s) => s.clearActive);

  const [error, setError] = useState<string | null>(null);
  const [showPicker, setShowPicker] = useState(false);

  useEffect(() => {
    if (!workspacePath) return;
    void loadList(workspacePath);
  }, [workspacePath, loadList]);

  const handleOpen = useCallback(
    async (analysisId: string) => {
      if (!workspacePath) return;
      try {
        await openAnalysis(workspacePath, analysisId);
      } catch (err) {
        setError(toSicroError(err).message);
      }
    },
    [workspacePath, openAnalysis],
  );

  const handlePickFrame = async (frame: VideoStoryboardFrame) => {
    if (!workspacePath) return;
    try {
      const title = frame.title?.trim() || `Frame ${frame.id.slice(0, 6)}`;
      const row = await createFromEvidence(workspacePath, {
        title,
        source_kind: "video_frame",
        source_id: frame.id,
        original_relative_path: frame.output_path,
        original_hash_sha256: null,
      });
      setShowPicker(false);
      await openAnalysis(workspacePath, row.id);
    } catch (err) {
      setError(toSicroError(err).message);
    }
  };

  const handlePickFile = async () => {
    if (!workspacePath) return;
    try {
      const selected = await openFileDialog({
        multiple: false,
        title: "Importar imagem para análise",
        filters: [
          { name: "Imagens", extensions: ["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff"] },
        ],
      });
      if (typeof selected !== "string") return;
      const row = await createFromFile(workspacePath, {
        source_path: selected,
      });
      setShowPicker(false);
      await openAnalysis(workspacePath, row.id);
    } catch (err) {
      setError(toSicroError(err).message);
    }
  };

  if (!workspacePath || !occurrence) {
    return (
      <NoOccurrenceState
        icon={<ImagePlus size={36} strokeWidth={1.5} />}
        moduleName="Imagens"
      />
    );
  }

  if (activeAnalysis && activeDoc) {
    return (
      <ImageEditor
        workspacePath={workspacePath}
        onClose={() => {
          clearActive();
          void loadList(workspacePath);
        }}
      />
    );
  }

  const laminas: CentralLamina[] = [
    { id: "imagem", nome: "Adicionar imagem", arte: imagemImg, onClick: () => void handlePickFile() },
    { id: "frames", nome: "Frames coletados", arte: framesImg, onClick: () => setShowPicker(true) },
  ];

  const itens: CentralItem[] = list.map((a) => {
    const src = assetUrl(workspacePath, a.original_relative_path);
    return {
      id: a.id,
      titulo: a.title,
      miniatura: src ? (
        <img src={src} alt="" loading="lazy" />
      ) : (
        <CentralMini>
          <ImageIcon size={20} />
        </CentralMini>
      ),
      tipo: "imagem",
      resumo: resumoDe(a),
      quando: a.updated_at,
      onAbrir: () => void handleOpen(a.id),
    };
  });

  return (
    <Central
      titulo="Central de Imagens"
      laminas={laminas}
      itens={itens}
      carregando={isLoadingList}
      erro={error}
    >
      {showPicker && (
        <SourcePicker
          workspacePath={workspacePath}
          onPickFrame={handlePickFrame}
          onClose={() => setShowPicker(false)}
        />
      )}
    </Central>
  );
}

function SourcePicker({
  workspacePath,
  onPickFrame,
  onClose,
}: {
  workspacePath: string;
  onPickFrame: (f: VideoStoryboardFrame) => void;
  onClose: () => void;
}) {
  const [frames, setFrames] = useState<VideoStoryboardFrame[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (frames === null) {
      void commands
        .listVideoMedia(workspacePath)
        .then(async (videos) => {
          const allFrames: VideoStoryboardFrame[] = [];
          for (const v of videos) {
            try {
              const bundle = await commands.openVideoMedia(workspacePath, v.id);
              allFrames.push(...bundle.storyboard);
            } catch {
              /* ignorado */
            }
          }
          if (!cancelled) setFrames(allFrames);
        })
        .catch(() => {
          if (!cancelled) setFrames([]);
        });
    }
    return () => {
      cancelled = true;
    };
  }, [workspacePath, frames]);

  return (
    <div
      className={styles.overlay}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className={styles.dialog}>
        <header className={styles.dialogHeader}>
          <strong>Frames coletados</strong>
          <button
            type="button"
            onClick={onClose}
            style={{
              background: "transparent",
              border: "1px solid var(--sicro-border)",
              padding: "4px 10px",
              borderRadius: 4,
              color: "var(--sicro-fg-muted)",
              cursor: "pointer",
            }}
          >
            Fechar
          </button>
        </header>
        <div className={styles.dialogBody}>
          {(
            <>
              {frames === null && <p className={styles.dim}>Carregando…</p>}
              {frames?.length === 0 && (
                <p className={styles.dim}>
                  Nenhum frame coletado em vídeos desta ocorrência.
                </p>
              )}
              {frames?.map((f) => (
                <button
                  key={f.id}
                  type="button"
                  className={styles.pickerItem}
                  onClick={() => onPickFrame(f)}
                >
                  <img
                    src={assetUrl(workspacePath, f.output_path) ?? ""}
                    alt=""
                    className={styles.pickerThumb}
                  />
                  <div className={styles.pickerInfo}>
                    <strong>{f.title || f.id.slice(0, 8)}</strong>
                    <code>{f.output_path}</code>
                  </div>
                </button>
              ))}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
