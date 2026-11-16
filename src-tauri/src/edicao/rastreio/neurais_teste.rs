//! Banco dos testes do P2.3 (VitTrack e EdgeTAM). Quadros pelo ffmpeg dos testes (rgb24 pelo filtro bitexact: bytes
//! iguais no 9.0.1 e no 7.0.2); referência em `<SICRO_SEQUENCIAS>/ref_p23/`, gerada por `ref_p23/gerar.py` com o
//! código de `p3a/referencia/` sobre os MESMOS quadros (o SHA-256 de cada um vai junto); as 38 sequências; as
//! métricas de `p3a/src/laco_teste.py`; o laço de `laco_sicro.py` portado linha a linha (a conferência do `laco.rs`
//! sobre as saídas gravadas do motor real); e a sessão de GPU dos testes.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::AtomicBool;

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::super::laco::{CRESCE_BUSCA, BUSCA_MAX, P_INCERTO, P_PRESENTE, RAZAO_MAX, RAZAO_REENCONTRO, REENCONTRO_REL, SALTO_MAX, T_INCERTO, T_PERDA, T_REENCONTRO_MIN};
use super::super::{Busca, CaixaPx, Imagem, Laco, LeitorQuadros, PedidoCorrida, Rastreador, Sentido, ESTADO_AUSENTE, ESTADO_INCERTO, ESTADO_OK, MAX_QUADROS};
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::modelo::{Caixa, Chave, TamanhoPx};
use crate::edicao::testes::fontes;
use crate::ia::{self, Modelo, Opcoes, Preferencia, Provedor, SessaoIa};

/// Mesmo filtro de `edicao::testes::medir` e de `ref_p23/gerar.py`.
const FILTRO: &str = "scale=flags=bicubic+accurate_rnd+bitexact+full_chroma_int,format=rgb24";

/// Quadros de um vídeo, em ordem, por pipe.
pub struct Quadros {
    filho: Child,
    pub w: u32,
    pub h: u32,
    buf: Vec<u8>,
    /// Índice do quadro em `buf` (o último lido).
    pub k: usize,
}

impl Quadros {
    pub fn abrir(cap: &Capacidades, video: &Path) -> Quadros {
        let saida = Command::new(&cap.ffprobe)
            .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=p=0"])
            .arg(video)
            .output()
            .expect("ffprobe");
        let txt = String::from_utf8_lossy(&saida.stdout);
        let mut d = txt.trim().split(',').map(|v| v.trim().parse::<u32>().expect("tamanho do vídeo"));
        let (w, h) = (d.next().unwrap(), d.next().unwrap());
        assert!(w * h <= 4096 * 2160, "vídeo de teste grande demais: {w}x{h}");
        let filho = Command::new(&cap.binario)
            .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-i"])
            .arg(video)
            .args(["-map", "0:v:0", "-fps_mode", "passthrough", "-vf", FILTRO, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("ffmpeg");
        Quadros { filho, w, h, buf: vec![0; (w * h * 3) as usize], k: usize::MAX }
    }

    /// Próximo quadro e o índice dele (None = acabou).
    pub fn proximo(&mut self) -> Option<(usize, Imagem<'_>)> {
        let out = self.filho.stdout.as_mut()?;
        out.read_exact(&mut self.buf).ok()?;
        self.k = self.k.wrapping_add(1);
        Some((self.k, Imagem { rgb: &self.buf, w: self.w, h: self.h }))
    }

    /// Avança até o quadro `k` (inclusive); o pipe não volta.
    pub fn ate(&mut self, k: usize) -> Option<Imagem<'_>> {
        assert!(self.k == usize::MAX || k >= self.k, "quadro {k} já passou (no {})", self.k);
        while self.k.wrapping_add(1) < k {
            self.proximo()?;
        }
        if self.k != k {
            self.proximo()?;
        }
        Some(Imagem { rgb: &self.buf, w: self.w, h: self.h })
    }

    pub fn sha(&self) -> String {
        sha256(&self.buf)
    }
}

impl Drop for Quadros {
    fn drop(&mut self) {
        let _ = self.filho.kill();
        let _ = self.filho.wait();
    }
}

pub fn sha256(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}

pub fn caixa(v: &Value) -> Option<CaixaPx> {
    let a = v.as_array()?;
    let f = |i: usize| a[i].as_f64().unwrap();
    Some(CaixaPx { x: f(0), y: f(1), w: f(2), h: f(3) })
}

/// Pasta da referência; None = dispensa (falha com `SICRO_EXIGE_MODELOS=1`).
pub fn referencia(teste: &str) -> Option<PathBuf> {
    let p = ia::testes::sequencias().map(|s| s.join("ref_p23")).filter(|p| p.is_dir());
    ia::testes::ou_dispensa(p, teste, "p3a/ref_p23 (gerar.py)")
}

pub fn json(p: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).unwrap()
}

