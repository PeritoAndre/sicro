/** Espelha as configurações globais do app (Rust: `commands/settings_commands.rs`). */

export interface PeritoProfile {
  full_name: string;
  registration: string; // matrícula
  role: string; // cargo
  formation: string; // formação
  signature_image_path: string;
  /** Vira o avatar do app. */
  photo_path: string;
  /** Pré-preenche o município de novas ocorrências. */
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
  /** 1 = 100%. */
  ui_zoom: number;
}

export interface PathsSettings {
  default_workspace_dir: string;
  default_export_dir: string;
}

/** Caminhos da IA de transcrição instalada pelo gerenciador. */
export interface AiSettings {
  whisper_bin_path: string;
  model_path: string;
  vad_model_path: string;
  whisper_version: string;
  /** Separação de locutores (sherpa-onnx local). */
  diar_bin_path: string;
  diar_segmentation_path: string;
  diar_embedding_path: string;
  diar_version: string;
}

/** Motor de OCR (Tesseract) + idiomas instalados. */
export interface OcrSettings {
  engine_bin_path: string;
  engine_version: string;
  tessdata_dir: string;
  ocr_version: string;
}

/** Doc ProseMirror genérico, para não acoplar as configs globais a um editor. */
export type HeaderTemplateContent = {
  type: string;
  content?: unknown[];
  attrs?: Record<string, unknown>;
};

/** Cabeçalhos salvos pelo perito; o padrão institucional é definido em código, não aqui. */
export interface HeaderTemplate {
  id: string;
  name: string;
  content: HeaderTemplateContent;
  /** cm; ausente = 2.5. */
  header_height_cm?: number;
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
  header_templates: HeaderTemplate[];
}

/** Usado antes do backend responder; espelha o default do Rust. */
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
    appearance: { theme: "dark", accent: "#d7a84f", ui_zoom: 1 },
    paths: { default_workspace_dir: "", default_export_dir: "" },
    ai: {
      whisper_bin_path: "",
      model_path: "",
      vad_model_path: "",
      whisper_version: "",
      diar_bin_path: "",
      diar_segmentation_path: "",
      diar_embedding_path: "",
      diar_version: "",
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
