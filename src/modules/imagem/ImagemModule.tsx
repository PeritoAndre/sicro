/**
 * ImagemModule — MVP 7 (Editor de Imagem Pericial).
 *
 * Tela de entrada do módulo: lista de análises da ocorrência ativa +
 * picker de origem para criar nova análise (foto do Dossiê, frame de
 * vídeo, arquivo local).
 *
 * Quando uma análise está aberta no store, delega para `ImageEditor`.
 */

import { useCallback, useEffect, useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  BoxSelect,
  Camera,
  Contrast,
  FolderOpen,
  ImageIcon,
  Layers,
  Plus,
  Ruler,
  Share2,
  ShieldCheck,
  SlidersHorizontal,
} from "lucide-react";
import { Button } from "@components/Button/Button";
import { NoOccurrenceState } from "@components/NoOccurrenceState/NoOccurrenceState";
import {
  ModuleLanding,
  type ModuleLandingFeature,
} from "@components/ModuleLanding/ModuleLanding";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import {
  selectActiveOccurrence,
  selectActiveWorkspacePath,
  useWorkspaceStore,
} from "@stores/workspaceStore";
import type { MediaAsset } from "@domain/import";
import type { VideoStoryboardFrame } from "@domain/video";
import { useImagemStore } from "./store/imagemStore";
import { ImageEditor } from "./editor/ImageEditor";
import { assetUrl, formatDateTime } from "./editor/shared";
import { useImageEditRoundtripStore } from "@stores/imageEditRoundtripStore";
import { joinWorkspace } from "@modules/laudo/document-engine";
import styles from "./ImagemModule.module.css";

type PickerTab = "dossie" | "frames" | "file";

