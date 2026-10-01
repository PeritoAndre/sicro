/**
 * Thin wrappers around Tauri's `invoke`. The UI must NOT call `invoke`
 * directly — go through this module so:
 *   1. Command names are typed and centralized;
 *   2. Errors are normalized into `SicroError`;
 *   3. Future cross-cutting concerns (logging, retries) have one place to land.
 */

import { invoke } from "@tauri-apps/api/core";
import type {
  LoadedOccurrence,
  NewOccurrenceInput,
  Occurrence,
  OccurrenceEdit,
  OccurrenceStatus,
  RecentOccurrence,
} from "@domain/occurrence";
import type { AppSettings } from "@domain/app_settings";
import type { CaseIndexEntry } from "@domain/case_index";
import type {
  Import,
  ImportReport,
  ImportResult,
  ImportSicroappInput,
  MediaAsset,
} from "@domain/import";
import type {
  Croqui,
  CroquiDocPayload,
  DroneImportInput,
  DroneImportResult,
  ExportCroquiPngInput,
  NewCroquiInput,
} from "@domain/croqui";
import type {
  CollectFrameInput,
  CollectFrameResult,
  CreateVideoEventInput,
  ExportClipInput,
  ExportClipResult,
  RegisterVideoInput,
  UpdateStoryboardFrameInput,
  SetVideoClockInput,
  UpdateVideoEventInput,
  VideoBundle,
  VideoClockCalibration,
  VideoEvent,
  VideoMedia,
  VideoOperationLog,
  VideoStoryboardFrame,
} from "@domain/video";
import type {
  ComputeSpeedInput,
  CreateSpeedCalibrationInput,
  VideoSpeedCalculation,
  VideoSpeedCalibration,
} from "@domain/video_speed";
import type {
  CreateDistanceMeasurementInput,
  VideoDistanceMeasurement,
} from "@domain/video_distance";
import type { EvidenceLink } from "@domain/evidence";
import type {
  IntegrityReportArtifact,
  VerifyOptions,
  WorkspaceIntegrityReport,
} from "@domain/evidence_registry";
import type {
  ApplyOperationPreviewResult,
  ApplyOperationStackInput,
  ApplyOperationStackResult,
  CreateImageAnalysisInput,
  ExportImageInput,
  ImageAnalysis,
  ImageAnalysisPayload,
  ImageAnalysisReportArtifact,
  ImageAssetBytes,
  ImageExport,
  ImageHistogram,
  ImageMetadata,
  ImageOperationLog,
  ImportLocalImageInput,
  SaveImageAnalysisInput,
} from "@domain/image_analysis";
import type {
  BackupArtifact,
  GlobalBackupReport,
  GlobalCaseInput,
  HealthReportArtifact,
  RestoreReport,
  SystemHealthSnapshot,
  WorkspaceCounters,
} from "@domain/alpha";
import type {
  AudioMarker,
  AudioMeasurements,
  AudioMedia,
  AudioTranscriptSegment,
  EnfResult,
  SpectrumResult,
  TranscriptCandidate,
  WhisperStatus,
  SpectroImage,
} from "@domain/audio";
import type { AiCatalog, AiStatus, AiUpdateInfo } from "@domain/ai";
import { toSicroError, type SicroError } from "./errors";

async function safeInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args ?? {});
  } catch (err) {
    throw toSicroError(err);
  }
}


