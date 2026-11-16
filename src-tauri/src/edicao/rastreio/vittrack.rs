//! Motor `vittrack` (OpenCV Zoo, Apache-2.0; §6.2): 715 kB embutidos no binário, sempre no provedor de CPU, 2 a 5 ms
//! por quadro; laço por reprocura. Porte de `p3a/referencia/rastreio_vittrack.py` (pontos em `modelos-rastreio.md`
//! §10: RGB, ImageNet, bilinear com centro em +0,5, recorte `ceil(sqrt(w·h)·fator)`, Hann de 16). Dono: P2.3.
//! As contas seguem a referência operação por operação (f32 onde o numpy usa f32, f64 onde usa f64, arredondamento
//! do `round` do Python): mesmos quadros dão os mesmos tensores.

use ort::value::{Tensor, TensorRef};

use super::laco::{PACIENCIA_S, T_INCERTO, T_PERDA};
use super::{ArquivoModelo, Busca, CaixaPx, Imagem, Laco, Limiares, ModeloCorrida, MotorCorrida, MotorRastreio, Passo, Rastreador, MAX_QUADROS};
use crate::commands::settings_commands::AiSettings;
use crate::edicao::mensagens::codigo;
use crate::error::{Result, SicroError};
use crate::ia::{self, Opcoes, Provedor, SessaoIa};

/// `object_tracking_vittrack_2023sep.onnx` (o instalador leva; nada a baixar).
pub const MODELO: &[u8] = include_bytes!("vittrack_2023sep.onnx");
pub const NOME_ARQUIVO: &str = "object_tracking_vittrack_2023sep.onnx";
pub const SHA256: &str = "2990f0b7cd44d92afa48cd97db6de7be113fc1d9594fddb74e2725c10478e91d";
pub const ORIGEM: &str = "huggingface.co/opencv/object_tracking_vittrack";
pub const COMMIT: &str = "868e4c941984c6363594ec34ee1c3ba0b75378a3";

const FATOR_TEMPLATE: f64 = 2.0;
const LADO_TEMPLATE: usize = 128;
const FATOR_BUSCA: f64 = 4.0;
const LADO_BUSCA: usize = 256;
const GRADE: usize = 16;

pub struct VitTrack {
    sessao: SessaoIa,
    hann: [f32; GRADE * GRADE],
    /// 1×3×128×128 do quadro marcado; vazio antes de `iniciar`.
    template: Vec<f32>,
}

impl VitTrack {
    /// Sessão em CPU (`Opcoes::so_cpu`) pelo onnxruntime instalado (`ia::criar_sessao_de_bytes`). Sem onnxruntime:
    /// `modelo_ausente: onnxruntime`; interruptor desligado: `ia_desligada`.
    pub fn novo(ai: &AiSettings) -> Result<Self> {
        Ok(Self::de_sessao(ia::criar_sessao_de_bytes(ai, MODELO, &Self::opcoes())?))
    }

    /// GPU não ajuda: o grafo é de 1,5 ms e o custo é o recorte (`modelos-rastreio.md` §5).
    pub(crate) fn opcoes() -> Opcoes {
        Opcoes::so_cpu()
    }

    pub(crate) fn de_sessao(sessao: SessaoIa) -> Self {
        Self { sessao, hann: hann(), template: Vec::new() }
    }
}

impl Rastreador for VitTrack {
    fn iniciar(&mut self, quadro: &Imagem, caixa: CaixaPx) -> Result<Passo> {
        conferir_quadro(quadro)?;
        conferir_caixa(&caixa)?;
        let (t, ..) = recorte(quadro, &caixa, FATOR_TEMPLATE, LADO_TEMPLATE);
        self.template = t;
        Ok(Passo { caixa: Some(caixa), conf: 1.0, silhueta: None })
    }

