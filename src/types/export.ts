/**
 * Mirror of `src-tauri/src/models/export.rs`.
 */

export type ExportKind = "html" | "pdf" | "docx";

export interface Export {
  id: string;
  occurrence_id: string;
  laudo_id: string;
  kind: ExportKind;
  relative_path: string;
  file_size: number;
  created_at: string;
}
