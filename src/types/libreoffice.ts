/**
 * Espelho TS do gerenciador de dependência LibreOffice
 * (Rust: `commands/libreoffice_commands.rs`). Snake_case porque o backend
 * serializa os campos como estão.
 */

export interface LibreOfficeStatus {
  installed: boolean;
  soffice_path: string;
  /** Linha de versão reportada pelo soffice (pode vir vazia). */
  version: string;
  /** Versão que o botão "Baixar/Atualizar" instalaria. */
  download_version: string;
  approx_mb: number;
  download_url: string;
  site_url: string;
}

/** Payload do evento `libreoffice-download-progress`. */
export interface LibreOfficeProgress {
  id: string;
  received: number;
  total: number;
}
