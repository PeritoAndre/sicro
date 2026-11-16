//! Motor `edgetam` (EdgeTAM vídeo, Meta, Apache-2.0; §6.2): 4 grafos (item "Rastreador" do Gerenciador, em
//! `AiSettings.rastreio_dir`), GPU com queda para CPU, caixa + silhueta + presença por quadro; laço por presença.
//! Porte de `p3a/referencia/rastreio_edgetam.py` (`modelos-rastreio.md` §6 e §10): quadro esticado para 1024×1024,
//! `input_labels` int32, banco de memória (condicionante + 6 quadros + 15 ponteiros), máscara de volta por bilinear
//! nos logits; as duas constantes que não vêm nos grafos embutidas. Dono: P2.3.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use ort::session::{OutputSelector, RunOptions};
use ort::value::{Tensor, TensorRef};
use sha2::{Digest, Sha256};

use super::laco::{PACIENCIA_MEMORIA_S, P_INCERTO, P_PRESENTE};
use super::vittrack::{conferir_caixa, conferir_quadro, eixo_inteiro, erro_ort, falha_do_motor, limitar_caixa, normalizar, paciencia};
use super::{ArquivoModelo, Busca, CaixaPx, Imagem, Laco, Limiares, Mascara, ModeloCorrida, MotorCorrida, MotorRastreio, Passo, Rastreador};
use crate::commands::settings_commands::AiSettings;
use crate::edicao::mensagens::codigo;
use crate::error::{Result, SicroError};
use crate::ia::{self, Opcoes, Preferencia, Provedor, SessaoIa};

/// `constantes_memoria.f32le` (2 816 bytes; SHA-256 `c69e56b1…a1ebb`), tiradas do checkpoint oficial.
pub const CONSTANTES: &[u8] = include_bytes!("edgetam_constantes.f32le");
pub const SHA256_CONSTANTES: &str = "c69e56b1d72fdc9104f9ccb4da175e1c698c3ce58305eb1888ae68b9706a1ebb";

/// Um arquivo do item do Gerenciador: nome, bytes e SHA-256 fixos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArquivoItem {
    pub nome: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

/// Os 4 grafos (65,2 MB), na pasta do item (`modelos-rastreio.md` §9).
pub const ARQUIVOS: [ArquivoItem; 4] = [
    ArquivoItem { nome: "vision_encoder.onnx", bytes: 19_826_101, sha256: "57fe1a2b3d500813fe4987c4209b856920f187ab0b7723d81586b607ea2cffc1" },
    ArquivoItem { nome: "mask_decoder.onnx", bytes: 17_800_677, sha256: "ee24ee1cae6ecc71889912c1893e08b09f645bb63875108ccd180f7218a3647d" },
    ArquivoItem { nome: "memory_attention.onnx", bytes: 20_914_958, sha256: "6477d775639905945949508c08a2c652d01e5933c0eb00a02225ae5f6aa1602c" },
    ArquivoItem { nome: "memory_encoder.onnx", bytes: 6_692_568, sha256: "035b30f8fb1f7c99211ada6eb9d72f0819de819ef94310b14e8147dc97f120ec" },
];
/// Fonte com commit fixo (terceiro: espelhar numa release do SICRO e deixar esta como segunda fonte, decisão do dono).
pub const URL_BASE: &str = "https://huggingface.co/jax-image-tools/edgetam-video-onnx/resolve/8ca3d3e4169938e65b552cdf14542fde25e8badb/";
pub const ORIGEM: &str = "jax-image-tools/edgetam-video-onnx";
pub const COMMIT: &str = "8ca3d3e4169938e65b552cdf14542fde25e8badb";
/// Desvio conhecido do export (§6.1, `modelos-rastreio.md` §6).
pub const DESVIO: &str = "melhor de 3 máscaras no quadro marcado";

/// Nome das constantes na corrida (o de `modelos/SHA256SUMS`).
const NOME_CONSTANTES: &str = "constantes_memoria.f32le";
const LADO: usize = 1024;
/// Memórias espaciais: a condicionante + 6 quadros; ponteiros: o condicionante + 15.
const N_MEM: usize = 7;
const MAX_PTR: usize = 16;
const TOKENS: usize = 512 * 64;

/// Memória de um quadro: tokens e posição (512×64 cada) e o ponteiro do objeto (256).
struct Memoria {
    tok: Vec<f32>,
    pos: Vec<f32>,
    ptr: Vec<f32>,
}

struct Codificado {
    f0: Vec<f32>,
    f1: Vec<f32>,
    f2: Vec<f32>,
    p2: Vec<f32>,
}

pub struct EdgeTam {
    ve: SessaoIa,
    md: SessaoIa,
    ma: SessaoIa,
    me: SessaoIa,
    no_mem: Vec<f32>,
    tpos: Vec<f32>,
    cond: Option<Memoria>,
    /// Mais nova no fim; no máximo `MAX_PTR`.
    recentes: VecDeque<Memoria>,
    /// Logits 1024×1024 do último quadro (a silhueta sai deles).
    logits: Vec<f32>,
}

fn ausente() -> SicroError {
    SicroError::Validation(format!("{}: edgetam", codigo::MODELO_AUSENTE))
}

fn pasta(ai: &AiSettings) -> Option<PathBuf> {
    (!ai.rastreio_dir.is_empty()).then(|| PathBuf::from(&ai.rastreio_dir))
}

fn sha256_confere(p: &Path, esperado: &str) -> bool {
    let Ok(mut f) = std::fs::File::open(p) else { return false };
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h).is_ok() && format!("{:x}", h.finalize()) == esperado
}