/// Lê um tensor float32 cru da referência.
pub fn f32s(p: &Path) -> Vec<f32> {
    std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())).chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()
}

pub fn dif_max(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max)
}

pub fn dif_caixa(a: &CaixaPx, b: &CaixaPx) -> f64 {
    [(a.x - b.x).abs(), (a.y - b.y).abs(), (a.w - b.w).abs(), (a.h - b.h).abs()].into_iter().fold(0.0, f64::max)
}

pub fn iou(a: &CaixaPx, b: &CaixaPx) -> f64 {
    let iw = ((a.x + a.w).min(b.x + b.w) - a.x.max(b.x)).max(0.0);
    let ih = ((a.y + a.h).min(b.y + b.h) - a.y.max(b.y)).max(0.0);
    let u = a.w * a.h + b.w * b.h - iw * ih;
    if u > 0.0 {
        iw * ih / u
    } else {
        0.0
    }
}

// ---------------------------------------------------------------- as 38 sequências

pub struct Gt {
    pub inteira: CaixaPx,
    pub fracao: f64,
}

pub struct Sequencia {
    pub nome: String,
    pub video: PathBuf,
    pub gt: Vec<Option<Gt>>,
}

impl Sequencia {
    /// Primeiro quadro com ≥ 95 % do alvo à vista (onde a bancada marca).
    pub fn ini(&self) -> usize {
        self.gt.iter().position(|g| g.as_ref().is_some_and(|g| g.fracao >= 0.95)).expect("sequência sem quadro inicial")
    }
}

/// As sequências na ordem de `bancada.sequencias()`: 16 sintéticas, depois 22 reais.
pub fn sequencias(teste: &str) -> Option<Vec<Sequencia>> {
    let raiz = ia::testes::ou_dispensa(ia::testes::sequencias(), teste, "as 38 sequências")?;
    let ler = |dir: &Path| -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir).map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect()).unwrap_or_default();
        v.sort();
        v
    };
    let gt = |j: &Value| -> Vec<Option<Gt>> {
        j["gt"].as_array().unwrap().iter().map(|g| (!g.is_null()).then(|| Gt { inteira: caixa(&g["inteira"]).unwrap(), fracao: g["fracao"].as_f64().unwrap() })).collect()
    };
    let mut s = Vec::new();
    for p in ler(&raiz.join("sint")) {
        let nome = p.file_stem().unwrap().to_string_lossy().into_owned();
        s.push(Sequencia { video: raiz.join("sint").join(format!("{nome}.mp4")), gt: gt(&json(&p)), nome });
    }
    for p in ler(&raiz.join("real/gt")) {
        let j = json(&p);
        s.push(Sequencia { nome: p.file_stem().unwrap().to_string_lossy().into_owned(), video: raiz.join(j["video"].as_str().unwrap()), gt: gt(&j) });
    }
    Some(s)
}

