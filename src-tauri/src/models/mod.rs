//! Serializable domain models that cross the Tauri boundary.
//!
//! These structs mirror `src/types/*.ts` on the front-end. Keep field names
//! in snake_case — serde defaults match the TypeScript wire format.

pub mod audio;
pub mod croqui;
pub mod documentoscopia;
pub mod dossie;
pub mod evidence;
pub mod export;
pub mod image_analysis;
pub mod import;
pub mod laudo;
pub mod occurrence;
pub mod registry;
pub mod video;
pub mod video_distance;
pub mod video_speed;

pub use audio::{
    AudioEnhancement, AudioMarker, AudioMedia, AudioTranscriptSegment, TranscriptSegmentInput,
};
pub use croqui::{Croqui, CroquiDoc, CroquiStatus, ExportCroquiPngInput, NewCroquiInput};
pub use documentoscopia::{
    ComparisonSession, DetectedField, DocumentAnalysis, DocumentCaseFile, DocumentLog,
    DocumentPage, DocumentRegion, OcrRun, OcrTextBlock,
};
pub use evidence::{
    EvidenceAsset, EvidenceLink, EvidenceSourceKind, RecordEvidenceLinkInput,
};
pub use dossie::{
    ChecklistItem, ChecklistSummary, DossieCounts, DossieSummary, Entity, FieldNote, Measurement,
    OccurrenceStats, RehydrateOutcome, TimelineEvent, Trace,
};
pub use export::{Export, ExportKind};
pub use import::{
    EvidenceItem, HashMismatch, Import, ImportReport, ImportResult, ImportSicroappInput,
    ImportStatus, MediaAsset, MediaAssetType,
};
pub use image_analysis::{
    BackendAdjustments, BackendOperation, CreateImageAnalysisInput, ExportImageInput,
    HashSet as ImageHashSet, ImageAnalysis, ImageAssetBytes, ImageExport, ImageHistogram,
    ImageMetadata, ImageOperationLog, ImageSourceKind, ImportLocalImageInput, HistogramStats,
    MaskSpec,
};
pub use laudo::{Laudo, LaudoDoc, LaudoStatus, NewLaudoInput};
pub use registry::{
    BrokenLaudoLink, EvidenceKind, EvidenceRegistryItem, IntegrityReportArtifact,
    IntegrityStatus, RegistrySummary, VerifyOptions, WorkspaceIntegrityReport,
};
pub use occurrence::{
    LoadedOccurrence, NewOccurrenceInput, Occurrence, OccurrenceEdit, OccurrenceStatus,
    RecentOccurrence,
};
pub use video::{
    CollectFrameInput, CollectFrameResult, CreateVideoEventInput, RegisterVideoInput,
    UpdateStoryboardFrameInput, UpdateVideoEventInput, VideoBundle, VideoEvent, VideoExport,
    VideoMedia, VideoOperationLog, VideoStoryboardFrame,
};
pub use video_distance::{
    CreateDistanceMeasurementInput, McSigmasDistance, VideoDistanceMeasurement,
};
pub use video_speed::{
    ComputeSpeedInput, ControlPoint, CreateSpeedCalibrationInput, McSigmas, TrajectoryPoint,
    VideoSpeedCalculation, VideoSpeedCalibration,
};