impl EdgeTam {
    /// Os 4 grafos de `ai.rastreio_dir` com o SHA-256 conferido; sessões por `ia::criar_sessao` (GPU com queda; os
    /// três últimos no provedor que o primeiro conseguiu). Item ausente ou alterado: `modelo_ausente: edgetam`.
    pub fn novo(ai: &AiSettings, op: &Opcoes) -> Result<Self> {
        if !ia::ligada(ai) {
            return Err(SicroError::Validation(ia::IA_DESLIGADA.into()));
        }
        let dir = pasta(ai).ok_or_else(ausente)?;
        if !ARQUIVOS.iter().all(|a| sha256_confere(&dir.join(a.nome), a.sha256)) {
            return Err(ausente());
        }
        let grafo = |i: usize| dir.join(ARQUIVOS[i].nome);
        let resto = |pref: Preferencia| -> Result<[SessaoIa; 3]> {
            let o = Opcoes { preferencia: pref, aquecer: None, ..op.clone() };
            Ok([ia::criar_sessao(ai, &grafo(1), &o)?, ia::criar_sessao(ai, &grafo(2), &o)?, ia::criar_sessao(ai, &grafo(3), &o)?])
        };
        // a falta do cuDNN e da memória de vídeo só aparece na 1ª inferência: aquece o codificador
        let ve = ia::criar_sessao(ai, &grafo(0), &Opcoes { aquecer: Some([1, 3, LADO, LADO]), ..op.clone() })?;
        if ve.provedor == Provedor::Cpu {
            let [md, ma, me] = resto(Preferencia::Cpu)?;
            return Ok(Self::de_sessoes(ve, md, ma, me));
        }
        match resto(Preferencia::Gpu) {
            Ok([md, ma, me]) => Ok(Self::de_sessoes(ve, md, ma, me)),
            Err(e) if op.preferencia == Preferencia::Gpu => Err(e),
            // o codificador coube na GPU e o resto não: tudo em CPU, um provedor só por corrida
            Err(e) => {
                tracing::warn!("edgetam: só o codificador coube na GPU, corrida em CPU: {e}");
                drop(ve);
                let ve = ia::criar_sessao(ai, &grafo(0), &Opcoes { preferencia: Preferencia::Cpu, aquecer: None, ..op.clone() })?;
                let [md, ma, me] = resto(Preferencia::Cpu)?;
                Ok(Self::de_sessoes(ve, md, ma, me))
            }
        }
    }

    /// Os 4 arquivos estão em `ai.rastreio_dir` com o tamanho certo (sem conferir o SHA-256: isso é no `novo`).
    pub fn instalado(ai: &AiSettings) -> bool {
        pasta(ai).is_some_and(|d| ARQUIVOS.iter().all(|a| std::fs::metadata(d.join(a.nome)).is_ok_and(|m| m.is_file() && m.len() == a.bytes)))
    }

    /// Sessões já criadas, na ordem de `ARQUIVOS`, todas no mesmo provedor.
    pub(crate) fn de_sessoes(ve: SessaoIa, md: SessaoIa, ma: SessaoIa, me: SessaoIa) -> Self {
        debug_assert!([&md, &ma, &me].iter().all(|s| s.provedor == ve.provedor));
        let k: Vec<f32> = CONSTANTES.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        Self {
            ve,
            md,
            ma,
            me,
            no_mem: k[..256].to_vec(),
            tpos: k[256..256 + N_MEM * 64].to_vec(),
            cond: None,
            recentes: VecDeque::new(),
            logits: Vec::new(),
        }
    }

    fn codificar(&mut self, q: &Imagem) -> Result<Codificado> {
        let pv = self.ve.provedor;
        let x = Tensor::from_array(([1usize, 3, LADO, LADO], para_1024(q))).map_err(erro_ort(pv))?;
        // pos0 e pos1 (80 MB) não são usados
        let op = RunOptions::new().map_err(erro_ort(pv))?.with_outputs(OutputSelector::no_default().with("feats0").with("feats1").with("feats2").with("pos2"));
        let s = self.ve.session.run_with_options(ort::inputs!["pixel_values" => x], &op).map_err(erro_ort(pv))?;
        let tira = |nome: &str, n: usize| -> Result<Vec<f32>> {
            let (_, v) = s[nome].try_extract_tensor::<f32>().map_err(erro_ort(pv))?;
            forma(v.len() == n, nome)?;
            Ok(v.to_vec())
        };
        Ok(Codificado { f0: tira("feats0", 32 * 256 * 256)?, f1: tira("feats1", 64 * 128 * 128)?, f2: tira("feats2", 256 * 4096)?, p2: tira("pos2", 256 * 4096)? })
    }

    /// Decodificador de máscara: guarda os logits 1024×1024 e devolve (ponteiro, logit da presença).
    fn decodificar(&mut self, c: &Codificado, feats2: &[f32], pontos: &[f32], rotulos: &[i32]) -> Result<(Vec<f32>, f32)> {
        let pv = self.ve.provedor;
        let n = rotulos.len();
        let op = RunOptions::new()
            .map_err(erro_ort(pv))?
            .with_outputs(OutputSelector::no_default().with("high_res_masks").with("object_pointer").with("object_score_logits"));
        let s = self
            .md
            .session
            .run_with_options(
                ort::inputs![
                    "feats0" => TensorRef::from_array_view(([1usize, 32, 256, 256], &c.f0[..])).map_err(erro_ort(pv))?,
                    "feats1" => TensorRef::from_array_view(([1usize, 64, 128, 128], &c.f1[..])).map_err(erro_ort(pv))?,
                    "feats2" => TensorRef::from_array_view(([1usize, 256, 64, 64], feats2)).map_err(erro_ort(pv))?,
                    "input_points" => TensorRef::from_array_view(([1usize, 1, n, 2], pontos)).map_err(erro_ort(pv))?,
                    "input_labels" => TensorRef::from_array_view(([1usize, 1, n], rotulos)).map_err(erro_ort(pv))?,
                ],
                &op,
            )
            .map_err(erro_ort(pv))?;
        let (_, hr) = s["high_res_masks"].try_extract_tensor::<f32>().map_err(erro_ort(pv))?;
        let (_, ptr) = s["object_pointer"].try_extract_tensor::<f32>().map_err(erro_ort(pv))?;
        let (_, obj) = s["object_score_logits"].try_extract_tensor::<f32>().map_err(erro_ort(pv))?;
        forma(hr.len() == LADO * LADO && ptr.len() == 256 && obj.len() == 1, "mask_decoder")?;
        self.logits.clear();
        self.logits.extend_from_slice(hr);
        Ok((ptr.to_vec(), obj[0]))
    }

