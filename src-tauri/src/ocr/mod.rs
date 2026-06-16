//! Abstração de motor OCR / extração de texto do módulo Documentoscopia.
//!
//! O motor de produção recomendado é o **PaddleOCR via sidecar Python** (ver
//! plano do módulo) — trabalho futuro. Esta camada existe para que a UI e os
//! comandos NÃO fiquem acoplados a um motor específico: todo motor implementa
//! [`OcrEngine`] e devolve um [`OcrOutcome`]. Trocar de motor (Paddle, Tesseract
//! fallback, TrOCR, Surya, olmOCR…) é só trocar a implementação registrada.
//!
//! Hoje há:
//!   * [`MockOcrEngine`] — determinístico e **claramente rotulado** como mock;
//!     serve para exercitar todo o fluxo (overlay → revisão → campos → laudo)
//!     sem depender de motor instalado. NÃO é OCR real.
//!   * [`PaddleSidecarEngine`] — placeholder do motor real; reporta-se como
//!     indisponível até o sidecar existir, mantendo o contrato para a troca.
//!
//! Coordenadas de bbox são NORMALIZADAS (0..1) em relação à página.

use std::path::{Path, PathBuf};

use crate::error::{Result, SicroError};

/// Nomes canônicos dos 3 arquivos do pacote latino do RapidOCR (no diretório de
/// modelos). Fonte ÚNICA de verdade — usada pelo gerenciador (download em
/// `commands/ocr_commands.rs`) e pelo seletor de motor abaixo.
pub const RAPIDOCR_DET_FILE: &str = "pp-ocrv5_mobile_det.onnx";
pub const RAPIDOCR_REC_FILE: &str = "latin_pp-ocrv5_mobile_rec.onnx";
pub const RAPIDOCR_DICT_FILE: &str = "ppocrv5_latin_dict.txt";

/// Um bloco reconhecido. `bbox` = [x, y, w, h] normalizado (0..1).
#[derive(Debug, Clone)]
pub struct OcrBlock {
    pub text: String,
    pub confidence: f64,
    pub bbox: [f64; 4],
    pub block_type: String,
    pub reading_order: i64,
}

/// Resultado de uma execução de OCR.
#[derive(Debug, Clone)]
pub struct OcrOutcome {
    pub engine: String,
    pub engine_version: String,
    pub blocks: Vec<OcrBlock>,
    pub avg_confidence: Option<f64>,
}

/// Contrato comum a todos os motores. A UI fala com isto, nunca com um motor
/// concreto.
pub trait OcrEngine {
    fn id(&self) -> &str;
    fn version(&self) -> &str;
    fn available(&self) -> bool;
    /// Reconhece o texto de uma imagem de página já renderizada.
    fn recognize(&self, image_path: &Path, language: &str) -> Result<OcrOutcome>;
}

fn outcome(engine: &str, version: &str, blocks: Vec<OcrBlock>) -> OcrOutcome {
    let avg = if blocks.is_empty() {
        None
    } else {
        Some(blocks.iter().map(|b| b.confidence).sum::<f64>() / blocks.len() as f64)
    };
    OcrOutcome {
        engine: engine.to_string(),
        engine_version: version.to_string(),
        blocks,
        avg_confidence: avg,
    }
}

/// Motor mock — placeholder determinístico, **não é OCR real**. Os blocos são
/// rotulados como tal para que o perito jamais os confunda com extração real.
pub struct MockOcrEngine;

impl OcrEngine for MockOcrEngine {
    fn id(&self) -> &str {
        "mock"
    }
    fn version(&self) -> &str {
        "0-mock"
    }
    fn available(&self) -> bool {
        true
    }
    fn recognize(&self, _image_path: &Path, _language: &str) -> Result<OcrOutcome> {
        let blocks = vec![
            OcrBlock {
                text: "[RASCUNHO MOCK — motor OCR real não instalado]".to_string(),
                confidence: 0.40,
                bbox: [0.06, 0.05, 0.88, 0.06],
                block_type: "header".to_string(),
                reading_order: 0,
            },
            OcrBlock {
                text: "Este conteúdo é um espaço reservado gerado pelo motor de \
                       demonstração. Instale o motor de OCR (sidecar) para obter o \
                       texto real do documento. Todo resultado exige revisão do perito."
                    .to_string(),
                confidence: 0.42,
                bbox: [0.06, 0.14, 0.88, 0.22],
                block_type: "paragraph".to_string(),
                reading_order: 1,
            },
            OcrBlock {
                text: "Bloco de exemplo · revisão humana obrigatória.".to_string(),
                confidence: 0.36,
                bbox: [0.06, 0.42, 0.60, 0.07],
                block_type: "paragraph".to_string(),
                reading_order: 2,
            },
        ];
        Ok(outcome(self.id(), self.version(), blocks))
    }
}

