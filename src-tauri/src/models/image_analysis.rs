//! Models for the Image Editor (MVP 7 — Editor de Imagem Pericial).
//!
//! Three persisted entities (tabelas em SQLite, migration 009):
//!   - `ImageAnalysis`        — uma sessão `.sicroimage`;
//!   - `ImageExport`          — uma imagem derivada + sidecar JSON;
//!   - `ImageOperationLog`    — log textual (audit) das operações.
//!
//! Plus one transient input/output:
//!   - `CreateImageAnalysisInput` (criação a partir de evidência ou
//!     arquivo local), `ExportImageInput` (parâmetros do export).
//!
//! Wire format é serde-default snake_case — os mirrors TypeScript em
//! `src/types/image_analysis.ts` mantêm os mesmos nomes.

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
    /// JSON object preserved verbatim — pode carregar dimensions,
    /// mime_type, EXIF resumido, etc.
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
// Inputs (Tauri commands)

#[derive(Debug, Clone, Deserialize)]
pub struct CreateImageAnalysisInput {
    pub title: String,
    pub source_kind: ImageSourceKind,
    pub source_id: Option<String>,
    /// Caminho relativo ao workspace. O backend valida com
    /// `sanitize_relative_path`.
    pub original_relative_path: String,
    /// Hash conhecido da imagem original (opcional). Quando ausente,
    /// o backend tenta computar a partir do arquivo no workspace.
    pub original_hash_sha256: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImportLocalImageInput {
    /// Caminho absoluto da imagem escolhida pelo usuário no diálogo do SO.
    /// O backend copia para `imagens/originais/` e cria a análise.
    pub source_path: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ExportImageInput {
    /// Quando `true` o pipeline tenta aplicar os ajustes no Rust
    /// (brilho/contraste/grayscale/inverter/rotação/flip/crop). Quando
    /// `false`, exporta o PNG já composto enviado pelo frontend (após
    /// aplicar anotações via Konva).
    #[serde(default)]
    pub apply_backend_adjustments: bool,
    /// PNG já composto pelo frontend (base64, opcional). Quando
    /// presente, o backend usa-o como bytes finais; o original e os
    /// ajustes ficam só na sessão.
    #[serde(default)]
    pub composed_png_base64: Option<String>,
    /// Ajustes que o backend deve aplicar (quando
    /// `apply_backend_adjustments=true`). O frontend é a fonte de
    /// verdade da sessão; o backend só re-aplica para o derivado.
    #[serde(default)]
    pub adjustments: Option<BackendAdjustments>,
    /// Lista de operações geométricas a aplicar (na ordem).
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
    /// Brilho em [-100, 100]. 0 = neutro.
    #[serde(default)]
    pub brightness: f32,
    /// Contraste em [-100, 100]. 0 = neutro.
    #[serde(default)]
    pub contrast: f32,
    /// Gamma. 1.0 = neutro. Aplicado como `pixel ^ (1/gamma)`.
    #[serde(default = "default_gamma")]
    pub gamma: f32,
    /// Saturação em [-100, 100]. 0 = neutro.
    #[serde(default)]
    pub saturation: f32,
    #[serde(default)]
    pub grayscale: bool,
    #[serde(default)]
    pub invert: bool,
    /// W14.2 — Matiz (graus). 0 = neutro. Matriz hueRotate do SVG/CSS
    /// (preservando luminância), idêntica ao preview no front.
    #[serde(default)]
    pub hue: f32,
    /// W14.2 — visibilidade de canal (GIMP-style). false zera o canal de
    /// saída. Default true (canal visível).
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

/// W20 (S2) — Geometria da máscara de seleção, em coordenadas **normalizadas**
/// `[0,1]` relativas à imagem corrente. Normalizar é o que permite rasterizar a
/// MESMA máscara tanto no preview reduzido quanto no export em resolução cheia
/// sem reescalar coordenadas (o backend multiplica por `width`/`height` atuais).
/// `inverted` troca dentro/fora (suporta "selecionar inverso").
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

    /// O ponto normalizado `(nx, ny)` está dentro da forma BASE (antes de
    /// aplicar `inverted`)?
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
    // ---------- Geometric (MVP 7 — original)
    // NB: `rename_all = "snake_case"` NÃO trata dígitos como fronteira de
    // palavra — `Rotate90Cw` viraria `rotate90_cw`, mas o front (catálogo de
    // filtros / processing_stack) usa `rotate_90_cw`. Sem o `rename` explícito
    // abaixo a desserialização do op falha e a rotação fixa "não funciona"
    // (enquanto flip, rotação livre etc. — sem dígitos — funcionam). Fixa a
    // tag exata nas duas direções.
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

    // ---------- G12.1 — Edge detection
    /// Detecção de bordas Sobel (gradiente).
    EdgeSobel {
        /// Intensidade da resposta em [0.0, 4.0]. 1.0 = neutro.
        #[serde(default = "default_one")]
        strength: f32,
    },
    /// Detecção de bordas Laplaciano (5x5 kernel).
    EdgeLaplacian {
        #[serde(default = "default_one")]
        strength: f32,
    },
    /// Canny (binário). Limites configuráveis.
    EdgeCanny {
        #[serde(default = "default_low_threshold")]
        low_threshold: f32,
        #[serde(default = "default_high_threshold")]
        high_threshold: f32,
    },

    // ---------- G12.2 — Blur / denoise
    /// Gaussian blur. `sigma` controla o spread; raio = ceil(3*sigma).
    BlurGaussian {
        sigma: f32,
    },
    /// Median filter — remove outliers preservando bordas. `radius` em pixels.
    BlurMedian {
        radius: u32,
    },
    /// Bilateral (simplificado) — suaviza preservando edges. Custoso.
    BlurBilateral {
        sigma_space: f32,
        sigma_color: f32,
    },

    // ---------- G12.3 — Enhancement
    /// CLAHE — equalização adaptativa por blocos (tiles). `bins` ≙
    /// "histogram bins" do Fiji (default 256 = comportamento anterior).
    Clahe {
        #[serde(default = "default_tile_size")]
        tile_size: u32,
        #[serde(default = "default_clip_limit")]
        clip_limit: f32,
        #[serde(default = "default_clahe_bins")]
        bins: u32,
    },
    /// Subtract Background (rolling ball, Sternberg 1983 — paridade ImageJ).
    /// `radius` em px; `light_background` para objeto escuro/fundo claro;
    /// `disable_smoothing` pula a pré-suavização 3×3.
    SubtractBackground {
        #[serde(default = "default_rolling_radius")]
        radius: f32,
        #[serde(default)]
        light_background: bool,
        #[serde(default)]
        disable_smoothing: bool,
    },
    /// Histogram equalization global na luminância.
    HistogramEqualize,
    /// Auto-levels — estica histograma por canal (RGB) para [percentile_low, percentile_high].
    AutoLevels {
        #[serde(default = "default_percentile_low")]
        percentile_low: f32,
        #[serde(default = "default_percentile_high")]
        percentile_high: f32,
    },
    /// White balance gray-world.
    WhiteBalanceGrayWorld,

    // ---------- G12.4 — Morphology (em luminância)
    /// Dilatação morfológica (kernel quadrado 3x3 ou 5x5).
    Dilate {
        #[serde(default = "default_radius")]
        radius: u32,
    },
    /// Erosão morfológica.
    Erode {
        #[serde(default = "default_radius")]
        radius: u32,
    },
    /// Abertura morfológica = erode → dilate.
    Open {
        #[serde(default = "default_radius")]
        radius: u32,
    },
    /// Fechamento morfológico = dilate → erode.
    Close {
        #[serde(default = "default_radius")]
        radius: u32,
    },

    // ---------- G12.6 — Perspective
    /// Correção de perspectiva — 4 cantos source → 4 cantos destination.
    /// Coordenadas em pixels da imagem original. Output: `output_width` x `output_height`.
    Perspective {
        src: [[f32; 2]; 4],
        dst: [[f32; 2]; 4],
        output_width: u32,
        output_height: u32,
    },

    // ---------- G12 — Extras úteis
    /// Unsharp mask — sharpening clássico.
    UnsharpMask {
        sigma: f32,
        #[serde(default = "default_one")]
        amount: f32,
    },
    /// Threshold simples (binarização).
    Threshold {
        value: u8,
    },
    /// Pixelize uma região (anonimização).
    Pixelize {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        block_size: u32,
    },

    // ---------- W12 (GIMP-parity) — Tonais
    /// Níveis (Levels): remapeia [in_black,in_white]→[out_black,out_white] + gama.
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
    /// Curvas: LUT por interpolação linear entre pontos (x,y) em 0..255.
    Curves {
        #[serde(default = "default_rgb_channel")]
        channel: String,
        #[serde(default)]
        points: Vec<[f32; 2]>,
    },
    /// Posterizar: reduz a `levels` níveis por canal (2..=255).
    Posterize {
        #[serde(default = "default_posterize_levels")]
        levels: u8,
    },

    // ---------- W12 — Canais / falsa-cor
    /// Extrai um canal (grayscale): r/g/b, luminance/luma, h/s/v, y/cb/cr, l_lab/a_lab/b_lab.
    ExtractChannel {
        #[serde(default = "default_luma_channel")]
        channel: String,
    },
    /// Falsa-cor: mapeia a luminância por um colormap (viridis/jet/ironbow/grayscale).
    FalseColor {
        #[serde(default = "default_colormap")]
        colormap: String,
    },

    // ---------- W12 — Forense por comparação / cor
    /// Error Level Analysis: recompressão JPEG na qualidade `quality` + diff×`scale`.
    Ela {
        #[serde(default = "default_ela_quality")]
        quality: u8,
        #[serde(default = "default_ela_scale")]
        scale: f32,
    },
    /// Difference of Gaussians (banda de frequência na luminância).
    DifferenceOfGaussians {
        #[serde(default = "default_dog_sigma1")]
        sigma1: f32,
        #[serde(default = "default_dog_sigma2")]
        sigma2: f32,
        #[serde(default = "default_dog_gain")]
        gain: f32,
    },
    /// Gradiente de luminância colorido (magnitude=brilho, direção=matiz).
    LuminanceGradient {
        #[serde(default = "default_one")]
        strength: f32,
    },
    /// Decorrelation stretch (estilo DStretch) — amplifica diferenças de cor sutis.
    DecorrelationStretch {
        #[serde(default = "default_decorr_sigma")]
        target_sigma: f32,
        #[serde(default = "default_decorr_mean")]
        target_mean: f32,
    },

    // ---------- W12 — Geométrica / genérica
    /// Rotação por ângulo arbitrário (bilinear). `expand` cresce a tela.
    RotateArbitrary {
        #[serde(default)]
        degrees: f32,
        #[serde(default = "default_true")]
        expand: bool,
    },
    /// Convolução genérica NxN (kernel editável + divisor + offset).
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

    // ---------- W20 (S2) — Operação confinada a uma seleção (estilo Photoshop)
    /// Aplica `op` (qualquer operação acima) e compõe o resultado SÓ dentro da
    /// `mask`; fora da máscara o pixel original é preservado (borda dura, sem
    /// feather nesta fase). A máscara é "congelada" no momento da aplicação —
    /// o filtro permanece reproduzível mesmo se o perito deselecionar depois.
    /// Auditável: o `op` interno e a `mask` vão íntegros para o sidecar do
    /// export (serde). Reaplica idêntico em preview (reduzido) e export (cheio)
    /// porque a máscara é normalizada `[0,1]`.
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
// W12 (GIMP-parity) defaults.
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
// Image metadata (returned by `get_image_metadata`)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMetadata {
    pub width: u32,
    pub height: u32,
    pub mime_type: Option<String>,
    pub format_label: Option<String>,
    pub size_bytes: u64,
    pub hash_sha256: Option<String>,
    /// JSON serialised — vazio quando não houver EXIF lido.
    pub exif_json: Option<String>,
    /// G12.8 — Conjunto completo de hashes pericial (opcional).
    /// Computado só quando o caller pede `compute_hash=true` e o backend
    /// suporta — fica `None` em metadados leves.
    #[serde(default)]
    pub hash_set: Option<HashSet>,
}

/// G12.8 — Conjunto de hashes para chain of custody pericial.
///
/// MD5 é matematicamente comprometido, mas ainda é exigido por convenção
/// em muitos laudos institucionais. SHA-1 idem. SHA-256 é o atual padrão
/// recomendado. SHA-3-256 é a próxima geração (Keccak), oferecido como
/// "future-proofing" para laudos que precisem sobreviver décadas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashSet {
    pub md5: String,
    pub sha1: String,
    pub sha256: String,
    pub sha3_256: String,
}

/// G12.9 — Histograma + estatísticas básicas de uma imagem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageHistogram {
    /// 256 bins (count de pixels com cada valor 0..255), canal R.
    pub red: Vec<u32>,
    pub green: Vec<u32>,
    pub blue: Vec<u32>,
    /// Luminância calculada via 0.299R + 0.587G + 0.114B.
    pub luminance: Vec<u32>,
    pub stats: HistogramStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramStats {
    /// Per-channel mean (0..255).
    pub mean_r: f32,
    pub mean_g: f32,
    pub mean_b: f32,
    pub mean_lum: f32,
    /// Per-channel stddev.
    pub stddev_r: f32,
    pub stddev_g: f32,
    pub stddev_b: f32,
    pub stddev_lum: f32,
    /// Min / Max do canal de luminância (útil para gauge de dinâmica).
    pub min_lum: u8,
    pub max_lum: u8,
    pub total_pixels: u32,
}

// ---------------------------------------------------------------------------
// Asset bytes (returned by `read_image_asset` — base64 igual ao do MVP 4).

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageAssetBytes {
    pub relative_path: String,
    pub mime_type: String,
    pub base64: String,
    pub size_bytes: u64,
}