    /// Codificador de memória sobre os logits guardados: (tokens, posição), 512×64 cada.
    fn memorizar(&mut self, f2: &[f32], de_prompt: bool) -> Result<(Vec<f32>, Vec<f32>)> {
        let pv = self.ve.provedor;
        let s = self
            .me
            .session
            .run(ort::inputs![
                "vision_features" => TensorRef::from_array_view(([1usize, 256, 64, 64], f2)).map_err(erro_ort(pv))?,
                "pred_masks_high_res" => TensorRef::from_array_view(([1usize, 1, LADO, LADO], &self.logits[..])).map_err(erro_ort(pv))?,
                "is_mask_from_pts" => Tensor::from_array(([1usize], vec![de_prompt])).map_err(erro_ort(pv))?,
            ])
            .map_err(erro_ort(pv))?;
        let (_, tok) = s["memory_tokens"].try_extract_tensor::<f32>().map_err(erro_ort(pv))?;
        let (_, pos) = s["memory_pos_enc"].try_extract_tensor::<f32>().map_err(erro_ort(pv))?;
        forma(tok.len() == TOKENS && pos.len() == TOKENS, "memory_encoder")?;
        Ok((tok.to_vec(), pos.to_vec()))
    }

    /// Atenção sobre o banco (`_prepare_memory_conditioned_features` do SAM 2): devolve as feições 1×256×64×64.
    fn atencao(&mut self, c: &Codificado) -> Result<Vec<f32>> {
        let pv = self.ve.provedor;
        let cond = self.cond.as_ref().ok_or_else(|| SicroError::Validation(format!("{}: edgetam sem iniciar", codigo::ITEM_INVALIDO)))?;
        let ultimas: Vec<&Memoria> = self.recentes.iter().skip(self.recentes.len().saturating_sub(N_MEM - 1)).collect();
        let mut mem = Vec::with_capacity(TOKENS * (1 + ultimas.len()));
        let mut mpos = Vec::with_capacity(mem.capacity());
        mem.extend_from_slice(&cond.tok);
        somar_tpos(&mut mpos, &cond.pos, &self.tpos[(N_MEM - 1) * 64..N_MEM * 64]);
        for (i, m) in ultimas.iter().enumerate() {
            let atras = ultimas.len() - i;
            mem.extend_from_slice(&m.tok);
            somar_tpos(&mut mpos, &m.pos, &self.tpos[(atras - 1) * 64..atras * 64]);
        }
        let mut ptrs = cond.ptr.clone();
        for m in self.recentes.iter().rev().take(MAX_PTR - 1) {
            ptrs.extend_from_slice(&m.ptr);
        }
        let zeros = vec![0f32; ptrs.len()];
        let (l, p) = (mem.len() / 64, ptrs.len() / 64);
        let s = self
            .ma
            .session
            .run(ort::inputs![
                "current_vision_features" => Tensor::from_array(([4096usize, 1, 256], transpor(&c.f2, 256, 4096))).map_err(erro_ort(pv))?,
                "current_vision_position_embeddings" => Tensor::from_array(([4096usize, 1, 256], transpor(&c.p2, 256, 4096))).map_err(erro_ort(pv))?,
                "spatial_memory" => TensorRef::from_array_view(([l, 1, 64], &mem[..])).map_err(erro_ort(pv))?,
                "spatial_memory_position_embeddings" => TensorRef::from_array_view(([l, 1, 64], &mpos[..])).map_err(erro_ort(pv))?,
                "pointer_memory" => TensorRef::from_array_view(([p, 1, 64], &ptrs[..])).map_err(erro_ort(pv))?,
                "pointer_memory_position_embeddings" => TensorRef::from_array_view(([p, 1, 64], &zeros[..])).map_err(erro_ort(pv))?,
            ])
            .map_err(erro_ort(pv))?;
        let (_, out) = s["conditioned_features"].try_extract_tensor::<f32>().map_err(erro_ort(pv))?;
        forma(out.len() == 4096 * 256, "memory_attention")?;
        // (1, 4096, 256) por posição → (1, 256, 64, 64)
        Ok(transpor(out, 4096, 256))
    }

    /// Caixa, presença e silhueta do último quadro (`_resultado` + `mascara_no_quadro`).
    fn resultado(&self, logit: f32, w: u32, h: u32) -> Passo {
        let logit = logit as f64;
        let presenca = 1.0 / (1.0 + (-logit).exp());
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
        for (y, linha) in self.logits.chunks_exact(LADO).enumerate() {
            if let Some(a) = linha.iter().position(|v| *v > 0.0) {
                let b = LADO - 1 - linha.iter().rev().position(|v| *v > 0.0).unwrap();
                (x0, x1, y0, y1) = (x0.min(a), x1.max(b), y0.min(y), y);
            }
        }
        if logit <= 0.0 || x0 == usize::MAX {
            return Passo { caixa: None, conf: presenca, silhueta: None };
        }
        let (sx, sy) = (w as f64 / LADO as f64, h as f64 / LADO as f64);
        let c = CaixaPx { x: x0 as f64 * sx, y: y0 as f64 * sy, w: (x1 - x0 + 1) as f64 * sx, h: (y1 - y0 + 1) as f64 * sy };
        Passo { caixa: Some(limitar_caixa(c, w, h)), conf: presenca, silhueta: Some(mascara(&self.logits, w, h)) }
    }
}

