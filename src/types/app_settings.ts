/**
 * Espelho TS das configurações GLOBAIS do app (o "cofrinho" que vive fora de
 * qualquer `.sicro`). Chaves em snake_case porque o backend Rust serializa os
 * nomes dos campos como estão (sem rename_all) — igual aos demais domínios.
 *
 * Backend: `src-tauri/src/commands/settings_commands.rs`.
 */

export interface PeritoProfile {
  full_name: string;
  registration: string; // matrícula
  role: string; // cargo
  formation: string; // formação
  signature_image_path: string;
  /** Foto do perito (caminho do arquivo) — exibida no avatar do app. */
  photo_path: string;
  /** Município de atuação/lotação — pré-preenche o município de novas ocorrências. */
  municipio_atuacao: string;
}

export interface InstitutionSettings {
  organization: string;
  unit: string;
  address: string;
  footer_text: string;
  brasao_left_path: string;
  brasao_right_path: string;
}

export type ThemeMode = "dark" | "light" | "auto";

export interface AppearanceSettings {
  theme: ThemeMode;
  accent: string; // hex (#rrggbb)
}

export interface PathsSettings {
  default_workspace_dir: string;
  default_export_dir: string;
}

/** Fase 2.1 — caminhos da IA de transcrição instalada pelo gerenciador. */
export interface AiSettings {
  whisper_bin_path: string;
  model_path: string;
  vad_model_path: string;
  whisper_version: string;
}

/** Documentoscopia — motor de OCR (Tesseract) + idiomas instalados. */
export interface OcrSettings {
  engine_bin_path: string;
  engine_version: string;
  tessdata_dir: string;
  ocr_version: string;
}

/**
 * Doc ProseMirror do cabeçalho (estrutura mínima — o módulo Laudo trata como
 * JSONContent). Genérico aqui pra não acoplar as configs globais ao TipTap.
 */
export type HeaderTemplateContent = {
  type: string;
  content?: unknown[];
  attrs?: Record<string, unknown>;
};

/**
 * Cabeçalho oficial salvo (criador de cabeçalho do Laudo), reutilizável entre
 * todos os laudos. O padrão institucional ("nosso") é definido em código e NÃO
 * fica aqui — esta lista guarda os que o perito salvar (outras unidades/órgãos).
 */
export interface HeaderTemplate {
  id: string;
  name: string;
  /** Conteúdo do cabeçalho (doc ProseMirror). */
  content: HeaderTemplateContent;
  /** Altura do cabeçalho em cm (default 2.5 quando ausente). */
  header_height_cm?: number;
  /** ISO-8601. */
  created_at: string;
}

export interface AppSettings {
  schema_version: string;
  profile: PeritoProfile;
  institution: InstitutionSettings;
  appearance: AppearanceSettings;
  paths: PathsSettings;
  ai: AiSettings;
  ocr: OcrSettings;
  /** Biblioteca de cabeçalhos oficiais salvos (global). */
  header_templates: HeaderTemplate[];
}

/** Default usado no front antes do backend responder (espelha o Rust). */
export function defaultAppSettings(): AppSettings {
  return {
    schema_version: "1",
    profile: {
      full_name: "",
      registration: "",
      role: "",
      formation: "",
      signature_image_path: "",
      photo_path: "",
      municipio_atuacao: "",
    },
    institution: {
      organization: "",
      unit: "",
      address: "",
      footer_text: "",
      brasao_left_path: "",
      brasao_right_path: "",
    },
    appearance: { theme: "dark", accent: "#d7a84f" },
    paths: { default_workspace_dir: "", default_export_dir: "" },
    ai: {
      whisper_bin_path: "",
      model_path: "",
      vad_model_path: "",
      whisper_version: "",
    },
    ocr: {
      engine_bin_path: "",
      engine_version: "",
      tessdata_dir: "",
      ocr_version: "",
    },
    header_templates: [],
  };
}
