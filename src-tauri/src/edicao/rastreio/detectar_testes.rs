//! Testes do P4.5: porte dos seis modelos × `p3b/ref/detectores.py` (IoU ≥ 0,99 nas mesmas imagens), trilhas ×
//! `p3b/ref/trilhas.py`, as coberturas medidas em `p3b/` reproduzidas e o fluxo propor → folha → confirmar → render.
//! Modelos e imagens do dossiê (`ia::testes`); sem eles, dispensa registrada (`SICRO_EXIGE_MODELOS=1` faz falhar).
//! Nada vai a serviço externo; o onnxruntime é o dos testes (`ia::testes::ort_lib`, telemetria desligada).

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use serde_json::{json, Value};

use super::*;
use crate::edicao::rastreio::decodificadores::{self as dec, Deteccao, Entrada, RetinaFace, Rosto, RtDetr, YoloxN, YuNet};
use crate::edicao::rastreio::tira;
use crate::ia::testes as t;
use crate::ia::Modelo;

const REFERENCIA: &str = include_str!("../../../tests/fixtures/edicao/deteccao-referencia.json");
const TRILHAS: &str = include_str!("../../../tests/fixtures/edicao/deteccao-trilhas.json");

fn dossie(rel: &str) -> PathBuf {
    Path::new(t::DOSSIE).join(rel)
}

fn redacao(a: &ArquivoRedacao) -> Option<PathBuf> {
    t::modelo(&format!("redacao/{}", a.nome), a.sha256)
}

fn yolox_coco() -> Option<PathBuf> {
    let p = dossie("p2/modelos/yolox_s.onnx");
    t::conferir_sha256(&p, VEICULO_COCO.sha256).then_some(p)
}

/// Quadro RGB de uma imagem do dossiê.
struct Img {
    rgb: Vec<u8>,
    w: u32,
    h: u32,
}

impl Img {
    /// Pelo decodificador de JPEG do Rust (coberturas: medida estatística, ±1 no pixel não muda o número).
    fn abrir(p: &Path) -> Option<Self> {
        let i = image::open(p).ok()?.to_rgb8();
        Some(Img { w: i.width(), h: i.height(), rgb: i.into_raw() })
    }

    /// Como o SICRO lê quadros (ffmpeg, `bgr24`): os mesmos pixels que a referência recebeu.
    fn do_ffmpeg(bin: &Path, p: &Path, w: u32, h: u32) -> Option<Self> {
        let o = std::process::Command::new(bin).args(["-v", "error", "-i"]).arg(p).args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "bgr24", "-"]).output().ok()?;
        (o.status.success() && o.stdout.len() == (w * h * 3) as usize).then(|| Img { rgb: o.stdout.chunks_exact(3).flat_map(|b| [b[2], b[1], b[0]]).collect(), w, h })
    }

    fn q(&self) -> Imagem<'_> {
        Imagem { rgb: &self.rgb, w: self.w, h: self.h }
    }

    fn bgr_sha256(&self) -> String {
        let bgr: Vec<u8> = self.rgb.chunks_exact(3).flat_map(|p| [p[2], p[1], p[0]]).collect();
        executar::sha256_bytes(&bgr)
    }
}

// ------------------------------------------------------------------ porte × referência

fn de_json(v: &Value) -> Vec<[f64; 6]> {
    v.as_array().unwrap().iter().map(|d| std::array::from_fn(|i| d[i].as_f64().unwrap())).collect()
}