/// Uma linha de corrida, por índice de quadro.
#[derive(Debug, Clone)]
pub struct Amostra {
    pub indice: usize,
    pub caixa: Option<CaixaPx>,
    pub estado: u8,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Metricas {
    pub iou: f64,
    pub certa: u32,
    pub meio: u32,
    pub errada: u32,
    pub sem_vista: u32,
    pub sem_oculto: u32,
    pub incerto: u32,
}

/// As de `p3a/src/laco_teste.py` (quadro marcado fora; IoU médio nos quadros com ≥ 30 % à vista).
pub fn metricas(gt: &[Option<Gt>], ini: usize, am: &[Amostra]) -> Metricas {
    let por: std::collections::HashMap<usize, &Amostra> = am.iter().map(|a| (a.indice, a)).collect();
    let mut m = Metricas::default();
    let mut ious = Vec::new();
    for (k, g) in gt.iter().enumerate().skip(ini + 1) {
        let Some(g) = g else { continue };
        let vis = g.fracao >= 0.3;
        match por.get(&k).and_then(|a| a.caixa.map(|c| (c, a.estado))) {
            None => {
                if vis {
                    m.sem_vista += 1;
                    ious.push(0.0);
                } else {
                    m.sem_oculto += 1;
                }
            }
            Some((c, estado)) => {
                let i = if g.fracao > 0.0 { iou(&g.inteira, &c) } else { 0.0 };
                if vis {
                    ious.push(i);
                }
                if estado == ESTADO_INCERTO {
                    m.incerto += 1;
                }
                if vis && i >= 0.5 {
                    m.certa += 1;
                } else if i < 0.1 || g.fracao < 0.05 {
                    m.errada += 1;
                } else {
                    m.meio += 1;
                }
            }
        }
    }
    m.iou = if ious.is_empty() { 0.0 } else { ious.iter().sum::<f64>() / ious.len() as f64 };
    m
}

/// De onde o laço de referência tira os quadros (a gravação de uma corrida).
pub trait Fonte {
    /// Próximo quadro (índice); None = acabou.
    fn avancar(&mut self) -> Option<usize>;
    /// O motor no quadro atual.
    fn seguir(&mut self, r: &mut dyn Rastreador, b: &Busca) -> super::super::Passo;
}

/// O laço de `laco_sicro.py` (`seguir` e `seguir_por_memoria`), portado linha a linha, depois do quadro marcado
/// (o `iniciar` já foi); `paciencia` em quadros.
pub fn laco_referencia_de(r: &mut dyn Rastreador, q: &mut dyn Fonte, ini: usize, caixa0: CaixaPx, paciencia: u32) -> Vec<Amostra> {
    let mut am = vec![Amostra { indice: ini, caixa: Some(caixa0), estado: ESTADO_OK }];
    let mut caixa = caixa0;
    let mut tipica: Option<f64> = None;
    let mut perdidos = 0u32;
    let presenca = r.laco() == Laco::Presenca;
    loop {
        let Some(indice) = q.avancar() else { break };
        let (nova, conf, aceita);
        if presenca {
            let p = q.seguir(r, &Busca { caixa, janela: true });
            conf = p.conf;
            nova = p.caixa;
            aceita = nova.is_some() && conf >= P_PRESENTE;
        } else if perdidos == 0 {
            let p = q.seguir(r, &Busca { caixa, janela: true });
            conf = p.conf;
            nova = p.caixa;
            aceita = conf >= T_PERDA && plausivel(&nova.unwrap(), &caixa, RAZAO_MAX, SALTO_MAX);
        } else {
            let k = (1.0 + CRESCE_BUSCA * perdidos as f64).min(BUSCA_MAX);
            let (cx, cy) = (caixa.x + caixa.w / 2.0, caixa.y + caixa.h / 2.0);
            let larga = CaixaPx { x: cx - caixa.w * k / 2.0, y: cy - caixa.h * k / 2.0, w: caixa.w * k, h: caixa.h * k };
            let cand = q.seguir(r, &Busca { caixa: larga, janela: false }).caixa.unwrap();
            let (ccx, ccy) = (cand.x + cand.w / 2.0, cand.y + cand.h / 2.0);
            let centrada = CaixaPx { x: ccx - caixa.w / 2.0, y: ccy - caixa.h / 2.0, w: caixa.w, h: caixa.h };
            let p = q.seguir(r, &Busca { caixa: centrada, janela: true });
            conf = p.conf;
            nova = p.caixa;
            let limiar = T_REENCONTRO_MIN.max(REENCONTRO_REL * tipica.unwrap_or(0.6));
            aceita = conf >= limiar && plausivel(&nova.unwrap(), &caixa, RAZAO_REENCONTRO, 1e9);
        }
        if aceita {
            let nova = nova.unwrap();
            let estado = if conf >= if presenca { P_INCERTO } else { T_INCERTO } { ESTADO_OK } else { ESTADO_INCERTO };
            if !presenca {
                if perdidos == 0 {
                    tipica = Some(tipica.map_or(conf, |t| 0.9 * t + 0.1 * conf));
                }
                caixa = nova;
            }
            perdidos = 0;
            am.push(Amostra { indice, caixa: Some(nova), estado });
        } else {
            perdidos += 1;
            am.push(Amostra { indice, caixa: None, estado: ESTADO_AUSENTE });
            if perdidos >= paciencia {
                break;
            }
        }
    }
    while am.last().is_some_and(|a| a.caixa.is_none()) {
        am.pop();
    }
    am
}

/// Uma chamada a `seguir` gravada: a busca pedida e o passo (sem a silhueta: só o laço olha).
#[derive(Clone)]
pub struct Chamada {
    pub busca: Busca,
    pub passo: super::super::Passo,
}

/// O motor real com cada `seguir` gravado em ordem (quadros seguidos a partir do marcado).
pub struct Gravador<'a> {
    pub motor: &'a mut dyn Rastreador,
    pub chamadas: Vec<Chamada>,
}