impl Rastreador for EdgeTam {
    fn iniciar(&mut self, quadro: &Imagem, caixa: CaixaPx) -> Result<Passo> {
        conferir_quadro(quadro)?;
        conferir_caixa(&caixa)?;
        let c = self.codificar(quadro)?;
        let mut f2m = c.f2.clone();
        for (canal, bloco) in f2m.chunks_exact_mut(4096).enumerate() {
            bloco.iter_mut().for_each(|v| *v += self.no_mem[canal]);
        }
        let (sx, sy) = (LADO as f64 / quadro.w as f64, LADO as f64 / quadro.h as f64);
        // caixa = 2 pontos (cantos), rótulos 2 e 3
        let pontos = [(caixa.x * sx) as f32, (caixa.y * sy) as f32, ((caixa.x + caixa.w) * sx) as f32, ((caixa.y + caixa.h) * sy) as f32];
        let (ptr, logit) = self.decodificar(&c, &f2m, &pontos, &[2, 3])?;
        let (tok, pos) = self.memorizar(&c.f2, true)?;
        self.cond = Some(Memoria { tok, pos, ptr });
        self.recentes.clear();
        Ok(self.resultado(logit, quadro.w, quadro.h))
    }

    /// Ele mesmo reencontra o alvo: a `busca` do laço não entra.
    fn seguir(&mut self, quadro: &Imagem, _busca: &Busca) -> Result<Passo> {
        conferir_quadro(quadro)?;
        let c = self.codificar(quadro)?;
        let feats = self.atencao(&c)?;
        // sem prompt: um ponto de enchimento com rótulo −1
        let (ptr, logit) = self.decodificar(&c, &feats, &[0.0, 0.0], &[-1])?;
        let (tok, pos) = self.memorizar(&c.f2, false)?;
        self.recentes.push_back(Memoria { tok, pos, ptr });
        while self.recentes.len() > MAX_PTR {
            self.recentes.pop_front();
        }
        Ok(self.resultado(logit, quadro.w, quadro.h))
    }

    fn laco(&self) -> Laco {
        Laco::Presenca
    }

    fn limiares(&self, fps: f64) -> Limiares {
        Limiares { ok: P_INCERTO, perdido: P_PRESENTE, paciencia: paciencia(PACIENCIA_MEMORIA_S, fps) }
    }

    fn corrida(&self) -> MotorCorrida {
        let mut arquivos: Vec<ArquivoModelo> = ARQUIVOS.iter().map(|a| ArquivoModelo { nome: a.nome.into(), sha256: a.sha256.into() }).collect();
        arquivos.push(ArquivoModelo { nome: NOME_CONSTANTES.into(), sha256: SHA256_CONSTANTES.into() });
        MotorCorrida {
            motor: MotorRastreio::Edgetam,
            modelo: ModeloCorrida { nome: "EdgeTAM vídeo".into(), origem: ORIGEM.into(), commit: COMMIT.into(), arquivos, desvio: DESVIO.into() },
            runtime: ia::sessao::versao_runtime(),
            provedor: self.ve.provedor.fio().into(),
        }
    }
}

fn forma(ok: bool, grafo: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(falha_do_motor(Provedor::Cpu, &format!("saída de {grafo} com forma inesperada")))
    }
}

/// `pos + tpos[k]` (512×64 + 64, em f32), acrescentado ao fim de `dst`.
fn somar_tpos(dst: &mut Vec<f32>, pos: &[f32], t: &[f32]) {
    for linha in pos.chunks_exact(64) {
        dst.extend(linha.iter().zip(t).map(|(a, b)| a + b));
    }
}

/// Matriz `l`×`c` (por linhas) transposta para `c`×`l`.
fn transpor(m: &[f32], l: usize, c: usize) -> Vec<f32> {
    let mut o = vec![0f32; l * c];
    for (i, linha) in m.chunks_exact(c).enumerate() {
        for (j, v) in linha.iter().enumerate() {
            o[j * l + i] = *v;
        }
    }
    o
}

/// O quadro inteiro esticado para 1024×1024 (escalas independentes em x e y), bilinear sem antisserrilhamento com
/// centro em +0,5 e borda repetida, ImageNet, NCHW (`_para_1024`). Primeiro as linhas, depois as colunas: a mesma
/// ordem de contas do numpy.
fn para_1024(q: &Imagem) -> Vec<f32> {
    let (w, h) = (q.w as usize, q.h as usize);
    let (ex, ey) = (eixo_inteiro(w, LADO), eixo_inteiro(h, LADO));
    // horizontal em cada linha da fonte: hz[y][j][c]
    let mut hz = vec![0f32; h * LADO * 3];
    for y in 0..h {
        let linha = &q.rgb[y * w * 3..(y + 1) * w * 3];
        let dst = &mut hz[y * LADO * 3..(y + 1) * LADO * 3];
        for j in 0..LADO {
            let (a, b, f) = (ex.a[j] as usize * 3, ex.b[j] as usize * 3, ex.f[j]);
            for c in 0..3 {
                dst[j * 3 + c] = linha[a + c] as f32 * (1.0 - f) + linha[b + c] as f32 * f;
            }
        }
    }
    let plano = LADO * LADO;
    let mut o = vec![0f32; 3 * plano];
    for i in 0..LADO {
        let (a, b, f) = (ey.a[i] as usize, ey.b[i] as usize, ey.f[i]);
        let (topo, base) = (&hz[a * LADO * 3..(a + 1) * LADO * 3], &hz[b * LADO * 3..(b + 1) * LADO * 3]);
        for j in 0..LADO {
            for c in 0..3 {
                o[c * plano + i * LADO + j] = normalizar(topo[j * 3 + c] * (1.0 - f) + base[j * 3 + c] * f, c);
            }
        }
    }
    o
}