    fn seguir(&mut self, quadro: &Imagem, busca: &Busca) -> Result<Passo> {
        conferir_quadro(quadro)?;
        conferir_caixa(&busca.caixa)?;
        if self.template.is_empty() {
            return Err(SicroError::Validation(format!("{}: vittrack sem iniciar", codigo::ITEM_INVALIDO)));
        }
        let (busca_t, x0, y0, lado) = recorte(quadro, &busca.caixa, FATOR_BUSCA, LADO_BUSCA);
        let pv = self.sessao.provedor;
        let template = TensorRef::from_array_view(([1usize, 3, LADO_TEMPLATE, LADO_TEMPLATE], &self.template[..])).map_err(erro_ort(pv))?;
        let busca_t = Tensor::from_array(([1usize, 3, LADO_BUSCA, LADO_BUSCA], busca_t)).map_err(erro_ort(pv))?;
        let saida = self.sessao.session.run(ort::inputs!["template" => template, "search" => busca_t]).map_err(erro_ort(pv))?;
        let conf = saida["output1"].try_extract_tensor::<f32>().map_err(erro_ort(pv))?.1;
        let tam = saida["output2"].try_extract_tensor::<f32>().map_err(erro_ort(pv))?.1;
        let des = saida["output3"].try_extract_tensor::<f32>().map_err(erro_ort(pv))?.1;
        let n = GRADE * GRADE;
        if conf.len() != n || tam.len() != 2 * n || des.len() != 2 * n {
            return Err(falha_do_motor(Provedor::Cpu, "saída do vittrack com forma inesperada"));
        }
        // argmax devolve o primeiro máximo, como o numpy
        let (mut k, mut melhor) = (0usize, f32::NEG_INFINITY);
        for (i, &c) in conf.iter().enumerate() {
            let v = if busca.janela { c * self.hann[i] } else { c };
            if v > melhor {
                (k, melhor) = (i, v);
            }
        }
        let (iy, ix) = (k / GRADE, k % GRADE);
        let cx = (ix as f64 + des[k] as f64) / GRADE as f64;
        let cy = (iy as f64 + des[n + k] as f64) / GRADE as f64;
        let (w, h) = (tam[k] as f64, tam[n + k] as f64);
        let lado = lado as f64;
        let c = CaixaPx { x: (cx - w / 2.0) * lado + x0 as f64, y: (cy - h / 2.0) * lado + y0 as f64, w: w * lado, h: h * lado };
        Ok(Passo { caixa: Some(limitar_caixa(c, quadro.w, quadro.h)), conf: melhor as f64, silhueta: None })
    }

    fn laco(&self) -> Laco {
        Laco::Reprocura
    }

    fn limiares(&self, fps: f64) -> Limiares {
        Limiares { ok: T_INCERTO, perdido: T_PERDA, paciencia: paciencia(PACIENCIA_S, fps) }
    }

    fn corrida(&self) -> MotorCorrida {
        MotorCorrida {
            motor: MotorRastreio::Vittrack,
            modelo: ModeloCorrida {
                nome: "VitTrack".into(),
                origem: ORIGEM.into(),
                commit: COMMIT.into(),
                arquivos: vec![ArquivoModelo { nome: NOME_ARQUIVO.into(), sha256: SHA256.into() }],
                desvio: String::new(),
            },
            runtime: ia::sessao::versao_runtime(),
            provedor: self.sessao.provedor.fio().into(),
        }
    }
}

// ---------------------------------------------------------------- preparo comum (`p3a/referencia/comum.py`)

const MEDIA: [f32; 3] = [0.485, 0.456, 0.406];
const DESVIO: [f32; 3] = [0.229, 0.224, 0.225];

/// Falha do motor no meio da corrida: o código do catálogo vai para a tela (na GPU, `gpu_*` pela regra única do P2.1;
/// em CPU, `panico`) e o texto do onnxruntime, que chega em inglês e com vários kB, vai para o log.
pub(super) fn falha_do_motor(provedor: Provedor, texto: &str) -> SicroError {
    let cod = match provedor {
        Provedor::Cuda => ia::gpu::motivo(texto),
        Provedor::Cpu => codigo::PANICO,
    };
    tracing::warn!(target: "rastreio", "onnxruntime ({}): {cod}: {texto}", provedor.fio());
    SicroError::Validation(cod.into())
}

/// `map_err` das chamadas ao onnxruntime de uma sessão no `provedor`.
pub(super) fn erro_ort<E: std::fmt::Display>(provedor: Provedor) -> impl Fn(E) -> SicroError {
    move |e| falha_do_motor(provedor, &e.to_string())
}

/// Quadro com o tamanho que diz ter (o leitor entrega rgb24 sem folga).
pub(super) fn conferir_quadro(q: &Imagem) -> Result<()> {
    if q.w == 0 || q.h == 0 || q.rgb.len() as u64 != q.w as u64 * q.h as u64 * 3 {
        return Err(SicroError::Validation(format!("{}: quadro {}x{} com {} bytes", codigo::ITEM_INVALIDO, q.w, q.h, q.rgb.len())));
    }
    Ok(())
}