fn iou(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let iw = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let ih = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let i = iw * ih;
    i / ((a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - i + 1e-9)
}

fn caixa(d: &Deteccao) -> [f64; 4] {
    [d.x1 as f64, d.y1 as f64, d.x2 as f64, d.y2 as f64]
}

/// Resultado da comparação de um caso: pares, menor IoU, maior diferença de score e o que faltou de cada lado acima
/// de `thr + margem` (perto do limiar uma diferença de ±1 no pixel decide se a caixa existe).
#[derive(Debug, Default)]
struct Comparacao {
    pares: usize,
    menor_iou: f64,
    maior_dif: f64,
    sem_par_ref: Vec<[f64; 6]>,
    sem_par_rust: Vec<Deteccao>,
}

fn comparar(refe: &[[f64; 6]], rust: &[Deteccao], thr: f64) -> Comparacao {
    let margem = (0.1 * thr).max(0.02);
    let mut usado = vec![false; rust.len()];
    let mut c = Comparacao { menor_iou: 1.0, ..Default::default() };
    let mut ordem: Vec<usize> = (0..refe.len()).collect();
    ordem.sort_by(|&a, &b| refe[b][4].total_cmp(&refe[a][4]));
    for i in ordem {
        let r = &refe[i];
        let rc = [r[0], r[1], r[2], r[3]];
        let melhor = (0..rust.len()).filter(|&j| !usado[j] && rust[j].classe as f64 == r[5]).max_by(|&a, &b| iou(&rc, &caixa(&rust[a])).total_cmp(&iou(&rc, &caixa(&rust[b]))));
        match melhor.filter(|&j| iou(&rc, &caixa(&rust[j])) >= 0.5) {
            Some(j) => {
                usado[j] = true;
                if r[4] >= thr + margem || rust[j].score as f64 >= thr + margem {
                    c.pares += 1;
                    c.menor_iou = c.menor_iou.min(iou(&rc, &caixa(&rust[j])));
                    c.maior_dif = c.maior_dif.max((r[4] - rust[j].score as f64).abs());
                }
            }
            None if r[4] >= thr + margem => c.sem_par_ref.push(*r),
            None => {}
        }
    }
    c.sem_par_rust = rust.iter().zip(&usado).filter(|(d, u)| !**u && d.score as f64 >= thr + margem).map(|(d, _)| *d).collect();
    c
}

struct Seis {
    r34: Rosto,
    yn: Rosto,
    corpo_s: YoloxN,
    corpo_n: YoloxN,
    rtdetr: RtDetr,
    placa: YoloxN,
    coco: YoloxN,
}

/// As sessões de CPU dos seis modelos e do YOLOX-S COCO pela lib dos testes (a referência rodou em CPU 1.31.1).
fn seis_em_cpu(teste: &str) -> Option<Seis> {
    let arqs = [ROSTO_R34, ROSTO_YUNET, CORPO_S, CORPO_N, PLACA_RTDETR, PLACA_YOLOX];
    let mut ps = Vec::new();
    for a in &arqs {
        ps.push(t::ou_dispensa(redacao(a), teste, a.nome)?);
    }
    let coco = t::ou_dispensa(yolox_coco(), teste, "p2/modelos/yolox_s.onnx")?;
    let s = |p: &Path| t::ou_dispensa(t::sessao_cpu(Modelo::Arquivo(p)), teste, "a lib dos testes");
    Some(Seis {
        r34: Rosto::Retina(RetinaFace::nova(s(&ps[0])?)),
        yn: Rosto::YuNet(YuNet { s: s(&ps[1])? }),
        corpo_s: YoloxN::nova(s(&ps[2])?, Entrada::Esticar).unwrap(),
        corpo_n: YoloxN::nova(s(&ps[3])?, Entrada::Esticar).unwrap(),
        rtdetr: RtDetr { s: s(&ps[4])? },
        placa: YoloxN::nova(s(&ps[5])?, Entrada::Letterbox).unwrap(),
        coco: YoloxN::nova(s(&coco)?, Entrada::Letterbox).unwrap(),
    })
}

/// Aceite do P4.5: os seis modelos portados (e o YOLOX-S COCO dos veículos, e os três caminhos do dossiê) dão as
/// caixas da referência Python nas mesmas imagens: toda caixa acima do limiar (com folga) tem par com IoU ≥ 0,99.
#[test]
fn seis_modelos_conferem_com_a_referencia() {
    let teste = "seis_modelos_conferem_com_a_referencia";
    let Some(mut m) = seis_em_cpu(teste) else { return };
    let r: Value = serde_json::from_str(REFERENCIA).unwrap();
    // os quadros como a referência os recebeu: o ffmpeg dos testes, ou o que gerou a referência
    let bins: Vec<PathBuf> = crate::edicao::testes::fontes::cap().map(|c| c.binario.clone()).into_iter().chain([PathBuf::from("/usr/bin/ffmpeg")]).collect();
    let mut imgs = HashMap::new();
    for i in r["imagens"].as_array().unwrap() {
        let (arq, sha) = (dossie(i["arquivo"].as_str().unwrap()), i["bgr_sha256"].as_str().unwrap());
        let (w, h) = (i["w"].as_u64().unwrap() as u32, i["h"].as_u64().unwrap() as u32);
        let Some(_) = t::ou_dispensa(arq.is_file().then_some(()), teste, i["arquivo"].as_str().unwrap()) else { return };
        let img = bins.iter().filter_map(|b| Img::do_ffmpeg(b, &arq, w, h)).find(|img| img.bgr_sha256() == sha);
        let img = img.unwrap_or_else(|| panic!("{}: nenhum ffmpeg dá os pixels da referência ({})", i["nome"], r["ffmpeg"]));
        imgs.insert(i["nome"].as_str().unwrap().to_string(), img);
    }
    // o redimensionamento é o do `cv2.resize` da referência, byte a byte
    let base = &imgs[r["imagens"][0]["nome"].as_str().unwrap()];
    for x in r["redimensionar"].as_array().unwrap() {
        let (w, h) = (x["w"].as_u64().unwrap() as usize, x["h"].as_u64().unwrap() as usize);
        let red = dec::redimensionar(&base.rgb, base.w as usize, base.h as usize, w, h);
        let img = Img { rgb: red, w: w as u32, h: h as u32 };
        assert_eq!(img.bgr_sha256(), x["bgr_sha256"].as_str().unwrap(), "redimensionar {}x{} → {w}x{h}", base.w, base.h);
    }
    let mut resumo: HashMap<String, (usize, f64, f64)> = HashMap::new();
    let mut falhas = Vec::new();
    for c in r["casos"].as_array().unwrap() {
        let (modelo, nome, p) = (c["modelo"].as_str().unwrap(), c["imagem"].as_str().unwrap(), &c["params"]);
        let q = imgs[nome].q();
        let f = |k: &str| p[k].as_f64().unwrap();
        let classes = |v: &Value| v.as_array().map(|a| a.iter().map(|x| x.as_u64().unwrap() as u16).collect::<Vec<_>>()).unwrap_or_default();
        let hw = |v: &Value| (v[0].as_u64().unwrap() as usize, v[1].as_u64().unwrap() as usize);
        let (dets, thr) = match modelo {
            "rosto_retinaface_r34" => match &mut m.r34 {
                Rosto::Retina(r) => (r.detectar(&q, f("thr") as f32, 0.4, f("lado_max") as usize, false).unwrap(), f("thr")),
                Rosto::YuNet(_) => unreachable!(),
            },
            "rosto_yunet_n" => match &mut m.yn {
                Rosto::YuNet(r) => (r.detectar(&q, f("thr") as f32, 0.3, f("lado_max") as usize, false).unwrap(), f("thr")),
                Rosto::Retina(_) => unreachable!(),
            },
            "corpo_yolox_s_wholebody12" => (m.corpo_s.detectar(&q, f("thr") as f32, 0.45, &classes(&p["classes"]), Some(hw(&p["hw"]))).unwrap(), f("thr")),
            "corpo_yolox_n_wholebody12" => (m.corpo_n.detectar(&q, f("thr") as f32, 0.45, &classes(&p["classes"]), Some(hw(&p["hw"]))).unwrap(), f("thr")),
            "placa_rtdetrv2_r18_openimages" => (m.rtdetr.detectar(&q, f("thr") as f32, 0.5).unwrap(), f("thr")),
            "placa_yolox_s_autolane" => (m.placa.detectar(&q, f("thr") as f32, 0.45, &[], None).unwrap(), f("thr")),
            "yolox_s_coco" => (m.coco.detectar(&q, f("thr") as f32, 0.45, &classes(&p["classes"]), None).unwrap(), f("thr")),
            "cabecas" => (dec::cabecas(&mut m.corpo_s, &q, f("escala")).unwrap(), 0.5 * 0.15 / 0.35),
            "rostos_r34" => (dec::rostos(&mut m.r34, &q, dec::lado_do_rosto(&q, f("escala"))).unwrap(), 0.25),
            "rostos_yunet" => (dec::rostos(&mut m.yn, &q, dec::lado_do_rosto(&q, f("escala"))).unwrap(), 0.25),
            "placas" => (dec::placas(Some(&mut m.rtdetr), &mut m.placa, &mut m.coco, &q).unwrap().0, 0.25),
            "placas_cpu" => (dec::placas(None, &mut m.placa, &mut m.coco, &q).unwrap().0, 0.25),
            outro => panic!("caso {outro}"),
        };
        let cmp = comparar(&de_json(&c["dets"]), &dets, thr);
        let e = resumo.entry(modelo.to_string()).or_insert((0, 1.0, 0.0));
        (e.0, e.1, e.2) = (e.0 + cmp.pares, e.1.min(cmp.menor_iou), e.2.max(cmp.maior_dif));
        if cmp.menor_iou < 0.99 || cmp.maior_dif > 0.01 || !cmp.sem_par_ref.is_empty() || !cmp.sem_par_rust.is_empty() {
            falhas.push(format!("{modelo} em {nome}: {cmp:?}"));
        }
    }
    let mut chaves: Vec<_> = resumo.keys().cloned().collect();
    chaves.sort();
    for k in chaves {
        let (n, i, d) = resumo[&k];
        eprintln!("{k:32} {n:3} pares, menor IoU {i:.4}, maior diferença de score {d:.4}");
    }
    assert!(falhas.is_empty(), "{}", falhas.join("\n"));
    assert!(resumo.values().map(|x| x.0).sum::<usize>() > 300, "poucos pares: {resumo:?}");
}

// ------------------------------------------------------------------ trilhas × referência

/// `.npz` do numpy (zip de `.npy` v1/v2, `<f4`, `<f8` ou `<i8`, ordem C): nome → (forma, valores).
fn npz(p: &Path) -> Option<HashMap<String, (Vec<usize>, Vec<f64>)>> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(p).ok()?).ok()?;
    let mut out = HashMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).ok()?;
        let nome = e.name().trim_end_matches(".npy").to_string();
        let mut b = Vec::new();
        e.read_to_end(&mut b).ok()?;
        assert_eq!(&b[..6], b"\x93NUMPY");
        let (ini, n) = if b[6] == 1 { (10, u16::from_le_bytes([b[8], b[9]]) as usize) } else { (12, u32::from_le_bytes([b[8], b[9], b[10], b[11]]) as usize) };
        let cab = std::str::from_utf8(&b[ini..ini + n]).ok()?;
        let entre = |a: &str, z: char| cab.split(a).nth(1).and_then(|s| s.split(z).next()).unwrap_or("");
        let descr = entre("'descr': '", '\'');
        let forma: Vec<usize> = entre("'shape': (", ')').split(',').filter_map(|s| s.trim().parse().ok()).collect();
        assert!(!cab.contains("'fortran_order': True"));
        let dados = &b[ini + n..];
        let v: Vec<f64> = match descr {
            "<f4" => dados.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64).collect(),
            "<f8" => dados.chunks_exact(8).map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect(),
            "<i8" => dados.chunks_exact(8).map(|c| i64::from_le_bytes(c.try_into().unwrap()) as f64).collect(),
            outro => panic!("npy {outro}"),
        };
        out.insert(nome, (forma, v));
    }
    Some(out)
}