/// Logits 1024×1024 → máscara `w`×`h` por bilinear nos logits e corte em 0 (`mascara_no_quadro`).
fn mascara(logits: &[f32], w: u32, h: u32) -> Mascara {
    let (wu, hu) = (w as usize, h as usize);
    let (ex, ey) = (eixo_inteiro(LADO, wu), eixo_inteiro(LADO, hu));
    let mut hz = vec![0f32; LADO * wu];
    for (y, linha) in logits.chunks_exact(LADO).enumerate() {
        let dst = &mut hz[y * wu..(y + 1) * wu];
        for j in 0..wu {
            let f = ex.f[j];
            dst[j] = linha[ex.a[j] as usize] * (1.0 - f) + linha[ex.b[j] as usize] * f;
        }
    }
    let mut bits = vec![0u8; wu * hu];
    for i in 0..hu {
        let (a, b, f) = (ey.a[i] as usize, ey.b[i] as usize, ey.f[i]);
        let (topo, base) = (&hz[a * wu..(a + 1) * wu], &hz[b * wu..(b + 1) * wu]);
        for j in 0..wu {
            bits[i * wu + j] = u8::from(topo[j] * (1.0 - f) + base[j] * f > 0.0);
        }
    }
    Mascara { w, h, bits }
}

#[cfg(test)]
mod testes {
    use super::super::vittrack::banco::{self, Quadros};
    use super::*;
    use crate::edicao::testes::fontes;
    use crate::ia::{testes as t, Modelo};

    const IOU_MASCARA_MIN: f64 = 0.99;
    const IOU_MIN_38: f64 = 0.85;
    const ERRADAS_MAX_38: u32 = 11;
    /// Diferença tolerada contra a referência de CPU, no mesmo provedor (medida: ver o relato do teste).
    const TOL_PRESENCA: f64 = 1e-3;
    const TOL_CAIXA_PX: f64 = 0.5;
    /// GPU × CPU (TF32 desligado): a máscara não é bit a bit a da CPU (`modelos-rastreio.md` §1).
    const TOL_PRESENCA_GPU: f64 = 0.01;
    const TOL_CAIXA_GPU_PX: f64 = 1.0;

    fn grafos(teste: &str) -> Option<[PathBuf; 4]> {
        let p: Option<Vec<PathBuf>> = ARQUIVOS.iter().map(|a| t::modelo(&format!("rastreio/edgetam_video/{}", a.nome), a.sha256)).collect();
        t::ou_dispensa(p, teste, "os grafos do EdgeTAM").map(|v| v.try_into().unwrap())
    }

    fn motor_cpu(teste: &str) -> Option<EdgeTam> {
        let g = grafos(teste)?;
        let mut s = g.iter().map(|p| banco::cpu(Modelo::Arquivo(p), teste));
        Some(EdgeTam::de_sessoes(s.next()??, s.next()??, s.next()??, s.next()??))
    }

    /// Quem chama segura `trava_gpu()` enquanto o motor vive.
    fn motor_gpu(teste: &str) -> Option<EdgeTam> {
        let g = grafos(teste)?;
        let ve = banco::gpu(&g[0], Some([1, 3, LADO, LADO]), teste)?;
        Some(EdgeTam::de_sessoes(ve, banco::gpu(&g[1], None, teste)?, banco::gpu(&g[2], None, teste)?, banco::gpu(&g[3], None, teste)?))
    }

    fn iou_bits(a: &[u8], b: &[u8]) -> f64 {
        let (mut i, mut u) = (0u64, 0u64);
        for (x, y) in a.iter().zip(b) {
            i += u64::from(*x & *y);
            u += u64::from(*x | *y);
        }
        if u == 0 {
            1.0
        } else {
            i as f64 / u as f64
        }
    }

    /// Máscara `n` da referência (np.packbits, primeiro pixel no bit mais alto) como 0/1 por pixel.
    fn mascara_ref(bin: &[u8], n: usize, px: usize) -> Vec<u8> {
        let por = px.div_ceil(8);
        let b = &bin[n * por..(n + 1) * por];
        (0..px).map(|i| (b[i / 8] >> (7 - i % 8)) & 1).collect()
    }

    #[derive(Default, Debug)]
    struct Comparacao {
        ms_por_quadro: f64,
        quadros: usize,
        com_alvo: usize,
        iou_min: f64,
        pior_presenca: f64,
        pior_caixa: f64,
    }

    /// O rastreador cru (iniciar no 1º quadro do trecho, seguir nos outros) contra `ref_p23/edgetam/<seq>`.
    fn comparar(m: &mut EdgeTam, cap: &crate::edicao::ffmpeg::sonda::Capacidades, dir: &Path, seq: &banco::Sequencia, tol_p: f64, tol_c: f64) -> Comparacao {
        let r = banco::json(&dir.join(format!("edgetam/{}.json", seq.nome)));
        let bin = std::fs::read(dir.join(format!("edgetam/{}.mascaras.bin", seq.nome))).unwrap();
        let (de, linhas) = (r["de"].as_u64().unwrap() as usize, r["linhas"].as_array().unwrap());
        let shas = r["quadros"].as_array().unwrap();
        let mut q = Quadros::abrir(cap, &seq.video);
        let px = (q.w * q.h) as usize;
        let mut c = Comparacao { iou_min: 1.0, ..Default::default() };
        let t0 = std::time::Instant::now();
        for (n, l) in linhas.iter().enumerate() {
            let k = l["q"].as_u64().unwrap() as usize;
            assert_eq!(k, de + n);
            let quadro = q.ate(k).unwrap();
            let p = if n == 0 {
                m.iniciar(&quadro, banco::caixa(&r["caixa0"]).unwrap()).unwrap()
            } else {
                m.seguir(&quadro, &Busca { caixa: CaixaPx { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }, janela: true }).unwrap()
            };
            assert_eq!(q.sha(), shas[n].as_str().unwrap(), "{} quadro {k}: diferente do da referência", seq.nome);
            let ref_m = mascara_ref(&bin, n, px);
            let dp = (p.conf - l["presenca"].as_f64().unwrap()).abs();
            c.pior_presenca = c.pior_presenca.max(dp);
            assert!(dp <= tol_p, "{} quadro {k}: presença {} × {}", seq.nome, p.conf, l["presenca"]);
            match (p.caixa, banco::caixa(&l["caixa"])) {
                (Some(a), Some(b)) => {
                    let s = p.silhueta.as_ref().expect("silhueta com a caixa");
                    assert_eq!((s.w, s.h), (q.w, q.h));
                    let i = iou_bits(&s.bits, &ref_m);
                    c.iou_min = c.iou_min.min(i);
                    c.pior_caixa = c.pior_caixa.max(banco::dif_caixa(&a, &b));
                    assert!(i >= IOU_MASCARA_MIN, "{} quadro {k}: IoU da máscara {i}", seq.nome);
                    assert!(banco::dif_caixa(&a, &b) <= tol_c, "{} quadro {k}: caixa {a:?} × {b:?}", seq.nome);
                    c.com_alvo += 1;
                }
                (None, None) => {
                    assert!(p.silhueta.is_none());
                    assert!(ref_m.iter().all(|v| *v == 0), "{} quadro {k}: sem alvo e a referência tem máscara", seq.nome);
                }
                (a, b) => panic!("{} quadro {k}: alvo {a:?} × referência {b:?}", seq.nome),
            }
            c.quadros += 1;
        }
        c.ms_por_quadro = t0.elapsed().as_secs_f64() * 1e3 / c.quadros as f64;
        c
    }

