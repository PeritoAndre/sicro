//! Ocultação assistida (§6.4, Fase 4, P4.5): o SICRO PROPÕE alvos (cabeça e rosto, placa, corpo, veículo) num trecho e
//! o perito confirma. Detecção quadro a quadro (`decodificadores.rs`), trilhas pelo ByteTrack com o pós-processamento
//! de `p3b/ref/trilhas.py` (costura, buracos interpolados, extensão de 0,25 s, centro suavizado, tamanho que não
//! encolhe, margem por classe) e cada trilha vira `Rastreio { origem: deteccao }` SEM confirmação, com o denso no
//! projeto. Quem grava o projeto é a tela (D2). Item "Ocultação" do Gerenciador: os seis modelos com SHA-256 fixo.
//! Uma detecção por vez no processo; eventos `edicao:deteccao`. Dono: P4.5.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::decodificadores::{self as dec, Deteccao, Entrada, RetinaFace, Rosto, RtDetr, YoloxN, YuNet};
use super::{arquivo, ArquivoDenso, ArquivoModelo, Corrida, Imagem, Limiares, LeitorQuadros, ModeloCorrida, Motivo, MotorCorrida, MotorRastreio, Sentido, MAX_QUADROS};
use super::{ESTADO_INCERTO, ESTADO_OK, TIPO_MEDIDO};
use crate::commands::settings_commands::AiSettings;
use crate::edicao::executar::{self, ERRO_CANCELADO, ERRO_LEITURA_DIVERGENTE};
use crate::edicao::mensagens::{self, codigo};
use crate::edicao::modelo::{BaseRastreio, ClasseDeteccao, ClipePrincipal, ClipeTrecho, OrigemRastreio, ProjetoEdicao, Rastreio, TamanhoPx, TipoRastreio, Tolerante};
use crate::edicao::quadros::{OrigemTabela, TabelaQuadros};
use crate::edicao::{armazem, tempo};
use crate::error::{Result, SicroError};
use crate::ia::{self, Opcoes, Preferencia, Provedor, SessaoIa};
use crate::video::pessoas::bytetrack::{ByteTrack, Params};
use crate::video::pessoas::yolox::Det;

pub const EVENTO_DETECCAO: &str = "edicao:deteccao";
const AVANCOS_POR_S: u64 = 5;

// ---------------------------------------------------------------- item "Ocultação" do Gerenciador (§6.5)

/// Um modelo do item: arquivo local, bytes, SHA-256, URL fixa e a identificação que vai para a corrida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArquivoRedacao {
    pub nome: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    pub url: &'static str,
    pub rotulo: &'static str,
    pub origem: &'static str,
    pub commit: &'static str,
}

/// Release do SICRO que re-hospeda os dois modelos do PINTO (só existem em tarballs de 2,56 GB e 260 MB).
pub const URL_SICRO: &str = "https://github.com/PeritoAndre/sicro/releases/download/modelos-redacao-1/";

pub const CORPO_S: ArquivoRedacao = ArquivoRedacao {
    nome: "corpo_yolox_s_wholebody12.onnx",
    bytes: 35_792_077,
    sha256: "6666533bde5d9a22d0f63ce3ce45e1e3da71ab35a96ea06bf68a178c8c968e0e",
    url: "https://github.com/PeritoAndre/sicro/releases/download/modelos-redacao-1/corpo_yolox_s_wholebody12.onnx",
    rotulo: "YOLOX WholeBody12 S",
    origem: "PINTO_model_zoo/449_YOLOX-WholeBody12 (resources_s.tar.gz, yolox_s_wholebody12_Nx3xHxW.onnx)",
    commit: "",
};
pub const CORPO_N: ArquivoRedacao = ArquivoRedacao {
    nome: "corpo_yolox_n_wholebody12.onnx",
    bytes: 3_660_959,
    sha256: "5bd58bfac58f2b13daffa2d261d511c3bd76af21fb02514ad4000197cbe05485",
    url: "https://github.com/PeritoAndre/sicro/releases/download/modelos-redacao-1/corpo_yolox_n_wholebody12.onnx",
    rotulo: "YOLOX WholeBody12 N",
    origem: "PINTO_model_zoo/449_YOLOX-WholeBody12 (resources_n.tar.gz, yolox_n_wholebody12_Nx3xHxW.onnx)",
    commit: "",
};
pub const ROSTO_R34: ArquivoRedacao = ArquivoRedacao {
    nome: "rosto_retinaface_r34.onnx",
    bytes: 88_502_283,
    sha256: "bd0263dc2a465d32859555cb1741f2d98991eb0053696e8ee33fec583d30e630",
    url: "https://github.com/yakhyo/retinaface-pytorch/releases/download/v0.0.1/retinaface_r34.onnx",
    rotulo: "RetinaFace R34",
    origem: "yakhyo/retinaface-pytorch",
    commit: "v0.0.1",
};
pub const ROSTO_YUNET: ArquivoRedacao = ArquivoRedacao {
    nome: "rosto_yunet_n.onnx",
    bytes: 322_906,
    sha256: "104ee26c71d5c270ce79a6b3dced91c91013260d29a6bf18f9296c5b8c4c2b12",
    url: "https://github.com/ShiqiYu/libfacedetection.train/raw/02246e79b1e976c83d1e135a85e0628120c93769/onnx/yunet_n_dynamic.onnx",
    rotulo: "YuNet-n",
    origem: "ShiqiYu/libfacedetection.train",
    commit: "02246e79b1e976c83d1e135a85e0628120c93769",
};
pub const PLACA_RTDETR: ArquivoRedacao = ArquivoRedacao {
    nome: "placa_rtdetrv2_r18_openimages.onnx",
    bytes: 80_671_879,
    sha256: "50f9bf9d7eaa97ade59063ba608107203d0575a84c0fb27e0ebf3bab6f594366",
    url: "https://huggingface.co/Topurrra/rtdetr-license-plate-detection-onnx/resolve/aecb24a78e31cea03f59b83156d55dde84f20d94/plate_rtdetr.onnx",
    rotulo: "RT-DETRv2-R18 placas",
    origem: "Topurrra/rtdetr-license-plate-detection-onnx",
    commit: "aecb24a78e31cea03f59b83156d55dde84f20d94",
};
pub const PLACA_YOLOX: ArquivoRedacao = ArquivoRedacao {
    nome: "placa_yolox_s_autolane.onnx",
    bytes: 35_753_707,
    sha256: "f9302f8c9a951861cf5e1266951d3962ce299b6636e73fa32032ffe38fe90ee7",
    url: "https://huggingface.co/autolane/yolox-s-alpr/resolve/72332940134a1afd244576e09f84dc8fe94414a2/yolox_plates_s.onnx",
    rotulo: "YOLOX-S placas",
    origem: "autolane/yolox-s-alpr",
    commit: "72332940134a1afd244576e09f84dc8fe94414a2",
};
/// O YOLOX-S COCO do item "Pessoas no vídeo" (`AiSettings.pessoas_model_path`): veículos para as placas.
pub const VEICULO_COCO: ArquivoRedacao = ArquivoRedacao {
    nome: "yolox_s.onnx",
    bytes: 35_858_002,
    sha256: "c5c2d13e59ae883e6af3b45daea64af4833a4951c92d116ec270d9ddbe998063",
    url: "https://github.com/Megvii-BaseDetection/YOLOX/releases/download/0.1.1rc0/yolox_s.onnx",
    rotulo: "YOLOX-S COCO",
    origem: "Megvii-BaseDetection/YOLOX",
    commit: "0.1.1rc0",
};