/// O pós-processamento das trilhas (ByteTrack com a caixa detectada, costura, buracos, extensão, suavização, margem)
/// dá as propostas de `p3b/ref/trilhas.py` nas detecções guardadas dos vídeos do Commons: mesmo número, mesma faixa
/// de quadros, mesmas origens por linha e caixas a até 0,05 px.
#[test]
fn trilhas_como_a_referencia() {
    let r: Value = serde_json::from_str(TRILHAS).unwrap();
    let mut conferidas = 0;
    for (nome, c) in r.as_object().unwrap() {
        let Some(z) = t::ou_dispensa(npz(&dossie(c["arquivo"].as_str().unwrap())), "trilhas_como_a_referencia", c["arquivo"].as_str().unwrap()) else { continue };
        let classe = c["classe"].as_f64().unwrap();
        let alto = c["alto"].as_f64();
        let mut qs: Vec<&String> = z.keys().filter(|k| k.starts_with('q')).collect();
        qs.sort();
        let dets: Vec<Vec<Deteccao>> = qs
            .iter()
            .map(|k| {
                let (f, v) = &z[*k];
                v.chunks_exact(f[1])
                    .filter(|l| l[5] == classe)
                    .map(|l| {
                        let s = l[4] as f32;
                        let s = match alto {
                            Some(a) => (0.5 * s / a as f32).min(1.0),
                            None => s,
                        };
                        Deteccao { x1: l[0] as f32, y1: l[1] as f32, x2: l[2] as f32, y2: l[3] as f32, score: s, classe: 0 }
                    })
                    .collect()
            })
            .collect();
        let p = ParamsTrilhas { margem: c["margem"].as_f64().unwrap(), margem_px: c["margem_px"].as_f64().unwrap(), ..ParamsTrilhas::da_classe(ClasseDeteccao::Cabeca) };
        let props = trilhas_de_redacao(&dets, c["fps"].as_f64().unwrap(), c["w"].as_f64().unwrap(), c["h"].as_f64().unwrap(), &p);
        let esperadas = c["propostas"].as_array().unwrap();
        assert_eq!(props.len(), esperadas.len(), "{nome}");
        for (pr, e) in props.iter().zip(esperadas) {
            let l = &pr.linhas;
            let origem: Vec<u64> = (0..3).map(|o| l.iter().filter(|x| x.origem == o).count() as u64).collect();
            assert_eq!((l[0].q, l[l.len() - 1].q, l.len() as u64), (e["de"].as_i64().unwrap(), e["ate"].as_i64().unwrap(), e["n"].as_u64().unwrap()), "{nome}");
            assert_eq!(origem, e["origem"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap()).collect::<Vec<_>>(), "{nome} {}", e["de"]);
            assert_eq!(l[l.len() / 2].q, e["meio_q"].as_i64().unwrap());
            for (linha, chave) in [(&l[0], "primeira"), (&l[l.len() / 2], "meio"), (&l[l.len() - 1], "ultima")] {
                let ec: Vec<f64> = e[chave].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
                for i in 0..4 {
                    assert!((linha.c[i] - ec[i]).abs() <= 0.05, "{nome} proposta {} {chave}: {:?} × {ec:?}", e["de"], linha.c);
                }
            }
        }
        conferidas += 1;
        eprintln!("{nome}: {} propostas iguais à referência", props.len());
    }
    assert!(conferidas == 4 || !t::exige(), "{conferidas}");
}

/// Regras próprias do SICRO sobre as propostas: cabeça abaixo de 12 px não vira proposta; rosto que uma cabeça
/// cobre em TODOS os quadros sai (nenhum quadro perde cobertura); o que fica descoberto em um quadro fica.
#[test]
fn rosto_dentro_da_cabeca_sai_sem_perder_cobertura() {
    let linha = |q: i64, c: [f64; 4]| Linha { q, c, score: 0.9, origem: 0 };
    let cabeca = Proposta { linhas: (1..=10).map(|q| linha(q, [100.0, 100.0, 140.0, 150.0])).collect(), altura_mediana: 40.0 };
    let dentro = Proposta { linhas: (2..=9).map(|q| linha(q, [110.0, 115.0, 130.0, 140.0])).collect(), altura_mediana: 20.0 };
    let escapa = Proposta { linhas: (8..=12).map(|q| linha(q, [110.0, 115.0, 130.0, 140.0])).collect(), altura_mediana: 20.0 };
    let fora = Proposta { linhas: (2..=5).map(|q| linha(q, [300.0, 100.0, 320.0, 130.0])).collect(), altura_mediana: 20.0 };
    let r = sem_rosto_coberto(vec![dentro, escapa.clone(), fora.clone()], std::slice::from_ref(&cabeca));
    assert_eq!(r, vec![escapa, fora]);
    // as classes de trilha de cada pedido
    use ClasseDeteccao::*;
    assert_eq!(classes_das_trilhas(&[Rosto]), vec![Cabeca, Rosto]);
    assert_eq!(classes_das_trilhas(&[Placa, Pessoa]), vec![Placa, Pessoa]);
    assert_eq!(ParamsTrilhas::da_classe(Rosto).margem, 0.25);
    assert_eq!((ParamsTrilhas::da_classe(Placa).margem, ParamsTrilhas::da_classe(Placa).margem_px), (0.15, 3.0));
}

/// Denso de uma proposta: uma linha por quadro, pts da tabela, caixa em 0..1, detecção fraca = incerto, interpolada e
/// estendida no `tipo`; buraco deixado pela extensão é interpolado; a corrida guarda os arquivos dos modelos.
#[test]
fn denso_da_proposta_tem_tipo_estado_e_corrida() {
    use crate::edicao::quadros::apoio;
    let tab = apoio::tabela((1, 15360), apoio::pts_regulares(40, (1, 30), (1, 15360), 0));
    let corrida = Corrida {
        motor: MotorCorrida { motor: MotorRastreio::Deteccao, modelo: ModeloCorrida { nome: "m".into(), origem: "o".into(), commit: "c".into(), arquivos: vec![ArquivoModelo { nome: "a.onnx".into(), sha256: "e".repeat(64) }], desvio: String::new() }, runtime: "onnxruntime 1.31.1".into(), provedor: "cpu".into() },
        sentido: Sentido::Frente,
        ancora_pts: 0,
        de_pts: 0,
        ate_pts: 1,
        limiares: Limiares { ok: 0.5, perdido: 0.25, paciencia: 30 },
        parou: Motivo::Fim,
        feito_em: "2026-11-17T00:00:00Z".into(),
    };
    let l = |q: i64, score: f64, origem: u8| Linha { q, c: [10.0 * q as f64, 20.0, 10.0 * q as f64 + 40.0, 60.0], score, origem };
    let p = Proposta { linhas: vec![l(3, 0.6, 2), l(4, 0.9, 0), l(5, 0.3, 0), l(6, 0.3, 1), l(8, 0.8, 0), l(60, 0.8, 2)], altura_mediana: 40.0 };
    let d = denso_da_proposta(&p, &tab, 10, TamanhoPx { w: 200, h: 100 }, "r1", &"a".repeat(64), &corrida).unwrap();
    // quadros 3..=8 do trecho (o 60 passa do fim da tabela e sai): índices 12..=17
    assert_eq!(d.pts, tab.pts[12..18].to_vec());
    assert_eq!(d.tipo, vec![2, 0, 0, 1, 1, 0]);
    assert_eq!(d.estado, vec![ESTADO_OK, ESTADO_OK, ESTADO_INCERTO, ESTADO_OK, ESTADO_OK, ESTADO_OK]);
    assert_eq!((d.x[0], d.y[0], d.w[0], d.h[0]), (30.0 / 200.0, 0.2, 0.2, 0.4));
    // o quadro 7 (sem linha) entre o 6 e o 8
    assert!((d.x[4] - 70.0 / 200.0).abs() < 1e-12 && d.conf[4] == 0.3);
    let lida: Vec<Corrida> = arquivo::corridas(&d).unwrap();
    assert_eq!((lida[0].motor.motor, lida[0].motor.modelo.arquivos.len()), (MotorRastreio::Deteccao, 1));
    assert!(d.corrida.iter().all(|&c| c == 0));
    // a corrida entra em `feito.ia` com o uso "deteccao"
    let pr = crate::edicao::motor::plano::ProvRastreio {
        id: "r1".into(),
        origem: OrigemRastreio::Deteccao { classe: ClasseDeteccao::Placa },
        ancoras: 0,
        denso_sha256: None,
        silhuetas_sha256: None,
        corridas: d.corridas.clone(),
        corridas_do_programa: vec![],
        confirmacao: None,
        ausencias: vec![],
    };
    let ia = crate::ia::uso::da_proveniencia(&[pr]);
    assert_eq!((ia.len(), ia[0].uso.as_str(), ia[0].arquivos[0].nome.as_str()), (1, "deteccao", "a.onnx"));
}