    /// Um quadro contra a referência, tensor a tensor: entrada esticada, feições, logits do quadro marcado e do
    /// seguinte, memória e ponteiro do condicionante.
    #[test]
    fn primeiro_quadro_igual_a_referencia() {
        const T: &str = "primeiro_quadro_igual_a_referencia";
        let (Some(dir), Some(cap), Some(mut m), Some(seqs)) = (banco::referencia(T), fontes::cap(), motor_cpu(T), banco::sequencias(T)) else { return };
        let dir = dir.join("tensores");
        let r = banco::json(&dir.join("etm.json"));
        let s = seqs.iter().find(|s| s.nome == r["seq"].as_str().unwrap()).unwrap();
        let k = r["q"].as_u64().unwrap() as usize;
        let mut q = Quadros::abrir(&cap, &s.video);
        let q0 = q.ate(k).unwrap();
        let d_px = banco::dif_max(&para_1024(&q0), &banco::f32s(&dir.join("etm_pixel_values.f32")));
        let c = m.codificar(&q0).unwrap();
        let d_f2 = banco::dif_max(&c.f2, &banco::f32s(&dir.join("etm_feats2.f32")));
        let d_p2 = banco::dif_max(&c.p2, &banco::f32s(&dir.join("etm_pos2.f32")));
        m.iniciar(&q0, s.gt[k].as_ref().unwrap().inteira).unwrap();
        let d_hr0 = banco::dif_max(&m.logits, &banco::f32s(&dir.join("etm_hr0.f32")));
        let cond = m.cond.as_ref().unwrap();
        let (d_tok, d_ptr) = (banco::dif_max(&cond.tok, &banco::f32s(&dir.join("etm_mem_tok0.f32"))), banco::dif_max(&cond.ptr, &banco::f32s(&dir.join("etm_ptr0.f32"))));
        let q1 = q.ate(k + 1).unwrap();
        let p = m.seguir(&q1, &Busca { caixa: CaixaPx { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }, janela: true }).unwrap();
        let d_hr1 = banco::dif_max(&m.logits, &banco::f32s(&dir.join("etm_hr1.f32")));
        let d_p = (p.conf - r["presenca1"].as_f64().unwrap()).abs();
        eprintln!("{T}: entrada {d_px:e} feats2 {d_f2:e} pos2 {d_p2:e} logits0 {d_hr0:e} memória {d_tok:e} ponteiro {d_ptr:e} logits1 {d_hr1:e} presença {d_p:e}");
        assert_eq!(d_px, 0.0, "entrada esticada");
        assert!(d_f2 <= 1e-3 && d_p2 <= 1e-3, "feições {d_f2} {d_p2}");
        assert!(d_hr0 <= 0.05 && d_hr1 <= 0.05 && d_tok <= 1e-2 && d_ptr <= 1e-3, "logits {d_hr0} {d_hr1} memória {d_tok} ponteiro {d_ptr}");
        assert!(d_p <= 1e-4, "presença {d_p}");
    }

    /// Três trechos (oclusão total com o alvo sumindo e voltando, real degradado, sintético 960×540): máscara com
    /// IoU ≥ 0,99 contra a referência em todo quadro com alvo; sem alvo nos mesmos quadros.
    #[test]
    fn mascara_iou_099_contra_a_referencia() {
        const T: &str = "mascara_iou_099_contra_a_referencia";
        let (Some(dir), Some(cap), Some(seqs)) = (banco::referencia(T), fontes::cap(), banco::sequencias(T)) else { return };
        let (mut com_alvo, mut sem_alvo) = (0, 0);
        for nome in ["oclusao_total_cftv", "c02_p1_cftv", "escala"] {
            let Some(mut m) = motor_cpu(T) else { return };
            let s = seqs.iter().find(|s| s.nome == nome).unwrap();
            let c = comparar(&mut m, &cap, &dir, s, TOL_PRESENCA, TOL_CAIXA_PX);
            eprintln!("{T}: {nome}: {c:?}");
            com_alvo += c.com_alvo;
            sem_alvo += c.quadros - c.com_alvo;
        }
        assert!(com_alvo >= 90 && sem_alvo >= 40, "{com_alvo} quadros com alvo, {sem_alvo} sem");
    }

