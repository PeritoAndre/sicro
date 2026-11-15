//! Régua de escala e velocidade exibida (§2.1 `EfRegua`/`EfVelocidadeExibida`, §3.2): retratos lidos das análises
//! (calibração e cálculo de velocidade do banco do caso), pontas da régua pela homografia, texto da velocidade sempre
//! com faixa ou "≈", e as regras que a validação pergunta. O desenho é de `camada/medidas.rs`. Dono: P1.3.
//!
//! Pontas e textos têm espelho em `palco/desenho/medidas.ts` (fixtures `medidas-*.json`): só `+ − × ÷`, `floor`,
//! `abs` e o `cos_sen_graus` do rasterizador; a homografia pela ordem de `applyH`/`invert3` da análise
//! (`video/editor/medir/geo.ts`), que o espelho importa.

use std::path::Path;

use rusqlite::Connection;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::camada::formas::cos_sen_graus;
use super::mensagens::{self, codigo};
use super::modelo::{
    ClipePrincipal, ClipeSobreposto, EfRegua, EfVelocidadeExibida, Efeito, MetodoCalibracao, Ponto, Problema, ProjetoEdicao,
    RetratoCalculo, RetratoCalibracao, TamanhoPx, Tolerante,
};
use crate::database::repositories::{video_repo, video_speed_repo};
use crate::error::{Result, SicroError};
use crate::models::{VideoMedia, VideoSpeedCalculation, VideoSpeedCalibration};

/// Teto de divisões da régua (alocação por entrada do perito).
pub const MAX_DIVISOES: u32 = 100;
/// Teto do comprimento da régua (m).
pub const MAX_COMPRIMENTO_M: f64 = 1000.0;
/// Folga ao comparar o tempo do quadro com o intervalo medido: o tempo do quadro coletado vem impresso pelo ffmpeg.
pub const TOLERANCIA_S: f64 = 0.0005;
/// Velocidade abaixo disto sai com uma casa decimal.
const KMH_COM_CASA: f64 = 10.0;
/// Teto dos números do texto da velocidade (inteiros exatos em f64 nos dois lados).
const KMH_MAX: f64 = 1e9;
const EPS: f64 = 1e-9;

/// Detalhe do `nao_suportado` enquanto régua e velocidade não renderizam; None = renderizam (ou outro efeito).
/// Uma regra só: `validar` e `clipe::cadeia` perguntam aqui (contratos (h) 1). Desde o P1.3, nenhuma.
pub fn recusa(e: &Efeito) -> Option<&'static str> {
    let _ = e;
    None
}

// ------------------------------------------------------------------ retratos (banco do caso)

/// Retrato da calibração `id` da análise (`list_speed_calibrations`), no espaço de pixel em que foi marcada (§3.2).
/// Erro `calibracao_ausente` se não existe. Lê o banco do caso.
pub fn retrato_calibracao(ws: &Path, id: &str) -> Result<RetratoCalibracao> {
    let (conn, occ) = super::executar::abrir_banco(ws)?;
    calibracao(&conn, &occ, id)
}

/// Retrato do cálculo de velocidade `id` (`list_speed_calculations`): km/h, faixa, intervalo medido e método.
/// Erro `calculo_ausente` se não existe.
pub fn retrato_calculo(ws: &Path, id: &str) -> Result<RetratoCalculo> {
    let (conn, occ) = super::executar::abrir_banco(ws)?;
    calculo(&conn, &occ, id)
}

fn ausente(cod: &str, id: &str) -> SicroError {
    // id vazio: "não há nenhum" (a tela pede o retrato da mais nova, que pode não existir)
    if id.is_empty() {
        SicroError::Validation(cod.to_string())
    } else {
        SicroError::Validation(format!("{cod}: {id}"))
    }
}

fn calibracao(conn: &Connection, occ: &Uuid, id: &str) -> Result<RetratoCalibracao> {
    let falta = || ausente(codigo::CALIBRACAO_AUSENTE, id);
    let uuid = Uuid::parse_str(id).map_err(|_| falta())?;
    let c = video_speed_repo::find_calibration_by_id(conn, &uuid)?.filter(|c| c.occurrence_id == *occ).ok_or_else(falta)?;
    let metodo = match c.method.as_str() {
        "plane" => MetodoCalibracao::Plane,
        "line" => MetodoCalibracao::Line,
        "cross_ratio" => MetodoCalibracao::CrossRatio,
        _ => return Err(SicroError::Validation(format!("{}: metodo", codigo::ITEM_INVALIDO))),
    };
    let midia = video_repo::find_media_by_sha256(conn, occ, &c.media_hash)?;
    let espaco = midia
        .as_ref()
        .and_then(espaco_do_png)
        .ok_or_else(|| SicroError::Validation(format!("{}: {}", codigo::MIDIA_AUSENTE, c.media_hash)))?;
    Ok(RetratoCalibracao {
        id: c.id.to_string(),
        metodo,
        h: c.homography.to_vec(),
        espaco,
        pontos_sha256: hex(&Sha256::digest(serde_json::to_vec(&c.control_points)?)),
        // `residuals_px` é o RMS de reprojeção em METROS (`rms_reprojection` da análise; o nome da coluna é antigo)
        residuo_m: c.residuals_px.filter(|r| r.is_finite()).unwrap_or(0.0),
        regiao: regiao(&c, metodo, espaco),
    })
}

/// Tamanho do PNG que a análise coleta (§3.2, M2 §0): o tamanho do fluxo com o giro do contêiner, SEM a proporção do
/// pixel (o ffmpeg gira o quadro e não reescala).
fn espaco_do_png(m: &VideoMedia) -> Option<TamanhoPx> {
    let bruto: serde_json::Value = serde_json::from_str(&m.raw_probe_json).unwrap_or_default();
    let s = bruto["streams"].as_array().and_then(|l| l.iter().find(|s| s["codec_type"].as_str() == Some("video")));
    let lado = |k: &str| s.and_then(|s| s[k].as_u64()).and_then(|v| u32::try_from(v).ok());
    let (w, h) = (m.width.or_else(|| lado("width"))?, m.height.or_else(|| lado("height"))?);
    if w == 0 || h == 0 {
        return None;
    }
    let giro = s
        .and_then(|s| {
            s["side_data_list"]
                .as_array()
                .and_then(|l| l.iter().find_map(|d| d["rotation"].as_f64()))
                .or_else(|| s["tags"]["rotate"].as_str().and_then(|r| r.trim().parse::<f64>().ok()))
        })
        .filter(|g| g.is_finite())
        .unwrap_or(0.0);
    let quartos = ((giro / 90.0).round() as i64).rem_euclid(4);
    Some(if quartos % 2 == 1 { TamanhoPx { w: h, h: w } } else { TamanhoPx { w, h } })
}

/// Região calibrada em 0..1: no plano, o polígono dos pontos de controle na ordem do retângulo do chão (ângulo em
/// volta do centro no mundo); nos modelos 1D (`line`, `cross_ratio`), o segmento entre os pontos extremos da linha.
fn regiao(c: &VideoSpeedCalibration, metodo: MetodoCalibracao, espaco: TamanhoPx) -> Vec<Ponto> {
    let (w, h) = (espaco.w as f64, espaco.h as f64);
    let em01 = |px: f64, py: f64| Ponto { x: px / w, y: py / h };
    let cps = &c.control_points;
    if cps.is_empty() {
        return Vec::new();
    }
    match metodo {
        MetodoCalibracao::Plane => {
            let n = cps.len() as f64;
            let (mx, my) = (cps.iter().map(|p| p.world_x_m).sum::<f64>() / n, cps.iter().map(|p| p.world_y_m).sum::<f64>() / n);
            let mut v: Vec<_> = cps.iter().collect();
            v.sort_by(|a, b| (a.world_y_m - my).atan2(a.world_x_m - mx).total_cmp(&(b.world_y_m - my).atan2(b.world_x_m - mx)));
            v.into_iter().map(|p| em01(p.px, p.py)).collect()
        }
        MetodoCalibracao::Line | MetodoCalibracao::CrossRatio => {
            let menor = cps.iter().min_by(|a, b| a.world_x_m.total_cmp(&b.world_x_m)).unwrap();
            let maior = cps.iter().max_by(|a, b| a.world_x_m.total_cmp(&b.world_x_m)).unwrap();
            vec![em01(menor.px, menor.py), em01(maior.px, maior.py)]
        }
    }
}

fn calculo(conn: &Connection, occ: &Uuid, id: &str) -> Result<RetratoCalculo> {
    let falta = || ausente(codigo::CALCULO_AUSENTE, id);
    let uuid = Uuid::parse_str(id).map_err(|_| falta())?;
    let c = video_speed_repo::find_calculation_by_id(conn, &uuid)?.filter(|c| c.occurrence_id == *occ).ok_or_else(falta)?;
    Ok(retrato_do_calculo(&c))
}