pub(super) fn conferir_caixa(c: &CaixaPx) -> Result<()> {
    let finita = [c.x, c.y, c.w, c.h].iter().all(|v| v.is_finite());
    if !finita || c.w <= 0.0 || c.h <= 0.0 {
        return Err(SicroError::Validation(format!("{}: caixa {c:?}", codigo::ITEM_INVALIDO)));
    }
    Ok(())
}

/// Paciência em quadros na taxa da mídia (2 s a 30 qps = 60); taxa inválida vale 30 qps.
pub(super) fn paciencia(segundos: f64, fps: f64) -> u32 {
    let fps = if fps.is_finite() && fps > 0.0 { fps } else { 30.0 };
    (segundos * fps).round().clamp(1.0, MAX_QUADROS as f64) as u32
}

/// `limitar_caixa`: tamanho mínimo de 2 px e pelo menos 4 px da caixa dentro do quadro.
pub(super) fn limitar_caixa(c: CaixaPx, w_quadro: u32, h_quadro: u32) -> CaixaPx {
    const MARGEM: f64 = 4.0;
    let (w, h) = (c.w.max(2.0), c.h.max(2.0));
    let x = c.x.max(MARGEM - w).min(w_quadro as f64 - MARGEM);
    let y = c.y.max(MARGEM - h).min(h_quadro as f64 - MARGEM);
    CaixaPx { x, y, w, h }
}

/// Janela de Hann 16×16 em f32, como `comum.hann2d`: h[i] = 0,5·(1 − cos(2π(i+1)/17)).
fn hann() -> [f32; GRADE * GRADE] {
    let dois_pi = (2.0 * std::f64::consts::PI) as f32;
    let h: Vec<f32> = (1..=GRADE).map(|i| 0.5f32 * (1.0f32 - (dois_pi * i as f32 / (GRADE + 1) as f32).cos())).collect();
    let mut o = [0f32; GRADE * GRADE];
    for (a, ha) in h.iter().enumerate() {
        for (b, hb) in h.iter().enumerate() {
            o[a * GRADE + b] = ha * hb;
        }
    }
    o
}

/// Quadrado de lado `ceil(sqrt(w·h)·fator)` centrado na caixa, canto pelo `round` do Python (metade para o par),
/// levado a `saida`×`saida` e normalizado. Devolve o tensor NCHW e o recorte no quadro (x0, y0, lado).
fn recorte(q: &Imagem, c: &CaixaPx, fator: f64, saida: usize) -> (Vec<f32>, i64, i64, i64) {
    // teto muito acima de qualquer quadro: caixa absurda (finita) não estoura o i64 em `eixo`
    const LIM: f64 = (1u64 << 40) as f64;
    let lado = ((c.w * c.h).sqrt() * fator).ceil().clamp(1.0, LIM) as i64;
    let x0 = (c.x + c.w / 2.0 - lado as f64 / 2.0).round_ties_even().clamp(-LIM, LIM) as i64;
    let y0 = (c.y + c.h / 2.0 - lado as f64 / 2.0).round_ties_even().clamp(-LIM, LIM) as i64;
    (amostrar_quadrado(q, x0, y0, lado, saida), x0, y0, lado)
}

/// Um eixo do bilinear: índices de amostra e peso, centro do pixel em +0,5, coordenada presa em [lo, hi].
pub(super) struct Eixo {
    pub a: Vec<i64>,
    pub b: Vec<i64>,
    pub f: Vec<f32>,
}

/// `u = x0 + (j + 0,5)·lado/saida − 0,5`, presa em [x0, x0 + lado − 1] (`amostrar_quadrado`) ou, com `x0 = 0`, em
/// [0, n − 1] (`redimensionar_bilinear`, `_para_1024`).
pub(super) fn eixo(x0: i64, lado: i64, saida: usize) -> Eixo {
    let e = lado as f64 / saida as f64;
    let (lo, hi) = (x0 as f64, (x0 + lado - 1) as f64);
    let mut r = Eixo { a: Vec::with_capacity(saida), b: Vec::with_capacity(saida), f: Vec::with_capacity(saida) };
    for j in 0..saida {
        let u = (x0 as f64 + (j as f64 + 0.5) * e - 0.5).max(lo).min(hi);
        let a = u.floor() as i64;
        r.a.push(a);
        r.b.push((a + 1).min(x0 + lado - 1));
        r.f.push((u - a as f64) as f32);
    }
    r
}