    /// A corrida grava motor, provedor e todos os arquivos do modelo (4 grafos e as constantes) com SHA-256, origem,
    /// commit e o desvio; laço e limiares do motor na taxa da mídia.
    #[test]
    fn corrida_grava_provedor_e_arquivos() {
        assert_eq!(banco::sha256(CONSTANTES), SHA256_CONSTANTES);
        let Some(m) = motor_cpu("corrida_grava_provedor_e_arquivos") else { return };
        let c = m.corrida();
        assert_eq!((c.motor, c.provedor.as_str(), c.runtime.as_str()), (MotorRastreio::Edgetam, "cpu", ia::sessao::versao_runtime().as_str()));
        let nomes: Vec<&str> = c.modelo.arquivos.iter().map(|a| a.nome.as_str()).collect();
        assert_eq!(nomes, ["vision_encoder.onnx", "mask_decoder.onnx", "memory_attention.onnx", "memory_encoder.onnx", "constantes_memoria.f32le"]);
        for (a, g) in c.modelo.arquivos.iter().zip(grafos("corrida_grava_provedor_e_arquivos").unwrap()) {
            assert!(t::conferir_sha256(&g, &a.sha256), "{}", a.nome);
        }
        assert_eq!(c.modelo.arquivos[4].sha256, SHA256_CONSTANTES);
        assert_eq!((c.modelo.origem.as_str(), c.modelo.commit.as_str(), c.modelo.desvio.as_str()), (ORIGEM, COMMIT, DESVIO));
        // como fica no denso (§6.1): `Corrida` achatada
        let corrida = super::super::Corrida {
            motor: c.clone(),
            sentido: super::super::Sentido::Frente,
            ancora_pts: 0,
            de_pts: 0,
            ate_pts: 10,
            limiares: m.limiares(30.0),
            parou: super::super::Motivo::Fim,
            feito_em: "2026-11-15T00:00:00Z".into(),
        };
        let j = serde_json::to_value(&corrida).unwrap();
        assert_eq!((j["motor"].as_str(), j["provedor"].as_str()), (Some("edgetam"), Some("cpu")));
        assert_eq!(j["modelo"]["arquivos"].as_array().unwrap().len(), 5);
        assert_eq!(j["limiares"], serde_json::json!({ "ok": 0.95, "perdido": 0.5, "paciencia": 90 }));
        assert_eq!(m.laco(), Laco::Presenca);
        assert_eq!((m.limiares(25.0).paciencia, m.limiares(29.97).paciencia), (75, 90));
    }

    /// O grafo do dossiê na pasta do teste sem copiar 65 MB (o job do Windows compila os testes: lá, cópia).
    fn ligar(origem: &Path, destino: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(origem, destino).unwrap();
        #[cfg(not(unix))]
        std::fs::copy(origem, destino).map(|_| ()).unwrap();
    }

    /// `novo` confere o SHA-256 de cada grafo antes de abrir sessão; `instalado` só olha nome e tamanho.
    #[test]
    fn sha256_conferido() {
        const T: &str = "sha256_conferido";
        let Some(g) = grafos(T) else { return };
        let Some(base) = t::ambiente() else { return };
        let pasta = fontes::pasta_temporaria("p23-sha256");
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        let ai = AiSettings { rastreio_dir: pasta.to_string_lossy().into_owned(), ..base.clone() };
        let codigo_de = |r: Result<EdgeTam>| match r {
            Ok(_) => "ok".to_string(),
            Err(SicroError::Validation(m)) => m,
            Err(e) => e.to_string(),
        };
        // vazio
        assert!(!EdgeTam::instalado(&AiSettings { rastreio_dir: String::new(), ..base.clone() }));
        assert_eq!(codigo_de(EdgeTam::novo(&AiSettings { rastreio_dir: String::new(), ..base.clone() }, &Opcoes::default())), "modelo_ausente: edgetam");
        // falta um
        for p in &g[..3] {
            ligar(p, &pasta.join(p.file_name().unwrap()));
        }
        assert!(!EdgeTam::instalado(&ai));
        assert_eq!(codigo_de(EdgeTam::novo(&ai, &Opcoes::default())), "modelo_ausente: edgetam");
        // cortado (download interrompido): nem instalado
        let mut b = std::fs::read(&g[3]).unwrap();
        std::fs::write(pasta.join(ARQUIVOS[3].nome), &b[..b.len() - 1]).unwrap();
        assert!(!EdgeTam::instalado(&ai));
        assert_eq!(codigo_de(EdgeTam::novo(&ai, &Opcoes::default())), "modelo_ausente: edgetam");
        // um byte trocado, mesmo tamanho: instalado, mas `novo` recusa antes de abrir sessão
        let meio = b.len() / 2;
        b[meio] ^= 0x40;
        std::fs::write(pasta.join(ARQUIVOS[3].nome), &b).unwrap();
        assert!(EdgeTam::instalado(&ai));
        assert_eq!(codigo_de(EdgeTam::novo(&ai, &Opcoes::default())), "modelo_ausente: edgetam");
        // os 4 certos: abre as sessões (CPU, a lib dos testes)
        std::fs::remove_file(pasta.join(ARQUIVOS[3].nome)).unwrap();
        ligar(&g[3], &pasta.join(ARQUIVOS[3].nome));
        assert!(EdgeTam::instalado(&ai));
        assert_eq!(codigo_de(EdgeTam::novo(&ai, &Opcoes::so_cpu())), "ok");
        // interruptor desligado: nem lê os arquivos
        assert_eq!(codigo_de(EdgeTam::novo(&AiSettings { modelos_desligados: true, ..ai.clone() }, &Opcoes::default())), "ia_desligada");
        std::fs::remove_dir_all(&pasta).unwrap();
    }

    /// O mesmo trecho com oclusão total em GPU (CUDA, TF32 desligado) contra a referência de CPU: mesmo alvo nos
    /// mesmos quadros, máscara com IoU ≥ 0,99; a corrida grava `cuda`.
    #[test]
    fn gpu_e_cpu() {
        const T: &str = "gpu_e_cpu";
        let (Some(dir), Some(cap), Some(seqs)) = (banco::referencia(T), fontes::cap(), banco::sequencias(T)) else { return };
        let _trava = t::trava_gpu();
        let Some(mut m) = motor_gpu(T) else { return };
        assert_eq!(m.corrida().provedor, "cuda");
        let s = seqs.iter().find(|s| s.nome == "oclusao_total_cftv").unwrap();
        let c = comparar(&mut m, &cap, &dir, s, TOL_PRESENCA_GPU, TOL_CAIXA_GPU_PX);
        eprintln!("{T}: {c:?}");
        assert!(c.com_alvo >= 25 && c.quadros - c.com_alvo >= 40, "{c:?}");
    }