/// Faixa como a análise mostra (`SpeedGuide.tsx`): Monte Carlo (p2,5 a p97,5), senão o IC da regressão, senão nada.
/// Intervalo medido = do primeiro ao último quadro marcado (tempo de fonte, o do quadro coletado).
fn retrato_do_calculo(c: &VideoSpeedCalculation) -> RetratoCalculo {
    let par = |a: Option<f64>, b: Option<f64>| match (a, b) {
        (Some(a), Some(b)) if a.is_finite() && b.is_finite() => Some((a, b)),
        _ => None,
    };
    let mc = par(c.mc_p2_5_kmh, c.mc_p97_5_kmh);
    let ic = par(c.ci_low, c.ci_high);
    let metodo = if mc.is_some() {
        "monte_carlo"
    } else if c.points.len() <= 2 {
        "media"
    } else {
        "regressao"
    };
    let t: Vec<f64> = c.points.iter().map(|p| p.actual_timestamp_s).filter(|t| t.is_finite()).collect();
    RetratoCalculo {
        id: c.id.to_string(),
        kmh: c.velocity_kmh,
        faixa: mc.or(ic),
        de_s: t.iter().copied().fold(f64::INFINITY, f64::min),
        ate_s: t.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        metodo: metodo.into(),
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// ------------------------------------------------------------------ régua

/// Falha da régua: (código, campo).
type Falha = (&'static str, &'static str);

/// `applyH` da análise: px → mundo (m) pela 3×3 em ordem de linha.
fn aplicar_h(h: &[f64; 9], x: f64, y: f64) -> (f64, f64) {
    let w = h[6] * x + h[7] * y + h[8];
    ((h[0] * x + h[1] * y + h[2]) / w, (h[3] * x + h[4] * y + h[5]) / w)
}

/// Peso projetivo do ponto (o `w` de `aplicar_h`).
fn peso(h: &[f64; 9], x: f64, y: f64) -> f64 {
    h[6] * x + h[7] * y + h[8]
}

/// `invert3` da análise; None = singular (o `cross_ratio` é sempre: a linha do meio é zero).
fn inverter3(m: &[f64; 9]) -> Option<[f64; 9]> {
    let [a, b, c, d, e, f, g, h, i] = *m;
    let ca = e * i - f * h;
    let cb = -(d * i - f * g);
    let cc = d * h - e * g;
    let det = a * ca + b * cb + c * cc;
    if !det.is_finite() || det.abs() < 1e-15 {
        return None;
    }
    let k = 1.0 / det;
    Some([
        ca * k,
        -(b * i - c * h) * k,
        (b * f - c * e) * k,
        cb * k,
        (a * i - c * g) * k,
        -(a * f - c * d) * k,
        cc * k,
        -(a * h - b * g) * k,
        (a * e - b * d) * k,
    ])
}

/// Pontas e divisões em 0..1 da origem; a primeira é a âncora. Erro com (código, campo).
fn pontas(e: &EfRegua) -> std::result::Result<Vec<Ponto>, Falha> {
    let c = &e.calibracao;
    let invalido = |campo| (codigo::ITEM_INVALIDO, campo);
    let fora = (codigo::REGUA_FORA_DA_CALIBRACAO, "comprimento_m");
    let h: [f64; 9] = c.h.as_slice().try_into().map_err(|_| invalido("calibracao"))?;
    if !h.iter().all(|v| v.is_finite()) || c.espaco.w == 0 || c.espaco.h == 0 {
        return Err(invalido("calibracao"));
    }
    if !(e.comprimento_m > 0.0 && e.comprimento_m <= MAX_COMPRIMENTO_M) {
        return Err(invalido("comprimento_m"));
    }
    if e.divisoes > MAX_DIVISOES {
        return Err(invalido("divisoes"));
    }
    if !e.direcao_graus.is_finite() {
        return Err(invalido("direcao_graus"));
    }
    if !(e.ancora.x.is_finite() && e.ancora.y.is_finite()) {
        return Err(invalido("ancora"));
    }
    let (w, a) = (c.espaco.w as f64, c.espaco.h as f64);
    let (px, py) = (e.ancora.x * w, e.ancora.y * a);
    let n = e.divisoes.max(1);
    let (cs, sn) = cos_sen_graus(e.direcao_graus);
    let mut v = Vec::with_capacity(n as usize + 1);
    v.push(e.ancora);
    match c.metodo {
        MetodoCalibracao::Plane | MetodoCalibracao::Line => {
            let inv = inverter3(&h).ok_or(fora)?;
            let (x0, y0) = aplicar_h(&h, px, py);
            // no mundo, o lado da câmera é o do peso da âncora: ponta do outro lado passou do horizonte
            let p0 = peso(&inv, x0, y0);
            if !(x0.is_finite() && y0.is_finite() && p0.is_finite() && p0 != 0.0) {
                return Err(fora);
            }
            for k in 1..=n {
                let d = e.comprimento_m * k as f64 / n as f64;
                let (xm, ym) = (x0 + d * cs, y0 + d * sn);
                let (qx, qy) = aplicar_h(&inv, xm, ym);
                let pk = peso(&inv, xm, ym);
                if !(qx.is_finite() && qy.is_finite() && pk * p0 > 0.0) {
                    return Err(fora);
                }
                v.push(Ponto { x: qx / w + 0.0, y: qy / a + 0.0 });
            }
        }
        MetodoCalibracao::CrossRatio => {
            // modelo 1D: a régua corre na direção da linha de referência; da direção só vale o sentido
            let (ex, ey) = if h[0].abs() + h[1].abs() >= h[6].abs() + h[7].abs() { (h[0], h[1]) } else { (h[6], h[7]) };
            let u0 = h[0] * px + h[1] * py + h[2];
            let w0 = h[6] * px + h[7] * py + h[8];
            let x0 = u0 / w0;
            let alfa = h[0] * ex + h[1] * ey;
            let gama = h[6] * ex + h[7] * ey;
            if !(x0.is_finite() && w0 != 0.0) {
                return Err(fora);
            }
            let sinal = if cs >= 0.0 { 1.0 } else { -1.0 };
            for k in 1..=n {
                let xk = x0 + sinal * (e.comprimento_m * k as f64 / n as f64);
                let d = (xk * w0 - u0) / (alfa - xk * gama);
                let wk = w0 + gama * d;
                if !(d.is_finite() && wk * w0 > 0.0) {
                    return Err(fora);
                }
                v.push(Ponto { x: (px + d * ex) / w + 0.0, y: (py + d * ey) / a + 0.0 });
            }
        }
    }
    Ok(v)
}

/// Pontas e divisões da régua em 0..1 da origem do clipe: âncora no chão, direção e comprimento pela homografia do
/// retrato (`line`, `plane`, `cross_ratio`). Primeiro e último = pontas.
/// `plane` e `line`: a ponta é `H⁻¹(H(âncora) + N·(cos, sen))`, com a direção em graus no mundo da calibração (0° =
/// eixo x dela). `cross_ratio` (1D): a ponta anda na direção da linha de referência até `X(âncora) ± N`; da direção
/// só vale o sentido (cos ≥ 0: para mais). `divisoes` 0 = só as pontas.
pub fn pontas_da_regua(e: &EfRegua) -> Result<Vec<Ponto>> {
    pontas(e).map_err(|(cod, campo)| SicroError::Validation(format!("{cod}: {campo}")))
}

// CONTRATO: acréscimo. Validação da régua inteira, para `validar::efeitos` trocar a conferência só da âncora por esta
// (a ponta, o segmento dos modelos 1D e os campos que o motor recusa ficavam de fora).
/// O que a validação acusa na régua, além de `regua_espaco`: (código, campo). `regua_fora_da_calibracao` com a
/// âncora (campo `ancora`) ou a ponta (`comprimento_m`) fora da região do retrato (polígono no plano; nos modelos 1D,
/// a projeção na linha fora do segmento dos pontos extremos) ou a régua passando do horizonte; `item_invalido` com o
/// campo que o motor recusaria. None = nada.
pub fn problema_da_regua(e: &EfRegua) -> Option<(&'static str, &'static str)> {
    conferidas(e).err()
}

/// As pontas, só com a régua inteira na região do retrato.
fn conferidas(e: &EfRegua) -> std::result::Result<Vec<Ponto>, Falha> {
    let v = pontas(e)?;
    let (ancora, ponta) = (v[0], v[v.len() - 1]);
    let r = &e.calibracao.regiao;
    let dentro = |p: Ponto| match r.len() {
        0 | 1 => true,
        2 => no_segmento(p, r[0], r[1], e.calibracao.espaco),
        _ => no_poligono(p, r),
    };
    if !dentro(ancora) {
        return Err((codigo::REGUA_FORA_DA_CALIBRACAO, "ancora"));
    }
    if !dentro(ponta) {
        return Err((codigo::REGUA_FORA_DA_CALIBRACAO, "comprimento_m"));
    }
    Ok(v)
}

// CONTRATO: acréscimo (revisão). O desenho usa esta: régua fora da região calibrada não sai no vídeo nem que a
// validação deixe passar (hoje `validar` só confere a âncora no polígono).
/// `pontas_da_regua` com a regra de `problema_da_regua`: erro `<código>: <campo>` em vez de pontas fora da região.
pub fn pontas_conferidas(e: &EfRegua) -> Result<Vec<Ponto>> {
    conferidas(e).map_err(|(cod, campo)| SicroError::Validation(format!("{cod}: {campo}")))
}

/// Projeção de `p` na reta de `a` a `b` (em px do espaço) cai entre os dois.
fn no_segmento(p: Ponto, a: Ponto, b: Ponto, espaco: TamanhoPx) -> bool {
    let (w, h) = (espaco.w as f64, espaco.h as f64);
    let (vx, vy) = ((b.x - a.x) * w, (b.y - a.y) * h);
    let l2 = vx * vx + vy * vy;
    if !(l2 > 0.0) {
        return false;
    }
    let t = ((p.x - a.x) * w * vx + (p.y - a.y) * h * vy) / l2;
    t >= -EPS && t <= 1.0 + EPS
}

/// Par-ímpar; na aresta conta como dentro (a mesma regra de `validar`).
fn no_poligono(p: Ponto, pol: &[Ponto]) -> bool {
    let n = pol.len();
    let mut dentro = false;
    for i in 0..n {
        let (a, b) = (pol[i], pol[(i + n - 1) % n]);
        let cruz = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        if cruz.abs() <= EPS && p.x >= a.x.min(b.x) - EPS && p.x <= a.x.max(b.x) + EPS && p.y >= a.y.min(b.y) - EPS && p.y <= a.y.max(b.y) + EPS {
            return true;
        }
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            dentro = !dentro;
        }
    }
    dentro
}

// ------------------------------------------------------------------ textos

/// Inteiro (f64 exato) em decimal com `casas` (0 ou 1, 2) depois da vírgula: `n` = valor × 10^casas.
fn decimal(n: f64, casas: u32) -> String {
    let esc = match casas {
        0 => 1.0,
        1 => 10.0,
        _ => 100.0,
    };
    let int = (n / esc).floor();
    let frac = n - int * esc;
    if casas == 0 || frac == 0.0 {
        return format!("{}", int as i64);
    }
    let mut f = format!("{:0width$}", frac as i64, width = casas as usize);
    while f.ends_with('0') {
        f.pop();
    }
    format!("{},{f}", int as i64)
}

/// Texto da velocidade: com a faixa ("42 km/h (39 a 45)") ou com "≈" quando o cálculo não tem faixa. Abaixo de
/// 10 km/h, uma casa decimal. O valor arredonda ao mais perto; a faixa só para fora (mínimo para baixo, máximo para
/// cima) e não desce de 0.
pub fn texto_da_velocidade(c: &RetratoCalculo) -> String {
    let lim = |v: f64| if v.is_nan() { 0.0 } else { v.max(0.0).min(KMH_MAX) };
    let kmh = lim(c.kmh);
    let casas = if kmh < KMH_COM_CASA { 1 } else { 0 };
    let esc = if casas == 1 { 10.0 } else { 1.0 };
    let valor = decimal((kmh * esc + 0.5).floor(), casas);
    match c.faixa {
        Some((a, b)) => {
            let (lo, hi) = (lim(a.min(b)), lim(a.max(b)));
            format!("{valor} km/h ({} a {})", decimal((lo * esc).floor(), casas), decimal((hi * esc).ceil(), casas))
        }
        None => format!("≈ {valor} km/h"),
    }
}

/// Rótulo da régua: o comprimento com até duas casas ("1 m", "0,5 m", "2,25 m").
pub(crate) fn texto_do_comprimento(m: f64) -> String {
    let m = if m.is_nan() { 0.0 } else { m.max(0.0).min(MAX_COMPRIMENTO_M) };
    format!("{} m", decimal((m * 100.0 + 0.5).floor(), 2))
}

/// A velocidade aparece fora de `[calculo.de_s, calculo.ate_s]`? `t_fonte` = tempo de FONTE do quadro mostrado num
/// instante em que o efeito está ativo (trecho: o quadro lido; congelado: o quadro congelado; o `ativo` do congelado
/// é local e não se compara com o cálculo); None = clipe sem fonte (foto, título): sem quadro medido, fora. A
/// validação acusa `velocidade_fora_do_intervalo` (O5). Folga de `TOLERANCIA_S`.
// CONTRATO: era `t: f64` no instante do `ativo`; no congelado esse instante é local (revisão da abertura).
pub fn fora_do_intervalo(e: &EfVelocidadeExibida, t_fonte: Option<f64>) -> bool {
    let c = &e.calculo;
    match t_fonte {
        None => true,
        Some(t) => !(t >= c.de_s - TOLERANCIA_S && t <= c.ate_s + TOLERANCIA_S),
    }
}

// ------------------------------------------------------------------ retrato desatualizado

/// Régua e velocidade ligadas do projeto, com o clipe de cada uma.
fn medidas_do_projeto(p: &ProjetoEdicao) -> Vec<(&str, &Efeito)> {
    let mut v = Vec::new();
    for c in &p.principal {
        let Tolerante::Conhecido(c) = c else { continue };
        let (id, listas): (&str, Vec<&[Tolerante<Efeito>]>) = match c {
            ClipePrincipal::Trecho(x) => (&x.base.id, vec![&x.base.efeitos]),
            ClipePrincipal::Congelar(x) => (&x.base.id, vec![&x.base.efeitos]),
            ClipePrincipal::Imagem(x) => (&x.base.id, vec![&x.base.efeitos]),
            ClipePrincipal::Titulo(x) => (&x.base.id, vec![&x.base.efeitos]),
            ClipePrincipal::Tela(x) => (&x.base.id, std::iter::once(&x.base.efeitos[..]).chain(x.celulas.iter().map(|c| &c.efeitos[..])).collect()),
        };
        for l in listas {
            v.extend(ligadas(l).map(|e| (id, e)));
        }
    }
    for t in &p.sobreposicoes {
        for c in &t.clipes {
            let Tolerante::Conhecido(c) = c else { continue };
            let b = match c {
                ClipeSobreposto::Video(x) => &x.base,
                ClipeSobreposto::Imagem(x) => &x.base,
                ClipeSobreposto::Titulo(x) => &x.base,
            };
            v.extend(ligadas(&b.efeitos).map(|e| (b.id.as_str(), e)));
        }
    }
    v
}

fn ligadas(l: &[Tolerante<Efeito>]) -> impl Iterator<Item = &Efeito> {
    l.iter().filter_map(|e| match e {
        Tolerante::Conhecido(e @ Efeito::Regua(r)) if r.base.ligado => Some(e),
        Tolerante::Conhecido(e @ Efeito::VelocidadeExibida(x)) if x.base.ligado => Some(e),
        _ => None,
    })
}

/// Avisos `retrato_desatualizado` (§2.3): cálculo ou calibração que mudaram (ou sumiram) na análise depois do retrato
/// gravado no projeto. `edicao_analisar` acrescenta à `Analise`. Caso sem banco: nenhum.
pub fn desatualizados(ws: &Path, p: &ProjetoEdicao) -> Vec<Problema> {
    let usos = medidas_do_projeto(p);
    if usos.is_empty() {
        return Vec::new();
    }
    let Ok((conn, occ)) = super::executar::abrir_banco(ws) else { return Vec::new() };
    let mut v = Vec::new();
    for (clipe, e) in usos {
        let (id, campo, mudou) = match e {
            Efeito::Regua(r) => (&r.base.id, "calibracao", calibracao(&conn, &occ, &r.calibracao.id).map_or(true, |x| x != r.calibracao)),
            Efeito::VelocidadeExibida(x) => (&x.base.id, "calculo", calculo(&conn, &occ, &x.calculo.id).map_or(true, |c| c != x.calculo)),
            _ => continue,
        };
        if mudou {
            let mut pr = mensagens::problema(codigo::RETRATO_DESATUALIZADO, None, None);
            pr.clipe = Some(clipe.to_string());
            pr.efeito = Some(id.clone());
            pr.campo = Some(campo.to_string());
            v.push(pr);
        }
    }
    v
}

// `pub(crate)`: as calibrações sintéticas servem também aos casos de `camada/medidas.rs`.
#[cfg(test)]
pub(crate) mod testes {
    use std::collections::HashMap;

    use chrono::Utc;
    use serde_json::{json, Value};

    use super::*;
    use crate::edicao::camada::formas::{self, Primitiva, QuadroCamada};
    use crate::edicao::camada::texto::{Atlas, FonteSprites, PedidoSprite, Sprite};
    use crate::edicao::curva::fixtures;
    use crate::edicao::geometria::Afim;
    use crate::edicao::modelo::EstiloTexto;
    use crate::edicao::motor::{Contexto, FonteInfo};
    use crate::edicao::quadros::apoio;
    use crate::edicao::testes::executor::{self, Opcoes};
    use crate::edicao::testes::fontes::{self, Fonte};
    use crate::edicao::testes::{aceite, medir};
    use crate::models::{ControlPoint, TrajectoryPoint};
    use crate::video::speed::crossratio::{fit_cross_ratio_homography, CrossRatioReference};
    use crate::video::speed::homography::{line_calibration, solve_homography_dlt, Homography};

    pub(crate) const ESPACO: TamanhoPx = TamanhoPx { w: 1920, h: 1080 };
    /// Aceite (§11): pontas a até 1 px.
    const TOLERANCIA_PX: f64 = 1.0;

    /// Câmera sintética, chão (m) → imagem (px): perspectiva forte (100 px/m perto, 40 px/m a 10 m).
    pub(crate) fn camera(x: f64, y: f64) -> (f64, f64) {
        let w = 1.0 + 0.15 * y;
        ((100.0 * x + 30.0 * y + 400.0) / w, (-20.0 * y + 1000.0) / w)
    }

    pub(crate) fn em01(p: (f64, f64)) -> Ponto {
        Ponto { x: p.0 / ESPACO.w as f64, y: p.1 / ESPACO.h as f64 }
    }

    fn linhas(h: &Homography) -> Vec<f64> {
        (0..3).flat_map(|i| (0..3).map(move |j| h.h[(i, j)])).collect()
    }

    fn retrato(metodo: MetodoCalibracao, h: &Homography, regiao: Vec<Ponto>) -> RetratoCalibracao {
        RetratoCalibracao { id: "k1".into(), metodo, h: linhas(h), espaco: ESPACO, pontos_sha256: "a".repeat(64), residuo_m: 0.0, regiao }
    }

    /// `plane`: retângulo de 8 × 10 m no chão da câmera sintética, pelo DLT da análise.
    pub(crate) fn plano() -> RetratoCalibracao {
        let mundo = [(0.0, 0.0), (8.0, 0.0), (8.0, 10.0), (0.0, 10.0)];
        let img = mundo.map(|(x, y)| camera(x, y));
        let h = solve_homography_dlt(&img, &mundo).unwrap();
        retrato(MetodoCalibracao::Plane, &h, img.iter().map(|p| em01(*p)).collect())
    }

    /// `line`: 2,5 m entre (300, 700) e (900, 500).
    pub(crate) const LINHA: ((f64, f64), (f64, f64), f64) = ((300.0, 700.0), (900.0, 500.0), 2.5);

    pub(crate) fn linha() -> RetratoCalibracao {
        let (p1, p2, d) = LINHA;
        retrato(MetodoCalibracao::Line, &line_calibration(p1, p2, d).unwrap(), vec![em01(p1), em01(p2)])
    }

    /// Fila de marcas a 0, 3, 6, 9 e 12 m numa linha da imagem, com a compressão da perspectiva:
    /// `s(w) = 150·w / (1 + 0,08·w)` px a partir de (200, 900), na direção (0,8; −0,6). `desvio` = px de lado.
    pub(crate) fn marca(w: f64, desvio: f64) -> (f64, f64) {
        let s = 150.0 * w / (1.0 + 0.08 * w);
        (200.0 + s * 0.8 + desvio * 0.6, 900.0 - s * 0.6 + desvio * 0.8)
    }

    pub(crate) fn fila() -> RetratoCalibracao {
        let refs: Vec<CrossRatioReference> = [0.0, 3.0, 6.0, 9.0, 12.0]
            .iter()
            .map(|&w| {
                let (px, py) = marca(w, 0.0);
                CrossRatioReference { px, py, world_m: w }
            })
            .collect();
        retrato(MetodoCalibracao::CrossRatio, &fit_cross_ratio_homography(&refs).unwrap(), vec![em01(marca(0.0, 0.0)), em01(marca(12.0, 0.0))])
    }

    pub(crate) fn regua(cal: &RetratoCalibracao, ancora_px: (f64, f64), direcao: f64, metros: f64, divisoes: u32) -> EfRegua {
        serde_json::from_value(json!({ "id": "g1", "ligado": true, "nome": "", "ativo": null, "calibracao": cal, "ancora": em01(ancora_px),
            "direcao_graus": direcao, "comprimento_m": metros, "divisoes": divisoes,
            "traco": { "cor": "#ff0000", "espessura": 0.004, "contorno": false }, "rotulo": false }))
        .unwrap()
    }

    fn distancia(p: Ponto, esperado: (f64, f64)) -> f64 {
        ((p.x * ESPACO.w as f64 - esperado.0).powi(2) + (p.y * ESPACO.h as f64 - esperado.1).powi(2)).sqrt()
    }

    /// Sem texto: a régua sem rótulo não pede sprite.
    struct SemTexto;
    impl FonteSprites for SemTexto {
        fn sprite(&self, _: &PedidoSprite) -> Result<Sprite> {
            Err(SicroError::Validation("sem texto".into()))
        }
        fn atlas(&self, _: &EstiloTexto, _: u32) -> Result<Atlas> {
            Err(SicroError::Validation("sem texto".into()))
        }
        fn imagem(&self, _: &str) -> Option<TamanhoPx> {
            None
        }
    }

    /// Canvas = espaço da calibração, A = identidade.
    fn quadro_do_espaco() -> QuadroCamada {
        QuadroCamada { q: 0, t: 0.0, pts: None, a: Afim::IDENTIDADE, canvas: ESPACO, rastreios: Default::default(), indice: None, programa_s: 0.0 }
    }

    /// A régua desenhada pelo rasterizador (canvas = espaço da calibração, A = identidade): o pixel das pontas tem a
    /// cor do traço; além de cada ponta, no sentido da régua, a meia largura mais 3 px já é transparente.
    fn conferir_desenho(e: &EfRegua, ancora: (f64, f64), ponta: (f64, f64)) {
        let p = crate::edicao::camada::medidas::regua(e, &quadro_do_espaco(), &SemTexto).unwrap();
        assert!(matches!(p.as_slice(), [Primitiva::Caminho { .. }]));
        let b = formas::desenhar(&p, ESPACO, [0, 0, ESPACO.w, ESPACO.h], Path::new("/nao/existe"), &[]).unwrap();
        let px = |x: f64, y: f64| {
            let (x, y) = (x.floor() as usize, y.floor() as usize);
            let i = (y * b.w as usize + x) * 4;
            [b.px[i], b.px[i + 1], b.px[i + 2], b.px[i + 3]]
        };
        let (vx, vy) = (ponta.0 - ancora.0, ponta.1 - ancora.1);
        let l = (vx * vx + vy * vy).sqrt();
        let (ux, uy) = (vx / l, vy / l);
        let alem = e.traco.espessura * ESPACO.h as f64 * 0.5 + 3.0;
        for (c, sentido) in [(ancora, -1.0), (ponta, 1.0)] {
            let [r, g, bb, a] = px(c.0, c.1);
            assert!(a >= 250 && r >= 253 && g <= 2 && bb <= 2, "ponta {c:?}: {:?}", [r, g, bb, a]);
            let fora = px(c.0 + sentido * alem * ux, c.1 + sentido * alem * uy);
            assert_eq!(fora[3], 0, "além da ponta {c:?}");
        }
    }

    /// Aceite (§11): régua de 1 m sobre calibração `plane` sintética, pontas (e divisões) a até 1 px em 4 posições.
    #[test]
    fn regua_de_1_m_plane_pontas_a_ate_1_px_em_4_posicoes() {
        let cal = plano();
        let mut pior = 0.0f64;
        let mut comprimentos = Vec::new();
        for ((x, y), dir) in [((1.0, 1.0), 0.0), ((6.0, 2.0), 90.0), ((2.0, 7.0), 45.0), ((5.0, 8.5), 180.0)] {
            let e = regua(&cal, camera(x, y), dir, 1.0, 10);
            let v = pontas_da_regua(&e).unwrap();
            assert_eq!(v.len(), 11);
            let (c, s) = (f64::to_radians(dir).cos(), f64::to_radians(dir).sin());
            for (k, p) in v.iter().enumerate() {
                let d = k as f64 / 10.0;
                let erro = distancia(*p, camera(x + d * c, y + d * s));
                pior = pior.max(erro);
                assert!(erro <= TOLERANCIA_PX, "({x}, {y}) {dir}° divisão {k}: {erro} px");
            }
            assert_eq!(problema_da_regua(&e), None);
            let ponta = camera(x + c, y + s);
            comprimentos.push(distancia(v[0], ponta));
            conferir_desenho(&e, camera(x, y), ponta);
        }
        // a perspectiva: 1 m perto da câmera mede quase o dobro de 1 m ao fundo
        assert!(comprimentos[0] > 1.8 * comprimentos[3], "{comprimentos:?}");
        eprintln!("régua plane: pior erro {pior:e} px; 1 m = {comprimentos:?} px");
    }

    /// `line` e `cross_ratio` (modelo 1D): pontas a até 1 px; o `cross_ratio` não tem inversa (só o 1D dá a régua);
    /// fora do segmento dos pontos extremos é `regua_fora_da_calibracao`.
    #[test]
    fn line_e_cross_ratio() {
        let ((x1, y1), (x2, y2), d) = LINHA;
        let (ux, uy) = ((x2 - x1) / d, (y2 - y1) / d);
        let cal = linha();
        for (dir, (ex, ey), dentro) in [(0.0, (ux, uy), true), (180.0, (-ux, -uy), false), (90.0, (-uy, ux), true)] {
            let e = regua(&cal, (x1, y1), dir, 1.0, 4);
            let v = pontas_da_regua(&e).unwrap();
            for (k, p) in v.iter().enumerate() {
                let f = k as f64 / 4.0;
                let erro = distancia(*p, (x1 + f * ex, y1 + f * ey));
                assert!(erro <= TOLERANCIA_PX, "line {dir}° divisão {k}: {erro} px");
            }
            let esperado = if dentro { None } else { Some((codigo::REGUA_FORA_DA_CALIBRACAO, "comprimento_m")) };
            assert_eq!(problema_da_regua(&e), esperado, "line {dir}°");
        }

        let cal = fila();
        let h: [f64; 9] = cal.h.clone().try_into().unwrap();
        assert!(inverter3(&h).is_none(), "cross_ratio sem linha do meio");
        let mut pior = 0.0f64;
        for (w0, dir, sinal, desvio) in [(3.0, 0.0, 1.0, 0.0), (9.0, 180.0, -1.0, 0.0), (3.0, 0.0, 1.0, 12.0), (6.0, 200.0, -1.0, -8.0)] {
            let e = regua(&cal, marca(w0, desvio), dir, 1.0, 5);
            let v = pontas_da_regua(&e).unwrap();
            for (k, p) in v.iter().enumerate() {
                let erro = distancia(*p, marca(w0 + sinal * k as f64 / 5.0, desvio));
                pior = pior.max(erro);
                assert!(erro <= TOLERANCIA_PX, "cross_ratio de {w0} m, {dir}°, desvio {desvio}: divisão {k} a {erro} px");
            }
            assert_eq!(problema_da_regua(&e), None);
        }
        // passo da fila encolhe com a distância (projetivo, não proporcional)
        let v = pontas_da_regua(&regua(&cal, marca(0.0, 0.0), 0.0, 12.0, 4)).unwrap();
        let passo = |i: usize| distancia(v[i + 1], (v[i].x * ESPACO.w as f64, v[i].y * ESPACO.h as f64));
        assert!(passo(0) > 1.5 * passo(3), "{} {}", passo(0), passo(3));
        // além da última marca
        let e = regua(&cal, marca(11.5, 0.0), 0.0, 1.0, 5);
        assert_eq!(problema_da_regua(&e), Some((codigo::REGUA_FORA_DA_CALIBRACAO, "comprimento_m")));
        let e = regua(&cal, marca(-0.5, 0.0), 0.0, 1.0, 5);
        assert_eq!(problema_da_regua(&e), Some((codigo::REGUA_FORA_DA_CALIBRACAO, "ancora")));
        eprintln!("cross_ratio: pior erro {pior:e} px");
    }

    /// Régua no polígono do plano, campos que o motor recusa e régua que passa do horizonte.
    #[test]
    fn regiao_e_campos_da_regua() {
        let cal = plano();
        // âncora dentro, 3 m para a direita a partir de x = 6 sai do retângulo de 8 m
        assert_eq!(problema_da_regua(&regua(&cal, camera(6.0, 5.0), 0.0, 3.0, 2)), Some((codigo::REGUA_FORA_DA_CALIBRACAO, "comprimento_m")));
        assert_eq!(problema_da_regua(&regua(&cal, camera(-1.0, 5.0), 0.0, 3.0, 2)), Some((codigo::REGUA_FORA_DA_CALIBRACAO, "ancora")));
        assert_eq!(problema_da_regua(&regua(&cal, camera(4.0, 5.0), 270.0, 20.0, 2)), Some((codigo::REGUA_FORA_DA_CALIBRACAO, "comprimento_m")), "atrás da câmera");
        let mut e = regua(&cal, camera(4.0, 5.0), 0.0, 1.0, MAX_DIVISOES + 1);
        assert_eq!(problema_da_regua(&e), Some((codigo::ITEM_INVALIDO, "divisoes")));
        e.divisoes = 0;
        assert_eq!(pontas_da_regua(&e).unwrap().len(), 2);
        for (m, ok) in [(0.0, false), (-1.0, false), (f64::NAN, false), (MAX_COMPRIMENTO_M, true), (MAX_COMPRIMENTO_M * 1.01, false)] {
            e.comprimento_m = m;
            assert_eq!(pontas(&e).err().map(|f| f.1) != Some("comprimento_m"), ok, "{m}");
        }
        e.comprimento_m = 1.0;
        e.calibracao.h.pop();
        assert_eq!(pontas_da_regua(&e).unwrap_err().to_string(), SicroError::Validation("item_invalido: calibracao".into()).to_string());
        let mut e = regua(&cal, camera(4.0, 5.0), 0.0, 1.0, 2);
        e.calibracao.espaco = TamanhoPx { w: 0, h: 1080 };
        assert_eq!(problema_da_regua(&e), Some((codigo::ITEM_INVALIDO, "calibracao")));
        // região do retrato vazia: só a conta
        let mut e = regua(&cal, camera(-1.0, 5.0), 0.0, 1.0, 2);
        e.calibracao.regiao.clear();
        assert_eq!(problema_da_regua(&e), None);
        // o desenho recusa o que a regra acusa: régua fora da região não sai no vídeo; na borda conta como dentro
        let desenho = |e: &EfRegua| crate::edicao::camada::medidas::regua(e, &quadro_do_espaco(), &SemTexto).map(|_| ()).map_err(|x| x.to_string());
        let fora = regua(&cal, camera(6.0, 5.0), 0.0, 3.0, 2);
        assert_eq!(pontas_da_regua(&fora).unwrap().len(), 3);
        assert!(desenho(&fora).unwrap_err().ends_with("regua_fora_da_calibracao: comprimento_m"));
        assert!(desenho(&regua(&cal, camera(-1.0, 5.0), 0.0, 1.0, 2)).unwrap_err().ends_with("regua_fora_da_calibracao: ancora"));
        assert_eq!(desenho(&regua(&cal, camera(6.0, 5.0), 0.0, 2.0, 2)), Ok(()));
        assert!(desenho(&regua(&fila(), marca(11.5, 0.0), 0.0, 1.0, 5)).unwrap_err().ends_with("regua_fora_da_calibracao: comprimento_m"));
    }

    fn calc(kmh: f64, faixa: Option<(f64, f64)>) -> RetratoCalculo {
        RetratoCalculo { id: "c".into(), kmh, faixa, de_s: 3.0, ate_s: 5.0, metodo: "x".into() }
    }

    /// Aceite (§11): velocidade sempre com faixa ou "≈"; a faixa impressa contém a calculada.
    #[test]
    fn texto_com_faixa_ou_aproximado() {
        for (kmh, faixa, texto) in [
            (42.0, Some((39.0, 45.0)), "42 km/h (39 a 45)"),
            (41.6, Some((38.7, 45.2)), "42 km/h (38 a 46)"),
            (41.6, Some((45.2, 38.7)), "42 km/h (38 a 46)"),
            (42.0, None, "≈ 42 km/h"),
            (30.5, None, "≈ 31 km/h"),
            (7.25, Some((6.04, 8.51)), "7,3 km/h (6 a 8,6)"),
            (9.99, None, "≈ 10 km/h"),
            (3.0, Some((-1.2, 7.0)), "3 km/h (0 a 7)"),
            (0.0, None, "≈ 0 km/h"),
            (120.0, Some((119.0, 121.0)), "120 km/h (119 a 121)"),
        ] {
            assert_eq!(texto_da_velocidade(&calc(kmh, faixa)), texto, "{kmh} {faixa:?}");
        }
        // sorteio: forma do texto e faixa impressa por fora da calculada
        let mut x = 0x2545_f491_4f6c_dd1du64;
        let mut sorteio = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 11) as f64 / (1u64 << 53) as f64
        };
        let num = |s: &str| s.replace(',', ".").parse::<f64>().unwrap();
        for _ in 0..2000 {
            let kmh = sorteio() * 180.0;
            let faixa = (sorteio() < 0.7).then(|| (kmh - sorteio() * 10.0, kmh + sorteio() * 10.0));
            let t = texto_da_velocidade(&calc(kmh, faixa));
            match faixa {
                None => assert!(t.starts_with("≈ ") && t.ends_with(" km/h") && !t.contains('('), "{t}"),
                Some((a, b)) => {
                    let (valor, resto) = t.split_once(" km/h (").unwrap_or_else(|| panic!("{t}"));
                    let (lo, hi) = resto.strip_suffix(')').unwrap().split_once(" a ").unwrap();
                    assert!(!valor.starts_with('≈'));
                    assert!(num(lo) <= a.max(0.0) + 1e-9 && num(hi) >= b - 1e-9, "{t} × ({a}, {b})");
                    assert!((num(valor) - kmh).abs() <= if kmh < 10.0 { 0.05 } else { 0.5 } + 1e-9, "{t} × {kmh}");
                }
            }
        }
        for (m, t) in [(1.0, "1 m"), (0.5, "0,5 m"), (2.25, "2,25 m"), (2.05, "2,05 m"), (0.125, "0,13 m"), (10.0, "10 m"), (0.004, "0 m")] {
            assert_eq!(texto_do_comprimento(m), t);
        }
    }

    /// Os glifos do texto (≈, vírgula, "km/h") existem nas três famílias embutidas: nada vira □.
    #[test]
    fn texto_da_velocidade_tem_os_glifos() {
        for fonte in ["sans", "serifa", "mono"] {
            let estilo: EstiloTexto = serde_json::from_value(json!({ "fonte": fonte, "negrito": true, "italico": false, "tamanho": 0.052, "cor": "#ffffff",
                "alinhamento": "centro", "caixa": null, "contorno": null })).unwrap();
            for texto in [texto_da_velocidade(&calc(7.25, Some((6.04, 8.51)))), texto_da_velocidade(&calc(42.0, None)), texto_do_comprimento(0.5)] {
                let pedido = PedidoSprite {
                    blocos: vec![serde_json::from_value(json!({ "id": "v", "papel": "corpo", "conteudo": texto, "estilo": estilo,
                        "lugar": { "x": 0.0, "y": 0.0, "h": "esq", "v": "topo" }, "largura_max": 0.5 })).unwrap()],
                    largura_max: 0.5 * 1920.0 / 1080.0,
                    altura_canvas: 1080,
                    fundo: true,
                };
                let (b, trocados) = crate::edicao::camada::texto::bitmap_sprite(&pedido).unwrap();
                assert!(trocados.is_empty(), "{fonte} «{texto}»: {trocados:?}");
                assert!(b.px.chunks_exact(4).filter(|p| p[3] > 128).count() > 50, "{fonte} «{texto}» vazio");
            }
        }
    }

    fn velocidade(ativo: Value) -> Value {
        aceite::efeito("velocidade_exibida", "v1", json!({ "ativo": ativo, "calculo": { "id": "c", "kmh": 42.0, "faixa": null, "de_s": 3.0, "ate_s": 5.0, "metodo": "media" },
            "onde": { "modo": "fixa", "lugar": { "x": 0.5, "y": 0.06, "h": "centro", "v": "topo" } },
            "estilo": { "fonte": "sans", "negrito": false, "italico": false, "tamanho": 0.052, "cor": "#ffffff", "alinhamento": "centro", "caixa": null, "contorno": null } }))
    }

    /// Só dentro do intervalo medido (O5): a regra direto e pela validação, com o tempo de fonte do quadro mostrado
    /// (trecho: o quadro lido; congelado: o instante dele, nunca o tempo local; título: fora).
    #[test]
    fn fora_do_intervalo() {
        let e: EfVelocidadeExibida = serde_json::from_value(velocidade(Value::Null)).unwrap();
        for (t, fora) in [(Some(2.9), true), (Some(3.0), false), (Some(4.0), false), (Some(5.0), false), (Some(5.0 + TOLERANCIA_S * 0.8), false), (Some(5.001), true), (Some(f64::NAN), true), (None, true)] {
            assert_eq!(super::fora_do_intervalo(&e, t), fora, "{t:?}");
        }

        let tab = apoio::tabela((1, 30), (0..300).collect());
        let fontes: HashMap<String, FonteInfo> = [("m1".to_string(), apoio::fonte(tab.clone(), (320, 180)))].into();
        let cap = apoio::cap_falsa();
        let ctx = Contexto { fontes: &fontes, cap: &cap, rastreios: &apoio::SemRastreios, pasta_projeto: Path::new("/nao/existe"), midias: &[] };
        let acusa = |clipe: Value| {
            let p = aceite::projeto(320, 180, (30, 1), vec![aceite::midia("m1", "video", json!([]))], vec![clipe], json!({}));
            let linha = crate::edicao::tempo::resolver(&p, &ctx);
            let probs = crate::edicao::validar::validar(&p, &linha, &ctx, None);
            probs.iter().any(|x| x.codigo == codigo::VELOCIDADE_FORA_DO_INTERVALO && x.efeito.as_deref() == Some("v1"))
        };
        // trecho de 2 s a 6 s (t = j/30)
        let tr = |ativo: Value| aceite::trecho("c1", "m1", &tab, 60, 180, 1.0, json!([velocidade(ativo)]));
        assert!(acusa(tr(Value::Null)), "ativo no trecho inteiro passa do medido");
        assert!(!acusa(tr(json!({ "de": 3.0, "ate": 5.0 }))));
        assert!(acusa(tr(json!({ "de": 3.0, "ate": tab.tempo(151) }))), "um quadro depois do medido");
        assert!(acusa(tr(json!({ "de": tab.tempo(89), "ate": 5.0 }))), "um quadro antes do medido");
        // congelado: o instante dele (4 s) dentro; o `ativo` local (0..0,5 s) não se compara com o cálculo
        let cg = |j: usize, ativo: Value| json!({ "tipo": "congelar", "id": "c2", "rotulo": "", "midia": "m1", "instante_s": tab.tempo(j), "instante_pts": tab.pts[j],
            "duracao_s": 1.0, "efeitos": [velocidade(ativo)] });
        assert!(!acusa(cg(120, Value::Null)));
        assert!(!acusa(cg(120, json!({ "de": 0.0, "ate": 0.5 }))));
        assert!(acusa(cg(180, Value::Null)), "congelado em 6 s");
        // título: sem quadro medido
        let titulo = json!({ "tipo": "titulo", "id": "c3", "duracao_s": 1.0, "titulo": { "modelo": "livre", "fundo": { "tipo": "transparente" }, "blocos": [] },
            "efeitos": [velocidade(Value::Null)] });
        assert!(acusa(titulo));
    }

    // ---------------------------------------------------------------- banco do caso

    fn ponto_de_controle(p: (f64, f64), x: f64, y: f64) -> ControlPoint {
        ControlPoint { px: p.0, py: p.1, world_x_m: x, world_y_m: y, label: None }
    }

    fn calibracao_no_banco(occ: Uuid, media_hash: &str, method: &str, cps: Vec<ControlPoint>, h: &Homography, residuo: Option<f64>) -> VideoSpeedCalibration {
        VideoSpeedCalibration {
            id: Uuid::new_v4(),
            occurrence_id: occ,
            media_hash: media_hash.into(),
            method: method.into(),
            control_points: cps,
            reference_source: "campo".into(),
            homography: linhas(h).try_into().unwrap(),
            residuals_px: residuo,
            distortion_model: None,
            author: "Perito".into(),
            created_at: Utc::now(),
        }
    }

    fn calculo_no_banco(occ: Uuid, cal: Uuid, tempos: &[f64], mc: Option<(f64, f64)>, ic: Option<(f64, f64)>) -> VideoSpeedCalculation {
        VideoSpeedCalculation {
            id: Uuid::new_v4(),
            occurrence_id: occ,
            media_hash: "a".repeat(64),
            calibration_id: cal,
            points: tempos
                .iter()
                .map(|&t| TrajectoryPoint { storyboard_frame_id: None, export_id: None, px: 1.0, py: 2.0, u_px: 1.0, actual_timestamp_s: t, delta_s: None, manual: true })
                .collect(),
            velocity_kmh: 41.6,
            vx_m_per_s: 11.0,
            vy_m_per_s: 3.0,
            se_m_per_s: ic.map(|_| 0.5),
            ci_low: ic.map(|x| x.0),
            ci_high: ic.map(|x| x.1),
            confidence: ic.map(|_| 0.95),
            r_squared: None,
            residuals: vec![],
            mc_seed: mc.map(|_| 7),
            mc_sigmas: None,
            mc_n: mc.map(|_| 10_000),
            mc_failed: mc.map(|_| 0),
            mc_mean_kmh: mc.map(|_| 41.7),
            mc_median_kmh: mc.map(|_| 41.6),
            mc_p2_5_kmh: mc.map(|x| x.0),
            mc_p97_5_kmh: mc.map(|x| x.1),
            limitations: vec![],
            audit: Value::Null,
            author: "Perito".into(),
            created_at: Utc::now(),
        }
    }

    fn area(pol: &[Ponto]) -> f64 {
        let n = pol.len();
        (0..n).map(|i| pol[i].x * pol[(i + 1) % n].y - pol[(i + 1) % n].x * pol[i].y).sum::<f64>().abs() * 0.5
    }

    /// Retratos lidos do banco de um caso de verdade (`create_workspace`): espaço do PNG com o giro do contêiner,
    /// região na ordem do retângulo, faixa e intervalo do cálculo, erros do catálogo e o aviso `retrato_desatualizado`.
    #[test]
    fn retratos_do_banco_num_caso() {
        let tmp = tempfile::Builder::new().prefix("p13-medidas-").tempdir().unwrap();
        let entrada = crate::models::NewOccurrenceInput { titulo: Some("Medidas".into()), ..Default::default() };
        let ws = crate::workspace::create::create_workspace(entrada, tmp.path()).unwrap().path;
        let occ = crate::workspace::manifest::Manifest::read(&ws).unwrap().occurrence_id;
        let conn = crate::database::connection::open_connection(&crate::workspace::paths::sqlite_path(&ws)).unwrap();
        let agora = Utc::now();
        let sha = "a".repeat(64);
        // 1920×1080 com giro de 90° no contêiner: o PNG coletado é 1080×1920
        let midia = VideoMedia {
            id: Uuid::new_v4(), occurrence_id: occ, original_path: None, relative_path: "Mídias/Vídeos/cam.mp4".into(), filename: "cam.mp4".into(),
            sha256: sha.clone(), size_bytes: 10, duration_s: Some(12.5), codec: Some("h264".into()), width: Some(1920), height: Some(1080),
            pixel_format: Some("yuv420p".into()), fps_declared: Some(30.0), avg_frame_rate: Some("30/1".into()), r_frame_rate: Some("30/1".into()),
            time_base: Some("1/15360".into()), frame_count: Some(375), bitrate: None,
            raw_probe_json: json!({ "streams": [{ "codec_type": "video", "width": 1920, "height": 1080, "sample_aspect_ratio": "4:3", "side_data_list": [{ "rotation": -90 }] }] }).to_string(),
            warnings_json: "[]".into(), created_at: agora, updated_at: agora, derived_from_hash: None, derivation_json: None,
        };
        video_repo::insert_media(&conn, &midia).unwrap();
        let espaco = TamanhoPx { w: 1080, h: 1920 };

        // plano com os pontos fora da ordem do retângulo (A, C, B, D)
        let mundo = [(0.0, 0.0), (8.0, 10.0), (8.0, 0.0), (0.0, 10.0)];
        let img = mundo.map(|(x, y)| camera(x, y)).map(|(x, y)| (x * 0.5, y + 400.0));
        let h = solve_homography_dlt(&img, &mundo).unwrap();
        let cps: Vec<_> = img.iter().zip(mundo).map(|(p, (x, y))| ponto_de_controle(*p, x, y)).collect();
        let cal = calibracao_no_banco(occ, &sha, "plane", cps.clone(), &h, Some(0.012));
        video_speed_repo::insert_calibration(&conn, &cal).unwrap();
        let r = retrato_calibracao(&ws, &cal.id.to_string()).unwrap();
        assert_eq!((r.id.as_str(), r.metodo, r.espaco, r.residuo_m), (cal.id.to_string().as_str(), MetodoCalibracao::Plane, espaco, 0.012));
        assert_eq!(r.h, linhas(&h));
        assert!(r.pontos_sha256.len() == 64 && r.pontos_sha256.chars().all(|c| c.is_ascii_hexdigit()));
        let em = |p: &(f64, f64)| Ponto { x: p.0 / 1080.0, y: p.1 / 1920.0 };
        let certo = [em(&img[0]), em(&img[2]), em(&img[1]), em(&img[3])];
        assert_eq!(r.regiao.len(), 4);
        assert!((area(&r.regiao) - area(&certo)).abs() < 1e-12, "{:?}", r.regiao);
        assert!(r.regiao.iter().all(|p| certo.contains(p)));
        // a régua do retrato: 1 m a partir de (2, 3) no chão, a até 1 px no espaço do PNG
        let mut e = regua(&r, (0.0, 0.0), 0.0, 1.0, 1);
        let a0 = camera(2.0, 3.0);
        e.ancora = em(&(a0.0 * 0.5, a0.1 + 400.0));
        let v = pontas_da_regua(&e).unwrap();
        let p1 = camera(3.0, 3.0);
        let p1 = (p1.0 * 0.5, p1.1 + 400.0);
        assert!(((v[1].x * 1080.0 - p1.0).powi(2) + (v[1].y * 1920.0 - p1.1).powi(2)).sqrt() <= TOLERANCIA_PX);
        assert_eq!(problema_da_regua(&e), None);

        // cross_ratio: a região é o segmento dos extremos da fila
        let refs: Vec<_> = [6.0, 0.0, 12.0, 3.0].iter().map(|&w| (marca(w, 0.0), w)).collect();
        let hf = fit_cross_ratio_homography(&refs.iter().map(|(p, w)| CrossRatioReference { px: p.0, py: p.1, world_m: *w }).collect::<Vec<_>>()).unwrap();
        let calf = calibracao_no_banco(occ, &sha, "cross_ratio", refs.iter().map(|(p, w)| ponto_de_controle(*p, *w, 0.0)).collect(), &hf, None);
        video_speed_repo::insert_calibration(&conn, &calf).unwrap();
        let rf = retrato_calibracao(&ws, &calf.id.to_string()).unwrap();
        assert_eq!((rf.metodo, rf.residuo_m), (MetodoCalibracao::CrossRatio, 0.0));
        assert_eq!(rf.regiao, vec![em(&marca(0.0, 0.0)), em(&marca(12.0, 0.0))]);

        // ausentes: id que não existe, vazio, sem forma de id
        for id in [Uuid::new_v4().to_string(), String::new(), "x".into()] {
            let m = retrato_calibracao(&ws, &id).unwrap_err().to_string();
            assert!(m.contains(codigo::CALIBRACAO_AUSENTE), "{m}");
            let m = retrato_calculo(&ws, &id).unwrap_err().to_string();
            assert!(m.contains(codigo::CALCULO_AUSENTE), "{m}");
        }
        // pelo comando: código e frase do catálogo
        let m = mensagens::erro_de_comando(retrato_calibracao(&ws, "").unwrap_err()).to_string();
        assert!(m.ends_with("calibracao_ausente: Calibração não encontrada"), "{m}");
        // mídia que saiu do caso: sem o espaço do PNG não há retrato
        let orfa = calibracao_no_banco(occ, &"b".repeat(64), "plane", cps, &h, None);
        video_speed_repo::insert_calibration(&conn, &orfa).unwrap();
        assert!(retrato_calibracao(&ws, &orfa.id.to_string()).unwrap_err().to_string().contains(codigo::MIDIA_AUSENTE));

        // cálculos: Monte Carlo antes do IC; 2 pontos sem faixa; intervalo = do primeiro ao último quadro marcado
        let mc = calculo_no_banco(occ, cal.id, &[3.4, 3.0, 4.2, 3.8], Some((38.7, 45.2)), Some((39.0, 44.0)));
        let ic = calculo_no_banco(occ, cal.id, &[1.0, 2.0, 1.5], None, Some((39.0, 44.0)));
        let dois = calculo_no_banco(occ, cal.id, &[5.0, 4.0], None, None);
        for c in [&mc, &ic, &dois] {
            video_speed_repo::insert_calculation(&conn, c).unwrap();
        }
        let rc = retrato_calculo(&ws, &mc.id.to_string()).unwrap();
        assert_eq!(rc, RetratoCalculo { id: mc.id.to_string(), kmh: 41.6, faixa: Some((38.7, 45.2)), de_s: 3.0, ate_s: 4.2, metodo: "monte_carlo".into() });
        let ri = retrato_calculo(&ws, &ic.id.to_string()).unwrap();
        assert_eq!((ri.faixa, ri.de_s, ri.ate_s, ri.metodo.as_str()), (Some((39.0, 44.0)), 1.0, 2.0, "regressao"));
        let rd = retrato_calculo(&ws, &dois.id.to_string()).unwrap();
        assert_eq!((rd.faixa, rd.de_s, rd.ate_s, rd.metodo.as_str()), (None, 4.0, 5.0, "media"));
        assert_eq!(texto_da_velocidade(&rd), "≈ 42 km/h");

        // retrato desatualizado: nada enquanto o banco é o do retrato; calibração mudada e cálculo apagado acusam
        let tab = apoio::tabela((1, 30), (0..300).collect());
        let ef_regua = |id: &str, cal: &RetratoCalibracao, ligado: bool| {
            json!({ "tipo": "regua", "id": id, "ligado": ligado, "nome": "", "ativo": null, "calibracao": cal, "ancora": e.ancora, "direcao_graus": 0.0,
                    "comprimento_m": 1.0, "divisoes": 2, "traco": { "cor": "#ffffff", "espessura": 0.004, "contorno": true }, "rotulo": true })
        };
        let mut ef_vel = velocidade(Value::Null);
        ef_vel["calculo"] = serde_json::to_value(&rc).unwrap();
        let mut vel_desligada = ef_vel.clone();
        (vel_desligada["id"], vel_desligada["ligado"], vel_desligada["calculo"]["kmh"]) = (json!("v2"), json!(false), json!(99.0));
        let titulo_sob = json!({ "sobreposicoes": [{ "id": "t1", "nome": "", "oculta": false, "clipes": [{ "tipo": "titulo", "id": "x1", "rotulo": "",
            "pos": { "ref": "programa", "t": 0.0 }, "duracao_s": 0.5, "transformacao": { "x": 0.5, "y": 0.5, "escala": 1.0, "rotacao": 0.0, "opacidade": 1.0 },
            "entrada_s": 0.0, "saida_s": 0.0, "efeitos": [ef_regua("g2", &rf, true)], "titulo": { "modelo": "livre", "fundo": { "tipo": "transparente" }, "blocos": [] } }] }] });
        let p = aceite::projeto(320, 180, (30, 1), vec![aceite::midia("m1", "video", json!([]))],
            vec![aceite::trecho("c1", "m1", &tab, 0, 30, 1.0, json!([ef_regua("g1", &r, true), ef_vel, vel_desligada]))], titulo_sob);
        assert_eq!(desatualizados(&ws, &p), vec![]);
        let mut outra = cal.homography;
        outra[0] *= 1.001;
        conn.execute("UPDATE video_speed_calibrations SET homography_json = ?1 WHERE id = ?2", rusqlite::params![serde_json::to_string(&outra).unwrap(), cal.id.to_string()]).unwrap();
        conn.execute("DELETE FROM video_speed_calculations WHERE id = ?1", [mc.id.to_string()]).unwrap();
        let d = desatualizados(&ws, &p);
        let resumo: Vec<_> = d.iter().map(|x| (x.codigo, x.nivel, x.clipe.as_deref(), x.efeito.as_deref(), x.campo.as_deref())).collect();
        use crate::edicao::modelo::Nivel::Aviso;
        assert_eq!(resumo, vec![(codigo::RETRATO_DESATUALIZADO, Aviso, Some("c1"), Some("g1"), Some("calibracao")), (codigo::RETRATO_DESATUALIZADO, Aviso, Some("c1"), Some("v1"), Some("calculo"))]);
        assert_eq!(d[0].mensagem, mensagens::frase(codigo::RETRATO_DESATUALIZADO, None, None));
        // caso sem banco: nenhum aviso (nem erro)
        assert_eq!(desatualizados(&tmp.path().join("nao-existe"), &p), vec![]);
    }

    // ---------------------------------------------------------------- pelo motor (ffmpeg)

    /// Tolerância das pontas medidas no vídeo (px): borda suavizada e croma 4:2:0. O aceite de 1 px é o das contas.
    const TOLERANCIA_VIDEO_PX: f64 = 2.0;

    /// Chão de 4 × 4 m visto num quadro `w`×`h`, com perspectiva (o fundo 1,4× mais longe).
    fn camera_no_quadro(w: f64, h: f64, x: f64, y: f64) -> (f64, f64) {
        let p = 1.0 + 0.1 * y;
        ((0.125 * x + 0.03 * y + 0.19) * w / p, (0.83 - 0.1 * y) * h / p)
    }

    /// Extremos dos pixels da cor ao longo da régua (a partir de `a`, no sentido de `b`) e o maior afastamento de lado.
    fn extremos(q: &medir::QuadroRgb, eh: impl Fn(&[u8]) -> bool, a: (f64, f64), b: (f64, f64)) -> Option<(f64, f64, f64)> {
        let l = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let (ux, uy) = ((b.0 - a.0) / l, (b.1 - a.1) / l);
        let mut r: Option<(f64, f64, f64)> = None;
        for (i, p) in q.px.chunks_exact(3).enumerate() {
            if eh(p) {
                let (x, y) = ((i as u32 % q.w) as f64 + 0.5 - a.0, (i as u32 / q.w) as f64 + 0.5 - a.1);
                let (t, s) = (x * ux + y * uy, (x * uy - y * ux).abs());
                r = Some(r.map_or((t, t, s), |(t0, t1, s1)| (t0.min(t), t1.max(t), s1.max(s))));
            }
        }
        r
    }

    /// Régua e velocidade pelo motor de verdade, nos pixels do vídeo renderizado: fonte comum no canvas dela e fonte
    /// com giro de 90° no contêiner num canvas deitado (A com escala e barras). As pontas das duas réguas caem onde a
    /// câmera sintética põe o metro, na cor do traço; a velocidade aparece só nos slots cujo quadro de fonte está no
    /// intervalo medido; a validação não acusa nada.
    #[test]
    fn regua_e_velocidade_no_video_renderizado() {
        let Some(cap) = fontes::cap() else { return };
        let verde = |p: &[u8]| p[1] as i32 - p[0] as i32 > 90 && p[1] as i32 - p[2] as i32 > 90;
        let azul = |p: &[u8]| p[2] as i32 - p[0] as i32 > 90 && p[2] as i32 - p[1] as i32 > 90;
        // ciano: nem a fonte (preto, branco, cinza, vermelho) nem a borda das réguas sobre ela dão essa cor
        let ciano = |p: &[u8]| p[1] as i32 - p[0] as i32 > 90 && p[2] as i32 - p[0] as i32 > 90;
        for (fonte, canvas) in [(Fonte::Mp4, (320u32, 180u32)), (Fonte::Giro90, (320, 180))] {
            let m = apoio::medida(&cap, fonte);
            let tab = &m.info.quadros;
            let (w, h) = m.info.exibido;
            let (wf, hf, cw, ch) = (w as f64, h as f64, canvas.0 as f64, canvas.1 as f64);
            // a fonte inteira no canvas, centrada (sem enquadramento)
            let escala = (cw / wf).min(ch / hf);
            let no_canvas = |p: (f64, f64)| ((cw - wf * escala) * 0.5 + p.0 * escala, (ch - hf * escala) * 0.5 + p.1 * escala);
            let cam = |x: f64, y: f64| camera_no_quadro(wf, hf, x, y);
            let em = |p: (f64, f64)| Ponto { x: p.0 / wf, y: p.1 / hf };
            let chao = [(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)];
            let img = chao.map(|(x, y)| cam(x, y));
            let hg = solve_homography_dlt(&img, &chao).unwrap();
            let cal = RetratoCalibracao { espaco: TamanhoPx { w, h }, regiao: img.iter().map(|p| em(*p)).collect(), ..retrato(MetodoCalibracao::Plane, &hg, vec![]) };
            let espessura = 0.03;
            // (cor, âncora no chão, direção, metros, divisões)
            let reguas = [("#00ff00", (0.5, 0.5), 0.0, 3.0, 3u32), ("#0000ff", (2.0, 1.5), 90.0, 2.0, 0)];
            let (j_in, j_out) = (30usize, 60usize);
            let (j_de, j_ate) = (j_in + 8, j_in + 19);
            let mut efeitos: Vec<Value> = reguas
                .iter()
                .enumerate()
                .map(|(k, (cor, de, dir, metros, div))| {
                    aceite::efeito("regua", &format!("g{k}"), json!({ "calibracao": cal, "ancora": em(cam(de.0, de.1)), "direcao_graus": dir, "comprimento_m": metros,
                        "divisoes": div, "traco": { "cor": cor, "espessura": espessura, "contorno": false }, "rotulo": false }))
                })
                .collect();
            efeitos.push(aceite::efeito("velocidade_exibida", "v1", json!({ "ativo": { "de": tab.tempo(j_de), "ate": tab.tempo(j_ate) },
                "calculo": { "id": "c", "kmh": 42.0, "faixa": null, "de_s": tab.tempo(j_de), "ate_s": tab.tempo(j_ate), "metodo": "media" },
                "onde": { "modo": "fixa", "lugar": { "x": 0.5, "y": 0.05, "h": "centro", "v": "topo" } },
                "estilo": { "fonte": "sans", "negrito": true, "italico": false, "tamanho": 0.05, "cor": "#000000", "alinhamento": "centro",
                            "caixa": { "cor": "#00ffff", "margem": 0.02, "raio": 0.0 }, "contorno": null } })));
            let p = aceite::projeto(canvas.0, canvas.1, (30, 1), vec![aceite::midia("m1", "video", json!([]))],
                vec![aceite::trecho("c1", "m1", tab, j_in, j_out, 1.0, Value::Array(efeitos))], json!({}));
            let mundo = aceite::Mundo::novo(&cap).com("m1", &m.info);
            let ctx = Contexto { fontes: &mundo.fontes, cap: &cap, rastreios: &apoio::SemRastreios, pasta_projeto: Path::new("/nao/existe"), midias: &[] };
            let erros: Vec<_> = crate::edicao::validar::validar(&p, &crate::edicao::tempo::resolver(&p, &ctx), &ctx, None)
                .into_iter()
                .filter(|x| x.nivel == crate::edicao::modelo::Nivel::Erro)
                .map(|x| (x.codigo, x.efeito))
                .collect();
            assert_eq!(erros, vec![], "{fonte:?}");

            let plano = mundo.compilar(&p, &aceite::alvo_final("cpu")).unwrap();
            let nome = format!("p13-medidas-{fonte:?}");
            let rodado = executor::rodar(&cap, &plano, &fontes::pasta_temporaria(&nome), &Opcoes { sem_perdas: true, ..Opcoes::default() })
                .unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
            let q: Vec<medir::QuadroRgb> = rodado.blocos.iter().flat_map(|b| medir::quadros(&cap, b).unwrap()).collect();
            assert_eq!(q.len(), j_out - j_in, "{fonte:?}: quadros × slots");
            let mapa = &plano.proveniencia.roteiro[0].origem.as_ref().unwrap().quadro_do_slot;

            // traço e tique em px do canvas (o traço não é ampliado por A)
            let meia = espessura * ch * 0.5;
            let mut pior = 0.0f64;
            for s in [0, q.len() / 2, q.len() - 1] {
                for (k, (cor, de, dir, metros, _)) in reguas.iter().enumerate() {
                    let (dx, dy) = (f64::to_radians(*dir).cos(), f64::to_radians(*dir).sin());
                    let (a, b) = (no_canvas(cam(de.0, de.1)), no_canvas(cam(de.0 + metros * dx, de.1 + metros * dy)));
                    let l = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
                    let (t0, t1, lado) = if k == 0 { extremos(&q[s], verde, a, b) } else { extremos(&q[s], azul, a, b) }
                        .unwrap_or_else(|| panic!("{fonte:?} slot {s}: régua {cor} sumiu ({})", cap.versao));
                    // ponta arredondada: a cor vai de −meia a comprimento + meia; de lado, o tique da ponta mais a meia largura
                    let tique = (0.05 * l).max(2.5 * espessura * ch);
                    let erros = [t0 + meia, t1 - (l + meia), lado - (tique + meia)];
                    pior = erros.iter().fold(pior, |m, e| m.max(e.abs()));
                    assert!(erros.iter().all(|e| e.abs() <= TOLERANCIA_VIDEO_PX),
                        "{fonte:?} slot {s} régua {cor}: de {t0:.2} a {t1:.2} px (esperado de {:.2} a {:.2}), {lado:.2} de lado (esperado {:.2}) ({})", -meia, l + meia, tique + meia, cap.versao);
                    for c in [a, b] {
                        let i = ((c.1.floor() as u32 * q[s].w + c.0.floor() as u32) * 3) as usize;
                        let px = &q[s].px[i..i + 3];
                        assert!(if k == 0 { verde(px) } else { azul(px) }, "{fonte:?} slot {s} régua {cor}: ponta {c:?} com {px:?}");
                    }
                }
            }
            eprintln!("{fonte:?} ({}): pontas e tiques das réguas no vídeo a até {pior:.2} px", cap.versao);
            // velocidade: a caixa do texto só nos slots de quadro medido, centrada no lugar
            for (s, x) in q.iter().enumerate() {
                let j = mapa[s] as usize;
                let n = x.px.chunks_exact(3).filter(|p| ciano(p)).count();
                if (j_de..=j_ate).contains(&j) {
                    assert!(n > 100, "{fonte:?} slot {s} (quadro {j}): velocidade sumiu ({n} px)");
                    let (sx, sy) = x.px.chunks_exact(3).enumerate().filter(|(_, p)| ciano(p)).fold((0.0, f64::INFINITY), |(sx, sy), (i, _)| {
                        (sx + (i as u32 % x.w) as f64 + 0.5, sy.min((i as u32 / x.w) as f64))
                    });
                    assert!((sx / n as f64 - cw * 0.5).abs() <= TOLERANCIA_VIDEO_PX && (sy - 0.05 * ch).abs() <= TOLERANCIA_VIDEO_PX,
                        "{fonte:?} slot {s}: caixa em ({:.1}, {sy:.1})", sx / n as f64);
                } else {
                    assert_eq!(n, 0, "{fonte:?} slot {s} (quadro {j}): velocidade fora do intervalo medido");
                }
            }
        }
    }

    // ---------------------------------------------------------------- fixtures do espelho

    /// `medidas-pontas.json` e `medidas-texto.json` (espelho em `palco/desenho/medidas.ts`): `SICRO_GRAVAR_FIXTURES=1` regrava.
    #[test]
    fn fixtures_medidas() {
        let (plano, linha, fila) = (plano(), linha(), fila());
        let ((x1, y1), _, _) = LINHA;
        let mut casos: Vec<(String, EfRegua)> = vec![
            ("plane 1 m a 0°".into(), regua(&plano, camera(1.0, 1.0), 0.0, 1.0, 10)),
            ("plane 1 m a 90°".into(), regua(&plano, camera(6.0, 2.0), 90.0, 1.0, 10)),
            ("plane 2,5 m a 137,5°".into(), regua(&plano, camera(5.0, 4.0), 137.5, 2.5, 7)),
            ("plane sem divisões".into(), regua(&plano, camera(2.0, 7.0), -30.0, 1.0, 0)),
            ("plane atrás da câmera".into(), regua(&plano, camera(4.0, 5.0), 270.0, 20.0, 2)),
            ("line a 0°".into(), regua(&linha, (x1, y1), 0.0, 1.0, 4)),
            ("line a 90°".into(), regua(&linha, (x1, y1), 90.0, 0.5, 5)),
            ("cross_ratio para mais".into(), regua(&fila, marca(3.0, 0.0), 0.0, 1.0, 5)),
            ("cross_ratio para menos e de lado".into(), regua(&fila, marca(9.0, 12.0), 180.0, 2.0, 4)),
            ("cross_ratio a 90° (cos 0: para mais)".into(), regua(&fila, marca(6.0, 0.0), 90.0, 1.0, 1)),
            ("divisões acima do teto".into(), regua(&plano, camera(4.0, 5.0), 0.0, 1.0, MAX_DIVISOES + 1)),
            ("comprimento zero".into(), regua(&plano, camera(4.0, 5.0), 0.0, 0.0, 2)),
        ];
        let mut curta = regua(&plano, camera(4.0, 5.0), 0.0, 1.0, 2);
        curta.calibracao.h.truncate(8);
        casos.push(("homografia com 8 números".into(), curta));
        let mut singular = regua(&plano, camera(4.0, 5.0), 0.0, 1.0, 2);
        singular.calibracao.h = vec![1.0, 2.0, 3.0, 2.0, 4.0, 6.0, 0.0, 0.0, 1.0];
        casos.push(("plane singular".into(), singular));
        let lista: Vec<Value> = casos
            .into_iter()
            .map(|(nome, e)| {
                let saida = match pontas_da_regua(&e) {
                    Ok(p) => json!({ "pontos": p }),
                    Err(SicroError::Validation(m)) => json!({ "erro": m }),
                    Err(o) => panic!("{o}"),
                };
                json!({ "nome": nome, "entrada": { "efeito": e }, "saida": saida })
            })
            .collect();
        fixtures::conferir("medidas-pontas.json", Value::Array(lista));

        let mut textos: Vec<Value> = [
            (42.0, Some((39.0, 45.0))),
            (41.6, Some((45.2, 38.7))),
            (42.0, None),
            (30.5, None),
            (7.25, Some((6.04, 8.51))),
            (9.99, None),
            (9.96, Some((9.91, 10.04))),
            (3.0, Some((-1.2, 7.0))),
            (0.04, None),
            (2e12, Some((1.0, 3e12))),
            (110.5, Some((100.49999999999999, 120.00000000000001))),
        ]
        .iter()
        .map(|&(kmh, faixa)| {
            let c = calc(kmh, faixa);
            json!({ "nome": format!("{kmh} {faixa:?}"), "entrada": { "calculo": c }, "saida": { "texto": texto_da_velocidade(&c) } })
        })
        .collect();
        for m in [1.0, 0.5, 2.25, 2.05, 0.125, 0.135, 10.0, 0.004, 999.999, 1e6] {
            textos.push(json!({ "nome": format!("{m} m"), "entrada": { "comprimento_m": m }, "saida": { "texto": texto_do_comprimento(m) } }));
        }
        fixtures::conferir("medidas-texto.json", Value::Array(textos));
    }
}