/// Igual ao `eixo` de `redimensionar_bilinear` (o numpy calcula `(j + 0,5)·n/saida`, não `·(n/saida)`).
pub(super) fn eixo_inteiro(n: usize, saida: usize) -> Eixo {
    let hi = (n - 1) as f64;
    let mut r = Eixo { a: Vec::with_capacity(saida), b: Vec::with_capacity(saida), f: Vec::with_capacity(saida) };
    for j in 0..saida {
        let u = ((j as f64 + 0.5) * n as f64 / saida as f64 - 0.5).max(0.0).min(hi);
        let a = u.floor() as i64;
        r.a.push(a);
        r.b.push((a + 1).min(n as i64 - 1));
        r.f.push((u - a as f64) as f32);
    }
    r
}

/// `amostrar_quadrado` + `normalizar_imagenet`: zero fora do quadro, contas em f32 na ordem do numpy.
fn amostrar_quadrado(q: &Imagem, x0: i64, y0: i64, lado: i64, saida: usize) -> Vec<f32> {
    let (ex, ey) = (eixo(x0, lado, saida), eixo(y0, lado, saida));
    let (w, h) = (q.w as i64, q.h as i64);
    let px = |y: i64, x: i64, c: usize| -> f32 {
        if (0..h).contains(&y) && (0..w).contains(&x) {
            q.rgb[((y * w + x) * 3) as usize + c] as f32
        } else {
            0.0
        }
    };
    let plano = saida * saida;
    let mut o = vec![0f32; 3 * plano];
    for i in 0..saida {
        let (va, vb, fv) = (ey.a[i], ey.b[i], ey.f[i]);
        for j in 0..saida {
            let (ua, ub, fu) = (ex.a[j], ex.b[j], ex.f[j]);
            for c in 0..3 {
                let topo = px(va, ua, c) * (1.0 - fu) + px(va, ub, c) * fu;
                let base = px(vb, ua, c) * (1.0 - fu) + px(vb, ub, c) * fu;
                o[c * plano + i * saida + j] = normalizar(topo * (1.0 - fv) + base * fv, c);
            }
        }
    }
    o
}

/// (x/255 − média)/desvio do ImageNet, em f32.
#[inline]
pub(super) fn normalizar(v: f32, c: usize) -> f32 {
    (v / 255.0 - MEDIA[c]) / DESVIO[c]
}

#[cfg(test)]
#[path = "neurais_teste.rs"]
pub(super) mod banco;

#[cfg(test)]
pub(crate) mod testes {
    use super::banco::{self, Quadros};
    use super::*;
    use crate::edicao::testes::fontes;
    use crate::ia::{Modelo, Preferencia};

    /// Limites do aceite (contratos-fase2 (d) P2.3).
    const TOL_CAIXA_PX: f64 = 0.5;
    const TOL_CONF: f64 = 0.002;
    const IOU_MIN_38: f64 = 0.75;
    const ERRADAS_MAX_38: u32 = 33;
    /// Sequências do passo a passo: limpas e degradadas, sintéticas e reais, com reprocura (oclusão, saída de cena,
    /// alvo tapado) e com parada.
    const PASSO_A_PASSO: [&str; 9] =
        ["escala", "distrator_cftv", "oclusao_total", "oclusao_total_cftv", "pequeno_cftv", "sai_do_quadro", "c02_p1", "c10_p1", "c13_p0_cftv"];

    fn motor(teste: &str) -> Option<VitTrack> {
        banco::cpu(Modelo::Bytes(MODELO), teste).map(VitTrack::de_sessao)
    }

