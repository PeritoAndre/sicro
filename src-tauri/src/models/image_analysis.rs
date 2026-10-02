//! Modelos do Editor de Imagem Pericial (tabelas da migration 009).
//! Espelhados em `src/types/image_analysis.ts` — mudar nos dois.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// ImageSourceKind

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImageSourceKind {
    Photo,
    VideoFrame,
    Evidence,
    LocalImport,
}

impl ImageSourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Photo => "photo",
            Self::VideoFrame => "video_frame",
            Self::Evidence => "evidence",
            Self::LocalImport => "local_import",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "photo" => Some(Self::Photo),
            "video_frame" => Some(Self::VideoFrame),
            "evidence" => Some(Self::Evidence),
            "local_import" => Some(Self::LocalImport),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// ImageAnalysis (row em `image_analyses`)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageAnalysis {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub title: String,
    pub source_kind: ImageSourceKind,
    pub source_id: Option<String>,
    pub original_relative_path: String,
    pub original_hash_sha256: Option<String>,
    pub analysis_relative_path: String,
    pub last_export_relative_path: Option<String>,
    pub status: String,
    /// JSON livre preservado como veio (dimensões, mime, EXIF resumido…).
    pub metadata_json: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// ImageExport (row em `image_exports`)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageExport {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub image_analysis_id: Uuid,
    pub output_relative_path: String,
    pub sidecar_relative_path: Option<String>,
    pub hash_sha256: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub format: String,
    pub created_at: DateTime<Utc>,
    pub operation_summary_json: String,
}

// ---------------------------------------------------------------------------
// ImageOperationLog (row em `image_operation_logs`)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageOperationLog {
    pub id: i64,
    pub occurrence_id: Uuid,
    pub image_analysis_id: Uuid,
    pub action: String,
    pub details_json: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Inputs dos comandos Tauri

#[derive(Debug, Clone, Deserialize)]
pub struct CreateImageAnalysisInput {
    pub title: String,
    pub source_kind: ImageSourceKind,
    pub source_id: Option<String>,
    /// Relativo ao workspace; validado com `sanitize_relative_path`.
    pub original_relative_path: String,
    /// Ausente, o backend calcula a partir do arquivo.
    pub original_hash_sha256: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImportLocalImageInput {
    /// Caminho absoluto (diálogo do SO); copiado para `imagens/originais/`.
    pub source_path: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ExportImageInput {
    /// `true`: o Rust reaplica os ajustes; `false`: exporta o PNG já composto pelo front.
    #[serde(default)]
    pub apply_backend_adjustments: bool,
    /// PNG composto pelo front (base64); quando presente, são os bytes finais.
    #[serde(default)]
    pub composed_png_base64: Option<String>,
    /// Reaplicados só no derivado; o front é a fonte de verdade da sessão.
    #[serde(default)]
    pub adjustments: Option<BackendAdjustments>,
    /// Operações geométricas, na ordem.
    #[serde(default)]
    pub operations: Vec<BackendOperation>,
    /// "png" (padrão) | "jpg".
    #[serde(default)]
    pub format: Option<String>,
    /// JSON livre — vai para o sidecar como "summary".
    #[serde(default)]
    pub operation_summary_json: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackendAdjustments {
    /// [-100, 100]; 0 = neutro.
    #[serde(default)]
    pub brightness: f32,
    /// [-100, 100]; 0 = neutro.
    #[serde(default)]
    pub contrast: f32,
    /// 1.0 = neutro; aplicado como `pixel ^ (1/gamma)`.
    #[serde(default = "default_gamma")]
    pub gamma: f32,
    /// [-100, 100]; 0 = neutro.
    #[serde(default)]
    pub saturation: f32,
    #[serde(default)]
    pub grayscale: bool,
    #[serde(default)]
    pub invert: bool,
    /// Matiz em graus; mesma matriz hueRotate (SVG/CSS) do preview no front.
    #[serde(default)]
    pub hue: f32,
    /// Visibilidade de canal (estilo GIMP): false zera o canal na saída.
    #[serde(default = "default_true")]
    pub channel_r: bool,
    #[serde(default = "default_true")]
    pub channel_g: bool,
    #[serde(default = "default_true")]
    pub channel_b: bool,
}

impl Default for BackendAdjustments {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 0.0,
            gamma: 1.0,
            saturation: 0.0,
            grayscale: false,
            invert: false,
            hue: 0.0,
            channel_r: true,
            channel_g: true,
            channel_b: true,
        }
    }
}

fn default_gamma() -> f32 {
    1.0
}

/// Máscara de seleção em coordenadas normalizadas `[0,1]` da imagem corrente:
/// a mesma máscara serve no preview reduzido e no export em resolução cheia.
/// `inverted` troca dentro/fora.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum MaskSpec {
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        #[serde(default)]
        inverted: bool,
    },
    Ellipse {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        #[serde(default)]
        inverted: bool,
    },
    Polygon {
        #[serde(default)]
        points: Vec<[f32; 2]>,
        #[serde(default)]
        inverted: bool,
    },
}

