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
import type { Laudo, LaudoDocPayload, NewLaudoInput } from "@domain/laudo";
import type { Export } from "@domain/export";
import type { AppSettings, HeaderTemplate } from "@domain/app_settings";
import type { CaseIndexEntry } from "@domain/case_index";
import type {
  Import,
  ImportReport,
  ImportResult,
  ImportSicroappInput,
  MediaAsset,
} from "@domain/import";
import type {
  ChecklistItem,
  DossieSummary,
  Entity,
  FieldNote,
  Measurement,
  RehydrateOutcome,
  TimelineEvent,
  Trace,
} from "@domain/dossie";
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
  RegisterVideoInput,
  UpdateStoryboardFrameInput,
  UpdateVideoEventInput,
  VideoBundle,
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
import type {
  EvidenceAsset,
  EvidenceLink,
  RecordEvidenceLinkInput,
} from "@domain/evidence";
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
import type { PhotoImportResult } from "@domain/photo_drop";
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
} from "@domain/audio";
import type { AiCatalog, AiStatus, AiUpdateInfo } from "@domain/ai";
import type { LibreOfficeStatus } from "@domain/libreoffice";
import type {
  ComparisonSession,
  DetectedField,
  DocumentCaseFile,
  DocumentLog,
  DocumentRegion,
  FieldInput,
  OcrRun,
  OcrRunInput,
  OcrRunResult,
  OcrTextBlock,
  RegionInput,
} from "@domain/documentoscopia";
import type { OcrCatalog, OcrStatus, OcrUpdateInfo } from "@domain/ocr";
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

  /** Creates a fresh laudo row + empty .sicrodoc on disk. */
  createLaudo(
    workspacePath: string,
    input: NewLaudoInput,
  ): Promise<LaudoDocPayload> {
    return safeInvoke<LaudoDocPayload>("create_laudo", {
      workspacePath,
      input,
    });
  },

  /**
   * POC — Importa um `.docx` do Word como um novo laudo (mão única,
   * melhor-esforço). Cria a linha + grava o `.sicrodoc` convertido.
   */
  importDocxAsLaudo(
    workspacePath: string,
    sourcePath: string,
    title?: string,
  ): Promise<LaudoDocPayload> {
    return safeInvoke<LaudoDocPayload>("import_docx_as_laudo", {
      workspacePath,
      sourcePath,
      title: title ?? null,
    });
  },

  /** Lists every laudo registered in the workspace's SQLite. */
  listLaudos(workspacePath: string): Promise<Laudo[]> {
    return safeInvoke<Laudo[]>("list_laudos", { workspacePath });
  },

  /** Reads a laudo (row + full .sicrodoc envelope). */
  readLaudo(workspacePath: string, laudoId: string): Promise<LaudoDocPayload> {
    return safeInvoke<LaudoDocPayload>("read_laudo", {
      workspacePath,
      laudoId,
    });
  },

  /** Overwrites the `.sicrodoc` on disk and bumps `updated_at`. */
  saveLaudo(
    workspacePath: string,
    laudoId: string,
    doc: unknown,
  ): Promise<Laudo> {
    return safeInvoke<Laudo>("save_laudo", {
      workspacePath,
      laudoId,
      doc,
    });
  },

  /**
   * Remove o laudo do workspace: apaga a linha em `laudos` e o
   * arquivo `.sicrodoc` em disco. Idempotente para o arquivo
   * (NotFound é tratado como sucesso). Grava `laudo.deleted` no
   * audit log antes da remoção.
   */
  deleteLaudo(workspacePath: string, laudoId: string): Promise<void> {
    return safeInvoke<void>("delete_laudo", {
      workspacePath,
      laudoId,
    });
  },

  /**
   * H — Importa um PDF assinado (gov.br ou SIGDOCS) de volta para o
   * workspace. Grava em `laudos/<id>/assinados/<filename>.pdf`,
   * computa SHA-256 e devolve metadados para o frontend persistir em
   * `doc.finalization.signature`.
   */
  importSignedPdf(
    workspacePath: string,
    input: {
      laudo_id: string;
      source_absolute_path: string;
      preferred_filename?: string | null;
    },
  ): Promise<{
    relative_path: string;
    sha256: string;
    size_bytes: number;
  }> {
    return safeInvoke("import_signed_pdf", { workspacePath, input });
  },

  // ----- I — Integração SIGDOCS -----

  /** Resolve a URL do SIGDOCS efetiva do workspace (manifest ou default). */
  getSigdocsUrl(
    workspacePath: string,
  ): Promise<{ url: string; source: "manifest" | "default" }> {
    return safeInvoke("get_sigdocs_url", { workspacePath });
  },

  /** Onda 1 — abre o SIGDOCS numa janela secundária do SO. */
  openSigdocsWindow(url?: string): Promise<void> {
    return safeInvoke("open_sigdocs_window", { url: url ?? null });
  },

  /**
   * Onda 3 — "Cover mode": abre o SIGDOC num webview borderless que
   * cobre EXATAMENTE a área de conteúdo do editor (entre topbar e
   * statusbar, à direita da rail). `bounds` em CSS px relativos ao
   * webview principal.
   */
  openSigdocsCover(
    url: string | null,
    bounds: { x: number; y: number; width: number; height: number },
  ): Promise<void> {
    return safeInvoke("open_sigdocs_cover", { url, bounds });
  },

  /** Reposiciona o cover quando a área disponível muda (resize, route change). */
  updateSigdocsCoverBounds(bounds: {
    x: number;
    y: number;
    width: number;
    height: number;
  }): Promise<void> {
    return safeInvoke("update_sigdocs_cover_bounds", { bounds });
  },

  closeSigdocsCover(): Promise<void> {
    return safeInvoke("close_sigdocs_cover", {});
  },

  /**
   * Abre o Explorer do SO (Windows/macOS/Linux) na pasta de um
   * arquivo, selecionando-o. Usado quando o perito vai arrastar o
   * PDF exportado pra dentro do SIGDOC (que bloqueia Ctrl+V).
   */
  revealPathInExplorer(absolutePath: string): Promise<void> {
    return safeInvoke("reveal_path_in_explorer", { absolutePath });
  },

  // K — Credenciais SIGDOC (Windows Credential Manager)

  /**
   * Salva email + senha do SIGDOC no Windows Credential Manager
   * (criptografado per-user). O autofill é injetado automaticamente
   * quando o cover do SIGDOC abre.
   */
  saveSigdocCredentials(email: string, password: string): Promise<void> {
    return safeInvoke("save_sigdoc_credentials", { email, password });
  },

  /**
   * Lê o status das credenciais — retorna o email cadastrado e SE há
   * senha no keyring. NUNCA retorna a senha em si por segurança.
   */
  getSigdocCredentialsStatus(): Promise<{
    email: string | null;
    has_password: boolean;
  }> {
    return safeInvoke("get_sigdoc_credentials_status", {});
  },

  /** Remove email + senha do SIGDOC do storage. */
  deleteSigdocCredentials(): Promise<void> {
    return safeInvoke("delete_sigdoc_credentials", {});
  },

  // ----- Configurações globais do app (o "cofrinho" fora do .sicro) -----

  /** Lê as configurações globais. Ausente/corrompido → defaults. */
  getAppSettings(): Promise<AppSettings> {
    return safeInvoke<AppSettings>("get_app_settings", {});
  },

  /** Grava as configurações globais (escrita atômica no app_config_dir). */
  saveAppSettings(settings: AppSettings): Promise<void> {
    return safeInvoke("save_app_settings", { settings });
  },

  /**
   * Cabeçalhos oficiais — pasta dedicada `<app_config_dir>/cabecalhos/`
   * (1 arquivo `.json` por cabeçalho). Lista os salvos.
   */
  listHeaderTemplates(): Promise<HeaderTemplate[]> {
    return safeInvoke<HeaderTemplate[]>("list_header_templates", {});
  },

  /** Grava/atualiza um cabeçalho (arquivo `<id>.json`, escrita atômica). */
  saveHeaderTemplate(template: HeaderTemplate): Promise<void> {
    return safeInvoke("save_header_template", { template });
  },

  /** Remove um cabeçalho salvo (idempotente). */
  deleteHeaderTemplate(templateId: string): Promise<void> {
    return safeInvoke("delete_header_template", { templateId });
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

  /**
   * Grava uma exportação do dashboard de estatísticas em
   * `<workspace>/exports/estatisticas/` e devolve o caminho relativo.
   */
  saveStatisticsExport(
    workspacePath: string,
    format: "html" | "csv" | "json",
    content: string,
  ): Promise<string> {
    return safeInvoke<string>("save_statistics_export", {
      workspacePath,
      format,
      content,
    });
  },

  // ----- Índice global de casos (estatísticas gerais) -----

  /** Lê o índice global de casos (todos os casos já vistos pelo app). */
  getCaseIndex(): Promise<CaseIndexEntry[]> {
    return safeInvoke<CaseIndexEntry[]>("get_case_index", {});
  },

  /**
   * Grava a exportação das estatísticas GERAIS em
   * `Documentos/SICRO/estatisticas-gerais/`. Devolve o caminho absoluto.
   */
  saveGeneralStatisticsExport(
    format: "html" | "csv" | "json",
    content: string,
  ): Promise<string> {
    return safeInvoke<string>("save_general_statistics_export", {
      format,
      content,
    });
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

  /** Writes the rendered HTML to `<workspace>/exports/html/`. */
  exportLaudoHtml(
    workspacePath: string,
    laudoId: string,
    html: string,
  ): Promise<Export> {
    return safeInvoke<Export>("export_laudo_html", {
      workspacePath,
      laudoId,
      html,
    });
  },

  /** Renders the HTML to PDF via headless Edge and writes to `exports/pdf/`. */
  exportLaudoPdf(
    workspacePath: string,
    laudoId: string,
    html: string,
    /** Rodapé "Folha X de Y" (HTML com spans pageNumber/totalPages) impresso em
     *  toda página via CDP. Passado quando o laudo usa os campos {page}/{pages}. */
    pageFooter?: string | null,
  ): Promise<Export> {
    return safeInvoke<Export>("export_laudo_pdf", {
      workspacePath,
      laudoId,
      html,
      pageFooter: pageFooter ?? null,
    });
  },

  /** Walks the `.sicrodoc` and produces a DOCX in `exports/docx/`.
   *  `fieldValues` = mapa {campo: valor} já resolvido pelo front (metadata +
   *  occurrence + catálogo), pra trocar as pílulas `{numero_laudo}` etc. pelo
   *  valor real no .docx. page/pages ficam vazios (viram campo nativo do Word). */
  exportLaudoDocx(
    workspacePath: string,
    laudoId: string,
    fieldValues?: Record<string, string>,
  ): Promise<Export> {
    return safeInvoke<Export>("export_laudo_docx", {
      workspacePath,
      laudoId,
      fieldValues: fieldValues ?? null,
    });
  },

  /** Exporta PDF via LibreOffice (DOCX→PDF headless): diagramação estilo Word
   * (numeração no lugar/tabela/cabeçalho repetindo). Requer LibreOffice.
   * `fieldValues`: ver exportLaudoDocx (mesmo mapa resolvido pelo front). */
  exportLaudoPdfLibreoffice(
    workspacePath: string,
    laudoId: string,
    /** true → gera PDF/A (ISO 19005, arquivamento de longo prazo). */
    pdfA = false,
    fieldValues?: Record<string, string>,
  ): Promise<Export> {
    return safeInvoke<Export>("export_laudo_pdf_libreoffice", {
      workspacePath,
      laudoId,
      pdfA,
      fieldValues: fieldValues ?? null,
    });
  },

  /** Lists every export already produced for a given laudo (newest first). */
  listLaudoExports(
    workspacePath: string,
    laudoId: string,
  ): Promise<Export[]> {
    return safeInvoke<Export[]>("list_laudo_exports", {
      workspacePath,
      laudoId,
    });
  },

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

  /** Aggregated summary: occurrence + latest import + counts + stats. */
  getDossieSummary(workspacePath: string): Promise<DossieSummary> {
    return safeInvoke<DossieSummary>("get_dossie_summary", { workspacePath });
  },

  /** Same as listWorkspacePhotos — kept for symmetry with the rest of the dossier API. */
  listDossiePhotos(workspacePath: string): Promise<MediaAsset[]> {
    return safeInvoke<MediaAsset[]>("list_dossie_photos", { workspacePath });
  },

  listDossieChecklist(workspacePath: string): Promise<ChecklistItem[]> {
    return safeInvoke<ChecklistItem[]>("list_dossie_checklist", {
      workspacePath,
    });
  },

  listDossieEntities(workspacePath: string): Promise<Entity[]> {
    return safeInvoke<Entity[]>("list_dossie_entities", { workspacePath });
  },

  listDossieTraces(workspacePath: string): Promise<Trace[]> {
    return safeInvoke<Trace[]>("list_dossie_traces", { workspacePath });
  },

  listDossieMeasurements(workspacePath: string): Promise<Measurement[]> {
    return safeInvoke<Measurement[]>("list_dossie_measurements", {
      workspacePath,
    });
  },

  listDossieNotes(workspacePath: string): Promise<FieldNote[]> {
    return safeInvoke<FieldNote[]>("list_dossie_notes", { workspacePath });
  },

  listDossieTimeline(workspacePath: string): Promise<TimelineEvent[]> {
    return safeInvoke<TimelineEvent[]>("list_dossie_timeline", {
      workspacePath,
    });
  },

  /**
   * Re-extract every dossier table from the staged
   * `imports/<id>/original_package.sicroapp`. Used by the "Recarregar
   * dados do pacote" button on the Import tab.
   */
  rehydrateDossie(workspacePath: string): Promise<RehydrateOutcome> {
    return safeInvoke<RehydrateOutcome>("rehydrate_dossie", { workspacePath });
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

  /**
   * Grava uma linha em `evidence_links` quando o perito insere uma
   * evidência no laudo. Os atributos completos continuam nos próprios
   * nodes do `.sicrodoc`; esta tabela é índice / audit log.
   */
  recordEvidenceLink(
    workspacePath: string,
    input: RecordEvidenceLinkInput,
  ): Promise<EvidenceLink> {
    return safeInvoke<EvidenceLink>("record_evidence_link", {
      workspacePath,
      input,
    });
  },

  /**
   * Lê bytes de um asset de evidência e retorna base64. Usado pelo
   * renderer para inlinear data URIs no HTML/PDF (não funciona com
   * convertFileSrc dentro de iframe srcdoc / headless Edge).
   */
  readEvidenceAsset(
    workspacePath: string,
    relativePath: string,
  ): Promise<EvidenceAsset> {
    return safeInvoke<EvidenceAsset>("read_evidence_asset", {
      workspacePath,
      relativePath,
    });
  },

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

  /**
   * O — Drag & drop de fotos no editor de laudo. Recebe um lote de
   * paths de arquivo (vindos do `onDragDropEvent` do Tauri). Para cada
   * foto: copia pra `<workspace>/laudos/<id>/evidencias/photos/`,
   * calcula SHA-256, lê dimensões + EXIF, escreve sidecar JSON, e
   * retorna metadata pra inserir no doc.
   *
   * O command NUNCA aborta o lote: itens inválidos vão no array
   * `errors` com a razão; itens válidos vão no `imported`.
   */
  importDraggedPhotosToLaudo(
    workspacePath: string,
    laudoId: string,
    filePaths: string[],
  ): Promise<PhotoImportResult> {
    return safeInvoke<PhotoImportResult>("import_dragged_photos_to_laudo", {
      input: {
        workspace_path: workspacePath,
        laudo_id: laudoId,
        file_paths: filePaths,
      },
    });
  },

  /**
   * T — Paste (Ctrl+V) de fotos no editor de laudo. Mesma fundação do
   * drag&drop, mas as fotos vêm como bytes (base64) em vez de paths.
   * Cobre dois casos do clipboard:
   *   1. Bitmap raw (screenshot do Windows, "copy image" do browser).
   *   2. Arquivo copiado do Explorer entregue como `File` pelo
   *      `DataTransfer.files` no evento `paste`.
   *
   * O command NUNCA aborta o lote: bytes inválidos vão no array `errors`.
   */
  importPastedPhotosToLaudo(
    workspacePath: string,
    laudoId: string,
    photos: { bytes_base64: string; filename: string }[],
  ): Promise<PhotoImportResult> {
    return safeInvoke<PhotoImportResult>("import_pasted_photos_to_laudo", {
      input: {
        workspace_path: workspacePath,
        laudo_id: laudoId,
        photos,
      },
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

  /** Importa um documento (PDF/imagem) preservando o original (cópia + hash). */
  importDocument(
    workspacePath: string,
    filePath: string,
    docType?: string,
    title?: string,
  ): Promise<DocumentCaseFile> {
    return safeInvoke<DocumentCaseFile>("import_document", {
      workspacePath,
      filePath,
      docType,
      title,
    });
  },
  listDocuments(workspacePath: string): Promise<DocumentCaseFile[]> {
    return safeInvoke<DocumentCaseFile[]>("list_documents", { workspacePath });
  },
  getDocument(workspacePath: string, documentId: string): Promise<DocumentCaseFile> {
    return safeInvoke<DocumentCaseFile>("get_document", { workspacePath, documentId });
  },
  deleteDocument(workspacePath: string, documentId: string): Promise<void> {
    return safeInvoke<void>("delete_document", { workspacePath, documentId });
  },
  updateDocumentMeta(
    workspacePath: string,
    documentId: string,
    title: string,
    docType: string,
    notes: string,
  ): Promise<DocumentCaseFile> {
    return safeInvoke<DocumentCaseFile>("update_document_meta", {
      workspacePath,
      documentId,
      title,
      docType,
      notes,
    });
  },
  setDocumentPageinfo(
    workspacePath: string,
    documentId: string,
    pageCount: number,
    hasTextLayer: boolean,
    metadataJson: string,
  ): Promise<DocumentCaseFile> {
    return safeInvoke<DocumentCaseFile>("set_document_pageinfo", {
      workspacePath,
      documentId,
      pageCount,
      hasTextLayer,
      metadataJson,
    });
  },
  saveOcrRun(
    workspacePath: string,
    documentId: string,
    run: OcrRunInput,
  ): Promise<OcrRunResult> {
    return safeInvoke<OcrRunResult>("save_ocr_run", { workspacePath, documentId, run });
  },
  /** Roda o motor de OCR sobre um documento **imagem** (RapidOCR/PP-OCRv5 se o
   * pacote de modelos foi baixado; senão, rascunho mock rotulado). PDFs usam
   * `runOcrPageImage` (página rasterizada pelo pdf.js). */
  runOcr(
    workspacePath: string,
    documentId: string,
    pageNumber?: number,
    language?: string,
  ): Promise<OcrRunResult> {
    return safeInvoke<OcrRunResult>("run_ocr", {
      workspacePath,
      documentId,
      pageNumber,
      language,
    });
  },
  /** Roda o OCR sobre uma página de PDF **já rasterizada no frontend** (pdf.js):
   * envia o PNG em base64 (sem o prefixo `data:`); o backend grava um temporário,
   * roda o RapidOCR e persiste a execução (registra `source = pdf_raster`). */
  runOcrPageImage(
    workspacePath: string,
    documentId: string,
    pageNumber: number,
    imageBase64: string,
    language?: string,
  ): Promise<OcrRunResult> {
    return safeInvoke<OcrRunResult>("run_ocr_page_image", {
      workspacePath,
      documentId,
      pageNumber,
      imageBase64,
      language,
    });
  },
  listOcrRuns(workspacePath: string, documentId: string): Promise<OcrRun[]> {
    return safeInvoke<OcrRun[]>("list_ocr_runs", { workspacePath, documentId });
  },
  getRunBlocks(workspacePath: string, runId: string): Promise<OcrTextBlock[]> {
    return safeInvoke<OcrTextBlock[]>("get_run_blocks", { workspacePath, runId });
  },
  reviewTextBlock(
    workspacePath: string,
    blockId: string,
    correctedText: string | null,
    reviewed: boolean,
  ): Promise<void> {
    return safeInvoke<void>("review_text_block", {
      workspacePath,
      blockId,
      correctedText,
      reviewed,
    });
  },
  /** Cria um bloco de texto MANUAL (perito) onde o OCR não detectou nada.
   * `bbox` normalizado 0..1. Entra na execução mais recente. */
  addManualBlock(
    workspacePath: string,
    documentId: string,
    pageNumber: number,
    text: string,
    bbox: { x: number; y: number; w: number; h: number },
  ): Promise<OcrTextBlock> {
    return safeInvoke<OcrTextBlock>("add_manual_block", {
      workspacePath,
      documentId,
      pageNumber,
      text,
      bboxX: bbox.x,
      bboxY: bbox.y,
      bboxW: bbox.w,
      bboxH: bbox.h,
    });
  },
  deleteTextBlock(workspacePath: string, blockId: string): Promise<void> {
    return safeInvoke<void>("delete_text_block", { workspacePath, blockId });
  },
  /** Atualiza posição/tamanho (bbox 0..1) de um bloco (move/redimensiona). */
  setBlockBbox(
    workspacePath: string,
    blockId: string,
    bbox: { x: number; y: number; w: number; h: number },
  ): Promise<void> {
    return safeInvoke<void>("set_block_bbox", {
      workspacePath,
      blockId,
      bboxX: bbox.x,
      bboxY: bbox.y,
      bboxW: bbox.w,
      bboxH: bbox.h,
    });
  },
  /** Gera um PDF pesquisável (imagem + camada de texto invisível) a partir das
   * páginas fornecidas (imagem em base64/data-URL + blocos posicionados).
   * Retorna o caminho relativo do PDF no workspace. */
  exportSearchablePdf(
    workspacePath: string,
    documentId: string,
    pages: {
      image_base64: string;
      width: number;
      height: number;
      blocks: {
        text: string;
        bbox_x: number;
        bbox_y: number;
        bbox_w: number;
        bbox_h: number;
      }[];
    }[],
  ): Promise<string> {
    return safeInvoke<string>("export_searchable_pdf", {
      workspacePath,
      documentId,
      pages,
    });
  },
  saveFields(
    workspacePath: string,
    documentId: string,
    fields: FieldInput[],
    replaceSource?: string,
  ): Promise<DetectedField[]> {
    return safeInvoke<DetectedField[]>("save_fields", {
      workspacePath,
      documentId,
      fields,
      replaceSource,
    });
  },
  listFields(workspacePath: string, documentId: string): Promise<DetectedField[]> {
    return safeInvoke<DetectedField[]>("list_fields", { workspacePath, documentId });
  },
  reviewField(
    workspacePath: string,
    fieldId: string,
    correctedValue: string | null,
    reviewed: boolean,
  ): Promise<void> {
    return safeInvoke<void>("review_field", {
      workspacePath,
      fieldId,
      correctedValue,
      reviewed,
    });
  },
  saveRegion(
    workspacePath: string,
    documentId: string,
    region: RegionInput,
  ): Promise<DocumentRegion> {
    return safeInvoke<DocumentRegion>("save_region", {
      workspacePath,
      documentId,
      region,
    });
  },
  listRegions(workspacePath: string, documentId: string): Promise<DocumentRegion[]> {
    return safeInvoke<DocumentRegion[]>("list_regions", { workspacePath, documentId });
  },
  deleteDocRegion(workspacePath: string, regionId: string): Promise<void> {
    return safeInvoke<void>("delete_region", { workspacePath, regionId });
  },
  /** Fase 4 — aplica pré-processamento (ids: cinza/endireitar/clahe/niveis/
   * otsu/inverter) e devolve a imagem processada em base64 PNG (sem prefixo). */
  preprocessImage(imageBase64: string, ops: string[]): Promise<string> {
    return safeInvoke<string>("preprocess_image", { imageBase64, ops });
  },
  /** Fase 5 (Bloco B) — ELA (Error Level Analysis). Heatmap PNG base64 (sem
   * prefixo). Indício, não conclusão (§13). */
  docEla(imageBase64: string, quality?: number, gain?: number): Promise<string> {
    return safeInvoke<string>("doc_ela", { imageBase64, quality, gain });
  },
  /** Fase 5 (Bloco B) — mapa de ruído (energia local de alta frequência).
   * Heatmap PNG base64 (sem prefixo). Indício, não conclusão (§13). */
  docNoiseMap(imageBase64: string, window?: number): Promise<string> {
    return safeInvoke<string>("doc_noise_map", { imageBase64, window });
  },
  /** Fase 5 (Bloco B) — copy-move (regiões clonadas na mesma imagem). Heatmap
   * PNG base64. Indício, não conclusão (§13). */
  docCopyMove(imageBase64: string, block?: number, step?: number): Promise<string> {
    return safeInvoke<string>("doc_copy_move", { imageBase64, block, step });
  },
  /** Fase 5 (Bloco B) — salva um heatmap de indício na bandeja do workspace
   * (documentoscopia/indicios). Retorna o caminho relativo. */
  saveDocIndicio(
    workspacePath: string,
    pngBase64: string,
    fileName: string,
  ): Promise<string> {
    return safeInvoke<string>("save_doc_indicio", {
      workspacePath,
      pngBase64,
      fileName,
    });
  },
  /** Fase 5 (Bloco B) — lista os indícios salvos (para a aba Evidências do laudo). */
  listDocIndicios(
    workspacePath: string,
  ): Promise<{ relative_path: string; file_name: string; created_at: string }[]> {
    return safeInvoke("list_doc_indicios", { workspacePath });
  },
  /** Fase 6 — gera o relatório técnico do documento (anexo do laudo): proveniência,
   * OCR, campos, regiões/códigos e histórico, em linguagem indiciária (§13).
   * Devolve os caminhos relativos do HTML e do PDF (PDF é best-effort). */
  generateDocReport(
    workspacePath: string,
    documentId: string,
  ): Promise<{ html_relative_path: string; pdf_relative_path: string | null }> {
    return safeInvoke("generate_doc_report", { workspacePath, documentId });
  },
  /** Fase 5 (Bloco B) — extrai o JPEG embutido (DCTDecode) de uma página de PDF
   * escaneado, em base64. `null` se a página não tiver imagem JPEG. */
  extractPdfJpeg(
    workspacePath: string,
    relativePath: string,
    page: number,
  ): Promise<string | null> {
    return safeInvoke<string | null>("extract_pdf_jpeg", {
      workspacePath,
      relativePath,
      page,
    });
  },
  /** Fase 5 (Bloco B) — gera uma amostra-teste de ELA (controle positivo) no
   * workspace; devolve o caminho absoluto do .jpg para importar. */
  generateElaTestSample(workspacePath: string): Promise<string> {
    return safeInvoke<string>("generate_ela_test_sample", { workspacePath });
  },
  /** Fase 4 — corrige perspectiva a partir de 4 cantos normalizados (0..1),
   * em ordem horária a partir do superior-esquerdo. Devolve base64 PNG. */
  perspectiveImage(
    imageBase64: string,
    points: [number, number][],
  ): Promise<string> {
    return safeInvoke<string>("perspective_image", { imageBase64, points });
  },
  /** Fase 3 — detecta QR/código de barras (decodificados) + candidato a tabela
   * na página e persiste como regiões. */
  detectLayout(
    workspacePath: string,
    documentId: string,
    pageNumber: number,
    imageBase64: string,
  ): Promise<DocumentRegion[]> {
    return safeInvoke<DocumentRegion[]>("detect_layout", {
      workspacePath,
      documentId,
      pageNumber,
      imageBase64,
    });
  },
  /** Fase 3 — tenta decodificar QR/código de barras dentro de uma região
   * (recorte isolado + ampliado). Devolve {region_type,label} ou null. */
  decodeRegion(
    imageBase64: string,
    bbox: { x: number; y: number; w: number; h: number },
  ): Promise<{ region_type: string; label: string } | null> {
    return safeInvoke<{ region_type: string; label: string } | null>(
      "decode_region",
      {
        imageBase64,
        bboxX: bbox.x,
        bboxY: bbox.y,
        bboxW: bbox.w,
        bboxH: bbox.h,
      },
    );
  },
  saveComparison(
    workspacePath: string,
    questionedDocumentId: string,
    referenceDocumentId: string,
    comparisonType: string,
    resultsJson: string,
    summary: string,
  ): Promise<ComparisonSession> {
    return safeInvoke<ComparisonSession>("save_comparison", {
      workspacePath,
      questionedDocumentId,
      referenceDocumentId,
      comparisonType,
      resultsJson,
      summary,
    });
  },
  listComparisons(workspacePath: string): Promise<ComparisonSession[]> {
    return safeInvoke<ComparisonSession[]>("list_comparisons", { workspacePath });
  },
  /** Salva o PNG composto de um confronto no workspace; retorna o caminho relativo. */
  saveConfrontoImage(
    workspacePath: string,
    pngBase64: string,
    fileName: string,
  ): Promise<string> {
    return safeInvoke<string>("save_confronto_image", {
      workspacePath,
      pngBase64,
      fileName,
    });
  },
  listDocumentLog(workspacePath: string, documentId: string): Promise<DocumentLog[]> {
    return safeInvoke<DocumentLog[]>("list_document_log", { workspacePath, documentId });
  },

  // ----- Gerenciador de OCR (Documentoscopia — Tesseract + idiomas) -----
  getOcrCatalog(): Promise<OcrCatalog> {
    return safeInvoke<OcrCatalog>("get_ocr_catalog", {});
  },
  getOcrStatus(): Promise<OcrStatus> {
    return safeInvoke<OcrStatus>("get_ocr_status", {});
  },
  installOcrAsset(assetId: string): Promise<OcrStatus> {
    return safeInvoke<OcrStatus>("install_ocr_asset", { assetId });
  },
  removeOcrAsset(assetId: string): Promise<OcrStatus> {
    return safeInvoke<OcrStatus>("remove_ocr_asset", { assetId });
  },
  /** OPT-IN: compara o pacote de modelos instalado com a release mais nova do oar-ocr. */
  checkOcrUpdates(): Promise<OcrUpdateInfo> {
    return safeInvoke<OcrUpdateInfo>("check_ocr_updates", {});
  },
  /** OPT-IN: baixa o pacote de modelos da release mais nova (troca a versão na URL). */
  updateOcrModels(): Promise<OcrStatus> {
    return safeInvoke<OcrStatus>("update_ocr_models", {});
  },

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
  ): Promise<AudioMedia> {
    return safeInvoke<AudioMedia>("enhance_audio", {
      workspacePath,
      sourceAudioId,
      filters,
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

  /** Status do LibreOffice (instalado? versão? + metadados de download). */
  getLibreofficeStatus(): Promise<LibreOfficeStatus> {
    return safeInvoke<LibreOfficeStatus>("get_libreoffice_status", {});
  },
  /** Baixa o instalador oficial (.msi) com progresso via evento
   * "libreoffice-download-progress" (cache temporário, fora do backup) e abre-o. */
  downloadLibreofficeInstaller(): Promise<void> {
    return safeInvoke<void>("download_libreoffice_installer", {});
  },
} as const;

export type { SicroError };