    /// Um quadro contra a referência, tensor a tensor: template, recorte de busca, Hann, saídas da rede e a caixa.
    #[test]
    fn primeiro_quadro_igual_a_referencia() {
        const T: &str = "primeiro_quadro_igual_a_referencia";
        let (Some(dir), Some(cap), Some(mut v), Some(seqs)) = (banco::referencia(T), fontes::cap(), motor(T), banco::sequencias(T)) else { return };
        let dir = dir.join("tensores");
        let s = seqs.iter().find(|s| s.nome == "escala").unwrap();
        let c0 = s.gt[0].as_ref().unwrap().inteira;
        let mut q = Quadros::abrir(&cap, &s.video);
        let (_, q0) = q.proximo().unwrap();
        let (t, ..) = recorte(&q0, &c0, FATOR_TEMPLATE, LADO_TEMPLATE);
        v.iniciar(&q0, c0).unwrap();
        let (_, q1) = q.proximo().unwrap();
        let (b, x0, y0, lado) = recorte(&q1, &c0, FATOR_BUSCA, LADO_BUSCA);
        let r = banco::json(&dir.join("vit.json"));
        assert_eq!([x0, y0, lado], [0, 1, 2].map(|i| r["busca"][i].as_i64().unwrap()));
        let d_t = banco::dif_max(&t, &banco::f32s(&dir.join("vit_template.f32")));
        let d_b = banco::dif_max(&b, &banco::f32s(&dir.join("vit_search.f32")));
        let d_h = banco::dif_max(&v.hann, &banco::f32s(&dir.join("vit_hann.f32")));
        let t_ = ort::value::Tensor::from_array(([1usize, 3, 128, 128], t)).unwrap();
        let b_ = ort::value::Tensor::from_array(([1usize, 3, 256, 256], b)).unwrap();
        let saida = v.sessao.session.run(ort::inputs!["template" => t_, "search" => b_]).unwrap();
        let d_o: Vec<f32> = (1..=3)
            .map(|i| banco::dif_max(saida[format!("output{i}").as_str()].try_extract_tensor::<f32>().unwrap().1, &banco::f32s(&dir.join(format!("vit_output{i}.f32")))))
            .collect();
        drop(saida);
        let p = v.seguir(&q1, &Busca { caixa: c0, janela: true }).unwrap();
        let d_c = banco::dif_caixa(&p.caixa.unwrap(), &banco::caixa(&r["caixa"]).unwrap());
        let d_conf = (p.conf - r["conf"].as_f64().unwrap()).abs();
        eprintln!("{T}: template {d_t:e} busca {d_b:e} hann {d_h:e} saídas {d_o:?} caixa {d_c:e} px conf {d_conf:e}");
        // o preparo é Rust puro nas mesmas contas do numpy: igual bit a bit
        assert!(d_t == 0.0 && d_b == 0.0, "recorte: template {d_t}, busca {d_b}");
        assert!(d_h <= 1e-7, "hann {d_h}");
        assert!(d_o.iter().all(|d| *d <= 1e-5), "saídas {d_o:?}");
        assert!(d_c <= 1e-4 && d_conf <= 1e-5, "caixa {d_c} conf {d_conf}");
    }

    /// Passo a passo com as caixas de busca da referência (as do laço em Python, reprocura inclusa) nos mesmos quadros
    /// (SHA-256 de cada um conferido): caixa a até 0,5 px e confiança a até 0,002 em toda chamada.
    #[test]
    fn porte_igual_a_referencia() {
        const T: &str = "porte_igual_a_referencia";
        let (Some(dir), Some(cap), Some(seqs)) = (banco::referencia(T), fontes::cap(), banco::sequencias(T)) else { return };
        let (mut chamadas, mut reprocuras, mut pior_c, mut pior_conf) = (0usize, 0usize, 0f64, 0f64);
        for nome in PASSO_A_PASSO {
            let Some(mut v) = motor(T) else { return };
            let r = banco::json(&dir.join(format!("vittrack/{nome}.json")));
            let s = seqs.iter().find(|s| s.nome == nome).unwrap();
            let shas = r["quadros"].as_array().unwrap();
            let ini = r["ini"].as_u64().unwrap() as usize;
            assert_eq!(ini, s.ini(), "{nome}");
            let mut q = Quadros::abrir(&cap, &s.video);
            let q0 = q.ate(ini).unwrap();
            v.iniciar(&q0, banco::caixa(&r["caixa0"]).unwrap()).unwrap();
            assert_eq!(q.sha(), shas[ini].as_str().unwrap(), "{nome}: quadro {ini} diferente do da referência");
            for c in r["chamadas"].as_array().unwrap() {
                let k = c["q"].as_u64().unwrap() as usize;
                let quadro = q.ate(k).unwrap();
                let busca = Busca { caixa: banco::caixa(&c["busca"]).unwrap(), janela: c["janela"].as_bool().unwrap() };
                let p = v.seguir(&quadro, &busca).unwrap();
                assert_eq!(q.sha(), shas[k].as_str().unwrap(), "{nome}: quadro {k} diferente do da referência");
                let dc = banco::dif_caixa(&p.caixa.unwrap(), &banco::caixa(&c["caixa"]).unwrap());
                let dconf = (p.conf - c["conf"].as_f64().unwrap()).abs();
                assert!(dc <= TOL_CAIXA_PX && dconf <= TOL_CONF, "{nome} quadro {k} ({busca:?}): caixa {dc} px, conf {dconf}");
                (pior_c, pior_conf) = (pior_c.max(dc), pior_conf.max(dconf));
                chamadas += 1;
                reprocuras += usize::from(!busca.janela);
            }
        }
        eprintln!("{T}: {chamadas} chamadas ({reprocuras} sem a janela) em {} sequências; pior caixa {pior_c:e} px, pior conf {pior_conf:e}", PASSO_A_PASSO.len());
        assert!(chamadas > 1000 && reprocuras > 20, "{chamadas} chamadas, {reprocuras} reprocuras");
    }