/// O interruptor, o item e o modelo dos veículos são conferidos antes de qualquer sessão.
#[test]
fn novos_recusa_sem_item_e_com_interruptor() {
    let msg = |r: Result<Detectores>| match r {
        Err(SicroError::Validation(m)) => m,
        Err(e) => e.to_string(),
        Ok(_) => "aberto".into(),
    };
    let desl = AiSettings { modelos_desligados: true, ..Default::default() };
    assert_eq!(msg(Detectores::novos(&desl, &[ClasseDeteccao::Rosto])), codigo::IA_DESLIGADA);
    assert_eq!(msg(Detectores::novos(&AiSettings::default(), &[ClasseDeteccao::Rosto])), "modelo_ausente: redacao");
    let pasta = tempfile::tempdir().unwrap();
    let ai = AiSettings { redacao_dir: pasta.path().to_string_lossy().into_owned(), ..Default::default() };
    assert_eq!(msg(Detectores::novos(&ai, &[ClasseDeteccao::Placa])), "modelo_ausente: pessoas");
    assert!(!instalado(&ai) && !instalado(&AiSettings::default()));
    // arquivo do tamanho certo com outro conteúdo: instalado pelo tamanho, recusado ao abrir (SHA-256)
    for a in &ARQUIVOS {
        let f = std::fs::File::create(pasta.path().join(a.nome)).unwrap();
        f.set_len(a.bytes).unwrap();
    }
    assert!(instalado(&ai));
    assert_eq!(msg(Detectores::novos(&ai, &[ClasseDeteccao::Rosto])), "modelo_ausente: redacao");
    assert_eq!(mb(), 245);
}

/// Pela fábrica (`ia::criar_sessao`) sem GPU pronta: o conjunto de queda em CPU, com os arquivos dele na corrida.
#[test]
fn sem_gpu_pronta_abre_o_conjunto_de_cpu() {
    let teste = "sem_gpu_pronta_abre_o_conjunto_de_cpu";
    let Some(mut ai) = t::ou_dispensa(t::ambiente(), teste, "a lib dos testes") else { return };
    let Some(m) = t::ou_dispensa(t::modelos(), teste, "a pasta de modelos") else { return };
    let Some(coco) = t::ou_dispensa(yolox_coco(), teste, "yolox_s.onnx") else { return };
    ai.redacao_dir = m.join("redacao").to_string_lossy().into_owned();
    ai.pessoas_model_path = coco.to_string_lossy().into_owned();
    let d = Detectores::novos(&ai, &[ClasseDeteccao::Rosto, ClasseDeteccao::Placa]).unwrap();
    assert_eq!((d.conjunto, d.provedor), (Conjunto::Queda, Provedor::Cpu));
    let nomes = |c| d.modelo(c).arquivos.iter().map(|a| a.nome.clone()).collect::<Vec<_>>();
    assert_eq!(nomes(ClasseDeteccao::Cabeca), vec![CORPO_N.nome]);
    assert_eq!(nomes(ClasseDeteccao::Rosto), vec![ROSTO_YUNET.nome]);
    assert_eq!(nomes(ClasseDeteccao::Placa), vec![PLACA_YOLOX.nome, VEICULO_COCO.nome]);
}

// ------------------------------------------------------------------ cobertura de p3b/ reproduzida

/// Sessão de GPU (CUDA) pela lib e pelas bibliotecas dos testes; None = sem placa ou sem as bibliotecas (dispensa,
/// falha com `SICRO_EXIGE_GPU=1`).
fn sessao_gpu(p: &Path, teste: &str) -> Option<SessaoIa> {
    let pac = t::ou_dispensa_gpu(t::pacote_gpu(), teste, "a lib e as bibliotecas da NVIDIA")?;
    t::ou_dispensa_gpu(crate::ia::gpu::placa(), teste, "placa NVIDIA")?;
    let s = crate::ia::criar_sessao_com(&pac.lib, Some(&pac), Modelo::Arquivo(p), &Opcoes { aquecer: Some([1, 3, 640, 640]), ..Opcoes::default() }).ok()?;
    t::ou_dispensa_gpu((s.provedor == Provedor::Cuda).then_some(s), teste, "sessão CUDA")
}

/// Imagens decodificadas em paralelo, em lotes (a inferência é em série).
fn em_lotes(caminhos: &[PathBuf], mut f: impl FnMut(usize, &Img)) {
    use rayon::prelude::*;
    for (k, lote) in caminhos.chunks(16).enumerate() {
        let imgs: Vec<Img> = lote.par_iter().map(|p| Img::abrir(p).unwrap_or_else(|| panic!("{}", p.display()))).collect();
        for (i, img) in imgs.iter().enumerate() {
            f(k * 16 + i, img);
        }
    }
}

/// Fração de cada caixa do gabarito coberta por UMA região (`cabecas_medir.py::cobertura`).
fn cobertura(g: &[[f64; 4]], r: &[[f64; 4]]) -> Vec<f64> {
    g.iter()
        .map(|a| {
            let area = (a[2] - a[0]) * (a[3] - a[1]);
            r.iter().map(|b| ((a[2].min(b[2]) - a[0].max(b[0])).max(0.0) * (a[3].min(b[3]) - a[1].max(b[1])).max(0.0)) / area).fold(0.0, f64::max)
        })
        .collect()
}

fn expandir(d: &Deteccao, m: f64) -> [f64; 4] {
    let (w, h) = (d.w() as f64, d.h() as f64);
    [d.x1 as f64 - w * m, d.y1 as f64 - h * m, d.x2 as f64 + w * m, d.y2 as f64 + h * m]
}

