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
  /** Sigla do estado do município de atuação. */
  uf: string;
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
  // CONTRATO: acréscimo do P2.1 (§5.6, contratos-fase2 (f)).
  /** ffmpeg escolhido nas Configurações (vazio = automático); `SICRO_FFMPEG` definido vence sempre. */
  ffmpeg_path: string;
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
  /** Pessoas no vídeo: biblioteca do onnxruntime e modelo YOLOX. */
  ort_lib_path: string;
  pessoas_model_path: string;
  pessoas_version: string;
  /** Super-resolução (Real-ESRGAN): modelo, onnxruntime próprio (vazio = o das Pessoas), versão. */
  sr_model_path: string;
  sr_ort_lib_path: string;
  sr_version: string;
  // CONTRATO: abertura da Fase 2 (contratos-fase2.md (b)); dono P2.1.
  /** Interruptor "Usar modelos de IA" desligado (padrão: ligado). */
  modelos_desligados: boolean;
  /** Aceleração por GPU: pasta `ai/gpu` (vazio = não instalada). */
  gpu_dir: string;
  /** Rastreador EdgeTAM: pasta `ai/rastreio` (vazio = não instalado) e versão. */
  rastreio_dir: string;
  rastreio_version: string;
  // CONTRATO: acréscimo (P4.5). Item "Ocultação": pasta `ai/redacao` (vazio = não instalado) e versão.
  redacao_dir: string;
  redacao_version: string;
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
      uf: "",
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
    paths: { default_workspace_dir: "", default_export_dir: "", ffmpeg_path: "" },
    ai: {
      whisper_bin_path: "",
      model_path: "",
      vad_model_path: "",
      whisper_version: "",
      diar_bin_path: "",
      diar_segmentation_path: "",
      diar_embedding_path: "",
      diar_version: "",
      ort_lib_path: "",
      pessoas_model_path: "",
      pessoas_version: "",
      sr_model_path: "",
      sr_ort_lib_path: "",
      sr_version: "",
      modelos_desligados: false,
      gpu_dir: "",
      rastreio_dir: "",
      rastreio_version: "",
      redacao_dir: "",
      redacao_version: "",
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