/// Os seis arquivos do item (`ai/redacao/`), na ordem do download.
pub const ARQUIVOS: [ArquivoRedacao; 6] = [CORPO_S, CORPO_N, ROSTO_R34, ROSTO_YUNET, PLACA_RTDETR, PLACA_YOLOX];

/// Pasta do item (`AiSettings.redacao_dir`); None = não instalado.
pub fn pasta(ai: &AiSettings) -> Option<PathBuf> {
    (!ai.redacao_dir.is_empty()).then(|| PathBuf::from(&ai.redacao_dir))
}

/// Os seis arquivos com o tamanho certo (o SHA-256 é conferido ao abrir as sessões).
pub fn instalado(ai: &AiSettings) -> bool {
    pasta(ai).is_some_and(|d| ARQUIVOS.iter().all(|a| std::fs::metadata(d.join(a.nome)).is_ok_and(|m| m.is_file() && m.len() == a.bytes)))
}

/// Tamanho do download em MB.
pub fn mb() -> u32 {
    (ARQUIVOS.iter().map(|a| a.bytes).sum::<u64>() as f64 / 1e6).round() as u32
}

fn ausente(qual: &str) -> SicroError {
    SicroError::Validation(format!("{}: {qual}", codigo::MODELO_AUSENTE))
}

// ---------------------------------------------------------------- detectores de um pedido

/// O que os alvos pedidos precisam (Rosto = cabeça E rosto, unidos; §6.4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Precisa {
    corpo: bool,
    rosto: bool,
    placa: bool,
    coco: bool,
}

impl Precisa {
    fn de(classes: &[ClasseDeteccao]) -> Self {
        let tem = |c: ClasseDeteccao| classes.contains(&c);
        Precisa {
            corpo: tem(ClasseDeteccao::Rosto) || tem(ClasseDeteccao::Cabeca) || tem(ClasseDeteccao::Pessoa),
            rosto: tem(ClasseDeteccao::Rosto),
            placa: tem(ClasseDeteccao::Placa),
            coco: tem(ClasseDeteccao::Placa) || tem(ClasseDeteccao::Veiculo),
        }
    }
}

/// Classes de trilha que cada pedido produz, na ordem em que viram propostas.
pub fn classes_das_trilhas(pedidas: &[ClasseDeteccao]) -> Vec<ClasseDeteccao> {
    use ClasseDeteccao::*;
    [Cabeca, Rosto, Placa, Pessoa, Veiculo]
        .into_iter()
        .filter(|c| match c {
            Cabeca => pedidas.contains(&Rosto) || pedidas.contains(&Cabeca),
            outro => pedidas.contains(outro),
        })
        .collect()
}

/// Conjunto de modelos: `Qualidade` (GPU: WholeBody12 S, RetinaFace R34, RT-DETR + YOLOX nas placas) ou `Queda`
/// (CPU: WholeBody12 N, YuNet-n, só o YOLOX de placas), `modelos-redacao.md` §1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conjunto {
    Qualidade,
    Queda,
}

/// Lado máximo da entrada do detector de rosto (2× do WIDER; 1080p fica perto de 1×) e do de cabeças.
pub const LADO_ROSTO_MAX: usize = 2048;
pub const LADO_CORPO_MAX: usize = 1920;

pub struct Detectores {
    pub conjunto: Conjunto,
    pub provedor: Provedor,
    corpo: Option<YoloxN>,
    rosto: Option<Rosto>,
    rtdetr: Option<RtDetr>,
    placa: Option<YoloxN>,
    coco: Option<YoloxN>,
}

fn sha256_confere(arq: &Path, esperado: &str) -> bool {
    use sha2::{Digest, Sha256};
    let Ok(mut f) = std::fs::File::open(arq) else { return false };
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h).is_ok() && format!("{:x}", h.finalize()) == esperado
}