impl Rastreador for Gravador<'_> {
    fn iniciar(&mut self, q: &Imagem, c: CaixaPx) -> crate::error::Result<super::super::Passo> {
        self.motor.iniciar(q, c)
    }
    fn seguir(&mut self, q: &Imagem, b: &Busca) -> crate::error::Result<super::super::Passo> {
        let p = self.motor.seguir(q, b)?;
        self.chamadas.push(Chamada { busca: *b, passo: super::super::Passo { silhueta: None, ..p.clone() } });
        Ok(p)
    }
    fn laco(&self) -> Laco {
        self.motor.laco()
    }
    fn limiares(&self, fps: f64) -> super::super::Limiares {
        self.motor.limiares(fps)
    }
    fn corrida(&self) -> super::super::MotorCorrida {
        self.motor.corrida()
    }
}

/// A gravação como fonte do laço de referência: quadros `ini+1, ini+2…`; cada `seguir` devolve a chamada gravada na
/// ordem e, no motor que usa a busca (reprocura), confere que a referência pediu a MESMA busca, bit a bit.
pub struct Repeticao<'a> {
    pub chamadas: &'a [Chamada],
    pub i: usize,
    pub quadro: usize,
    pub conferir_busca: bool,
    /// O laço pediu um quadro depois do último lido pela corrida.
    pub pediu_alem: bool,
    pub divergencia: Option<String>,
}

impl Fonte for Repeticao<'_> {
    fn avancar(&mut self) -> Option<usize> {
        if self.i >= self.chamadas.len() {
            self.pediu_alem = true;
            return None;
        }
        self.quadro += 1;
        Some(self.quadro)
    }
    fn seguir(&mut self, _: &mut dyn Rastreador, b: &Busca) -> super::super::Passo {
        let Some(c) = self.chamadas.get(self.i) else {
            self.divergencia.get_or_insert(format!("quadro {}: chamada a mais", self.quadro));
            return super::super::Passo { caixa: Some(b.caixa), conf: 0.0, silhueta: None };
        };
        self.i += 1;
        let igual = |a: &CaixaPx, r: &CaixaPx| [a.x, a.y, a.w, a.h].iter().zip([r.x, r.y, r.w, r.h]).all(|(x, y)| x.to_bits() == y.to_bits());
        if self.conferir_busca && (c.busca.janela != b.janela || !igual(&c.busca.caixa, &b.caixa)) {
            self.divergencia.get_or_insert(format!("quadro {}: busca {b:?}, a corrida pediu {:?}", self.quadro, c.busca));
        }
        c.passo.clone()
    }
}

fn plausivel(nova: &CaixaPx, velha: &CaixaPx, razao_max: f64, salto_max: f64) -> bool {
    let (rw, rh) = (nova.w / velha.w, nova.h / velha.h);
    if !((1.0 / razao_max..=razao_max).contains(&rw) && (1.0 / razao_max..=razao_max).contains(&rh)) {
        return false;
    }
    let dx = (nova.x + nova.w / 2.0) - (velha.x + velha.w / 2.0);
    let dy = (nova.y + nova.h / 2.0) - (velha.y + velha.h / 2.0);
    dx.hypot(dy) <= salto_max * velha.w.hypot(velha.h)
}

/// Leitor da corrida sobre `Quadros`: do quadro `ini` em diante, pts = índice do quadro.
struct Leitor<'a> {
    q: &'a mut Quadros,
    ini: usize,
    /// A corrida pediu um quadro depois do último do vídeo.
    acabou: bool,
}

