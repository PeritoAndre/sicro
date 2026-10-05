/**
 * Wrappers tipados do `invoke` do Tauri. A UI não chama `invoke` direto:
 * os nomes dos comandos ficam centralizados aqui e os erros normalizados.
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
  AudioDiarization,
  AuthenticityReport,
  EnfComparison,
  SpectroImage,
  TranscriptAi,
} from "@domain/audio";
import type { AiCatalog, AiStatus, AiUpdateInfo } from "@domain/ai";
import { toSicroError } from "./errors";

async function safeInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args ?? {});
  } catch (err) {
    throw toSicroError(err);
  }
}

export const commands = {
  getOccurrence(workspacePath: string): Promise<Occurrence> {
    return safeInvoke<Occurrence>("get_occurrence", {
      workspacePath,
    });
  },

  createOccurrence(input: NewOccurrenceInput): Promise<LoadedOccurrence> {
    return safeInvoke<LoadedOccurrence>("create_occurrence", { input });
  },

  openOccurrence(workspacePath: string): Promise<LoadedOccurrence> {
    return safeInvoke<LoadedOccurrence>("open_occurrence", {
      workspacePath,
    });
  },

  updateOccurrence(
    workspacePath: string,
    edit: OccurrenceEdit,
  ): Promise<Occurrence> {
    return safeInvoke<Occurrence>("update_occurrence", { workspacePath, edit });
  },

  /** Só o status: um patch parcial em update_occurrence zeraria os campos não enviados. */
  setOccurrenceStatus(
    workspacePath: string,
    status: OccurrenceStatus,
  ): Promise<Occurrence> {
    return safeInvoke<Occurrence>("set_occurrence_status", {
      workspacePath,
      status,
    });
  },

  listRecentOccurrences(): Promise<RecentOccurrence[]> {
    return safeInvoke<RecentOccurrence[]>("list_recent_occurrences");
  },

  /** Só tira da lista de recentes; não apaga o workspace do disco. */
  forgetRecentOccurrence(workspaceId: string): Promise<void> {
    return safeInvoke<void>("forget_recent_occurrence", { workspaceId });
  },

  /** Apaga a pasta .sicro do disco (irreversível); o backend recusa se não for um workspace válido. */
  deleteOccurrence(workspacePath: string): Promise<void> {
    return safeInvoke<void>("delete_occurrence", { workspacePath });
  },

  /** Abre o explorador do SO na pasta do arquivo, selecionando-o (o SIGDOC bloqueia Ctrl+V). */
  revealPathInExplorer(absolutePath: string): Promise<void> {
    return safeInvoke("reveal_path_in_explorer", { absolutePath });
  },

  // ----- Configurações globais do app -----

  /** Ausente/corrompido → defaults. */
  getAppSettings(): Promise<AppSettings> {
    return safeInvoke<AppSettings>("get_app_settings", {});
  },

  saveAppSettings(settings: AppSettings): Promise<void> {
    return safeInvoke("save_app_settings", { settings });
  },

  getSettingsFilePath(): Promise<string> {
    return safeInvoke<string>("get_settings_file_path", {});
  },

  /** Posições da numeração POP do croqui corporal (`{ "tpl_vista_n_idx": [nx, ny] }`); global, ausente → `{}`. */
  loadPopCalibration(): Promise<Record<string, readonly [number, number]>> {
    return safeInvoke<Record<string, readonly [number, number]>>(
      "load_pop_calibration",
      {},
    );
  },

  savePopCalibration(
    calibration: Record<string, readonly [number, number]>,
  ): Promise<void> {
    return safeInvoke("save_pop_calibration", { calibration });
  },

  // ----- Índice global de casos -----

  getCaseIndex(): Promise<CaseIndexEntry[]> {
    return safeInvoke<CaseIndexEntry[]>("get_case_index", {});
  },

  upsertCaseIndex(entry: CaseIndexEntry): Promise<void> {
    return safeInvoke("upsert_case_index", { entry });
  },

  /** Só tira do índice (nada é apagado do disco); o caso reaparece se for reaberto. */
  removeCaseIndex(workspaceId: string): Promise<void> {
    return safeInvoke<void>("remove_case_index", { workspaceId });
  },

  // ----- Importação (.sicroapp) -----

  /** Valida o .sicroapp e materializa num .sicro novo; já devolve o ImportReport completo. */
  importSicroapp(input: ImportSicroappInput): Promise<ImportResult> {
    return safeInvoke<ImportResult>("import_sicroapp", { input });
  },

  listWorkspaceImports(workspacePath: string): Promise<Import[]> {
    return safeInvoke<Import[]>("list_workspace_imports", { workspacePath });
  },

  readImportReport(
    workspacePath: string,
    importId: string,
  ): Promise<ImportReport> {
    return safeInvoke<ImportReport>("read_import_report", {
      workspacePath,
      importId,
    });
  },

  listDossiePhotos(workspacePath: string): Promise<MediaAsset[]> {
    return safeInvoke<MediaAsset[]>("list_dossie_photos", { workspacePath });
  },

  // ----- Croqui -----
  /** Consulta Overpass QL pelo Rust (do WebView o fetch falha no Linux). Devolve o JSON cru. */
  fetchOverpass(query: string): Promise<string> {
    return safeInvoke<string>("fetch_overpass", { query });
  },
  /** Tile do mapa de referência do OSM, PNG em base64 (pelo Rust, idem). */
  fetchOsmTile(z: number, x: number, y: number): Promise<string> {
    return safeInvoke<string>("fetch_osm_tile", { z, x, y });
  },

  createCroqui(
    workspacePath: string,
    input: NewCroquiInput,
  ): Promise<CroquiDocPayload> {
    return safeInvoke<CroquiDocPayload>("create_croqui", {
      workspacePath,
      input,
    });
  },

  listCroquis(workspacePath: string): Promise<Croqui[]> {
    return safeInvoke<Croqui[]>("list_croquis", { workspacePath });
  },

  readCroqui(workspacePath: string, croquiId: string): Promise<CroquiDocPayload> {
    return safeInvoke<CroquiDocPayload>("read_croqui", {
      workspacePath,
      croquiId,
    });
  },

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

  /** Apaga a linha e o .sicrocroqui; PNGs já exportados ficam (histórico pericial). */
  deleteCroqui(workspacePath: string, croquiId: string): Promise<void> {
    return safeInvoke<void>("delete_croqui", {
      workspacePath,
      croquiId,
    });
  },

  /** Devolve o caminho do PNG relativo ao workspace. */
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

  /** Corrige a lente, recorta o retângulo escolhido e grava derivado + sidecar no workspace. */
  importDroneImage(
    workspacePath: string,
    input: DroneImportInput,
  ): Promise<DroneImportResult> {
    return safeInvoke<DroneImportResult>("import_drone_image", {
      workspacePath,
      input,
    });
  },

  // ----- Vídeo -----

  /** Copia para videos/originais/, calcula SHA-256 e roda ffprobe. */
  registerVideoMedia(
    workspacePath: string,
    input: RegisterVideoInput,
  ): Promise<VideoMedia> {
    return safeInvoke<VideoMedia>("register_video_media", {
      workspacePath,
      input,
    });
  },

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

  /** Exporta o trecho como cópia registrada no caso; o original não muda. */
  exportVideoClip(workspacePath: string, input: ExportClipInput): Promise<ExportClipResult> {
    return safeInvoke<ExportClipResult>("export_video_clip", { workspacePath, input });
  },

  /** Caminho absoluto da miniatura (gerada na 1ª vez, no cache do app). */
  videoThumbnail(workspacePath: string, mediaId: string): Promise<string> {
    return safeInvoke<string>("video_thumbnail", { workspacePath, mediaId });
  },

  listVideoMedia(workspacePath: string): Promise<VideoMedia[]> {
    return safeInvoke<VideoMedia[]>("list_video_media", { workspacePath });
  },

  /** Mídia + eventos + exportações + storyboard. */
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

  /** Extrai o quadro via FFmpeg (não é captura do player) e persiste PNG + sidecar. */
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

  // ----- Velocidade (vídeo) -----

  /** Resolve a homografia (DLT 4 pts para "plane", afim 2 pts para "line"), calcula o RMS e persiste. */
  createSpeedCalibration(
    workspacePath: string,
    input: CreateSpeedCalibrationInput,
  ): Promise<VideoSpeedCalibration> {
    return safeInvoke<VideoSpeedCalibration>("create_speed_calibration", {
      workspacePath,
      input,
    });
  },

  /** Projeta a trajetória pela homografia; regressão + Monte Carlo com ≥3 pts, média sem IC com 2. */
  computeSpeed(
    workspacePath: string,
    input: ComputeSpeedInput,
  ): Promise<VideoSpeedCalculation> {
    return safeInvoke<VideoSpeedCalculation>("compute_speed", {
      workspacePath,
      input,
    });
  },

  listSpeedCalibrations(
    workspacePath: string,
    mediaHash: string,
  ): Promise<VideoSpeedCalibration[]> {
    return safeInvoke<VideoSpeedCalibration[]>("list_speed_calibrations", {
      workspacePath,
      mediaHash,
    });
  },

  listSpeedCalculations(
    workspacePath: string,
    mediaHash: string,
  ): Promise<VideoSpeedCalculation[]> {
    return safeInvoke<VideoSpeedCalculation[]>("list_speed_calculations", {
      workspacePath,
      mediaHash,
    });
  },

  listSpeedCalculationsForOccurrence(
    workspacePath: string,
  ): Promise<VideoSpeedCalculation[]> {
    return safeInvoke<VideoSpeedCalculation[]>(
      "list_speed_calculations_for_occurrence",
      { workspacePath },
    );
  },

  getSpeedCalibration(
    workspacePath: string,
    id: string,
  ): Promise<VideoSpeedCalibration> {
    return safeInvoke<VideoSpeedCalibration>("get_speed_calibration", {
      workspacePath,
      id,
    });
  },

  // ----- Medição de distância (vídeo) -----

  /** Usa a MESMA homografia da velocidade; Monte Carlo só com σ (2 pontos não têm IC de regressão). */
  createDistanceMeasurement(
    workspacePath: string,
    input: CreateDistanceMeasurementInput,
  ): Promise<VideoDistanceMeasurement> {
    return safeInvoke<VideoDistanceMeasurement>("create_distance_measurement", {
      workspacePath,
      input,
    });
  },

  listDistanceMeasurements(
    workspacePath: string,
    mediaHash: string,
  ): Promise<VideoDistanceMeasurement[]> {
    return safeInvoke<VideoDistanceMeasurement[]>("list_distance_measurements", {
      workspacePath,
      mediaHash,
    });
  },

  listDistanceMeasurementsForOccurrence(
    workspacePath: string,
  ): Promise<VideoDistanceMeasurement[]> {
    return safeInvoke<VideoDistanceMeasurement[]>(
      "list_distance_measurements_for_occurrence",
      { workspacePath },
    );
  },

  // ----- Central de Evidências + Integridade -----

  /** `{ deep: true }` recalcula o SHA-256 dos itens com hash (lento em vídeos grandes). */
  verifyWorkspaceIntegrity(
    workspacePath: string,
    options?: VerifyOptions,
  ): Promise<WorkspaceIntegrityReport> {
    return safeInvoke<WorkspaceIntegrityReport>(
      "verify_workspace_integrity",
      { workspacePath, options: options ?? null },
    );
  },

  listEvidenceLinks(workspacePath: string): Promise<EvidenceLink[]> {
    return safeInvoke<EvidenceLink[]>("list_evidence_links", {
      workspacePath,
    });
  },

  openEvidenceFile(
    workspacePath: string,
    relativePath: string,
  ): Promise<void> {
    return safeInvoke<void>("open_evidence_file", {
      workspacePath,
      relativePath,
    });
  },

  revealEvidenceInFolder(
    workspacePath: string,
    relativePath: string,
  ): Promise<void> {
    return safeInvoke<void>("reveal_evidence_in_folder", {
      workspacePath,
      relativePath,
    });
  },

  /** Verifica e grava o relatório HTML em reports/. */
  generateWorkspaceIntegrityReport(
    workspacePath: string,
    options?: VerifyOptions,
  ): Promise<IntegrityReportArtifact> {
    return safeInvoke<IntegrityReportArtifact>(
      "generate_workspace_integrity_report",
      { workspacePath, options: options ?? null },
    );
  },

  // ----- Imagem -----

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

  /** Todos os metadados (exiftool quando instalado; senão EXIF + XMP internos). */
  readAllImageMetadata(
    workspacePath: string,
    relativePath: string,
  ): Promise<{ source: string; entries: { group: string; tag: string; value: string }[] }> {
    return safeInvoke("read_all_image_metadata", { workspacePath, relativePath });
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

  /** 256 bins R/G/B/Lum + estatísticas. */
  computeImageHistogram(
    workspacePath: string,
    relativePath: string,
  ): Promise<ImageHistogram> {
    return safeInvoke<ImageHistogram>("compute_image_histogram", {
      workspacePath,
      relativePath,
    });
  },

  applyOperationStack(
    workspacePath: string,
    input: ApplyOperationStackInput,
  ): Promise<ApplyOperationStackResult> {
    return safeInvoke<ApplyOperationStackResult>("apply_operation_stack", {
      workspacePath,
      input,
    });
  },

  /** Preview sobre um bitmap já reduzido no cliente (base64): não abre o original, por isso é rápido. */
  /** Miniaturas da galeria de filtros: pilha atual + cada candidato (JPEG base64; "" se falhar). */
  filterThumbnails(input: {
    image_base64: string;
    operations: ApplyOperationStackInput["operations"];
    adjustments?: ApplyOperationStackInput["adjustments"];
    candidates: ApplyOperationStackInput["operations"];
  }): Promise<string[]> {
    return safeInvoke<string[]>("filter_thumbnails", { input });
  },
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

  /** Grava a região da seleção como camada PNG; `apply_processing` recorta do resultado em vez do original. */
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

  /** HTML autocontido em imagens/relatorios/. */
  generateImageAnalysisReport(
    workspacePath: string,
    analysisId: string,
  ): Promise<ImageAnalysisReportArtifact> {
    return safeInvoke<ImageAnalysisReportArtifact>(
      "generate_image_analysis_report",
      { workspacePath, analysisId },
    );
  },

  // ----- Backup, saúde do sistema e contadores -----

  /** ZIP .sicrobackup; `destination` padrão é <workspace>/backups/. */
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

  /** Incremental, 1 .sicrobackup por caso; emite `global-backup-progress` por caso. */
  generateGlobalBackup(
    cases: GlobalCaseInput[],
    destination: string,
  ): Promise<GlobalBackupReport> {
    return safeInvoke<GlobalBackupReport>("generate_global_backup", {
      cases,
      destination,
    });
  },

  /** Não sobrescreve casos existentes sem `overwrite`; emite `restore-backup-progress` por caso. */
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

  getSystemHealthSnapshot(
    workspacePath?: string | null,
  ): Promise<SystemHealthSnapshot> {
    return safeInvoke<SystemHealthSnapshot>("get_system_health_snapshot", {
      workspacePath: workspacePath ?? null,
    });
  },

  getOccurrenceCounts(workspacePath: string): Promise<WorkspaceCounters> {
    return safeInvoke<WorkspaceCounters>("get_occurrence_counts", {
      workspacePath,
    });
  },

  /** HTML em <workspace>/reports/system_health_<TS>.html. */
  generateSystemHealthReport(
    workspacePath?: string | null,
  ): Promise<HealthReportArtifact> {
    return safeInvoke<HealthReportArtifact>(
      "generate_system_health_report",
      { workspacePath: workspacePath ?? null },
    );
  },

  // ----- Áudio -----

  /** Extrai a trilha para WAV de análise, com hash e custódia. */
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

  /** Preserva o original e gera o WAV de análise. */
  importAudioFile(workspacePath: string, sourcePath: string): Promise<AudioMedia> {
    return safeInvoke<AudioMedia>("import_audio_file", {
      workspacePath,
      sourcePath,
    });
  },

  listAudioMedia(workspacePath: string): Promise<AudioMedia[]> {
    return safeInvoke<AudioMedia[]>("list_audio_media", { workspacePath });
  },

  openAudioMedia(workspacePath: string, audioId: string): Promise<AudioMedia> {
    return safeInvoke<AudioMedia>("open_audio_media", { workspacePath, audioId });
  },

  /** PNG via FFmpeg; devolve o caminho relativo. */
  audioSpectrogram(workspacePath: string, audioId: string): Promise<string> {
    return safeInvoke<string>("audio_spectrogram", { workspacePath, audioId });
  },

  /** Pico/RMS/DC/clipping do WAV de análise. */
  audioMeasure(
    workspacePath: string,
    audioId: string,
  ): Promise<AudioMeasurements> {
    return safeInvoke<AudioMeasurements>("audio_measure", {
      workspacePath,
      audioId,
    });
  },

  /** Espectrograma da janela [t0, t1] no tamanho da tela. */
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

  /** Welch FFT; `fftSize` potência de 2 (padrão 4096). */
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

  audioAuthenticity(workspacePath: string, audioId: string): Promise<AuthenticityReport> {
    return safeInvoke<AuthenticityReport>("audio_authenticity", { workspacePath, audioId });
  },

  /** Compara o ENF com uma gravação de referência da rede. */
  audioEnfCompare(
    workspacePath: string,
    audioId: string,
    referenceAudioId: string,
    nominalHz: number | null,
  ): Promise<EnfComparison> {
    return safeInvoke<EnfComparison>("audio_enf_compare", {
      workspacePath,
      audioId,
      referenceAudioId,
      nominalHz,
    });
  },

  /** `nominalHz` null/ausente = automático (50/60). */
  audioEnf(
    workspacePath: string,
    audioId: string,
    nominalHz?: number | null,
  ): Promise<EnfResult> {
    return safeInvoke<EnfResult>("audio_enf", {
      workspacePath,
      audioId,
      nominalHz: nominalHz ?? null,
    });
  },

  /** Novo clipe derivado (kind "recorte") com hash + custódia; o original não muda. */
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

  /** Novo derivado (kind "compilacao") com manifesto .compilacao.json; `segments` em snake_case (struct serde). */
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

  listAudioMarkers(
    workspacePath: string,
    audioSha256: string,
  ): Promise<AudioMarker[]> {
    return safeInvoke<AudioMarker[]>("list_audio_markers", {
      workspacePath,
      audioSha256,
    });
  },

  deleteAudioMarker(workspacePath: string, markerId: string): Promise<void> {
    return safeInvoke<void>("delete_audio_marker", { workspacePath, markerId });
  },

  /** Derivado realçado (kind "realce") por filtros FFmpeg: "denoise" | "highpass" | "lowpass" | "normalize". */
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

  listAudioTranscript(
    workspacePath: string,
    audioSha256: string,
  ): Promise<AudioTranscriptSegment[]> {
    return safeInvoke<AudioTranscriptSegment[]>("list_audio_transcript", {
      workspacePath,
      audioSha256,
    });
  },

  /** Replace-all; devolve os segmentos com os ids gerados pelo backend. */
  saveAudioTranscript(
    workspacePath: string,
    audioSha256: string,
    segments: {
      idx: number;
      t_start: number;
      t_end: number | null;
      speaker: string;
      text: string;
      ai: TranscriptAi | null;
    }[],
  ): Promise<AudioTranscriptSegment[]> {
    return safeInvoke<AudioTranscriptSegment[]>("save_audio_transcript", {
      workspacePath,
      audioSha256,
      segments,
    });
  },

  whisperStatus(whisperBin?: string): Promise<WhisperStatus> {
    return safeInvoke<WhisperStatus>("whisper_status", {
      whisperBin: whisperBin ?? null,
    });
  },

  /** Rascunho local (whisper.cpp, offline); não persiste — o perito revisa. */
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

  // ----- Gerenciador de IA -----

  /** Builds do whisper.cpp + modelos + se há GPU NVIDIA. */
  getAiCatalog(): Promise<AiCatalog> {
    return safeInvoke<AiCatalog>("get_ai_catalog", {});
  },

  getAiStatus(): Promise<AiStatus> {
    return safeInvoke<AiStatus>("get_ai_status", {});
  },

  /** Progresso via evento "ai-download-progress"; auto-configura os caminhos. */
  installAiAsset(assetId: string): Promise<AiStatus> {
    return safeInvoke<AiStatus>("install_ai_asset", { assetId });
  },

  removeAiAsset(assetId: string): Promise<AiStatus> {
    return safeInvoke<AiStatus>("remove_ai_asset", { assetId });
  },

  /** sherpa-onnx + 2 modelos, hash fixo. */
  installDiarization(): Promise<AiStatus> {
    return safeInvoke<AiStatus>("install_diarization", {});
  },

  removeDiarization(): Promise<AiStatus> {
    return safeInvoke<AiStatus>("remove_diarization", {});
  },

  /** `numSpeakers` null = automático. */
  diarizeAudio(
    workspacePath: string,
    audioId: string,
    numSpeakers: number | null,
  ): Promise<AudioDiarization> {
    return safeInvoke<AudioDiarization>("diarize_audio", { workspacePath, audioId, numSpeakers });
  },

  getAudioDiarization(workspacePath: string, audioSha256: string): Promise<AudioDiarization | null> {
    return safeInvoke<AudioDiarization | null>("get_audio_diarization", { workspacePath, audioSha256 });
  },

  saveDiarizationNames(workspacePath: string, audioSha256: string, names: string[]): Promise<void> {
    return safeInvoke<void>("save_diarization_names", { workspacePath, audioSha256, names });
  },

  /** Opt-in: só informa a última release do whisper.cpp. */
  checkAiUpdates(): Promise<AiUpdateInfo> {
    return safeInvoke<AiUpdateInfo>("check_ai_updates", {});
  },
  /** Opt-in: atualiza o motor para a última release upstream. */
  updateWhisperEngine(): Promise<AiStatus> {
    return safeInvoke<AiStatus>("update_whisper_engine", {});
  },
} as const;