export const commands = {
  /** Returns the currently loaded occurrence for a given workspace path. */
  getOccurrence(workspacePath: string): Promise<Occurrence> {
    return safeInvoke<Occurrence>("get_occurrence", {
      workspacePath,
    });
  },

  /** Creates a fresh .sicro workspace with an initial occurrence row. */
  createOccurrence(input: NewOccurrenceInput): Promise<LoadedOccurrence> {
    return safeInvoke<LoadedOccurrence>("create_occurrence", { input });
  },

  /** Opens an existing .sicro workspace. */
  openOccurrence(workspacePath: string): Promise<LoadedOccurrence> {
    return safeInvoke<LoadedOccurrence>("open_occurrence", {
      workspacePath,
    });
  },

  /** Atualiza a identificação do caso (cabeçalho editável do Dossiê). */
  updateOccurrence(
    workspacePath: string,
    edit: OccurrenceEdit,
  ): Promise<Occurrence> {
    return safeInvoke<Occurrence>("update_occurrence", { workspacePath, edit });
  },

  /**
   * Muda SÓ o status da ocorrência (concluir / reabrir) — sem tocar no cabeçalho.
   * Comando dedicado: não corre o risco de zerar campos não enviados como o
   * update_occurrence faria com um patch parcial.
   */
  setOccurrenceStatus(
    workspacePath: string,
    status: OccurrenceStatus,
  ): Promise<Occurrence> {
    return safeInvoke<Occurrence>("set_occurrence_status", {
      workspacePath,
      status,
    });
  },

  /** Returns the list of recently opened workspaces (newest first). */
  listRecentOccurrences(): Promise<RecentOccurrence[]> {
    return safeInvoke<RecentOccurrence[]>("list_recent_occurrences");
  },

  /** Removes an entry from the recents list (does NOT delete the workspace on disk). */
  forgetRecentOccurrence(workspaceId: string): Promise<void> {
    return safeInvoke<void>("forget_recent_occurrence", { workspaceId });
  },

  /**
   * EXCLUI PERMANENTEMENTE a pasta `.sicro` do disco. Destrutivo e
   * irreversível — só chamar após confirmação explícita do usuário. O backend
   * recusa se a pasta não for um workspace .sicro válido (trava de segurança).
   */
  deleteOccurrence(workspacePath: string): Promise<void> {
    return safeInvoke<void>("delete_occurrence", { workspacePath });
  },

  // ----- Laudo (Spike B) -----








  // ----- I — Integração SIGDOCS -----






  /**
   * Abre o Explorer do SO (Windows/macOS/Linux) na pasta de um
   * arquivo, selecionando-o. Usado quando o perito vai arrastar o
   * PDF exportado pra dentro do SIGDOC (que bloqueia Ctrl+V).
   */
  revealPathInExplorer(absolutePath: string): Promise<void> {
    return safeInvoke("reveal_path_in_explorer", { absolutePath });
  },

  // K — Credenciais SIGDOC (Windows Credential Manager)




  // ----- Configurações globais do app (o "cofrinho" fora do .sicro) -----

  /** Lê as configurações globais. Ausente/corrompido → defaults. */
  getAppSettings(): Promise<AppSettings> {
    return safeInvoke<AppSettings>("get_app_settings", {});
  },

  /** Grava as configurações globais (escrita atômica no app_config_dir). */
  saveAppSettings(settings: AppSettings): Promise<void> {
    return safeInvoke("save_app_settings", { settings });
  },




  /** Caminho absoluto do arquivo app-settings.json (diagnóstico). */
  getSettingsFilePath(): Promise<string> {
    return safeInvoke<string>("get_settings_file_path", {});
  },

  /**
   * Calibração das posições da numeração POP do croqui corporal (mapa
   * `{ "tpl_vista_n_idx": [nx, ny] }`). Arquivo próprio em app_config_dir;
   * ausente → `{}`. Global, vale para todas as ocorrências.
   */
  loadPopCalibration(): Promise<Record<string, readonly [number, number]>> {
    return safeInvoke<Record<string, readonly [number, number]>>(
      "load_pop_calibration",
      {},
    );
  },

  /** Grava a calibração da numeração POP (escrita atômica). */
  savePopCalibration(
    calibration: Record<string, readonly [number, number]>,
  ): Promise<void> {
    return safeInvoke("save_pop_calibration", { calibration });
  },

  // ----- Estatísticas (exportação do dashboard) -----


  // ----- Índice global de casos (estatísticas gerais) -----

  /** Lê o índice global de casos (todos os casos já vistos pelo app). */
  getCaseIndex(): Promise<CaseIndexEntry[]> {
    return safeInvoke<CaseIndexEntry[]>("get_case_index", {});
  },


  /** Insere/atualiza um caso no índice global (idempotente por id). */
  upsertCaseIndex(entry: CaseIndexEntry): Promise<void> {
    return safeInvoke("upsert_case_index", { entry });
  },

  /**
   * Remove um caso do índice global (NÃO apaga nada do disco — só tira das
   * listas/estatísticas). O caso reaparece se for reaberto.
   */
  removeCaseIndex(workspaceId: string): Promise<void> {
    return safeInvoke<void>("remove_case_index", { workspaceId });
  },

  // ----- Export (Spike C) -----






  // ----- Importer (Spike D — .sicroapp) -----

  /**
   * Open a .sicroapp picked by the user, validate it, and materialise it
   * into a fresh .sicro workspace. The full ImportReport is included in
   * the response so the UI can display the summary immediately without a
   * second round-trip.
   */
  importSicroapp(input: ImportSicroappInput): Promise<ImportResult> {
    return safeInvoke<ImportResult>("import_sicroapp", { input });
  },

  /** Lists every import row stored in a workspace's SQLite. */
  listWorkspaceImports(workspacePath: string): Promise<Import[]> {
    return safeInvoke<Import[]>("list_workspace_imports", { workspacePath });
  },

  /** Reads the persisted import_report.json from disk. */
  readImportReport(
    workspacePath: string,
    importId: string,
  ): Promise<ImportReport> {
    return safeInvoke<ImportReport>("read_import_report", {
      workspacePath,
      importId,
    });
  },

  /** Lists the photos imported into a workspace (newest captured first). */
  // ----- Dossiê Operacional (MVP 3) -----


  /** Same as listWorkspacePhotos — kept for symmetry with the rest of the dossier API. */
  listDossiePhotos(workspacePath: string): Promise<MediaAsset[]> {
    return safeInvoke<MediaAsset[]>("list_dossie_photos", { workspacePath });
  },








  // ----- Croqui (Spike E) -----

  /** Creates an empty .sicrocroqui + row in the `croquis` table. */
  createCroqui(
    workspacePath: string,
    input: NewCroquiInput,
  ): Promise<CroquiDocPayload> {
    return safeInvoke<CroquiDocPayload>("create_croqui", {
      workspacePath,
      input,
    });
  },

  /** Lists every croqui of the active occurrence (most recent first). */
  listCroquis(workspacePath: string): Promise<Croqui[]> {
    return safeInvoke<Croqui[]>("list_croquis", { workspacePath });
  },

  /** Reads a croqui (row + full .sicrocroqui envelope). */
  readCroqui(workspacePath: string, croquiId: string): Promise<CroquiDocPayload> {
    return safeInvoke<CroquiDocPayload>("read_croqui", {
      workspacePath,
      croquiId,
    });
  },

  /** Overwrites the .sicrocroqui on disk + bumps updated_at. */
  saveCroqui(
    workspacePath: string,
    croquiId: string,
    doc: unknown,
  ): Promise<Croqui> {
    return safeInvoke<Croqui>("save_croqui", {
      workspacePath,
      croquiId,
      doc,
    });
  },

  /**
   * Remove o croqui do workspace: apaga a linha em `croquis` e o
   * arquivo `.sicrocroqui` em disco. PNGs já exportados em
   * `croquis/exports/` NÃO são removidos (preservam o histórico
   * pericial). Grava `croqui.deleted` no audit log.
   */
  deleteCroqui(workspacePath: string, croquiId: string): Promise<void> {
    return safeInvoke<void>("delete_croqui", {
      workspacePath,
      croquiId,
    });
  },

  /**
   * Persist a PNG export produced by Konva.toDataURL(). Returns the
   * workspace-relative path of the saved PNG.
   */
  exportCroquiPng(
    workspacePath: string,
    croquiId: string,
    input: ExportCroquiPngInput,
  ): Promise<string> {
    return safeInvoke<string>("export_croqui_png", {
      workspacePath,
      croquiId,
      input,
    });
  },

  /**
   * MVP 9 Round 4 — Drone import.
   *
   * Reads an aerial photo, applies radial lens correction at the chosen
   * intensity, crops to the rectangle the user drew in the wizard,
   * persists the derivative + sidecar inside the workspace, and returns
   * the workspace-relative paths so the caller can drop the result as
   * a croqui background image.
   */
  importDroneImage(
    workspacePath: string,
    input: DroneImportInput,
  ): Promise<DroneImportResult> {
    return safeInvoke<DroneImportResult>("import_drone_image", {
      workspacePath,
      input,
    });
  },

  // ----- Video (Spike F) -----

  /**
   * Copies the user-picked video into `videos/originais/`, hashes it
   * (SHA-256), runs ffprobe and persists the metadata. Returns the
   * `VideoMedia` row.
   */
  registerVideoMedia(
    workspacePath: string,
    input: RegisterVideoInput,
  ): Promise<VideoMedia> {
    return safeInvoke<VideoMedia>("register_video_media", {
      workspacePath,
      input,
    });
  },

  /** Relógio da câmera de todos os vídeos da ocorrência. */
  listVideoClocks(workspacePath: string): Promise<VideoClockCalibration[]> {
    return safeInvoke<VideoClockCalibration[]>("list_video_clocks", { workspacePath });
  },

  setVideoClock(
    workspacePath: string,
    input: SetVideoClockInput,
  ): Promise<VideoClockCalibration> {
    return safeInvoke<VideoClockCalibration>("set_video_clock", { workspacePath, input });
  },

  deleteVideoClock(workspacePath: string, mediaHash: string): Promise<void> {
    return safeInvoke<void>("delete_video_clock", { workspacePath, mediaHash });
  },

  /** Exporta um trecho como CÓPIA registrada no caso (o original não muda). */
  exportVideoClip(workspacePath: string, input: ExportClipInput): Promise<ExportClipResult> {
    return safeInvoke<ExportClipResult>("export_video_clip", { workspacePath, input });
  },

  /** Caminho absoluto da miniatura (gerada na 1ª vez, no cache do app). */
  videoThumbnail(workspacePath: string, mediaId: string): Promise<string> {
    return safeInvoke<string>("video_thumbnail", { workspacePath, mediaId });
  },

  /** Lists every video registered in the active occurrence. */
  listVideoMedia(workspacePath: string): Promise<VideoMedia[]> {
    return safeInvoke<VideoMedia[]>("list_video_media", { workspacePath });
  },

  /** Aggregated bundle (media + events + exports + storyboard). */
  openVideoMedia(workspacePath: string, mediaId: string): Promise<VideoBundle> {
    return safeInvoke<VideoBundle>("open_video_media", {
      workspacePath,
      mediaId,
    });
  },

  createVideoEvent(
    workspacePath: string,
    input: CreateVideoEventInput,
  ): Promise<VideoEvent> {
    return safeInvoke<VideoEvent>("create_video_event", {
      workspacePath,
      input,
    });
  },

  updateVideoEvent(
    workspacePath: string,
    eventId: string,
    input: UpdateVideoEventInput,
  ): Promise<VideoEvent> {
    return safeInvoke<VideoEvent>("update_video_event", {
      workspacePath,
      eventId,
      input,
    });
  },

  deleteVideoEvent(workspacePath: string, eventId: string): Promise<void> {
    return safeInvoke<void>("delete_video_event", { workspacePath, eventId });
  },

  /**
   * Extracts a single PNG frame via FFmpeg (NOT a screenshot of the
   * player). Writes the PNG + sidecar JSON to
   * `videos/storyboards/frames/` and persists `video_exports` +
   * `video_storyboard_frames`.
   */
  collectVideoFrame(
    workspacePath: string,
    input: CollectFrameInput,
  ): Promise<CollectFrameResult> {
    return safeInvoke<CollectFrameResult>("collect_video_frame", {
      workspacePath,
      input,
    });
  },

  updateStoryboardFrame(
    workspacePath: string,
    frameId: string,
    input: UpdateStoryboardFrameInput,
  ): Promise<VideoStoryboardFrame> {
    return safeInvoke<VideoStoryboardFrame>("update_storyboard_frame", {
      workspacePath,
      frameId,
      input,
    });
  },

  deleteStoryboardFrame(
    workspacePath: string,
    frameId: string,
    deletePng?: boolean,
  ): Promise<void> {
    return safeInvoke<void>("delete_storyboard_frame", {
      workspacePath,
      frameId,
      deletePng,
    });
  },

  listVideoOperationLogs(
    workspacePath: string,
    mediaHash: string,
    limit?: number,
  ): Promise<VideoOperationLog[]> {
    return safeInvoke<VideoOperationLog[]>("list_video_operation_logs", {
      workspacePath,
      mediaHash,
      limit,
    });
  },

  // ----- Calculador de Velocidade (vídeo / speed) -----

  /**
   * Resolve a homografia (DLT 4-pts para `method: "plane"` OU calibração
   * afim para `method: "line"` com 2 pts), calcula o RMS de reprojeção e
   * persiste a calibração. O `occurrence_id` vem do Manifest do workspace.
   */
  createSpeedCalibration(
    workspacePath: string,
    input: CreateSpeedCalibrationInput,
  ): Promise<VideoSpeedCalibration> {
    return safeInvoke<VideoSpeedCalibration>("create_speed_calibration", {
      workspacePath,
      input,
    });
  },

  /**
   * Projeta a trajetória pixel→mundo pela homografia e estima a velocidade:
   * regressão por eixo + Monte Carlo (≥3 pontos, calibração de plano) ou
   * média sem incerteza (2 pontos). Persiste mc_seed + mc_sigmas para
   * reprodutibilidade. Campos de IC/MC vêm `null` quando não se aplicam.
   */
  computeSpeed(
    workspacePath: string,
    input: ComputeSpeedInput,
  ): Promise<VideoSpeedCalculation> {
    return safeInvoke<VideoSpeedCalculation>("compute_speed", {
      workspacePath,
      input,
    });
  },

  /** Lista as calibrações de velocidade de uma mídia (mais recentes primeiro). */
  listSpeedCalibrations(
    workspacePath: string,
    mediaHash: string,
  ): Promise<VideoSpeedCalibration[]> {
    return safeInvoke<VideoSpeedCalibration[]>("list_speed_calibrations", {
      workspacePath,
      mediaHash,
    });
  },

  /** Lista os cálculos de velocidade de uma mídia (mais recentes primeiro). */
  listSpeedCalculations(
    workspacePath: string,
    mediaHash: string,
  ): Promise<VideoSpeedCalculation[]> {
    return safeInvoke<VideoSpeedCalculation[]>("list_speed_calculations", {
      workspacePath,
      mediaHash,
    });
  },

  /**
   * Lista TODOS os cálculos de velocidade da ocorrência (qualquer mídia),
   * mais recentes primeiro. Usado pelo laudo para escolher um cálculo a
   * transcrever na seção de metodologia.
   */
  listSpeedCalculationsForOccurrence(
    workspacePath: string,
  ): Promise<VideoSpeedCalculation[]> {
    return safeInvoke<VideoSpeedCalculation[]>(
      "list_speed_calculations_for_occurrence",
      { workspacePath },
    );
  },

  /** Lê uma calibração de velocidade pelo id. */
  getSpeedCalibration(
    workspacePath: string,
    id: string,
  ): Promise<VideoSpeedCalibration> {
    return safeInvoke<VideoSpeedCalibration>("get_speed_calibration", {
      workspacePath,
      id,
    });
  },

  // ----- Medição de distância (vídeo / measure) -----

  /**
   * Consome uma calibração existente: projeta os 2 pontos pixel→mundo pela
   * MESMA homografia da velocidade e calcula a distância pontual. Com σ
   * informado (e calibração de plano ou razão cruzada), roda o Monte Carlo e
   * persiste mc_seed + mc_sigmas. Sem σ, sai só a distância pontual (mc_* null)
   * — distância de 2 pontos NÃO tem IC de regressão. O occurrence_id e o
   * media_hash vêm do backend (Manifest / calibração).
   */
  createDistanceMeasurement(
    workspacePath: string,
    input: CreateDistanceMeasurementInput,
  ): Promise<VideoDistanceMeasurement> {
    return safeInvoke<VideoDistanceMeasurement>("create_distance_measurement", {
      workspacePath,
      input,
    });
  },

  /** Lista as medições de distância de uma mídia (mais recentes primeiro). */
  listDistanceMeasurements(
    workspacePath: string,
    mediaHash: string,
  ): Promise<VideoDistanceMeasurement[]> {
    return safeInvoke<VideoDistanceMeasurement[]>("list_distance_measurements", {
      workspacePath,
      mediaHash,
    });
  },

  /**
   * Lista TODAS as medições de distância da ocorrência (qualquer mídia), mais
   * recentes primeiro. Usado pelo laudo para escolher uma medição a transcrever
   * na seção de metodologia.
   */
  listDistanceMeasurementsForOccurrence(
    workspacePath: string,
  ): Promise<VideoDistanceMeasurement[]> {
    return safeInvoke<VideoDistanceMeasurement[]>(
      "list_distance_measurements_for_occurrence",
      { workspacePath },
    );
  },

  // ----- Evidência → Laudo (MVP 4) -----



  // ----- Central de Evidências + Integridade (MVP 5) -----

  /**
   * Full integrity check. Pass `{ deep: true }` to recompute SHA-256
   * for items that store a hash (slow on large videos).
   */
  verifyWorkspaceIntegrity(
    workspacePath: string,
    options?: VerifyOptions,
  ): Promise<WorkspaceIntegrityReport> {
    return safeInvoke<WorkspaceIntegrityReport>(
      "verify_workspace_integrity",
      { workspacePath, options: options ?? null },
    );
  },

  /** Lists every `evidence_links` row of the active occurrence. */
  listEvidenceLinks(workspacePath: string): Promise<EvidenceLink[]> {
    return safeInvoke<EvidenceLink[]>("list_evidence_links", {
      workspacePath,
    });
  },

  /** Open the file with the OS default handler. */
  openEvidenceFile(
    workspacePath: string,
    relativePath: string,
  ): Promise<void> {
    return safeInvoke<void>("open_evidence_file", {
      workspacePath,
      relativePath,
    });
  },

  /** Reveal the file in the platform file explorer. */
  revealEvidenceInFolder(
    workspacePath: string,
    relativePath: string,
  ): Promise<void> {
    return safeInvoke<void>("reveal_evidence_in_folder", {
      workspacePath,
      relativePath,
    });
  },

  /**
   * Run verification and persist an HTML report under `reports/`.
   * Returns the descriptor (relative path + status snapshot).
   */
  generateWorkspaceIntegrityReport(
    workspacePath: string,
    options?: VerifyOptions,
  ): Promise<IntegrityReportArtifact> {
    return safeInvoke<IntegrityReportArtifact>(
      "generate_workspace_integrity_report",
      { workspacePath, options: options ?? null },
    );
  },

  // ----- Editor de Imagem Pericial (MVP 7) -----

  createImageAnalysisFromEvidence(
    workspacePath: string,
    input: CreateImageAnalysisInput,
  ): Promise<ImageAnalysis> {
    return safeInvoke<ImageAnalysis>(
      "create_image_analysis_from_evidence",
      { workspacePath, input },
    );
  },

  createImageAnalysisFromFile(
    workspacePath: string,
    input: ImportLocalImageInput,
  ): Promise<ImageAnalysis> {
    return safeInvoke<ImageAnalysis>("create_image_analysis_from_file", {
      workspacePath,
      input,
    });
  },



  listImageAnalyses(workspacePath: string): Promise<ImageAnalysis[]> {
    return safeInvoke<ImageAnalysis[]>("list_image_analyses", {
      workspacePath,
    });
  },

  readImageAnalysis(
    workspacePath: string,
    analysisId: string,
  ): Promise<ImageAnalysisPayload> {
    return safeInvoke<ImageAnalysisPayload>("read_image_analysis", {
      workspacePath,
      analysisId,
    });
  },

  saveImageAnalysis(
    workspacePath: string,
    analysisId: string,
    input: SaveImageAnalysisInput,
  ): Promise<ImageAnalysis> {
    return safeInvoke<ImageAnalysis>("save_image_analysis", {
      workspacePath,
      analysisId,
      input,
    });
  },

  exportImageDerivative(
    workspacePath: string,
    analysisId: string,
    input: ExportImageInput,
  ): Promise<ImageExport> {
    return safeInvoke<ImageExport>("export_image_derivative", {
      workspacePath,
      analysisId,
      input,
    });
  },

  readImageAsset(
    workspacePath: string,
    relativePath: string,
  ): Promise<ImageAssetBytes> {
    return safeInvoke<ImageAssetBytes>("read_image_asset", {
      workspacePath,
      relativePath,
    });
  },

  getImageMetadata(
    workspacePath: string,
    relativePath: string,
    computeHash?: boolean,
  ): Promise<ImageMetadata> {
    return safeInvoke<ImageMetadata>("get_image_metadata", {
      workspacePath,
      relativePath,
      computeHash: computeHash ?? false,
    });
  },

  listImageOperationLogs(
    workspacePath: string,
    analysisId: string,
    limit?: number,
  ): Promise<ImageOperationLog[]> {
    return safeInvoke<ImageOperationLog[]>("list_image_operation_logs", {
      workspacePath,
      analysisId,
      limit,
    });
  },

  // ----- G12 — Image Engine Pro -----

  /**
   * G12.9 — Calcula histograma (256 bins R/G/B/Lum) + estatísticas
   * de uma imagem do workspace.
   */
  computeImageHistogram(
    workspacePath: string,
    relativePath: string,
  ): Promise<ImageHistogram> {
    return safeInvoke<ImageHistogram>("compute_image_histogram", {
      workspacePath,
      relativePath,
    });
  },

  /**
   * G12 — Aplica uma pilha de operações sobre a imagem original.
   * Útil para o ProcessingStackPanel mostrar resultado consolidado.
   */
  applyOperationStack(
    workspacePath: string,
    input: ApplyOperationStackInput,
  ): Promise<ApplyOperationStackResult> {
    return safeInvoke<ApplyOperationStackResult>("apply_operation_stack", {
      workspacePath,
      input,
    });
  },

  /**
   * W17 — Preview da pilha de filtros sobre um bitmap JÁ reduzido no cliente
   * (base64). NÃO abre o arquivo original: como entrada e saída são pequenas,
   * o preview ao vivo é rápido mesmo para originais de dezenas de MP. O export
   * continua em resolução cheia.
   */
  applyOperationStackPreview(input: {
    image_base64: string;
    operations: ApplyOperationStackInput["operations"];
    adjustments?: ApplyOperationStackInput["adjustments"];
  }): Promise<ApplyOperationPreviewResult> {
    return safeInvoke<ApplyOperationPreviewResult>(
      "apply_operation_stack_preview",
      { input },
    );
  },

  /**
   * W20 (S3) — Recorta a região da seleção e grava como camada de pixels
   * (PNG em `imagens/camadas/`). `apply_processing=true` recorta do RESULTADO
   * (reaplica `adjustments`+`operations`); `false` recorta do ORIGINAL fiel.
   * Devolve offset/dims (px da imagem), hash e o PNG base64 p/ exibir já.
   */
  copyRegionToLayer(
    workspacePath: string,
    input: {
      relative_path: string;
      mask: Record<string, unknown>;
      layer_id: string;
      apply_processing?: boolean;
      adjustments?: ApplyOperationStackInput["adjustments"];
      operations?: ApplyOperationStackInput["operations"];
    },
  ): Promise<{
    relative_path: string;
    x: number;
    y: number;
    width: number;
    height: number;
    hash_sha256: string;
    base64: string;
    mime: string;
  }> {
    return safeInvoke("copy_region_to_layer", { workspacePath, input });
  },

  /**
   * G12.21 — Gera relatório HTML de análise pericial. Backend coleta
   * tudo (EXIF, hashes, ops, logs, thumbnail) e produz HTML auto-contido
   * gravado em `imagens/relatorios/`.
   */
  generateImageAnalysisReport(
    workspacePath: string,
    analysisId: string,
  ): Promise<ImageAnalysisReportArtifact> {
    return safeInvoke<ImageAnalysisReportArtifact>(
      "generate_image_analysis_report",
      { workspacePath, analysisId },
    );
  },

  // ----- Consolidação Alpha (MVP 8) -----

  /**
   * Gera um `.sicrobackup` (ZIP) do workspace ativo. `destination`
   * opcional escolhe outra pasta; padrão é `<workspace>/backups/`.
   * `boLabel` ajuda o nome do arquivo.
   */
  generateWorkspaceBackup(
    workspacePath: string,
    destination?: string,
    boLabel?: string,
  ): Promise<BackupArtifact> {
    return safeInvoke<BackupArtifact>("generate_workspace_backup", {
      workspacePath,
      destination: destination ?? null,
      boLabel: boLabel ?? null,
    });
  },

  /**
   * Backup geral (todos os casos) — incremental, 1 `.sicrobackup` por caso,
   * numa pasta-espelho em `destination`. Pula casos cujo conteúdo não mudou.
   * Emite eventos `global-backup-progress` por caso (escute com `listen`).
   */
  generateGlobalBackup(
    cases: GlobalCaseInput[],
    destination: string,
  ): Promise<GlobalBackupReport> {
    return safeInvoke<GlobalBackupReport>("generate_global_backup", {
      cases,
      destination,
    });
  },

  /**
   * Restaura um conjunto de backup (HD externo, pendrive, nuvem, rede):
   * descompacta os casos na pasta local e, opcionalmente, restaura a config
   * (perfil/instituição/cabeçalhos). Não sobrescreve casos existentes (a menos
   * de `overwrite`). Emite `restore-backup-progress` por caso.
   */
  restoreBackup(
    sourceDir: string,
    options?: {
      casesParent?: string | null;
      restoreConfig?: boolean;
      overwrite?: boolean;
    },
  ): Promise<RestoreReport> {
    return safeInvoke<RestoreReport>("restore_backup", {
      sourceDir,
      casesParent: options?.casesParent ?? null,
      restoreConfig: options?.restoreConfig ?? true,
      overwrite: options?.overwrite ?? false,
    });
  },

  /**
   * Snapshot rápido (JSON) do estado de saúde do app — versão do app,
   * dependências externas, contadores do workspace ativo, integridade.
   */
  getSystemHealthSnapshot(
    workspacePath?: string | null,
  ): Promise<SystemHealthSnapshot> {
    return safeInvoke<SystemHealthSnapshot>("get_system_health_snapshot", {
      workspacePath: workspacePath ?? null,
    });
  },

  /**
   * Contagens por módulo de UM caso (leve: só consulta o banco). Alimenta o
   * índice global para os KPIs de produção da Home.
   */
  getOccurrenceCounts(workspacePath: string): Promise<WorkspaceCounters> {
    return safeInvoke<WorkspaceCounters>("get_occurrence_counts", {
      workspacePath,
    });
  },

  /**
   * Grava o relatório de saúde como HTML auto-suficiente em
   * `<workspace>/reports/system_health_<TS>.html` e retorna o
   * descritor.
   */
  generateSystemHealthReport(
    workspacePath?: string | null,
  ): Promise<HealthReportArtifact> {
    return safeInvoke<HealthReportArtifact>(
      "generate_system_health_report",
      { workspacePath: workspacePath ?? null },
    );
  },

  // ----- Áudio (módulo Áudio — Camada 1) -----

  /** Extrai a trilha de áudio de um vídeo para WAV de análise (+ hash/custódia). */
  extractAudioFromVideo(
    workspacePath: string,
    videoPath: string,
    sourceVideoSha256?: string | null,
  ): Promise<AudioMedia> {
    return safeInvoke<AudioMedia>("extract_audio_from_video", {
      workspacePath,
      videoPath,
      sourceVideoSha256: sourceVideoSha256 ?? null,
    });
  },

  /** Importa um áudio externo (WhatsApp/gravador): preserva original + gera WAV. */
  importAudioFile(workspacePath: string, sourcePath: string): Promise<AudioMedia> {
    return safeInvoke<AudioMedia>("import_audio_file", {
      workspacePath,
      sourcePath,
    });
  },

  /** Lista os áudios registrados na ocorrência. */
  listAudioMedia(workspacePath: string): Promise<AudioMedia[]> {
    return safeInvoke<AudioMedia[]>("list_audio_media", { workspacePath });
  },

  /** Lê uma mídia de áudio pelo id. */
  openAudioMedia(workspacePath: string, audioId: string): Promise<AudioMedia> {
    return safeInvoke<AudioMedia>("open_audio_media", { workspacePath, audioId });
  },

  /** Gera (FFmpeg) o espectrograma PNG do áudio; devolve o caminho relativo. */
  audioSpectrogram(workspacePath: string, audioId: string): Promise<string> {
    return safeInvoke<string>("audio_spectrogram", { workspacePath, audioId });
  },

  /** W12 — Medições objetivas (pico/RMS/DC/clipping) do WAV de análise. */
  audioMeasure(
    workspacePath: string,
    audioId: string,
  ): Promise<AudioMeasurements> {
    return safeInvoke<AudioMeasurements>("audio_measure", {
      workspacePath,
      audioId,
    });
  },

  /** W12 — Espectro (Welch FFT). `fftSize` potência de 2 (default 4096). */
  /** Espectrograma interativo da janela [t0, t1] no tamanho da tela. */
  audioSpectrogramData(
    workspacePath: string,
    audioId: string,
    opts: { t0: number; t1: number; width: number; height: number; fftSize: number; logFreq: boolean; fMax: number | null },
  ): Promise<SpectroImage> {
    return safeInvoke<SpectroImage>("audio_spectrogram_data", {
      workspacePath,
      audioId,
      t0: opts.t0,
      t1: opts.t1,
      width: opts.width,
      height: opts.height,
      fftSize: opts.fftSize,
      logFreq: opts.logFreq,
      fMax: opts.fMax,
    });
  },

  audioSpectrum(
    workspacePath: string,
    audioId: string,
    fftSize?: number,
  ): Promise<SpectrumResult> {
    return safeInvoke<SpectrumResult>("audio_spectrum", {
      workspacePath,
      audioId,
      fftSize: fftSize ?? null,
    });
  },

  /** W12 — Curva ENF + continuidade. `nominalHz` 50 ou 60 (default 60). */
  audioEnf(
    workspacePath: string,
    audioId: string,
    nominalHz?: number,
  ): Promise<EnfResult> {
    return safeInvoke<EnfResult>("audio_enf", {
      workspacePath,
      audioId,
      nominalHz: nominalHz ?? null,
    });
  },

  /**
   * Recorta o trecho [startS, endS] (segundos) de um áudio num novo clipe
   * derivado (kind "recorte"), com hash + custódia. Não altera o original.
   */
  extractAudioClip(
    workspacePath: string,
    audioId: string,
    startS: number,
    endS: number,
  ): Promise<AudioMedia> {
    return safeInvoke<AudioMedia>("extract_audio_clip", {
      workspacePath,
      audioId,
      startS,
      endS,
    });
  },

  /**
   * Compila vários trechos (de um ou mais áudios) num novo derivado rotulado
   * (kind "compilacao"), com hash + custódia + manifesto .compilacao.json.
   * Não-destrutivo. `segments` usa snake_case (campos de struct serde aninhada).
   */
  compileAudioClips(
    workspacePath: string,
    segments: {
      audio_id: string;
      start_s: number;
      end_s: number;
      label: string;
    }[],
    gapMs?: number,
  ): Promise<AudioMedia> {
    return safeInvoke<AudioMedia>("compile_audio_clips", {
      workspacePath,
      segments,
      gapMs,
    });
  },

  // ----- Documentoscopia (OCR, layout, campos, regiões, comparação) -----



  /** Adiciona um marcador temporal (timestamp + rótulo) a um áudio. */
  addAudioMarker(
    workspacePath: string,
    audioSha256: string,
    tSeconds: number,
    label: string,
  ): Promise<AudioMarker> {
    return safeInvoke<AudioMarker>("add_audio_marker", {
      workspacePath,
      audioSha256,
      tSeconds,
      label,
    });
  },

  /** Lista os marcadores de um áudio (ordenados por tempo). */
  listAudioMarkers(
    workspacePath: string,
    audioSha256: string,
  ): Promise<AudioMarker[]> {
    return safeInvoke<AudioMarker[]>("list_audio_markers", {
      workspacePath,
      audioSha256,
    });
  },

  /** Remove um marcador pelo id. */
  deleteAudioMarker(workspacePath: string, markerId: string): Promise<void> {
    return safeInvoke<void>("delete_audio_marker", { workspacePath, markerId });
  },

  /**
   * Gera um DERIVADO realçado (auxílio de escuta) de um áudio, aplicando uma
   * cadeia de filtros FFmpeg reproduzível. NÃO-destrutivo: cria uma nova mídia
   * (kind="realce"); o WAV de análise original permanece intacto. `filters`:
   * chaves dentre "denoise" | "highpass" | "lowpass" | "normalize".
   */
  enhanceAudio(
    workspacePath: string,
    sourceAudioId: string,
    filters: string[],
    /** Trecho só de ruído (A–B) para a redução por amostra. */
    noiseProfile?: { start_s: number; end_s: number } | null,
  ): Promise<AudioMedia> {
    return safeInvoke<AudioMedia>("enhance_audio", {
      workspacePath,
      sourceAudioId,
      filters,
      noiseProfile: noiseProfile ?? null,
    });
  },

  /** Lista os segmentos de degravação manual de um áudio (ordem por idx). */
  listAudioTranscript(
    workspacePath: string,
    audioSha256: string,
  ): Promise<AudioTranscriptSegment[]> {
    return safeInvoke<AudioTranscriptSegment[]>("list_audio_transcript", {
      workspacePath,
      audioSha256,
    });
  },

  /**
   * Substitui toda a degravação de um áudio (replace-all). Devolve os segmentos
   * persistidos (com ids gerados pelo backend). A transcrição é trabalho do
   * perito — o tool não transcreve.
   */
  saveAudioTranscript(
    workspacePath: string,
    audioSha256: string,
    segments: {
      idx: number;
      t_start: number;
      t_end: number | null;
      speaker: string;
      text: string;
    }[],
  ): Promise<AudioTranscriptSegment[]> {
    return safeInvoke<AudioTranscriptSegment[]>("save_audio_transcript", {
      workspacePath,
      audioSha256,
      segments,
    });
  },

  /** Diz se o whisper.cpp está disponível (PATH ou caminho informado). */
  whisperStatus(whisperBin?: string): Promise<WhisperStatus> {
    return safeInvoke<WhisperStatus>("whisper_status", {
      whisperBin: whisperBin ?? null,
    });
  },

  /**
   * Gera um RASCUNHO de transcrição (whisper.cpp local, offline) para o áudio.
   * A saída é rascunho de máquina — o perito DEVE revisar. Não persiste:
   * devolve candidatos para a tela de degravação.
   */
  transcribeAudio(
    workspacePath: string,
    audioId: string,
    opts: {
      modelPath: string;
      whisperBin?: string | null;
      language?: string | null;
      vadModelPath?: string | null;
    },
  ): Promise<TranscriptCandidate[]> {
    return safeInvoke<TranscriptCandidate[]>("transcribe_audio", {
      workspacePath,
      audioId,
      options: {
        model_path: opts.modelPath,
        whisper_bin: opts.whisperBin ?? null,
        language: opts.language ?? null,
        vad_model_path: opts.vadModelPath ?? null,
      },
    });
  },

  // ---- Gerenciador de IA (Fase 2.1) -------------------------------------

  /** Catálogo curado (builds whisper.cpp + modelos) + se há GPU NVIDIA. */
  getAiCatalog(): Promise<AiCatalog> {
    return safeInvoke<AiCatalog>("get_ai_catalog", {});
  },

  /** O que está instalado/configurado (caminhos + modelos presentes). */
  getAiStatus(): Promise<AiStatus> {
    return safeInvoke<AiStatus>("get_ai_status", {});
  },

  /** Baixa e instala um item do catálogo (progresso via evento
   * "ai-download-progress"); auto-configura os caminhos. Devolve o status. */
  installAiAsset(assetId: string): Promise<AiStatus> {
    return safeInvoke<AiStatus>("install_ai_asset", { assetId });
  },

  /** Remove um item instalado e limpa a configuração. */
  removeAiAsset(assetId: string): Promise<AiStatus> {
    return safeInvoke<AiStatus>("remove_ai_asset", { assetId });
  },

  /** OPT-IN: consulta a última release do whisper.cpp (só informa). */
  checkAiUpdates(): Promise<AiUpdateInfo> {
    return safeInvoke<AiUpdateInfo>("check_ai_updates", {});
  },
  /** OPT-IN: atualiza o motor whisper.cpp para a última release upstream. */
  updateWhisperEngine(): Promise<AiStatus> {
    return safeInvoke<AiStatus>("update_whisper_engine", {});
  },


  // ----- SICRO 3.0 — Laudo como `.docx` (registro + ponte com o Word) -----




} as const;

export type { SicroError };