/// Cabeças do `pinto_s@1024` (`cabecas_rodar.py`): lado até 1024, classes 0, 2 e 3 juntas, limiar 0,1.
fn cabecas_1024(m: &mut YoloxN, q: &Imagem) -> Vec<Deteccao> {
    let (h, w) = (q.h as f64, q.w as f64);
    let r = (1024.0 / h.max(w)).min(1.0);
    let hw = ((dec::arred(h * r / 32.0).max(1) * 32) as usize, (dec::arred(w * r / 32.0).max(1) * 32) as usize);
    m.detectar(q, 0.1, 0.45, &[0, 2, 3], Some(hw)).unwrap().into_iter().filter(|d| d.classe == dec::CABECA_WB12).collect()
}

/// `p3b/`: cabeças do CrowdHuman val (800 imagens, 17 749 cabeças), YOLOX-S WholeBody12 com lado 1024, cabeça ≥ 0,2
/// e margem de 15 %: 80,0 % cobertas (14 197). Reproduzido em GPU a até 0,3 ponto.
#[test]
#[ignore = "pesado (16 min com GPU, máquina ocupada): cobertura de p3b, rodar com --ignored"]
fn cobertura_de_cabecas_reproduzida() {
    let teste = "cobertura_de_cabecas_reproduzida";
    let _trava = t::trava_gpu();
    let Some(p) = t::ou_dispensa(redacao(&CORPO_S), teste, CORPO_S.nome) else { return };
    let anot = dossie("p3b/dados/crowdhuman/annotation_val.odgt");
    let Some(texto) = t::ou_dispensa(std::fs::read_to_string(&anot).ok(), teste, "o CrowdHuman val") else { return };
    let Some(s) = sessao_gpu(&p, teste) else { return };
    let mut m = YoloxN::nova(s, Entrada::Esticar).unwrap();
    let r: Value = serde_json::from_str(REFERENCIA).unwrap();
    let esperado = &r["cobertura"]["cabecas"];
    let linhas: Vec<Value> = texto.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let passo = (linhas.len() / 800).max(1);
    let lista: Vec<&Value> = linhas.iter().step_by(passo).take(800).collect();
    let gt: Vec<Vec<[f64; 4]>> = lista
        .iter()
        .map(|a| {
            a["gtboxes"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|b| b["tag"] == "person" && b["head_attr"]["ignore"].as_i64().unwrap_or(0) == 0 && b["extra"]["ignore"].as_i64().unwrap_or(0) == 0)
                .filter_map(|b| {
                    let v: Vec<f64> = b["hbox"].as_array()?.iter().map(|x| x.as_f64().unwrap()).collect();
                    (v[2] > 1.0 && v[3] > 1.0).then(|| [v[0], v[1], v[0] + v[2], v[1] + v[3]])
                })
                .collect()
        })
        .collect();
    let caminhos: Vec<PathBuf> = lista.iter().map(|a| dossie(&format!("p3b/dados/crowdhuman/Images/{}.jpg", a["ID"].as_str().unwrap()))).collect();
    let (mut total, mut cobertas) = (0usize, 0usize);
    em_lotes(&caminhos, |i, img| {
        let reg: Vec<[f64; 4]> = cabecas_1024(&mut m, &img.q()).iter().filter(|d| d.score >= 0.2).map(|d| expandir(d, 0.15)).collect();
        let c = cobertura(&gt[i], &reg);
        total += c.len();
        cobertas += c.iter().filter(|&&x| x >= 0.8).count();
    });
    let (et, ec) = (esperado["cabecas"].as_u64().unwrap() as usize, esperado["cobertas"].as_u64().unwrap() as usize);
    eprintln!("cabeças do CrowdHuman: {cobertas}/{total} = {:.2} % (referência {ec}/{et} = {:.2} %)", 100.0 * cobertas as f64 / total as f64, 100.0 * ec as f64 / et as f64);
    assert_eq!(total, et);
    assert!((cobertas as f64 - ec as f64).abs() <= 0.003 * et as f64, "{cobertas} × {ec}");
}

/// `p3b/`: rostos do WIDER val (267 imagens, 4 233 rostos) cobertos por cabeça (YOLOX-S WB12, ≥ 0,2, +15 %) OU rosto
/// (RetinaFace R34 a 2×, ≥ 0,67, +25 %): 87,1 % (3 686). Reproduzido em GPU a até 0,5 ponto.
#[test]
#[ignore = "pesado (16 min com GPU, máquina ocupada): cobertura de p3b, rodar com --ignored"]
fn cobertura_de_rostos_reproduzida() {
    let teste = "cobertura_de_rostos_reproduzida";
    let _trava = t::trava_gpu();
    let (Some(pc), Some(pr)) = (t::ou_dispensa(redacao(&CORPO_S), teste, CORPO_S.nome), t::ou_dispensa(redacao(&ROSTO_R34), teste, ROSTO_R34.nome)) else { return };
    let Some(gt_txt) = t::ou_dispensa(std::fs::read_to_string(dossie("p3b/dados/widerface/wider_face_split/wider_face_val_bbx_gt.txt")).ok(), teste, "o gabarito do WIDER") else { return };
    let Some(sc) = sessao_gpu(&pc, teste) else { return };
    let Some(sr) = sessao_gpu(&pr, teste) else { return };
    let (mut corpo, mut retina) = (YoloxN::nova(sc, Entrada::Esticar).unwrap(), RetinaFace::nova(sr));
    // gabarito: x y w h blur expr illum invalid occl pose; só os válidos com lado
    let mut gt: HashMap<String, Vec<[f64; 4]>> = HashMap::new();
    let ls: Vec<&str> = gt_txt.lines().collect();
    let mut i = 0;
    while i < ls.len() && !ls[i].trim().is_empty() {
        let nome = ls[i].trim().to_string();
        let n: usize = ls[i + 1].trim().parse().unwrap();
        let caixas = ls[i + 2..i + 2 + n.max(1)]
            .iter()
            .take(n)
            .map(|l| l.split_whitespace().map(|x| x.parse::<f64>().unwrap()).collect::<Vec<_>>())
            .filter(|v| v[7] == 0.0 && v[2] > 0.0 && v[3] > 0.0)
            .map(|v| [v[0], v[1], v[0] + v[2], v[1] + v[3]])
            .collect();
        gt.insert(nome, caixas);
        i += 2 + n.max(1);
    }
    let r: Value = serde_json::from_str(REFERENCIA).unwrap();
    let esperado = &r["cobertura"]["rostos"];
    let nomes: Vec<String> = esperado["nomes"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().replace('|', "/")).collect();
    let caminhos: Vec<PathBuf> = nomes.iter().map(|n| dossie(&format!("p3b/dados/widerface/WIDER_val/images/{n}"))).collect();
    let (mut total, mut cobertos) = (0usize, 0usize);
    em_lotes(&caminhos, |k, img| {
        let q = img.q();
        let mut reg: Vec<[f64; 4]> = cabecas_1024(&mut corpo, &q).iter().filter(|d| d.score >= 0.2).map(|d| expandir(d, 0.15)).collect();
        reg.extend(retina.detectar(&q, 0.05, 0.4, 2048, true).unwrap().iter().filter(|d| d.score >= 0.67).map(|d| expandir(d, 0.25)));
        let c = cobertura(&gt[&nomes[k]], &reg);
        total += c.len();
        cobertos += c.iter().filter(|&&x| x >= 0.8).count();
    });
    let (et, ec) = (esperado["rostos"].as_u64().unwrap() as usize, esperado["cobertos"].as_u64().unwrap() as usize);
    eprintln!("rostos do WIDER (cabeça OU rosto): {cobertos}/{total} = {:.2} % (referência {ec}/{et} = {:.2} %)", 100.0 * cobertos as f64 / total as f64, 100.0 * ec as f64 / et as f64);
    assert_eq!(total, et);
    assert!((cobertos as f64 - ec as f64).abs() <= 0.005 * et as f64, "{cobertos} × {ec}");
}