impl Detectores {
    /// As sessões dos alvos pedidos por `ia::criar_sessao`: GPU pronta = conjunto de qualidade (o primeiro grafo
    /// aquece; se cair para CPU, tudo vai para a queda); senão a queda em CPU. Interruptor desligado = `ia_desligada`;
    /// item ausente ou alterado = `modelo_ausente: redacao`; placa sem o YOLOX-S das Pessoas = `modelo_ausente: pessoas`.
    pub fn novos(ai: &AiSettings, classes: &[ClasseDeteccao]) -> Result<Self> {
        if !ia::ligada(ai) {
            return Err(SicroError::Validation(ia::IA_DESLIGADA.into()));
        }
        let precisa = Precisa::de(classes);
        let dir = pasta(ai).ok_or_else(|| ausente("redacao"))?;
        let coco = PathBuf::from(&ai.pessoas_model_path);
        if precisa.coco && !(coco.is_file() && sha256_confere(&coco, VEICULO_COCO.sha256)) {
            return Err(ausente("pessoas"));
        }
        let abrir = |a: &ArquivoRedacao, pref: Preferencia, aquecer: bool| -> Result<SessaoIa> {
            let p = dir.join(a.nome);
            if !sha256_confere(&p, a.sha256) {
                return Err(ausente("redacao"));
            }
            ia::criar_sessao(ai, &p, &Opcoes { preferencia: pref, aquecer: aquecer.then_some([1, 3, 640, 640]), ..Opcoes::default() })
        };
        let abrir_coco = |pref: Preferencia| {
            ia::criar_sessao(ai, &coco, &Opcoes { preferencia: pref, aquecer: (pref == Preferencia::Gpu).then_some([1, 3, 640, 640]), ..Opcoes::default() })
        };
        if ia::gpu::pronta(ai).is_some() {
            // cada grafo aquece na GPU (a falta do cuDNN e da memória só aparece aí); um que não sirva derruba o
            // conjunto inteiro para a CPU: um provedor só por detecção
            let qualidade = (|| -> Result<Self> {
                let g = |a: &ArquivoRedacao| abrir(a, Preferencia::Gpu, true);
                let corpo = precisa.corpo.then(|| g(&CORPO_S)).transpose()?;
                let rosto = precisa.rosto.then(|| g(&ROSTO_R34)).transpose()?;
                let rtdetr = precisa.placa.then(|| g(&PLACA_RTDETR)).transpose()?;
                let placa = precisa.placa.then(|| g(&PLACA_YOLOX)).transpose()?;
                let coco = precisa.coco.then(|| abrir_coco(Preferencia::Gpu)).transpose()?;
                Self::com(Conjunto::Qualidade, corpo, rosto, rtdetr, placa, coco)
            })();
            match qualidade {
                Ok(d) => return Ok(d),
                Err(e) if executar::codigo_de(&e) == Some(codigo::MODELO_AUSENTE) => return Err(e),
                Err(e) => tracing::warn!(target: "ia", "detecção: GPU não serviu, conjunto de CPU: {e}"),
            }
        }
        let corpo = precisa.corpo.then(|| abrir(&CORPO_N, Preferencia::Cpu, false)).transpose()?;
        let rosto = precisa.rosto.then(|| abrir(&ROSTO_YUNET, Preferencia::Cpu, false)).transpose()?;
        let placa = precisa.placa.then(|| abrir(&PLACA_YOLOX, Preferencia::Cpu, false)).transpose()?;
        let coco = precisa.coco.then(|| abrir_coco(Preferencia::Cpu)).transpose()?;
        Self::com(Conjunto::Queda, corpo, rosto, None, placa, coco)
    }

    /// A partir de sessões prontas (testes e `novos`).
    pub(crate) fn com(conjunto: Conjunto, corpo: Option<SessaoIa>, rosto: Option<SessaoIa>, rtdetr: Option<SessaoIa>, placa: Option<SessaoIa>, coco: Option<SessaoIa>) -> Result<Self> {
        let provedores: Vec<Provedor> = [&corpo, &rosto, &rtdetr, &placa, &coco].iter().filter_map(|s| s.as_ref().map(|s| s.provedor)).collect();
        let provedor = if !provedores.is_empty() && provedores.iter().all(|p| *p == Provedor::Cuda) { Provedor::Cuda } else { Provedor::Cpu };
        Ok(Self {
            conjunto,
            provedor,
            corpo: corpo.map(|s| YoloxN::nova(s, Entrada::Esticar)).transpose()?,
            rosto: rosto.map(|s| if conjunto == Conjunto::Qualidade { Rosto::Retina(RetinaFace::nova(s)) } else { Rosto::YuNet(YuNet { s }) }),
            rtdetr: rtdetr.map(|s| RtDetr { s }),
            placa: placa.map(|s| YoloxN::nova(s, Entrada::Letterbox)).transpose()?,
            coco: coco.map(|s| YoloxN::nova(s, Entrada::Letterbox)).transpose()?,
        })
    }

    /// Detecções de um quadro, por classe de trilha pedida, com o score normalizado (0,5 = limiar de trabalho).
    pub fn quadro(&mut self, q: &Imagem, classes: &[ClasseDeteccao]) -> Result<Vec<(ClasseDeteccao, Vec<Deteccao>)>> {
        use ClasseDeteccao::*;
        let trilhas = classes_das_trilhas(classes);
        let mut out = Vec::new();
        let maior = q.w.max(q.h) as usize;
        if let Some(corpo) = self.corpo.as_mut() {
            let escala = (LADO_CORPO_MAX as f64 / maior as f64).min(1.0);
            let hw = dec::multiplo32(q.h as usize, q.w as usize, escala);
            let (alto, baixo) = dec::CABECA;
            let s = corpo.saida(q, Some(hw))?;
            if trilhas.contains(&Cabeca) {
                out.push((Cabeca, dec::normalizar(s.deteccoes(baixo, 0.45, &[dec::CABECA_WB12]), alto)));
            }
            if trilhas.contains(&Pessoa) {
                out.push((Pessoa, dec::normalizar(s.deteccoes(baixo, 0.45, &[dec::CORPO]), alto)));
            }
        }
        if let (true, Some(rosto)) = (trilhas.contains(&Rosto), self.rosto.as_mut()) {
            let escala = match rosto {
                dec::Rosto::Retina(_) => 2.0,
                dec::Rosto::YuNet(_) => 1.5,
            };
            let lado = dec::lado_do_rosto(q, escala).min(LADO_ROSTO_MAX);
            out.push((Rosto, dec::rostos(rosto, q, lado)?));
        }
        if trilhas.contains(&Placa) {
            let (placa, coco) = (self.placa.as_mut().ok_or_else(|| ausente("redacao"))?, self.coco.as_mut().ok_or_else(|| ausente("pessoas"))?);
            let (p, _) = dec::placas(self.rtdetr.as_mut(), placa, coco, q)?;
            out.push((Placa, p));
        }
        if trilhas.contains(&Veiculo) {
            let coco = self.coco.as_mut().ok_or_else(|| ausente("pessoas"))?;
            let (alto, baixo) = dec::VEICULO;
            let v = dec::nms(coco.detectar(q, baixo, 0.45, &dec::VEICULOS_COCO, None)?, 0.6, false);
            out.push((Veiculo, dec::normalizar(v, alto)));
        }
        Ok(out)
    }