const IMAGEM_FEATURES: ModuleLandingFeature[] = [
  {
    icon: <Contrast size={18} />,
    title: "Filtros forenses",
    desc: "Sobel, CLAHE, níveis, ELA e mais — realçam o que é difícil de ver.",
  },
  {
    icon: <SlidersHorizontal size={18} />,
    title: "Ajustes não-destrutivos",
    desc: "Brilho, contraste, matiz, canais — só no derivado, em pilha reversível.",
  },
  {
    icon: <BoxSelect size={18} />,
    title: "Seleção e camadas",
    desc: "Região estilo Photoshop: mascarar filtros, recortar e mover camadas.",
  },
  {
    icon: <Ruler size={18} />,
    title: "Medições com escala",
    desc: "Distâncias, áreas e ângulos em unidade real quando calibrado.",
  },
  {
    icon: <ShieldCheck size={18} />,
    title: "Integridade e custódia",
    desc: "Hash, EXIF e histórico. O original nunca é alterado.",
  },
  {
    icon: <Share2 size={18} />,
    title: "Integra à suíte",
    desc: "O derivado tratado volta ao Laudo e à Documentoscopia, com proveniência.",
  },
];

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

  // Pós-laudo S — Round-trip Laudo → Imagem.
  //
  // Quando o perito clicou "Editar" numa foto do laudo, o store de
  // roundtrip foi setado com a source_relative_path da foto + workspace.
  // Aqui detectamos isso e auto-criamos uma análise a partir desse path
  // (uma vez só por roundtrip; o flag local evita re-disparar enquanto
  // o store permanece "editing").
  const roundtripState = useImageEditRoundtripStore((s) => s.state);
  const roundtripRequest = useImageEditRoundtripStore((s) => s.request);
  const [roundtripBootstrapped, setRoundtripBootstrapped] = useState(false);

  useEffect(() => {
    if (!workspacePath) return;
    void loadList(workspacePath);
  }, [workspacePath, loadList]);

  useEffect(() => {
    if (
      roundtripState !== "editing" ||
      !roundtripRequest ||
      !workspacePath ||
      roundtripBootstrapped ||
      activeAnalysis // já tem análise aberta — não duplica
    ) {
      return;
    }
    // Marca antes do await pra evitar segunda invocação concorrente.
    setRoundtripBootstrapped(true);
    void (async () => {
      try {
        const absolutePath = joinWorkspace(
          roundtripRequest.workspace_path,
          roundtripRequest.source_relative_path,
        );
        const title = roundtripRequest.laudo_title
          ? `Edição de foto — ${roundtripRequest.laudo_title}`
          : `Edição de foto do laudo`;
        const row = await createFromFile(workspacePath, {
          source_path: absolutePath,
          title,
        });
        await openAnalysis(workspacePath, row.id);
      } catch (err) {
        setError(
          `Falha ao abrir foto vinda do laudo: ${toSicroError(err).message}`,
        );
        // Reset flag pra usuário poder tentar de novo via "Voltar".
        setRoundtripBootstrapped(false);
      }
    })();
  }, [
    roundtripState,
    roundtripRequest,
    workspacePath,
    roundtripBootstrapped,
    activeAnalysis,
    createFromFile,
    openAnalysis,
  ]);

  // Resetar flag quando roundtrip terminar (state → idle).
  useEffect(() => {
    if (roundtripState === "idle") setRoundtripBootstrapped(false);
  }, [roundtripState]);

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

  const handlePickPhoto = async (asset: MediaAsset) => {
    if (!workspacePath) return;
    try {
      const row = await createFromEvidence(workspacePath, {
        title: asset.caption?.trim() || asset.original_filename || `Foto ${asset.original_id ?? asset.id.slice(0, 6)}`,
        source_kind: "photo",
        source_id: asset.id,
        original_relative_path: asset.relative_path,
        original_hash_sha256: asset.sha256,
      });
      setShowPicker(false);
      await openAnalysis(workspacePath, row.id);
    } catch (err) {
      setError(toSicroError(err).message);
    }
  };

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
        icon={<ImageIcon size={36} strokeWidth={1.5} />}
        moduleName="Imagens"
      />
    );
  }

  // Quando uma análise está aberta, exibir o editor.
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

  const isEmpty = !isLoadingList && list.length === 0;

  return (
    <div className={styles.wrap}>
      {error && (
        <div className={styles.errorBanner} style={{ margin: "var(--space-3)" }}>
          <AlertTriangle size={14} /> {error}
        </div>
      )}
      {isLoadingList && list.length === 0 ? (
        <p className={styles.dim} style={{ padding: "var(--space-6)" }}>
          Carregando análises…
        </p>
      ) : isEmpty ? (
        <ModuleLanding
          icon={<ImageIcon size={44} strokeWidth={1.2} />}
          title="Imagem — Tratamento Pericial"
          subtitle="Realce e revele o que é difícil de enxergar: filtros, ajustes e seleções não-destrutivos sobre derivados. O original nunca é alterado e tudo fica na cadeia de custódia."
          actions={
            <Button
              variant="primary"
              leftIcon={<Plus size={15} />}
              onClick={() => setShowPicker(true)}
            >
              Nova análise
            </Button>
          }
          features={IMAGEM_FEATURES}
          note="O tratamento de imagem é apoio à visualização — não conclui por si. A autenticação documental e as conclusões periciais cabem ao perito e ao módulo de Documentoscopia."
        />
      ) : (
        <>
          <header className={styles.topBar}>
            <div className={styles.title}>
              <h1>Imagem — Editor de Análise Pericial</h1>
              <p className={styles.subtitle}>
                {`${list.length} análise(s) registrada(s).`}
              </p>
            </div>
            <div className={styles.topActions}>
              <Button
                variant="primary"
                leftIcon={<Plus size={14} />}
                onClick={() => setShowPicker(true)}
              >
                Nova análise
              </Button>
            </div>
          </header>
          <main className={styles.content}>
            <div className={styles.listGrid}>
              {list.map((a) => (
                <AnalysisCard
                  key={a.id}
                  analysis={a}
                  workspacePath={workspacePath}
                  onOpen={() => void handleOpen(a.id)}
                />
              ))}
            </div>
          </main>
        </>
      )}

      {showPicker && (
        <SourcePicker
          workspacePath={workspacePath}
          onPickPhoto={handlePickPhoto}
          onPickFrame={handlePickFrame}
          onPickFile={() => void handlePickFile()}
          onClose={() => setShowPicker(false)}
        />
      )}
    </div>
  );
}