/// `p3b/`: placas brasileiras das 67 fotos do Commons pelo `placas()` (RT-DETR + YOLOX no quadro inteiro e no
/// recorte de cada veículo, score normalizado ≥ 0,5): a referência acha 101 de 107 (94,4 %) com 38 falsas. Em GPU
/// quando há placa, senão em CPU; até 1 placa e 4 falsas de diferença.
#[test]
#[ignore = "pesado (16 min com GPU, máquina ocupada): cobertura de p3b, rodar com --ignored"]
fn cobertura_de_placas_reproduzida() {
    let teste = "cobertura_de_placas_reproduzida";
    let _trava = t::trava_gpu();
    let (Some(prt), Some(ppl), Some(pco)) = (t::ou_dispensa(redacao(&PLACA_RTDETR), teste, PLACA_RTDETR.nome), t::ou_dispensa(redacao(&PLACA_YOLOX), teste, PLACA_YOLOX.nome), t::ou_dispensa(yolox_coco(), teste, "yolox_s.onnx")) else { return };
    let Some(gtxt) = t::ou_dispensa(std::fs::read_to_string(dossie("p3b/dados/placas_commons/gabarito.json")).ok(), teste, "o gabarito das placas") else { return };
    let gpu = t::pacote_gpu().is_some() && crate::ia::gpu::placa().is_some();
    let abrir = |p: &Path| if gpu { sessao_gpu(p, teste) } else { t::sessao_cpu(Modelo::Arquivo(p)) };
    let (Some(srt), Some(spl), Some(sco)) = (abrir(&prt), abrir(&ppl), abrir(&pco)) else { return };
    let mut rt = RtDetr { s: srt };
    let mut pl = YoloxN::nova(spl, Entrada::Letterbox).unwrap();
    let mut co = YoloxN::nova(sco, Entrada::Letterbox).unwrap();
    let gt: Value = serde_json::from_str(&gtxt).unwrap();
    let mut nomes: Vec<&String> = gt.as_object().unwrap().keys().collect();
    nomes.sort();
    let caminhos: Vec<PathBuf> = nomes.iter().map(|n| dossie(&format!("p3b/dados/placas_commons/img/{n}"))).collect();
    let ioa = |a: &[f64; 4], b: &[f64; 4]| (a[2].min(b[2]) - a[0].max(b[0])).max(0.0) * (a[3].min(b[3]) - a[1].max(b[1])).max(0.0) / ((a[2] - a[0]) * (a[3] - a[1]) + 1e-9);
    let caixas = |v: &Value| -> Vec<[f64; 4]> { v.as_array().unwrap().iter().map(|c| std::array::from_fn(|i| c[i].as_f64().unwrap())).collect() };
    let (mut achadas, mut falsas, mut placas) = (0usize, 0usize, 0usize);
    em_lotes(&caminhos, |k, img| {
        let g = &gt[nomes[k].as_str()];
        let (p, ign) = (caixas(&g["P"]), caixas(&g["I"]));
        let d: Vec<Deteccao> = dec::placas(Some(&mut rt), &mut pl, &mut co, &img.q()).unwrap().0.into_iter().filter(|d| d.score >= 0.5).collect();
        let mut usado = vec![false; p.len()];
        for x in &d {
            let xc = caixa(x);
            let ious: Vec<f64> = p.iter().enumerate().map(|(j, b)| if usado[j] { 0.0 } else { iou(&xc, b) }).collect();
            if let Some((j, v)) = ious.iter().copied().enumerate().max_by(|a, b| a.1.total_cmp(&b.1)) {
                if v >= 0.4 {
                    usado[j] = true;
                    continue;
                }
            }
            if p.iter().chain(&ign).any(|b| ioa(&xc, b) >= 0.5 || iou(&xc, b) >= 0.2) {
                continue;
            }
            falsas += 1;
        }
        placas += p.len();
        achadas += usado.iter().filter(|&&u| u).count();
    });
    let e = &r_placas();
    eprintln!("placas ({}): {achadas}/{placas} = {:.1} %, {falsas} falsas em {} imagens (referência {}/{}, {} falsas)", if gpu { "GPU" } else { "CPU" }, 100.0 * achadas as f64 / placas as f64, nomes.len(), e.1, e.0, e.2);
    assert_eq!(placas, e.0);
    assert!(achadas.abs_diff(e.1) <= 1 && falsas.abs_diff(e.2) <= 4, "{achadas} achadas, {falsas} falsas");
}

fn r_placas() -> (usize, usize, usize) {
    let r: Value = serde_json::from_str(REFERENCIA).unwrap();
    let p = &r["cobertura"]["placas"];
    (p["placas"].as_u64().unwrap() as usize, p["achadas"].as_u64().unwrap() as usize, p["fp"].as_u64().unwrap() as usize)
}

// ------------------------------------------------------------------ propor → folha → confirmar → render

/// Caso com o banco, a foto `c032` do Commons (10 placas) virada em 6 quadros de vídeo e registrada como o app
/// registra, e o projeto com um trecho do vídeo inteiro.
struct CasoFluxo {
    _d: tempfile::TempDir,
    ws: PathBuf,
    cache: PathBuf,
    cap: std::sync::Arc<crate::edicao::ffmpeg::sonda::Capacidades>,
    tab: TabelaQuadros,
    sha: String,
    projeto: Value,
}

fn caso_fluxo(teste: &str) -> Option<CasoFluxo> {
    caso_fluxo_com(teste, "6", None, 6, (6, 1))
}

/// `taxa` do vídeo (`-framerate`), início (`-output_ts_offset`), `n` quadros e a taxa do projeto.
fn caso_fluxo_com(teste: &str, taxa: &str, inicio: Option<&str>, n: usize, fps: (i64, i64)) -> Option<CasoFluxo> {
    let cap = crate::edicao::testes::fontes::cap()?;
    let foto = t::ou_dispensa(Some(dossie("p3b/dados/placas_commons/img/c032.jpg")).filter(|p| p.is_file()), teste, "c032.jpg")?;
    let d = tempfile::Builder::new().prefix("p45-fluxo-").tempdir().unwrap();
    let pai = d.path().join("Casos ç");
    std::fs::create_dir_all(&pai).unwrap();
    let ws = crate::workspace::create::create_workspace(crate::models::NewOccurrenceInput { peritos: vec!["Perito P45".into()], ..Default::default() }, &pai).unwrap().path;
    let video = d.path().join("placas.mp4");
    let mut ff = std::process::Command::new(&cap.binario);
    ff.args(["-v", "error", "-y", "-loop", "1", "-framerate", taxa, "-i"])
        .arg(&foto)
        .args(["-frames:v", &n.to_string(), "-c:v", "libx264", "-preset", "veryfast", "-crf", "12", "-pix_fmt", "yuv420p"]);
    if let Some(s) = inicio {
        ff.args(["-output_ts_offset", s]);
    }
    let st = ff.arg(&video).status().unwrap();
    assert!(st.success(), "{}", cap.versao);
    let vm = tauri::async_runtime::block_on(crate::commands::video_commands::register_video_media(
        ws.to_string_lossy().into(),
        crate::models::RegisterVideoInput { source_path: video.to_string_lossy().into(), allow_duplicate: false },
    ))
    .unwrap();
    let caminho = crate::workspace::paths::resolver_relativo(&ws, &vm.relative_path);
    let tab = TabelaQuadros::do_decodificador(&cap, &caminho, &AtomicBool::new(false), &mut |_| {}).unwrap();
    assert_eq!(tab.pts.len(), n);
    let mut m = crate::edicao::testes::aceite::midia("m1", "video", json!([]));
    (m["sha256"], m["relativo"], m["media_id"]) = (json!(vm.sha256), json!(vm.relative_path), json!(vm.id.to_string()));
    let p = crate::edicao::testes::aceite::projeto(1920, 1280, fps, vec![m], vec![crate::edicao::testes::aceite::trecho("c1", "m1", &tab, 0, n, 1.0, json!([]))], json!({}));
    let cache = d.path().join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    Some(CasoFluxo { ws, cache, cap, tab, sha: vm.sha256.clone(), projeto: serde_json::to_value(&p).unwrap(), _d: d })
}