impl MaskSpec {
    pub fn inverted(&self) -> bool {
        match self {
            MaskSpec::Rect { inverted, .. } => *inverted,
            MaskSpec::Ellipse { inverted, .. } => *inverted,
            MaskSpec::Polygon { inverted, .. } => *inverted,
        }
    }

    /// O ponto normalizado está dentro da forma base (antes de `inverted`)?
    pub fn contains_base(&self, nx: f32, ny: f32) -> bool {
        match self {
            MaskSpec::Rect {
                x,
                y,
                width,
                height,
                ..
            } => nx >= *x && nx <= *x + *width && ny >= *y && ny <= *y + *height,
            MaskSpec::Ellipse {
                x,
                y,
                width,
                height,
                ..
            } => {
                let rx = *width / 2.0;
                let ry = *height / 2.0;
                if rx <= 0.0 || ry <= 0.0 {
                    return false;
                }
                let cx = *x + rx;
                let cy = *y + ry;
                let dx = (nx - cx) / rx;
                let dy = (ny - cy) / ry;
                dx * dx + dy * dy <= 1.0
            }
            MaskSpec::Polygon { points, .. } => {
                crate::image_editor::mask::point_in_polygon(nx, ny, points)
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BackendOperation {
    // ---------- Geométricas
    // `rename_all = "snake_case"` não trata dígito como fronteira (`rotate90_cw`);
    // o front usa `rotate_90_cw`, então a tag é fixada à mão.
    #[serde(rename = "rotate_90_cw")]
    Rotate90Cw,
    #[serde(rename = "rotate_90_ccw")]
    Rotate90Ccw,
    #[serde(rename = "rotate_180")]
    Rotate180,
    FlipHorizontal,
    FlipVertical,
    Crop {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    Resize {
        width: u32,
        height: u32,
    },

    // ---------- Detecção de bordas
    EdgeSobel {
        /// [0.0, 4.0]; 1.0 = neutro.
        #[serde(default = "default_one")]
        strength: f32,
    },
    /// Kernel 5x5.
    EdgeLaplacian {
        #[serde(default = "default_one")]
        strength: f32,
    },
    /// Binário.
    EdgeCanny {
        #[serde(default = "default_low_threshold")]
        low_threshold: f32,
        #[serde(default = "default_high_threshold")]
        high_threshold: f32,
    },

    // ---------- Blur / denoise
    /// Raio = ceil(3*sigma).
    BlurGaussian {
        sigma: f32,
    },
    BlurMedian {
        radius: u32,
    },
    /// Simplificado; custoso.
    BlurBilateral {
        sigma_space: f32,
        sigma_color: f32,
    },

    // ---------- Realce
    /// Equalização adaptativa por blocos. `bins` = "histogram bins" do Fiji.
    Clahe {
        #[serde(default = "default_tile_size")]
        tile_size: u32,
        #[serde(default = "default_clip_limit")]
        clip_limit: f32,
        #[serde(default = "default_clahe_bins")]
        bins: u32,
    },
    /// Rolling ball (Sternberg 1983, paridade ImageJ). `light_background`:
    /// objeto escuro em fundo claro; `disable_smoothing` pula a suavização 3×3.
    SubtractBackground {
        #[serde(default = "default_rolling_radius")]
        radius: f32,
        #[serde(default)]
        light_background: bool,
        #[serde(default)]
        disable_smoothing: bool,
    },
    /// Global, na luminância.
    HistogramEqualize,
    /// Estica o histograma por canal para [percentile_low, percentile_high].
    AutoLevels {
        #[serde(default = "default_percentile_low")]
        percentile_low: f32,
        #[serde(default = "default_percentile_high")]
        percentile_high: f32,
    },
    WhiteBalanceGrayWorld,

    // ---------- Morfologia (em luminância)
    /// Kernel quadrado 3x3 ou 5x5.
    Dilate {
        #[serde(default = "default_radius")]
        radius: u32,
    },
    Erode {
        #[serde(default = "default_radius")]
        radius: u32,
    },
    /// erode → dilate.
    Open {
        #[serde(default = "default_radius")]
        radius: u32,
    },
    /// dilate → erode.
    Close {
        #[serde(default = "default_radius")]
        radius: u32,
    },

    // ---------- Perspectiva
    /// 4 cantos origem → 4 cantos destino, em pixels da imagem original.
    Perspective {
        src: [[f32; 2]; 4],
        dst: [[f32; 2]; 4],
        output_width: u32,
        output_height: u32,
    },

    // ---------- Extras
    UnsharpMask {
        sigma: f32,
        #[serde(default = "default_one")]
        amount: f32,
    },
    Threshold {
        value: u8,
    },
    /// Anonimização de uma região.
    Pixelize {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        block_size: u32,
    },

    // ---------- Tonais
    /// Remapeia [in_black,in_white]→[out_black,out_white] + gama.
    /// `channel`: "rgb" | "r" | "g" | "b".
    Levels {
        #[serde(default = "default_rgb_channel")]
        channel: String,
        #[serde(default)]
        in_black: u8,
        #[serde(default = "default_255")]
        in_white: u8,
        #[serde(default = "default_gamma")]
        gamma: f32,
        #[serde(default)]
        out_black: u8,
        #[serde(default = "default_255")]
        out_white: u8,
    },
    /// LUT por interpolação linear entre pontos (x,y) em 0..255.
    Curves {
        #[serde(default = "default_rgb_channel")]
        channel: String,
        #[serde(default)]
        points: Vec<[f32; 2]>,
    },
    /// Reduz a `levels` níveis por canal (2..=255).
    Posterize {
        #[serde(default = "default_posterize_levels")]
        levels: u8,
    },

    // ---------- Canais / falsa-cor
    /// `channel`: r/g/b, luminance/luma, h/s/v, y/cb/cr, l_lab/a_lab/b_lab.
    ExtractChannel {
        #[serde(default = "default_luma_channel")]
        channel: String,
    },
    /// `colormap`: viridis/jet/ironbow/grayscale.
    FalseColor {
        #[serde(default = "default_colormap")]
        colormap: String,
    },

    // ---------- Forense / cor
    /// Error Level Analysis: recompressão JPEG em `quality` + diff×`scale`.
    Ela {
        #[serde(default = "default_ela_quality")]
        quality: u8,
        #[serde(default = "default_ela_scale")]
        scale: f32,
    },
    /// Banda de frequência na luminância.
    DifferenceOfGaussians {
        #[serde(default = "default_dog_sigma1")]
        sigma1: f32,
        #[serde(default = "default_dog_sigma2")]
        sigma2: f32,
        #[serde(default = "default_dog_gain")]
        gain: f32,
    },
    /// Magnitude = brilho, direção = matiz.
    LuminanceGradient {
        #[serde(default = "default_one")]
        strength: f32,
    },
    /// Estilo DStretch — amplifica diferenças de cor sutis.
    DecorrelationStretch {
        #[serde(default = "default_decorr_sigma")]
        target_sigma: f32,
        #[serde(default = "default_decorr_mean")]
        target_mean: f32,
    },

    // ---------- Geométrica / genérica
    /// Bilinear; `expand` cresce a tela.
    RotateArbitrary {
        #[serde(default)]
        degrees: f32,
        #[serde(default = "default_true")]
        expand: bool,
    },
    /// Kernel NxN editável + divisor + offset.
    Convolve {
        #[serde(default)]
        kernel: Vec<f32>,
        #[serde(default = "default_kernel_size")]
        size: u32,
        #[serde(default = "default_one")]
        divisor: f32,
        #[serde(default)]
        offset: f32,
    },

    // ---------- Operação confinada a uma seleção
    /// Aplica `op` só dentro de `mask`; fora, o pixel original fica (borda dura).
    /// A máscara vai congelada e íntegra para o sidecar — reproduzível depois.
    Masked {
        op: Box<BackendOperation>,
        mask: MaskSpec,
    },
}

fn default_one() -> f32 {
    1.0
}
fn default_low_threshold() -> f32 {
    50.0
}
fn default_high_threshold() -> f32 {
    150.0
}
fn default_tile_size() -> u32 {
    8
}
fn default_clip_limit() -> f32 {
    2.0
}
fn default_clahe_bins() -> u32 {
    256
}
fn default_rolling_radius() -> f32 {
    50.0
}
fn default_percentile_low() -> f32 {
    1.0
}
fn default_percentile_high() -> f32 {
    99.0
}
fn default_radius() -> u32 {
    1
}
fn default_255() -> u8 {
    255
}
fn default_true() -> bool {
    true
}
fn default_rgb_channel() -> String {
    "rgb".to_string()
}
fn default_luma_channel() -> String {
    "luma".to_string()
}
fn default_colormap() -> String {
    "viridis".to_string()
}
fn default_posterize_levels() -> u8 {
    4
}
fn default_ela_quality() -> u8 {
    90
}
fn default_ela_scale() -> f32 {
    15.0
}
fn default_dog_sigma1() -> f32 {
    1.0
}
fn default_dog_sigma2() -> f32 {
    3.0
}
fn default_dog_gain() -> f32 {
    5.0
}
fn default_decorr_sigma() -> f32 {
    50.0
}
fn default_decorr_mean() -> f32 {
    128.0
}
fn default_kernel_size() -> u32 {
    3
}

// ---------------------------------------------------------------------------
// Metadados (devolvidos por `get_image_metadata`)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMetadata {
    pub width: u32,
    pub height: u32,
    pub mime_type: Option<String>,
    pub format_label: Option<String>,
    pub size_bytes: u64,
    pub hash_sha256: Option<String>,
    /// JSON; `None` sem EXIF lido.
    pub exif_json: Option<String>,
    /// Só quando o caller pede `compute_hash=true`; `None` em metadados leves.
    #[serde(default)]
    pub hash_set: Option<HashSet>,
}

/// Hashes para cadeia de custódia. MD5 e SHA-1 estão comprometidos, mas ainda
/// são exigidos por convenção em laudos; SHA3-256 é o reforço de longo prazo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashSet {
    pub md5: String,
    pub sha1: String,
    pub sha256: String,
    pub sha3_256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageHistogram {
    /// 256 bins (0..255), um por canal.
    pub red: Vec<u32>,
    pub green: Vec<u32>,
    pub blue: Vec<u32>,
    /// 0.299R + 0.587G + 0.114B.
    pub luminance: Vec<u32>,
    pub stats: HistogramStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramStats {
    pub mean_r: f32,
    pub mean_g: f32,
    pub mean_b: f32,
    pub mean_lum: f32,
    pub stddev_r: f32,
    pub stddev_g: f32,
    pub stddev_b: f32,
    pub stddev_lum: f32,
    pub min_lum: u8,
    pub max_lum: u8,
    pub total_pixels: u32,
}

// ---------------------------------------------------------------------------
// Bytes de um asset (devolvido por `read_image_asset`)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageAssetBytes {
    pub relative_path: String,
    pub mime_type: String,
    pub base64: String,
    pub size_bytes: u64,
}