    /// Modelo da corrida de uma classe de trilha: todos os arquivos que deram detecção a ela (§6.6).
    pub fn modelo(&self, classe: ClasseDeteccao) -> ModeloCorrida {
        use ClasseDeteccao::*;
        let q = self.conjunto == Conjunto::Qualidade;
        let arquivos: Vec<&ArquivoRedacao> = match classe {
            Cabeca | Pessoa => vec![if q { &CORPO_S } else { &CORPO_N }],
            Rosto => vec![if q { &ROSTO_R34 } else { &ROSTO_YUNET }],
            Placa if q => vec![&PLACA_RTDETR, &PLACA_YOLOX, &VEICULO_COCO],
            Placa => vec![&PLACA_YOLOX, &VEICULO_COCO],
            Veiculo => vec![&VEICULO_COCO],
        };
        let junta = |f: fn(&ArquivoRedacao) -> &'static str| arquivos.iter().map(|a| f(a)).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" + ");
        ModeloCorrida {
            nome: junta(|a| a.rotulo),
            origem: junta(|a| a.origem),
            commit: junta(|a| a.commit),
            arquivos: arquivos.iter().map(|a| ArquivoModelo { nome: a.nome.into(), sha256: a.sha256.into() }).collect(),
            desvio: String::new(),
        }
    }
}

// ---------------------------------------------------------------- trilhas (porte de `p3b/ref/trilhas.py`)

/// Uma linha de proposta: quadro analisado (desde 1), caixa (x1, y1, x2, y2) em px, score e origem (0 detecção,
/// 1 interpolada, 2 estendida).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Linha {
    pub q: i64,
    pub c: [f64; 4],
    pub score: f64,
    pub origem: u8,
}

/// Proposta pronta (depois da margem) e a altura mediana das caixas detectadas antes da margem.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposta {
    pub linhas: Vec<Linha>,
    pub altura_mediana: f64,
}

/// Parâmetros de `trilhas_de_redacao` (os padrões do dossiê: alto 0,5 e baixo 0,25 no score normalizado).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamsTrilhas {
    pub alto: f64,
    pub baixo: f64,
    pub extra_nova: f64,
    pub folga: f64,
    pub piso: f64,
    pub margem: f64,
    pub margem_px: f64,
    pub min_quadros: usize,
}

impl ParamsTrilhas {
    /// A da Edição para cada classe (margem: cabeça 15 %, rosto 25 %, placa 15 % com piso de 3 px).
    pub fn da_classe(c: ClasseDeteccao) -> Self {
        let (margem, margem_px) = match c {
            ClasseDeteccao::Rosto => (0.25, 2.0),
            ClasseDeteccao::Placa => (0.15, 3.0),
            _ => (0.15, 2.0),
        };
        Self { alto: 0.5, baixo: 0.25, extra_nova: 0.1, folga: 0.25, piso: 24.0, margem, margem_px, min_quadros: 3 }
    }
}

type Trilha = Vec<(i64, [f64; 4], f64)>;

/// ByteTrack com a caixa detectada no histórico: {id → [(quadro, caixa, score)]} (quadro desde 1).
fn rastrear(dets: &[Vec<Deteccao>], p: &ParamsTrilhas, max_perdido: u32) -> Vec<(u32, Trilha)> {
    let mut bt = ByteTrack::new(Params {
        high: p.alto as f32,
        low: p.baixo as f32,
        match_thr: 0.8,
        max_lost: max_perdido,
        extra_nova: p.extra_nova as f32,
        folga: p.folga as f32,
        piso: p.piso as f32,
        guardar_detectada: true,
    });
    for d in dets {
        let v: Vec<Det> = d.iter().map(|x| Det { x: x.x1, y: x.y1, w: x.w(), h: x.h(), score: x.score, classe: 0 }).collect();
        bt.update(&v);
    }
    bt.history
        .into_iter()
        .map(|(id, h)| (id, h.into_iter().map(|s| (s.frame as i64, [s.x as f64, s.y as f64, (s.x + s.w) as f64, (s.y + s.h) as f64], s.score as f64)).collect()))
        .collect()
}

fn centro(c: &[f64; 4]) -> (f64, f64) {
    ((c[0] + c[2]) / 2.0, (c[1] + c[3]) / 2.0)
}

/// Une B ao fim de A quando B começa até `max_intervalo` quadros depois de A e o centro previsto de A (velocidade
/// dos últimos 5 pontos) cai a menos de `fator` × o tamanho; altura entre 0,5× e 2×.
fn costurar(trilhas: Vec<(u32, Trilha)>, max_intervalo: i64, fator: f64) -> Vec<Trilha> {
    let mut tr: Vec<Option<Trilha>> = {
        let mut v = trilhas;
        v.sort_by_key(|(_, t)| t[0].0);
        v.into_iter().map(|(_, t)| Some(t)).collect()
    };
    let mut mudou = true;
    while mudou {
        mudou = false;
        for a in 0..tr.len() {
            let Some(at) = tr[a].as_ref() else { continue };
            let fim = *at.last().unwrap_or_else(|| unreachable!());
            let k = &at[at.len() - at.len().min(5)..];
            let dt = ((k[k.len() - 1].0 - k[0].0) as f64).max(1.0);
            let (c0, c1) = (centro(&k[0].1), centro(&k[k.len() - 1].1));
            let vel = if k.len() > 1 { ((c1.0 - c0.0) / dt, (c1.1 - c0.1) / dt) } else { (0.0, 0.0) };
            let mut melhor: Option<(f64, usize)> = None;
            for (b, bt) in tr.iter().enumerate() {
                let Some(bt) = bt.as_ref() else { continue };
                let g = bt[0].0 - fim.0;
                if b == a || g <= 0 || g > max_intervalo {
                    continue;
                }
                let cb = centro(&bt[0].1);
                let (b0, f) = (&bt[0].1, &fim.1);
                let tam = (b0[2] - b0[0]).max(b0[3] - b0[1]).max(f[2] - f[0]).max(f[3] - f[1]);
                let (px, py) = (c1.0 + vel.0 * g as f64 - cb.0, c1.1 + vel.1 * g as f64 - cb.1);
                let dist = (px * px + py * py).sqrt();
                let razao = (b0[3] - b0[1]) / (f[3] - f[1]).max(1e-3);
                if dist <= fator * tam && (0.5..=2.0).contains(&razao) && melhor.is_none_or(|(m, _)| dist < m) {
                    melhor = Some((dist, b));
                }
            }
            if let Some((_, b)) = melhor {
                let resto = tr[b].take().unwrap_or_default();
                if let Some(at) = tr[a].as_mut() {
                    at.extend(resto);
                }
                mudou = true;
            }
        }
    }
    tr.into_iter().flatten().collect()
}