impl LeitorQuadros for Leitor<'_> {
    fn proximo(&mut self) -> crate::error::Result<Option<(i64, Imagem<'_>)>> {
        let ini = self.ini;
        let r = if self.q.k == usize::MAX || self.q.k.wrapping_add(1) < ini {
            self.q.ate(ini).map(|i| (ini as i64, i))
        } else {
            self.q.proximo().map(|(k, i)| (k as i64, i))
        };
        self.acabou |= r.is_none();
        Ok(r)
    }
}

/// A corrida pelo `rastreio::correr` (laço do P2.2, o aceite) sobre o motor gravado; depois o laço de referência
/// (`laco_sicro.py` portado) roda sobre as MESMAS saídas do motor e tem de dar a mesma decisão em cada quadro, o mesmo
/// fim e (na reprocura) as mesmas buscas. Divergência = pânico com o quadro.
pub fn laco_do_sicro(r: &mut dyn Rastreador, q: &mut Quadros, ini: usize, caixa0: CaixaPx) -> Vec<Amostra> {
    let pixel = TamanhoPx { w: q.w, h: q.h };
    let p = PedidoCorrida {
        rastreio: "p23".into(),
        midia_sha256: "0".repeat(64),
        espaco: "original".into(),
        pixel,
        ancora: Chave { t: ini as f64 / 30.0, pts: Some(ini as i64), v: caixa0.caixa(pixel), interp: Default::default() },
        sentido: Sentido::Frente,
        ate_pts: None,
        max_quadros: MAX_QUADROS,
        fps: 30.0,
    };
    let mut g = Gravador { motor: r, chamadas: Vec::new() };
    let mut leitor = Leitor { q, ini, acabou: false };
    let res = super::super::correr(&p, &mut leitor, &mut g, &AtomicBool::new(false), &mut |_| {}).expect("rastreio::correr");
    let am: Vec<Amostra> = res
        .amostras
        .iter()
        .map(|a| Amostra { indice: a.pts as usize, caixa: a.caixa.map(|c| CaixaPx::de_caixa(&c, pixel)), estado: a.estado })
        .collect();
    // a referência sobre as mesmas saídas
    let paciencia = g.limiares(30.0).paciencia;
    let conferir_busca = g.laco() == Laco::Reprocura;
    let chamadas = std::mem::take(&mut g.chamadas);
    let mut rep = Repeticao { chamadas: &chamadas, i: 0, quadro: ini, conferir_busca, pediu_alem: false, divergencia: None };
    let ref_am = laco_referencia_de(&mut g, &mut rep, ini, CaixaPx::de_caixa(&p.ancora.v, pixel), paciencia);
    if let Some(d) = rep.divergencia {
        panic!("laço do SICRO × referência: {d}");
    }
    assert_eq!(rep.i, chamadas.len(), "a referência fez {} chamadas, a corrida {}", rep.i, chamadas.len());
    // parar pela paciência: nenhuma das duas pede outro quadro; chegar ao fim do vídeo (com o alvo à vista ou ainda
    // perdido, sem esgotar a paciência): as duas pedem
    let acabou = leitor.acabou;
    assert_eq!(rep.pediu_alem, acabou, "fim: corrida {:?}", res.parou);
    // com quadros perdidos no fim, "perdido" no primeiro deles; senão "fim" no último lido
    let aceito = res.amostras.last().map_or(ini, |a| a.pts as usize);
    let esperado = if aceito < rep.quadro { (super::super::Motivo::Perdido, aceito + 1) } else { (super::super::Motivo::Fim, rep.quadro) };
    assert_eq!((res.parou.motivo, res.parou.pts), (esperado.0, Some(esperado.1 as i64)), "fim do vídeo: {acabou}");
    // comparadas onde o laço grava, em 0..1, bit a bit: a caixa da referência (px do motor) passa pela mesma conta
    // (`CaixaPx::caixa`); voltar a gravada para px não serve (x / w · w difere de x em até 1 ULP); no quadro marcado
    // as duas são a âncora
    let bits = |c: Caixa| [c.x, c.y, c.w, c.h].map(f64::to_bits);
    let x: Vec<_> = res.amostras.iter().map(|a| (a.pts as usize, a.estado, a.caixa.map(bits))).collect();
    let y: Vec<_> = ref_am
        .iter()
        .map(|a| (a.indice, a.estado, a.caixa.map(|c| if a.indice == ini { p.ancora.v } else { c.caixa(pixel) }).map(bits)))
        .collect();
    if x != y {
        let k = x.iter().zip(&y).position(|(a, b)| a != b).unwrap_or(x.len().min(y.len()));
        panic!("laço do SICRO × referência: amostra {k} ({} × {}): {:?} × {:?}", x.len(), y.len(), x.get(k), y.get(k));
    }
    am
}