    /// Sempre CPU (a fábrica recebe `Preferencia::Cpu`); a corrida grava motor, arquivo com SHA-256, origem, commit,
    /// runtime e provedor; laço e limiares do motor na taxa da mídia.
    #[test]
    fn so_cpu() {
        assert_eq!(VitTrack::opcoes().preferencia, Preferencia::Cpu);
        assert_eq!(banco::sha256(MODELO), SHA256);
        let Some(v) = motor("so_cpu") else { return };
        let c = v.corrida();
        assert_eq!((c.motor, c.provedor.as_str(), c.runtime.as_str()), (MotorRastreio::Vittrack, "cpu", ia::sessao::versao_runtime().as_str()));
        assert_eq!(c.modelo.arquivos, vec![ArquivoModelo { nome: NOME_ARQUIVO.into(), sha256: SHA256.into() }]);
        assert_eq!((c.modelo.origem.as_str(), c.modelo.commit.as_str()), (ORIGEM, COMMIT));
        let j = serde_json::to_value(&c).unwrap();
        assert_eq!((j["provedor"].as_str(), j["modelo"]["arquivos"][0]["sha256"].as_str()), (Some("cpu"), Some(SHA256)));
        assert_eq!(v.laco(), Laco::Reprocura);
        assert_eq!(v.limiares(30.0), Limiares { ok: 0.45, perdido: 0.30, paciencia: 60 });
        assert_eq!((v.limiares(25.0).paciencia, v.limiares(29.97).paciencia, v.limiares(0.0).paciencia), (50, 60, 60));
        // pela fábrica (P2.1): CPU
        if let Some(ai) = crate::ia::testes::ambiente() {
            assert_eq!(VitTrack::novo(&ai).unwrap().corrida().provedor, "cpu");
        }
    }

    #[test]
    fn caixa_e_quadro_invalidos_recusados() {
        let Some(mut v) = motor("caixa_e_quadro_invalidos_recusados") else { return };
        let rgb = vec![0u8; 64 * 48 * 3];
        let q = Imagem { rgb: &rgb, w: 64, h: 48 };
        let caixa = CaixaPx { x: 10.0, y: 10.0, w: 8.0, h: 12.0 };
        assert!(v.seguir(&q, &Busca { caixa, janela: true }).is_err(), "sem iniciar");
        for c in [CaixaPx { w: 0.0, ..caixa }, CaixaPx { x: f64::NAN, ..caixa }, CaixaPx { h: -1.0, ..caixa }] {
            assert!(v.iniciar(&q, c).is_err(), "{c:?}");
        }
        assert!(v.iniciar(&Imagem { rgb: &rgb[1..], w: 64, h: 48 }, caixa).is_err());
        let p = v.iniciar(&q, caixa).unwrap();
        assert_eq!((p.caixa, p.conf), (Some(caixa), 1.0));
        // caixa enorme: recorte fora do quadro vira zero, sem alocar pelo tamanho da caixa
        let p = v.seguir(&q, &Busca { caixa: CaixaPx { x: -1e9, y: -1e9, w: 1e9, h: 1e9 }, janela: false }).unwrap();
        assert!(p.caixa.unwrap().w >= 2.0 && p.conf.is_finite());
        // finita, mas fora da faixa do i64: sem estouro de inteiro no recorte
        for c in [CaixaPx { x: 1e300, y: -1e300, w: 8.0, h: 8.0 }, CaixaPx { x: 0.0, y: 0.0, w: 1e200, h: 1e200 }, CaixaPx { x: 9.2e18, y: -9.2e18, w: 1e19, h: 1e19 }] {
            assert!(v.iniciar(&q, c).is_ok(), "{c:?}");
            assert!(v.seguir(&q, &Busca { caixa: c, janela: true }).unwrap().conf.is_finite(), "{c:?}");
        }
    }