/// Interpola os buracos de até `max_buraco` quadros (origem 1); buraco maior parte a trilha em segmentos.
fn preencher(t: &Trilha, max_buraco: i64) -> Vec<Vec<Linha>> {
    let lin = |x: &(i64, [f64; 4], f64), origem: u8| Linha { q: x.0, c: x.1, score: x.2, origem };
    let mut segs = Vec::new();
    let mut atual = vec![lin(&t[0], 0)];
    for w in t.windows(2) {
        let (p, q) = (&w[0], &w[1]);
        let g = q.0 - p.0;
        if g > max_buraco + 1 {
            segs.push(std::mem::take(&mut atual));
        } else {
            for k in 1..g {
                let a = k as f64 / g as f64;
                let c = std::array::from_fn(|i| p.1[i] * (1.0 - a) + q.1[i] * a);
                atual.push(Linha { q: p.0 + k, c, score: p.2.min(q.2), origem: 1 });
            }
        }
        atual.push(lin(q, 0));
    }
    segs.push(atual);
    segs
}

/// `n` quadros antes e depois por velocidade constante (origem 2), sem sair do quadro `w`×`h`.
fn estender(seg: &[Linha], n: i64, w: f64, h: f64) -> Vec<Linha> {
    let vel = |k: &[Linha]| -> [f64; 4] {
        let dt = ((k[k.len() - 1].q - k[0].q) as f64).max(1.0);
        std::array::from_fn(|i| (k[k.len() - 1].c[i] - k[0].c[i]) / dt)
    };
    let m = seg.len().min(5);
    let (v0, v1) = (vel(&seg[..m]), vel(&seg[seg.len() - m..]));
    let (s0, s1) = (seg[0], seg[seg.len() - 1]);
    let mut out: Vec<Linha> = (1..=n)
        .rev()
        .filter(|k| s0.q - k >= 1)
        .map(|k| Linha { q: s0.q - k, c: std::array::from_fn(|i| s0.c[i] - v0[i] * k as f64), score: s0.score, origem: 2 })
        .collect();
    out.extend_from_slice(seg);
    out.extend((1..=n).map(|k| Linha { q: s1.q + k, c: std::array::from_fn(|i| s1.c[i] + v1[i] * k as f64), score: s1.score, origem: 2 }));
    out.retain(|l| l.c[2] > 0.0 && l.c[3] > 0.0 && l.c[0] < w && l.c[1] < h);
    out
}

/// Centro por média móvel; tamanho por MÁXIMO móvel (a caixa não encolhe por ruído).
fn suavizar(seg: &[Linha], janela: usize) -> Vec<Linha> {
    let n = seg.len();
    let m = janela / 2;
    (0..n)
        .map(|i| {
            let (a, b) = (i.saturating_sub(m), (i + m + 1).min(n));
            let k = &seg[a..b];
            let x = k.iter().map(|l| (l.c[0] + l.c[2]) / 2.0).sum::<f64>() / k.len() as f64;
            let y = k.iter().map(|l| (l.c[1] + l.c[3]) / 2.0).sum::<f64>() / k.len() as f64;
            let ww = k.iter().map(|l| l.c[2] - l.c[0]).fold(f64::MIN, f64::max);
            let hh = k.iter().map(|l| l.c[3] - l.c[1]).fold(f64::MIN, f64::max);
            Linha { c: [x - ww / 2.0, y - hh / 2.0, x + ww / 2.0, y + hh / 2.0], ..seg[i] }
        })
        .collect()
}

/// Margem de segurança: `fracao` do tamanho de cada lado, com piso em px; recorta ao quadro.
fn margem(seg: &[Linha], fracao: f64, minimo: f64, w: f64, h: f64) -> Vec<Linha> {
    seg.iter()
        .map(|l| {
            let (dx, dy) = (((l.c[2] - l.c[0]) * fracao).max(minimo), ((l.c[3] - l.c[1]) * fracao).max(minimo));
            Linha { c: [(l.c[0] - dx).clamp(0.0, w), (l.c[1] - dy).clamp(0.0, h), (l.c[2] + dx).clamp(0.0, w), (l.c[3] + dy).clamp(0.0, h)], ..*l }
        })
        .collect()
}

fn mediana(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 }
}

/// Das detecções por quadro analisado às propostas (`trilhas_de_redacao` da referência).
pub fn trilhas_de_redacao(dets: &[Vec<Deteccao>], fps: f64, w: f64, h: f64, p: &ParamsTrilhas) -> Vec<Proposta> {
    let seg1 = (dec::arred(fps).max(1)) as i64;
    let tr = rastrear(dets, p, seg1 as u32);
    let tr = costurar(tr, 2 * seg1, 1.5);
    let mut props = Vec::new();
    for t in &tr {
        if t.len() < p.min_quadros {
            continue;
        }
        for s in preencher(t, 2 * seg1) {
            if s.len() < p.min_quadros {
                continue;
            }
            let altura_mediana = mediana(s.iter().filter(|l| l.origem == 0).map(|l| l.c[3] - l.c[1]).collect());
            let s = estender(&s, dec::arred(0.25 * fps).max(1), w, h);
            props.push(Proposta { linhas: margem(&suavizar(&s, 5), p.margem, p.margem_px, w, h), altura_mediana });
        }
    }
    props
}