    /// Falha do onnxruntime no meio da corrida (aqui: um grafo errado no lugar do codificador) sai com código do
    /// catálogo, nunca com o texto do onnxruntime: `panico` em CPU, `gpu_*` (regra do P2.1) na GPU. A frase que a
    /// tela mostra é a do catálogo.
    #[test]
    fn erro_de_inferencia_pelo_catalogo() {
        const T: &str = "erro_de_inferencia_pelo_catalogo";
        let q = vec![90u8; 64 * 48 * 3];
        let quadro = Imagem { rgb: &q, w: 64, h: 48 };
        let caixa = CaixaPx { x: 10.0, y: 10.0, w: 20.0, h: 20.0 };
        let mensagem = |r: Result<Passo>| match r {
            Err(SicroError::Validation(m)) => m,
            outro => panic!("{outro:?}"),
        };
        assert_eq!(super::super::vittrack::falha_do_motor(Provedor::Cuda, "CUDA failure 2: out of memory").to_string(), SicroError::Validation(codigo::GPU_SEM_MEMORIA.into()).to_string());
        assert_eq!(
            crate::edicao::mensagens::erro_de_comando(super::super::vittrack::falha_do_motor(Provedor::Cuda, "CUDA failure 2: out of memory")).to_string(),
            SicroError::Validation("gpu_sem_memoria: Memória de vídeo insuficiente".into()).to_string()
        );
        let Some(g) = grafos(T) else { return };
        let Some(vit) = t::modelo("rastreio/vittrack/object_tracking_vittrack_2023sep.onnx", super::super::vittrack::SHA256) else {
            t::ou_dispensa::<()>(None, T, "o VitTrack em arquivo");
            return;
        };
        let cpu = |p: &Path| banco::cpu(Modelo::Arquivo(p), T);
        let (Some(ve), Some(md), Some(ma), Some(me)) = (cpu(&vit), cpu(&g[1]), cpu(&g[2]), cpu(&g[3])) else { return };
        let m = mensagem(EdgeTam::de_sessoes(ve, md, ma, me).iniciar(&quadro, caixa));
        assert_eq!(m, codigo::PANICO);
        let _trava = t::trava_gpu();
        let Some(ve) = banco::gpu(&vit, None, T) else { return };
        let (Some(md), Some(ma), Some(me)) = (banco::gpu(&g[1], None, T), banco::gpu(&g[2], None, T), banco::gpu(&g[3], None, T)) else { return };
        let m = mensagem(EdgeTam::de_sessoes(ve, md, ma, me).iniciar(&quadro, caixa));
        assert!(m.starts_with("gpu_") && !m.contains(':') && crate::edicao::mensagens::nivel(&m).is_some(), "{m}");
    }

    /// O mesmo motor nos dois lados de "ambos" (corrida.rs): o `iniciar` recomeça o banco de memória, e a segunda
    /// corrida dá o mesmo que um motor novo (CPU).
    #[test]
    fn iniciar_de_novo_e_motor_novo() {
        const T: &str = "iniciar_de_novo_e_motor_novo";
        let (Some(mut usado), Some(mut novo)) = (motor_cpu(T), motor_cpu(T)) else { return };
        let (w, h) = (128, 96);
        let q: Vec<Vec<u8>> = (0..6).map(|k| super::super::vittrack::testes::quadro_sintetico(k, w, h)).collect();
        let im = |k: usize| Imagem { rgb: &q[k], w, h };
        let c = CaixaPx { x: 40.0, y: 30.0, w: 24.0, h: 20.0 };
        let b = Busca { caixa: c, janela: true };
        usado.iniciar(&im(0), CaixaPx { x: 10.0, ..c }).unwrap();
        for k in 1..3 {
            usado.seguir(&im(k), &b).unwrap();
        }
        let corre = |m: &mut EdgeTam| -> Vec<Passo> {
            let mut v = vec![m.iniciar(&im(3), c).unwrap()];
            v.extend((4..6).map(|k| m.seguir(&im(k), &b).unwrap()));
            v
        };
        assert_eq!(corre(&mut usado), corre(&mut novo));
    }

    /// `runtime` da corrida pela lib carregada (P2.1).
    #[test]
    fn runtime_gravado() {
        let Some(m) = motor_cpu("runtime_gravado") else { return };
        assert_eq!(m.corrida().runtime, "onnxruntime 1.31.1");
        let Some(v) = banco::cpu(Modelo::Bytes(super::super::vittrack::MODELO), "runtime_gravado") else { return };
        assert_eq!(super::super::vittrack::VitTrack::de_sessao(v).corrida().runtime, "onnxruntime 1.31.1");
    }

    /// Aceite do §11 nas 38 sequências, em GPU, pelo `rastreio::correr` (laço do P2.2); na mesma rodada, o laço de
    /// `laco_sicro.py` portado roda sobre as MESMAS saídas do motor e tem de decidir igual em cada quadro (banco).
    #[test]
    fn trinta_e_oito_sequencias_com_laco() {
        const T: &str = "trinta_e_oito_sequencias_com_laco";
        let _trava = t::trava_gpu();
        if motor_gpu(T).is_none() {
            return;
        }
        let mut novo = || -> Box<dyn Rastreador> { Box::new(motor_gpu(T).unwrap()) };
        let Some((iou, m)) = banco::trinta_e_oito(T, &mut novo, banco::laco_do_sicro) else { return };
        assert!(iou >= IOU_MIN_38, "{T}: IoU médio {iou}");
        assert!(m.errada <= ERRADAS_MAX_38, "{T}: {} caixas erradas aceitas", m.errada);
    }
}