    /// Quadro sintético de teste: textura que anda com `k`.
    pub(crate) fn quadro_sintetico(k: u32, w: u32, h: u32) -> Vec<u8> {
        (0..w * h).flat_map(|i| {
            let (x, y) = (i % w + 2 * k, i / w + k);
            let v = (x.wrapping_mul(2654435761) ^ y.wrapping_mul(40503)).rotate_left(k % 7) >> 24;
            [v as u8, (v as u8).wrapping_mul(3), 255 - v as u8]
        }).collect()
    }

    /// O mesmo motor nos dois lados de "ambos" (corrida.rs): o `iniciar` recomeça o estado, e a segunda corrida dá o
    /// mesmo que um motor novo.
    #[test]
    fn iniciar_de_novo_e_motor_novo() {
        const T: &str = "iniciar_de_novo_e_motor_novo";
        let (Some(mut usado), Some(mut novo)) = (motor(T), motor(T)) else { return };
        let (w, h) = (128, 96);
        let q: Vec<Vec<u8>> = (0..6).map(|k| quadro_sintetico(k, w, h)).collect();
        let im = |k: usize| Imagem { rgb: &q[k], w, h };
        let c = CaixaPx { x: 40.0, y: 30.0, w: 24.0, h: 20.0 };
        usado.iniciar(&im(0), CaixaPx { x: 10.0, ..c }).unwrap();
        for k in 1..3 {
            usado.seguir(&im(k), &Busca { caixa: c, janela: true }).unwrap();
        }
        let corre = |m: &mut VitTrack| -> Vec<Passo> {
            let mut v = vec![m.iniciar(&im(3), c).unwrap()];
            v.extend((4..6).map(|k| m.seguir(&im(k), &Busca { caixa: c, janela: k == 4 }).unwrap()));
            v
        };
        assert_eq!(corre(&mut usado), corre(&mut novo));
    }

    /// Borda do quadro (o passo a passo não chega nela): valores de `comum.limitar_caixa(c, 64, 48)`.
    #[test]
    fn limitar_caixa_como_a_referencia() {
        let k = |x, y, w, h| CaixaPx { x, y, w, h };
        for (c, r) in [
            (k(-50.0, 10.0, 20.0, 30.0), k(-16.0, 10.0, 20.0, 30.0)),
            (k(70.0, 50.0, 1.0, 0.5), k(60.0, 44.0, 2.0, 2.0)),
            (k(10.0, 10.0, 8.0, 12.0), k(10.0, 10.0, 8.0, 12.0)),
            (k(61.5, -40.0, 3.0, 40.0), k(60.0, -36.0, 3.0, 40.0)),
        ] {
            assert_eq!(limitar_caixa(c, 64, 48), r, "{c:?}");
        }
    }

    /// Aceite do §11 nas 38 sequências pelo `rastreio::correr` (laço do P2.2); na mesma rodada, o laço de
    /// `laco_sicro.py` portado roda sobre as MESMAS saídas do motor e tem de pedir as mesmas buscas e decidir igual
    /// em cada quadro (banco).
    #[test]
    fn trinta_e_oito_sequencias_com_laco() {
        const T: &str = "trinta_e_oito_sequencias_com_laco";
        if motor(T).is_none() {
            return;
        }
        let mut novo = || -> Box<dyn Rastreador> { Box::new(motor(T).unwrap()) };
        let Some((iou, m)) = banco::trinta_e_oito(T, &mut novo, banco::laco_do_sicro) else { return };
        assert!(iou >= IOU_MIN_38, "{T}: IoU médio {iou}");
        assert!(m.errada <= ERRADAS_MAX_38, "{T}: {} caixas erradas aceitas", m.errada);
    }
}