/// Cabeças abaixo disso piscam metade do tempo: não viram proposta (`modelos-redacao.md` §8 item 10).
pub const CABECA_MIN_PX: f64 = 12.0;
/// Rosto coberto por uma cabeça em todos os quadros: a cabeça já o oculta, a proposta do rosto sai.
pub const ROSTO_DENTRO_DA_CABECA: f64 = 0.95;

fn fracao_dentro(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let iw = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let ih = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let area = (a[2] - a[0]).max(0.0) * (a[3] - a[1]).max(0.0);
    if area <= 0.0 { 1.0 } else { iw * ih / area }
}

/// Tira as propostas de rosto que as de cabeça já cobrem em TODOS os quadros (sem perda de cobertura).
pub fn sem_rosto_coberto(rostos: Vec<Proposta>, cabecas: &[Proposta]) -> Vec<Proposta> {
    let por_quadro = |q: i64| cabecas.iter().filter_map(move |c| c.linhas.iter().find(|l| l.q == q));
    rostos
        .into_iter()
        .filter(|r| !r.linhas.iter().all(|l| por_quadro(l.q).any(|c| fracao_dentro(&l.c, &c.c) >= ROSTO_DENTRO_DA_CABECA)))
        .collect()
}

// ---------------------------------------------------------------- denso e rastreio de uma proposta

/// Denso da proposta: uma linha por quadro do trecho (`j0` = índice na tabela do quadro analisado 1), caixa em
/// 0..1 de `pixel`; detecção abaixo do limiar de trabalho = incerto; interpolada e estendida no `tipo`.
pub fn denso_da_proposta(p: &Proposta, tab: &TabelaQuadros, j0: usize, pixel: TamanhoPx, rastreio: &str, midia_sha256: &str, corrida: &Corrida) -> Result<ArquivoDenso> {
    let (w, h) = (pixel.w as f64, pixel.h as f64);
    let linhas: Vec<&Linha> = p.linhas.iter().filter(|l| l.q >= 1 && j0 + (l.q as usize - 1) < tab.pts.len()).collect();
    let (Some(a), Some(b)) = (linhas.first(), linhas.last()) else { return Err(SicroError::Validation(format!("{}: proposta vazia", codigo::INTERVALO))) };
    let mut d = ArquivoDenso {
        esquema: arquivo::ESQUEMA_DENSO,
        rastreio: rastreio.to_string(),
        midia_sha256: midia_sha256.to_string(),
        espaco: "original".into(),
        pixel,
        pts: Vec::new(),
        x: Vec::new(),
        y: Vec::new(),
        w: Vec::new(),
        h: Vec::new(),
        conf: Vec::new(),
        estado: Vec::new(),
        tipo: Vec::new(),
        corrida: Vec::new(),
        corridas: vec![serde_json::to_value(corrida)?],
    };
    // quadros que a extensão tirou no meio (fora da imagem) são interpolados entre os vizinhos
    let mut k = 0usize;
    for q in a.q..=b.q {
        while linhas[k].q < q {
            k += 1;
        }
        let l = linhas[k];
        let (c, score, tipo) = if l.q == q {
            (l.c, l.score, l.origem)
        } else {
            let p0 = linhas[k - 1];
            let t = (q - p0.q) as f64 / (l.q - p0.q) as f64;
            (std::array::from_fn(|i| p0.c[i] * (1.0 - t) + l.c[i] * t), p0.score.min(l.score), super::TIPO_INTERPOLADO)
        };
        d.pts.push(tab.pts[j0 + q as usize - 1]);
        d.x.push(c[0] / w);
        d.y.push(c[1] / h);
        d.w.push((c[2] - c[0]) / w);
        d.h.push((c[3] - c[1]) / h);
        d.conf.push(score);
        d.estado.push(if tipo == TIPO_MEDIDO && score < 0.5 { ESTADO_INCERTO } else { ESTADO_OK });
        d.tipo.push(tipo);
        d.corrida.push(0);
    }
    Ok(d)
}

/// Rótulo da classe no nome do rastreio (é dado: a lista o mostra; o relatório não).
fn rotulo(c: ClasseDeteccao) -> &'static str {
    match c {
        ClasseDeteccao::Rosto => "Rosto",
        ClasseDeteccao::Cabeca => "Cabeça",
        ClasseDeteccao::Placa => "Placa",
        ClasseDeteccao::Pessoa => "Corpo",
        ClasseDeteccao::Veiculo => "Veículo",
    }
}

fn cor(c: ClasseDeteccao) -> &'static str {
    match c {
        ClasseDeteccao::Rosto | ClasseDeteccao::Cabeca => "#ff5a5a",
        ClasseDeteccao::Placa => "#4fc3f7",
        ClasseDeteccao::Pessoa => "#ffb74d",
        ClasseDeteccao::Veiculo => "#81c784",
    }
}

/// Rastreio de uma proposta: origem `deteccao`, SEM confirmação (§6.6), denso já gravado.
pub fn rastreio_da_proposta(id: &str, classe: ClasseDeteccao, n: usize, midia_id: &str, denso: crate::edicao::modelo::Denso) -> Rastreio {
    Rastreio {
        id: id.to_string(),
        nome: format!("{} {n}", rotulo(classe)),
        cor: cor(classe).into(),
        midia: Some(midia_id.to_string()),
        base: BaseRastreio::Fonte,
        tipo: TipoRastreio::Caixa,
        espaco: "original".into(),
        ancoras: Vec::new(),
        denso: Some(denso),
        silhuetas: None,
        ausencias: Vec::new(),
        origem: OrigemRastreio::Deteccao { classe },
        confirmacao: None,
        ancoras_do_programa: Vec::new(),
        corridas_do_programa: Vec::new(),
    }
}

// ---------------------------------------------------------------- a tarefa (uma por vez)

/// O que a tela pede: o trecho do clipe (`clipe_id`, sempre no espaço original da mídia) e os alvos.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PedidoDetectar {
    pub clipe_id: String,
    pub classes: Vec<ClasseDeteccao>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvancoDeteccao {
    pub tarefa_id: String,
    pub feitos: u32,
    pub total: u32,
}