/// As 38 sequências com o laço `laco` sobre o motor que `motor()` cria: devolve as métricas somadas e imprime cada
/// sequência (o formato do `laco_teste.py`).
pub fn trinta_e_oito(
    teste: &str,
    motor: &mut dyn FnMut() -> Box<dyn Rastreador>,
    laco: fn(&mut dyn Rastreador, &mut Quadros, usize, CaixaPx) -> Vec<Amostra>,
) -> Option<(f64, Metricas)> {
    let cap = fontes::cap()?;
    let seqs = sequencias(teste)?;
    assert_eq!(seqs.len(), 38, "{teste}: sequências");
    let mut soma = Metricas::default();
    let mut ious = Vec::new();
    for s in &seqs {
        let ini = s.ini();
        let mut q = Quadros::abrir(&cap, &s.video);
        let mut r = motor();
        let t0 = std::time::Instant::now();
        let am = laco(r.as_mut(), &mut q, ini, s.gt[ini].as_ref().unwrap().inteira);
        let ms = t0.elapsed().as_secs_f64() * 1e3 / (s.gt.len() - ini).max(1) as f64;
        let m = metricas(&s.gt, ini, &am);
        eprintln!(
            "{:22} IoU {:.3} certa {:3} meio {:3} ERRADA {:3} | sem caixa: vista {:3} oculto {:3} | incerto {:3} | {} amostras | {ms:.1} ms/q",
            s.nome, m.iou, m.certa, m.meio, m.errada, m.sem_vista, m.sem_oculto, m.incerto, am.len()
        );
        ious.push(m.iou);
        soma.certa += m.certa;
        soma.meio += m.meio;
        soma.errada += m.errada;
        soma.sem_vista += m.sem_vista;
        soma.sem_oculto += m.sem_oculto;
        soma.incerto += m.incerto;
    }
    let media = ious.iter().sum::<f64>() / ious.len() as f64;
    eprintln!(
        "TOTAL: IoU médio {media:.4} | certa {} meio {} ERRADA {} | sem caixa à vista {} oculto {}",
        soma.certa, soma.meio, soma.errada, soma.sem_vista, soma.sem_oculto
    );
    Some((media, soma))
}

// ---------------------------------------------------------------- sessões

/// Sessão de CPU com a lib dos testes (`ia::testes::sessao_cpu`); None = dispensa.
pub fn cpu(modelo: Modelo<'_>, teste: &str) -> Option<SessaoIa> {
    ia::testes::ou_dispensa(ia::testes::sessao_cpu(modelo), teste, "a lib dos testes")
}

/// Sessão de GPU dos testes: `ia::criar_sessao_com` (P2.1) com o pacote dos testes e `Preferencia::Gpu`. None = sem
/// placa ou sem as libs (dispensa; falha com `SICRO_EXIGE_GPU=1`). Quem chama segura `ia::testes::trava_gpu()`
/// enquanto a sessão vive.
pub fn gpu(modelo: &Path, aquecer: Option<[usize; 4]>, teste: &str) -> Option<SessaoIa> {
    let pacote = ia::testes::ou_dispensa_gpu(ia::testes::pacote_gpu(), teste, "o pacote de GPU dos testes")?;
    let op = Opcoes { preferencia: Preferencia::Gpu, aquecer, ..Opcoes::default() };
    let s = ia::criar_sessao_com(&pacote.lib, Some(&pacote), Modelo::Arquivo(modelo), &op).map_err(|e| e.to_string());
    let s = match s {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("{teste}: GPU: {e}");
            None
        }
    };
    let s = ia::testes::ou_dispensa_gpu(s, teste, "uma placa NVIDIA que rode o CUDA")?;
    assert_eq!(s.provedor, Provedor::Cuda, "{teste}");
    Some(s)
}