/// Detecção de placas pelo comando (conjunto de CPU com a lib dos testes), à espera do fim.
fn propor(c: &CasoFluxo, placa: &Path, coco: &Path) -> FimDeteccao {
    // uma detecção por processo (`rastreio_em_curso`): os testes de fluxo esperam a vez
    static VEZ: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _vez = VEZ.lock().unwrap_or_else(|e| e.into_inner());
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let emitir: EmissorDeteccao = Box::new(move |e| {
        let _ = tx.lock().unwrap().send(e);
    });
    let mut criar = |_: &AiSettings, classes: &[ClasseDeteccao]| -> Result<Detectores> {
        assert_eq!(classes, [ClasseDeteccao::Placa]);
        let s = |p: &Path| t::sessao_cpu(Modelo::Arquivo(p)).unwrap();
        Detectores::com(Conjunto::Queda, None, None, None, Some(s(placa)), Some(s(coco)))
    };
    let pedido = PedidoDetectar { clipe_id: "c1".into(), classes: vec![ClasseDeteccao::Placa] };
    let tarefa = pedir_com(&c.ws, &c.cache, &AiSettings::default(), &c.projeto, &pedido, emitir, &mut criar).unwrap();
    let mut avancos = 0;
    loop {
        match rx.recv_timeout(std::time::Duration::from_secs(600)).expect("fim da detecção") {
            EventoDeteccao::Avanco(a) => {
                assert_eq!((a.tarefa_id.as_str(), a.total as usize), (tarefa.as_str(), c.tab.pts.len()));
                avancos += 1;
            }
            EventoDeteccao::Fim(f) => {
                assert!(avancos >= 1 && f.tarefa_id == tarefa && !f.cancelada);
                return f;
            }
            EventoDeteccao::Falhou(f) => panic!("{}", f.mensagem),
        }
    }
}

/// Aceite do P4.5, de ponta a ponta nos dois binários: propostas de placa nascem sem confirmação; com elas no projeto
/// (rastreio + ocultar da mídia) o render RECUSA (`rastreio_nao_confirmado`); a folha de contato de cada uma sai com
/// as células e os tempos; confirmada (o que o perito viu: `denso_sha256` e o intervalo), compila e o render cobre as
/// placas com o sólido (pixel medido), onde o original mostrava a placa.
#[test]
fn propor_folha_confirmar_render() {
    let teste = "propor_folha_confirmar_render";
    let Some(c) = caso_fluxo(teste) else { return };
    let (Some(placa), Some(coco)) = (t::ou_dispensa(redacao(&PLACA_YOLOX), teste, PLACA_YOLOX.nome), t::ou_dispensa(yolox_coco(), teste, "yolox_s.onnx")) else { return };
    if t::ou_dispensa(t::ort_lib(), teste, "a lib dos testes").is_none() {
        return;
    }
    let fim = propor(&c, &placa, &coco);
    // toda sugestão nasce sem confirmação, de origem deteccao, com denso gravado no projeto
    assert!(fim.rastreios.len() >= 3, "{} propostas", fim.rastreios.len());
    let pasta = armazem::pasta_projeto(&c.ws, "p-teste");
    for r in &fim.rastreios {
        assert_eq!((r.origem.clone(), r.confirmacao.is_none(), r.midia.as_deref()), (OrigemRastreio::Deteccao { classe: ClasseDeteccao::Placa }, true, Some("m1")));
        let d = r.denso.as_ref().unwrap();
        let a = arquivo::ler(&pasta, &r.id, &d.sha256).unwrap();
        assert_eq!(a.pts.first(), c.tab.pts.first());
        assert_eq!(arquivo::corridas(&a).unwrap()[0].motor.motor, MotorRastreio::Deteccao);
    }
    // como a tela aplica: rastreios + ocultar da mídia (sólido, placa), no intervalo do denso
    let mut doc = c.projeto.clone();
    doc["rastreios"] = serde_json::to_value(&fim.rastreios).unwrap();
    doc["midias"][0]["ocultacoes"] = Value::Array(
        fim.rastreios
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let d = r.denso.as_ref().unwrap();
                crate::edicao::testes::aceite::ocultar(&format!("o{i}"), &r.id, "solido", json!({ "ativo": { "de": d.de_s, "ate": d.ate_s }, "conteudo": "placa",
                    "alvo": { "rastreio": r.id, "forma": "retangulo", "vertices": null, "margem": 0.0, "borda_suave": 0.0, "inverter": false } }))
            })
            .collect(),
    );
    let compilar = |doc: &Value| {
        let p = armazem::tipado(doc).unwrap();
        let prep = executar::preparar(&c.ws, &p, &c.cache, false).unwrap();
        crate::edicao::motor::compilar(&p, &prep.contexto(), &crate::edicao::testes::aceite::alvo_final("cpu"))
    };
    let e = compilar(&doc).map(|_| ()).unwrap_err().to_string();
    assert!(e.contains(codigo::RASTREIO_NAO_CONFIRMADO), "{e}");
    // folha de contato de cada sugestão
    for r in &fim.rastreios {
        let d = r.denso.as_ref().unwrap();
        let f = tira::folha(&c.ws, &c.cache, "p-teste", &c.sha, &d.sha256, 12).unwrap();
        assert_eq!(f.tempos.len(), 6);
        assert_eq!(f.tempos, c.tab.pts.iter().map(|&p| p as f64 * c.tab.tb.num as f64 / c.tab.tb.den as f64).collect::<Vec<_>>());
        let img = image::open(c.cache.join(&f.relativo)).unwrap().to_rgb8();
        assert_eq!((img.width(), img.height()), (f.colunas * tira::LADO_CELULA, tira::LADO_CELULA));
        // cada célula com imagem (não o fundo) e a moldura do alvo na cor da detecção
        for k in 0..6u32 {
            let cel: Vec<&image::Rgb<u8>> = (0..tira::LADO_CELULA).flat_map(|y| (0..tira::LADO_CELULA).map(move |x| (x, y))).map(|(x, y)| img.get_pixel(k * tira::LADO_CELULA + x, y)).collect();
            let fundo = cel.iter().filter(|p| p.0 == [40, 40, 40]).count();
            assert!(fundo < cel.len() / 2, "célula {k} vazia");
            assert!(cel.iter().any(|p| p.0[0] > 200 && p.0[1] > 150 && p.0[2] < 140), "célula {k} sem a moldura");
        }
        // pedida de novo: o mesmo arquivo, sem ler o vídeo
        assert_eq!(tira::folha(&c.ws, &c.cache, "p-teste", &c.sha, &d.sha256, 12).unwrap(), f);
    }
    // confirmar = o perito viu a folha: quem, quando, o conteúdo (SHA-256) e o intervalo
    for (i, r) in fim.rastreios.iter().enumerate() {
        let d = r.denso.as_ref().unwrap();
        doc["rastreios"][i]["confirmacao"] = json!({ "por": "Perito P45", "em": "2026-11-17T20:00:00Z", "denso_sha256": d.sha256, "de_s": d.de_s, "ate_s": d.ate_s });
    }
    // confirmação de outro conteúdo não vale
    let mut outro = doc.clone();
    outro["rastreios"][0]["confirmacao"]["denso_sha256"] = json!("f".repeat(64));
    assert!(compilar(&outro).map(|_| ()).unwrap_err().to_string().contains(codigo::RASTREIO_NAO_CONFIRMADO));
    let plano = compilar(&doc).expect("confirmada compila");
    let r = crate::edicao::testes::executor::rodar(&c.cap, &plano, &crate::edicao::testes::fontes::pasta_temporaria("p45-fluxo"), &crate::edicao::testes::executor::Opcoes { sem_perdas: true, ..Default::default() }).expect("render");
    let mut qs = Vec::new();
    for b in &r.blocos {
        qs.extend(crate::edicao::testes::medir::quadros(&c.cap, b).unwrap());
    }
    assert_eq!(qs.len(), 6);
    let original = Img::abrir(&dossie("p3b/dados/placas_commons/img/c032.jpg")).unwrap();
    let mut placa_visivel_no_original = 0;
    for (j, q) in qs.iter().enumerate() {
        for rr in &fim.rastreios {
            let a = arquivo::ler(&pasta, &rr.id, &rr.denso.as_ref().unwrap().sha256).unwrap();
            let k = a.pts.iter().position(|&p| p == c.tab.pts[j]).unwrap();
            let (x0, y0) = ((a.x[k] * 1920.0).ceil() as u32 + 2, (a.y[k] * 1280.0).ceil() as u32 + 2);
            let (x1, y1) = (((a.x[k] + a.w[k]) * 1920.0).floor() as u32 - 2, ((a.y[k] + a.h[k]) * 1280.0).floor() as u32 - 2);
            let (mut escuros, mut n, mut claros_orig) = (0, 0, 0);
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = &q.px[((y * q.w + x) * 3) as usize..][..3];
                    escuros += u32::from(p.iter().all(|&v| v <= 24));
                    let o = &original.rgb[((y * original.w + x) * 3) as usize..][..3];
                    claros_orig += u32::from(o.iter().map(|&v| v as u32).sum::<u32>() > 3 * 60);
                    n += 1;
                }
            }
            assert!(n > 0 && escuros as f64 >= 0.99 * n as f64, "quadro {j}, {}: {escuros}/{n} escuros ({})", rr.nome, c.cap.versao);
            placa_visivel_no_original += u32::from(claros_orig as f64 > 0.2 * n as f64);
        }
    }
    assert!(placa_visivel_no_original > 0, "a medida não distingue: nenhuma placa clara no original");
    eprintln!("{}: {} propostas de placa, todas cobertas nos 6 quadros; {placa_visivel_no_original} caixas claras no original", c.cap.versao, fim.rastreios.len());
}