/// Fim: as propostas (rastreios SEM confirmação, denso gravado na pasta do projeto) da mídia `midia_id`; vazio quando
/// parou antes do fim (`cancelada`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FimDeteccao {
    pub tarefa_id: String,
    pub clipe_id: String,
    pub midia_id: String,
    pub rastreios: Vec<Rastreio>,
    pub cancelada: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FalhaDeteccao {
    pub tarefa_id: String,
    pub mensagem: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum EventoDeteccao {
    Avanco(AvancoDeteccao),
    Fim(FimDeteccao),
    Falhou(FalhaDeteccao),
}

pub type EmissorDeteccao = Box<dyn Fn(EventoDeteccao) + Send + Sync>;

static EM_CURSO: Mutex<Option<(String, Arc<AtomicBool>)>> = Mutex::new(None);

struct Vez(String);

impl Drop for Vez {
    fn drop(&mut self) {
        let mut g = EM_CURSO.lock().unwrap_or_else(|e| e.into_inner());
        if g.as_ref().is_some_and(|(t, _)| *t == self.0) {
            *g = None;
        }
    }
}

/// Para a detecção em andamento; tarefa que já acabou = `tarefa_ausente`.
pub fn cancelar(tarefa_id: &str) -> Result<()> {
    match EM_CURSO.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        Some((t, c)) if t == tarefa_id => {
            c.store(true, Ordering::SeqCst);
            Ok(())
        }
        _ => Err(SicroError::Validation(format!("{}: {tarefa_id}", codigo::TAREFA_AUSENTE))),
    }
}

fn trecho(p: &ProjetoEdicao, clipe_id: &str) -> Option<ClipeTrecho> {
    p.principal.iter().find_map(|c| match c {
        Tolerante::Conhecido(ClipePrincipal::Trecho(t)) if t.base.id == clipe_id => Some(t.clone()),
        _ => None,
    })
}

/// Começa a detecção em segundo plano e devolve o `tarefa_id`; os modelos abrem antes (falta de item ou interruptor
/// desligado voltam na hora). Outra em andamento = `rastreio_em_curso`. Não grava nada no projeto.
pub fn pedir(ws: &Path, cache: &Path, ai: &AiSettings, projeto: &Value, pedido: &PedidoDetectar, emitir: EmissorDeteccao) -> Result<String> {
    pedir_com(ws, cache, ai, projeto, pedido, emitir, &mut |ai, c| Detectores::novos(ai, c))
}

type Criar<'a> = &'a mut dyn FnMut(&AiSettings, &[ClasseDeteccao]) -> Result<Detectores>;

pub(crate) fn pedir_com(ws: &Path, cache: &Path, ai: &AiSettings, projeto: &Value, pedido: &PedidoDetectar, emitir: EmissorDeteccao, criar: Criar) -> Result<String> {
    let p = armazem::tipado(projeto)?;
    if !armazem::id_valido(&p.id) {
        return Err(SicroError::Validation(format!("{}: id", codigo::ITEM_INVALIDO)));
    }
    let clipe = trecho(&p, &pedido.clipe_id).ok_or_else(|| SicroError::Validation(format!("{}: clipe {}", codigo::ITEM_INVALIDO, pedido.clipe_id)))?;
    if pedido.classes.is_empty() {
        return Err(SicroError::Validation(format!("{}: classes", codigo::ITEM_INVALIDO)));
    }
    let ocupado = || SicroError::Validation(codigo::RASTREIO_EM_CURSO.into());
    if EM_CURSO.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
        return Err(ocupado());
    }
    let det = criar(ai, &pedido.classes)?;
    let tarefa = uuid::Uuid::new_v4().to_string();
    let cancelar = Arc::new(AtomicBool::new(false));
    {
        let mut g = EM_CURSO.lock().unwrap_or_else(|e| e.into_inner());
        if g.is_some() {
            return Err(ocupado());
        }
        *g = Some((tarefa.clone(), cancelar.clone()));
    }
    let vez = Vez(tarefa.clone());
    let t = Trabalho { ws: ws.to_path_buf(), cache: cache.to_path_buf(), p, clipe, pedido: pedido.clone(), tarefa: tarefa.clone(), cancelar };
    std::thread::Builder::new()
        .name("sicro-deteccao".into())
        .spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| trabalhar(&t, det, &emitir)));
            let fim = |rastreios: Vec<Rastreio>, cancelada: bool| {
                EventoDeteccao::Fim(FimDeteccao { tarefa_id: t.tarefa.clone(), clipe_id: t.clipe.base.id.clone(), midia_id: t.clipe.midia.clone(), rastreios, cancelada })
            };
            let ev = match r {
                Ok(Ok(v)) => fim(v, false),
                Ok(Err(e)) if executar::codigo_de(&e) == Some(ERRO_CANCELADO) => fim(Vec::new(), true),
                Ok(Err(e)) => falha(&t, e),
                Err(_) => falha(&t, SicroError::Validation(format!("{}: detecção", codigo::PANICO))),
            };
            drop(vez);
            emitir(ev);
        })
        .map_err(SicroError::Io)?;
    Ok(tarefa)
}

struct Trabalho {
    ws: PathBuf,
    cache: PathBuf,
    p: ProjetoEdicao,
    clipe: ClipeTrecho,
    pedido: PedidoDetectar,
    tarefa: String,
    cancelar: Arc<AtomicBool>,
}

fn falha(t: &Trabalho, e: SicroError) -> EventoDeteccao {
    let mensagem = match mensagens::erro_de_comando(e) {
        SicroError::Validation(m) => m,
        outro => outro.to_string(),
    };
    tracing::warn!("detecção {}: {mensagem}", t.tarefa);
    EventoDeteccao::Falhou(FalhaDeteccao { tarefa_id: t.tarefa.clone(), mensagem })
}

fn cancelado() -> SicroError {
    SicroError::Validation(format!("{ERRO_CANCELADO}: detecção"))
}