function AnalysisCard({
  analysis,
  workspacePath,
  onOpen,
}: {
  analysis: ReturnType<typeof useImagemStore.getState>["list"][number];
  workspacePath: string;
  onOpen: () => void;
}) {
  const src = assetUrl(workspacePath, analysis.original_relative_path);
  return (
    <button type="button" className={styles.card} onClick={onOpen}>
      <div className={styles.cardThumb}>
        {src ? <img src={src} alt={analysis.title} /> : <ImageIcon size={28} />}
      </div>
      <div className={styles.cardBody}>
        <strong>{analysis.title}</strong>
        <small>
          {analysis.source_kind} · {formatDateTime(analysis.updated_at)}
        </small>
      </div>
    </button>
  );
}

function SourcePicker({
  workspacePath,
  onPickPhoto,
  onPickFrame,
  onPickFile,
  onClose,
}: {
  workspacePath: string;
  onPickPhoto: (a: MediaAsset) => void;
  onPickFrame: (f: VideoStoryboardFrame) => void;
  onPickFile: () => void;
  onClose: () => void;
}) {
  const [tab, setTab] = useState<PickerTab>("dossie");
  const [photos, setPhotos] = useState<MediaAsset[] | null>(null);
  const [frames, setFrames] = useState<VideoStoryboardFrame[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (tab === "dossie" && photos === null) {
      void commands
        .listDossiePhotos(workspacePath)
        .then((data) => {
          if (!cancelled) setPhotos(data);
        })
        .catch(() => {
          if (!cancelled) setPhotos([]);
        });
    }
    if (tab === "frames" && frames === null) {
      void commands
        .listVideoMedia(workspacePath)
        .then(async (videos) => {
          const allFrames: VideoStoryboardFrame[] = [];
          for (const v of videos) {
            try {
              const bundle = await commands.openVideoMedia(workspacePath, v.id);
              allFrames.push(...bundle.storyboard);
            } catch {
              /* ignore */
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
  }, [tab, workspacePath, photos, frames]);

  return (
    <div
      className={styles.overlay}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className={styles.dialog}>
        <header className={styles.dialogHeader}>
          <strong>Nova análise — escolher imagem de origem</strong>
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
        <nav className={styles.dialogTabs}>
          <button
            type="button"
            className={`${styles.dialogTab} ${tab === "dossie" ? styles.dialogTabActive : ""}`}
            onClick={() => setTab("dossie")}
          >
            <Camera size={12} /> Dossiê (fotos)
          </button>
          <button
            type="button"
            className={`${styles.dialogTab} ${tab === "frames" ? styles.dialogTabActive : ""}`}
            onClick={() => setTab("frames")}
          >
            <Layers size={12} /> Frames de vídeo
          </button>
          <button
            type="button"
            className={`${styles.dialogTab} ${tab === "file" ? styles.dialogTabActive : ""}`}
            onClick={() => setTab("file")}
          >
            <FolderOpen size={12} /> Arquivo local
          </button>
        </nav>
        <div className={styles.dialogBody}>
          {tab === "dossie" && (
            <>
              {photos === null && <p className={styles.dim}>Carregando…</p>}
              {photos?.length === 0 && (
                <p className={styles.dim}>Nenhuma foto importada no Dossiê.</p>
              )}
              {photos?.map((p) => (
                <button
                  key={p.id}
                  type="button"
                  className={styles.pickerItem}
                  onClick={() => onPickPhoto(p)}
                >
                  {p.relative_path && (
                    <img
                      src={assetUrl(workspacePath, p.relative_path) ?? ""}
                      alt=""
                      className={styles.pickerThumb}
                    />
                  )}
                  <div className={styles.pickerInfo}>
                    <strong>{p.original_id ?? p.id.slice(0, 8)}</strong>
                    <code>{p.relative_path}</code>
                  </div>
                </button>
              ))}
            </>
          )}
          {tab === "frames" && (
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
          {tab === "file" && (
            <div style={{ padding: "var(--space-3)", display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
              <p className={styles.dim}>
                Selecionar uma imagem do disco. O arquivo será copiado para{" "}
                <code>imagens/originais/</code> dentro do workspace e
                hasheado.
              </p>
              <Button variant="primary" leftIcon={<FolderOpen size={14} />} onClick={onPickFile}>
                Escolher arquivo…
              </Button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