/// Vídeo a 29,97 que começa em 2,5 s: as propostas ficam nos pts da tabela (o primeiro quadro não é o pts 0), a folha
/// nos tempos deles e, confirmadas, o render cobre a caixa de cada uma em todos os quadros (nos dois binários).
#[test]
fn propor_em_video_a_2997_que_nao_comeca_em_zero() {
    let teste = "propor_em_video_a_2997_que_nao_comeca_em_zero";
    let Some(c) = caso_fluxo_com(teste, "30000/1001", Some("2.5"), 8, (30000, 1001)) else { return };
    let (Some(placa), Some(coco)) = (t::ou_dispensa(redacao(&PLACA_YOLOX), teste, PLACA_YOLOX.nome), t::ou_dispensa(yolox_coco(), teste, "yolox_s.onnx")) else { return };
    if t::ou_dispensa(t::ort_lib(), teste, "a lib dos testes").is_none() {
        return;
    }
    assert!(c.tab.tempo(0) > 2.4, "início {} ({})", c.tab.tempo(0), c.cap.versao);
    let fim = propor(&c, &placa, &coco);
    assert!(fim.rastreios.len() >= 3, "{} propostas", fim.rastreios.len());
    let pasta = armazem::pasta_projeto(&c.ws, "p-teste");
    let mut doc = c.projeto.clone();
    doc["rastreios"] = serde_json::to_value(&fim.rastreios).unwrap();
    let mut ocs = Vec::new();
    for (i, r) in fim.rastreios.iter().enumerate() {
        let d = r.denso.as_ref().unwrap();
        let a = arquivo::ler(&pasta, &r.id, &d.sha256).unwrap();
        assert_eq!(a.pts, c.tab.pts, "{}", r.nome);
        assert!((d.de_s - c.tab.tempo(0)).abs() < 1e-9 && (d.ate_s - c.tab.tempo(7)).abs() < 1e-9);
        let f = tira::folha(&c.ws, &c.cache, "p-teste", &c.sha, &d.sha256, 12).unwrap();
        assert_eq!(f.tempos, (0..8).map(|j| c.tab.tempo(j)).collect::<Vec<_>>());
        doc["rastreios"][i]["confirmacao"] = json!({ "por": "Perito P45", "em": "2026-11-17T20:00:00Z", "denso_sha256": d.sha256, "de_s": d.de_s, "ate_s": d.ate_s });
        ocs.push(crate::edicao::testes::aceite::ocultar(&format!("o{i}"), &r.id, "solido", json!({ "ativo": { "de": d.de_s, "ate": d.ate_s }, "conteudo": "placa",
            "alvo": { "rastreio": r.id, "forma": "retangulo", "vertices": null, "margem": 0.0, "borda_suave": 0.0, "inverter": false } })));
    }
    doc["midias"][0]["ocultacoes"] = Value::Array(ocs);
    let p = armazem::tipado(&doc).unwrap();
    let prep = executar::preparar(&c.ws, &p, &c.cache, false).unwrap();
    let plano = crate::edicao::motor::compilar(&p, &prep.contexto(), &crate::edicao::testes::aceite::alvo_final("cpu")).expect("confirmada compila");
    let r = crate::edicao::testes::executor::rodar(&c.cap, &plano, &crate::edicao::testes::fontes::pasta_temporaria("p45-2997"), &crate::edicao::testes::executor::Opcoes { sem_perdas: true, ..Default::default() }).expect("render");
    let mut qs = Vec::new();
    for b in &r.blocos {
        qs.extend(crate::edicao::testes::medir::quadros(&c.cap, b).unwrap());
    }
    assert_eq!(qs.len(), 8);
    for (j, q) in qs.iter().enumerate() {
        for rr in &fim.rastreios {
            let a = arquivo::ler(&pasta, &rr.id, &rr.denso.as_ref().unwrap().sha256).unwrap();
            let (x0, y0) = ((a.x[j] * 1920.0).ceil() as u32 + 2, (a.y[j] * 1280.0).ceil() as u32 + 2);
            let (x1, y1) = (((a.x[j] + a.w[j]) * 1920.0).floor() as u32 - 2, ((a.y[j] + a.h[j]) * 1280.0).floor() as u32 - 2);
            let (mut escuros, mut n) = (0u32, 0u32);
            for y in y0..y1 {
                for x in x0..x1 {
                    escuros += u32::from(q.px[((y * q.w + x) * 3) as usize..][..3].iter().all(|&v| v <= 24));
                    n += 1;
                }
            }
            assert!(n > 0 && escuros as f64 >= 0.99 * n as f64, "quadro {j}, {}: {escuros}/{n} escuros ({})", rr.nome, c.cap.versao);
        }
    }
}