/// Lê o trecho do clipe da fonte (espaço original), detecta quadro a quadro, monta as trilhas e grava os densos.
fn trabalhar(t: &Trabalho, mut det: Detectores, emitir: &EmissorDeteccao) -> Result<Vec<Rastreio>> {
    let m = t.p.midias.iter().find(|m| m.id == t.clipe.midia).ok_or_else(|| SicroError::Validation(format!("{}: {}", codigo::MIDIA_AUSENTE, t.clipe.midia)))?;
    let (cap, mut fonte) = executar::preparar_uma(&t.ws, &t.p, &m.id, &t.cache, &t.cancelar)?;
    if fonte.quadros.origem != OrigemTabela::Decodificador {
        let tab = TabelaQuadros::do_decodificador(&cap, &fonte.caminho, &t.cancelar, &mut |_| {})?.guardar(&t.cache, &m.sha256)?;
        fonte = executar::fonte_info_memo(&cap, &fonte.caminho, &m.sha256, tab, m.cor.as_ref())?;
    }
    let tab = fonte.quadros.clone();
    let (fa, fb) = tempo::faixa_do_trecho(&tab, t.clipe.inicio_s, t.clipe.inicio_pts, t.clipe.fim_s, t.clipe.fim_pts)?;
    let n = fb - fa;
    if n > MAX_QUADROS as usize {
        return Err(SicroError::Validation(format!("{}: {n} quadros", codigo::INTERVALO)));
    }
    let pixel = TamanhoPx { w: fonte.exibido.0, h: fonte.exibido.1 };
    let classes = classes_das_trilhas(&t.pedido.classes);
    let mut dets: Vec<Vec<Vec<Deteccao>>> = vec![Vec::with_capacity(n); classes.len()];
    let mut leitor = super::corrida::LeitorPipe::abrir_com_cancelar(&cap, &fonte, tab.pts[fa], Some(tab.pts[fb - 1]), Sentido::Frente, pixel, Some(t.cancelar.clone()))?;
    let mut ultimo: Option<Instant> = None;
    let mut lidos = 0usize;
    while let Some((pts, q)) = leitor.proximo()? {
        if lidos >= n || pts != tab.pts[fa + lidos] {
            return Err(SicroError::Validation(format!("{ERRO_LEITURA_DIVERGENTE}: pts {pts}")));
        }
        let r = det.quadro(&q, &t.pedido.classes)?;
        for (c, d) in r {
            if let Some(i) = classes.iter().position(|x| *x == c) {
                dets[i].push(d);
            }
        }
        lidos += 1;
        let agora = Instant::now();
        if ultimo.is_none_or(|u| agora.duration_since(u) >= Duration::from_millis(1000 / AVANCOS_POR_S)) || lidos == n {
            ultimo = Some(agora);
            emitir(EventoDeteccao::Avanco(AvancoDeteccao { tarefa_id: t.tarefa.clone(), feitos: lidos as u32, total: n as u32 }));
        }
    }
    drop(leitor);
    if t.cancelar.load(Ordering::SeqCst) {
        return Err(cancelado());
    }
    if lidos != n {
        return Err(SicroError::Validation(format!("{ERRO_LEITURA_DIVERGENTE}: lidos {lidos} de {n} quadros")));
    }
    propostas_em_rastreios(t, &det, &classes, &dets, &tab, fa, pixel, &m.sha256)
}

#[allow(clippy::too_many_arguments)]
fn propostas_em_rastreios(
    t: &Trabalho,
    det: &Detectores,
    classes: &[ClasseDeteccao],
    dets: &[Vec<Vec<Deteccao>>],
    tab: &TabelaQuadros,
    fa: usize,
    pixel: TamanhoPx,
    sha: &str,
) -> Result<Vec<Rastreio>> {
    let fps = {
        let r = tab.taxa_nominal();
        r.num as f64 / r.den as f64
    };
    let (w, h) = (pixel.w as f64, pixel.h as f64);
    let mut por_classe: Vec<(ClasseDeteccao, Vec<Proposta>)> = classes
        .iter()
        .zip(dets)
        .map(|(&c, d)| {
            let mut v = trilhas_de_redacao(d, fps, w, h, &ParamsTrilhas::da_classe(c));
            if c == ClasseDeteccao::Cabeca {
                v.retain(|p| p.altura_mediana >= CABECA_MIN_PX);
            }
            (c, v)
        })
        .collect();
    let cabecas: Vec<Proposta> = por_classe.iter().find(|(c, _)| *c == ClasseDeteccao::Cabeca).map(|(_, v)| v.clone()).unwrap_or_default();
    for (c, v) in por_classe.iter_mut() {
        if *c == ClasseDeteccao::Rosto {
            *v = sem_rosto_coberto(std::mem::take(v), &cabecas);
        }
    }
    let pasta = armazem::pasta_projeto(&t.ws, &t.p.id);
    let feito_em = chrono::Utc::now().to_rfc3339();
    let seg1 = dec::arred(fps).max(1) as u32;
    let mut out = Vec::new();
    for (c, props) in por_classe {
        let mut props = props;
        props.sort_by_key(|p| p.linhas.first().map_or(0, |l| l.q));
        for (i, p) in props.iter().enumerate() {
            if t.cancelar.load(Ordering::SeqCst) {
                return Err(cancelado());
            }
            let id = uuid::Uuid::new_v4().to_string();
            let (Some(a), Some(b)) = (p.linhas.first(), p.linhas.last()) else { continue };
            let j = |q: i64| (fa + q as usize - 1).min(tab.pts.len() - 1);
            let corrida = Corrida {
                motor: MotorCorrida { motor: MotorRastreio::Deteccao, modelo: det.modelo(c), runtime: crate::ia::sessao::versao_runtime(), provedor: det.provedor.fio().into() },
                sentido: Sentido::Frente,
                ancora_pts: tab.pts[j(a.q)],
                de_pts: tab.pts[j(a.q)],
                ate_pts: tab.pts[j(b.q)],
                limiares: Limiares { ok: 0.5, perdido: 0.25, paciencia: seg1 },
                parou: Motivo::Fim,
                feito_em: feito_em.clone(),
            };
            let d = denso_da_proposta(p, tab, fa, pixel, &id, sha, &corrida)?;
            let ponteiro = arquivo::gravar(&pasta, &d, tab.tb)?;
            out.push(rastreio_da_proposta(&id, c, i + 1, &t.clipe.midia, ponteiro));
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "detectar_testes.rs"]
mod testes;