/// Placeholder do motor real (PaddleOCR via sidecar Python). Indisponível até o
/// sidecar ser implementado/instalado; mantém o contrato para troca futura.
pub struct PaddleSidecarEngine;

impl OcrEngine for PaddleSidecarEngine {
    fn id(&self) -> &str {
        "paddleocr"
    }
    fn version(&self) -> &str {
        ""
    }
    fn available(&self) -> bool {
        // Futuro: detectar o sidecar (binário/serviço Python). Por ora, ausente.
        false
    }
    fn recognize(&self, _image_path: &Path, _language: &str) -> Result<OcrOutcome> {
        Err(SicroError::Validation(
            "motor de OCR real (PaddleOCR via sidecar Python) ainda não está \
             instalado neste computador. Use o rascunho de demonstração ou \
             instale o motor para OCR de produção."
                .to_string(),
        ))
    }
}



/// Motor de produção: **RapidOCR / PaddleOCR (PP-OCRv5) via ONNX** (crate
/// `oar-ocr`, rodando sobre o ONNX Runtime). Offline, sem Python. Constrói a
/// pipeline a partir de 3 arquivos de modelo (detecção + reconhecimento latino
/// + dicionário) e devolve blocos por LINHA com bbox NORMALIZADO (0..1) e
/// confiança. É o motor padrão da Documentoscopia (qualidade muito superior ao
/// Tesseract em documentos reais).
pub struct RapidOcrEngine {
    pub det_model: PathBuf,
    pub rec_model: PathBuf,
    pub dict: PathBuf,
}

impl OcrEngine for RapidOcrEngine {
    fn id(&self) -> &str {
        "rapidocr"
    }
    fn version(&self) -> &str {
        "PP-OCRv5"
    }
    fn available(&self) -> bool {
        self.det_model.is_file() && self.rec_model.is_file() && self.dict.is_file()
    }
    fn recognize(&self, image_path: &Path, _language: &str) -> Result<OcrOutcome> {
        // A construção carrega os modelos ONNX (caro). Mantemos por chamada por
        // ora; otimização (cache do motor) é trabalho futuro.
        let ocr = oar_ocr::prelude::OAROCRBuilder::new(
            self.det_model.clone(),
            self.rec_model.clone(),
            self.dict.clone(),
        )
        .build()
        .map_err(|e| {
            SicroError::Validation(format!("falha ao iniciar o motor de OCR: {e}"))
        })?;

        let img = oar_ocr::prelude::load_image(image_path)
            .map_err(|e| SicroError::Validation(format!("falha ao abrir a imagem: {e}")))?;
        let iw = (img.width().max(1)) as f64;
        let ih = (img.height().max(1)) as f64;

        let results = ocr
            .predict(vec![img])
            .map_err(|e| SicroError::Validation(format!("falha no OCR: {e}")))?;

        let mut blocks = Vec::new();
        if let Some(result) = results.into_iter().next() {
            for (i, region) in result.text_regions.into_iter().enumerate() {
                let text = match &region.text {
                    Some(t) => t.to_string(),
                    None => continue,
                };
                if text.trim().is_empty() {
                    continue;
                }
                let conf = region.confidence.unwrap_or(0.0) as f64;
                let bb = &region.bounding_box;
                let x0 = bb.x_min() as f64;
                let y0 = bb.y_min() as f64;
                let x1 = bb.x_max() as f64;
                let y1 = bb.y_max() as f64;
                blocks.push(OcrBlock {
                    text,
                    confidence: conf,
                    bbox: [
                        (x0 / iw).clamp(0.0, 1.0),
                        (y0 / ih).clamp(0.0, 1.0),
                        ((x1 - x0) / iw).clamp(0.0, 1.0),
                        ((y1 - y0) / ih).clamp(0.0, 1.0),
                    ],
                    block_type: "line".to_string(),
                    reading_order: i as i64,
                });
            }
        }
        Ok(outcome(self.id(), self.version(), blocks))
    }
}



/// Constrói o motor a partir do diretório de modelos: se os 3 arquivos do
/// pacote latino existem, usa o [`RapidOcrEngine`] (PP-OCRv5/ONNX); senão, o
/// mock rotulado. É o seletor usado pela Documentoscopia — o motor (ONNX
/// Runtime) é embutido; só os modelos são baixados.
pub fn engine_from_models_dir(models_dir: &Path) -> Box<dyn OcrEngine> {
    let det = models_dir.join(RAPIDOCR_DET_FILE);
    let rec = models_dir.join(RAPIDOCR_REC_FILE);
    let dict = models_dir.join(RAPIDOCR_DICT_FILE);
    if det.is_file() && rec.is_file() && dict.is_file() {
        Box::new(RapidOcrEngine {
            det_model: det,
            rec_model: rec,
            dict,
        })
    } else {
        Box::new(MockOcrEngine)
    }
}
