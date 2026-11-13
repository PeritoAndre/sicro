//! Monta o `-filter_complex` da Montagem (sequência de trechos, cartelas e quadros
//! congelados). Função pura: não toca em disco nem roda o ffmpeg.

use std::path::PathBuf;

use crate::error::{Result, SicroError};
use crate::models::{
    Montagem, MontagemDesfoque, MontagemDestaque, MontagemItem, MontagemQuadro, MontagemTexto, MontagemZoom,
};

const FADE_S: f64 = 0.5;

/// Vídeo de origem já resolvido pelo chamador.
pub struct Fonte {
    pub path: PathBuf,
    pub tem_audio: bool,
}

pub struct Plano {
    /// Um `-i` por item que usa vídeo, na ordem dos itens.
    pub inputs: Vec<PathBuf>,
    pub filter_complex: String,
    /// Textos de usuário vão em arquivo (`textfile=`): evita o escape duplo do filtro.
    /// (nome do arquivo, conteúdo); o ffmpeg roda na pasta onde eles são gravados.
    pub textos: Vec<(String, String)>,
    pub duracao_s: f64,
    /// PNGs dos destaques (nome, bytes): gravados ao lado dos textos, lidos por `movie=`.
    pub imagens: Vec<(String, Vec<u8>)>,
}

/// Fonte do `drawtext`: `font=Sans` via fontconfig; no Windows, o Arial do sistema.
pub fn fonte_padrao() -> String {
    if cfg!(windows) {
        let f = std::path::Path::new("C:/Windows/Fonts/arial.ttf");
        if f.is_file() {
            return "fontfile=C\\:/Windows/Fonts/arial.ttf".into();
        }
    }
    "font=Sans".into()
}

fn tam_px(t: &str, altura: u32) -> u32 {
    let base = match t {
        "p" => 0.033,
        "g" => 0.078,
        _ => 0.052,
    };
    ((altura as f64 * base).round() as u32).max(12)
}

fn cor(c: &str) -> Result<String> {
    let h = c.trim().trim_start_matches('#');
    if h.len() == 6 && h.chars().all(|x| x.is_ascii_hexdigit()) {
        Ok(format!("0x{h}"))
    } else {
        Err(SicroError::Validation(format!("cor inválida: {c}")))
    }
}

fn segundos(x: f64) -> String {
    let s = format!("{x:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn hms(t: f64) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as u64;
    format!("{:02}\\:{:02}\\:{:02}.{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

/// Cadeia de `atempo` (cada estágio fica entre 0,5 e 2,0, valid em qualquer ffmpeg).
fn atempo(v: f64) -> String {
    let mut partes = Vec::new();
    let mut r = v;
    while r < 0.5 {
        partes.push("atempo=0.5".to_string());
        r /= 0.5;
    }
    while r > 2.0 {
        partes.push("atempo=2.0".to_string());
        r /= 2.0;
    }
    partes.push(format!("atempo={}", segundos(r)));
    partes.join(",")
}

struct Ctx<'a> {
    h: u32,
    fonte: &'a str,
    textos: Vec<(String, String)>,
}

impl Ctx<'_> {
    fn arquivo_texto(&mut self, conteudo: &str) -> String {
        let nome = format!("t{}.txt", self.textos.len());
        self.textos.push((nome.clone(), conteudo.to_string()));
        nome
    }

    fn drawtext_usuario(&mut self, t: &MontagemTexto) -> Option<String> {
        if t.conteudo.trim().is_empty() {
            return None;
        }
        let f = self.arquivo_texto(&t.conteudo);
        let y = match t.posicao.as_str() {
            "topo" => "h*0.06".to_string(),
            "centro" => "(h-text_h)/2".to_string(),
            _ => "h-text_h-h*0.06".to_string(),
        };
        Some(format!(
            "drawtext={}:textfile={f}:expansion=none:fontsize={}:fontcolor=white:x=(w-text_w)/2:y={y}:box=1:boxcolor=black@0.55:boxborderw=14",
            self.fonte,
            tam_px(&t.tamanho, self.h)
        ))
    }

    fn drawtext_carimbo(&self, expr: &str) -> String {
        let fs = tam_px("p", self.h);
        format!(
            "drawtext={}:text='{expr}':expansion=normal:fontsize={fs}:fontcolor=white:x=w-text_w-30:y=30:box=1:boxcolor=black@0.55:boxborderw=8",
            self.fonte
        )
    }
}

// ---------------------------------------------------------------------------
// Efeitos por trecho: zoom, destaques e desfoques (coordenadas normalizadas).

const PASSO_RETANGULO_S: f64 = 0.1;
const MAX_SEGMENTOS_RETANGULO: usize = 300;

/// Interpolação linear por partes de (t, valor), já em ordem de `t`; antes do primeiro vale o
/// primeiro, depois do último vale o último.
fn interp(pontos: &[(f64, f64)]) -> String {
    let Some(&(_, ultimo)) = pontos.last() else { return "0".into() };
    let mut expr = segundos(ultimo);
    for par in pontos.windows(2).rev() {
        let ((t0, v0), (t1, v1)) = (par[0], par[1]);
        let lerp = if t1 - t0 < 1e-6 {
            segundos(v1)
        } else {
            format!("({}+({})*(t-{})/{})", segundos(v0), segundos(v1 - v0), segundos(t0), segundos(t1 - t0))
        };
        expr = format!("if(lt(t,{}),{lerp},{expr})", segundos(t1));
    }
    expr
}

fn quadros_validos(q: &[MontagemQuadro], dur: f64, exige_wh: bool, rotulo: &str) -> Result<Vec<MontagemQuadro>> {
    if q.is_empty() {
        return Err(SicroError::Validation(format!("{rotulo} sem quadro-chave")));
    }
    let mut v = q.to_vec();
    for k in &v {
        let ok = k.t.is_finite() && k.x.is_finite() && k.y.is_finite() && k.t >= 0.0 && k.t <= dur + 1e-6;
        let wh = !exige_wh
            || matches!((k.w, k.h), (Some(w), Some(h)) if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0);
        if !ok || !wh {
            return Err(SicroError::Validation(format!("{rotulo} com quadro-chave inválido")));
        }
    }
    v.sort_by(|a, b| a.t.total_cmp(&b.t));
    Ok(v)
}

fn clamp(v: f64, a: f64, b: f64) -> f64 {
    v.max(a).min(b.max(a))
}

/// Cobertura (0..1) de um traço de espessura `esp` a distância `d` do eixo.
fn cobertura(d: f64, esp: f64) -> f64 {
    clamp(esp / 2.0 + 0.5 - d, 0.0, 1.0)
}

fn dist_segmento(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 == 0.0 { 0.0 } else { clamp(((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2, 0.0, 1.0) };
    ((p.0 - a.0 - t * dx).powi(2) + (p.1 - a.1 - t * dy).powi(2)).sqrt()
}

/// Traço colorido com contorno escuro (legível em fundo claro ou escuro).
fn pintar(lado: u32, rgb: [u8; 3], esp: f64, dist: impl Fn((f64, f64)) -> f64) -> Vec<u8> {
    let mut img = image::RgbaImage::new(lado, lado);
    for (x, y, px) in img.enumerate_pixels_mut() {
        let d = dist((x as f64 + 0.5, y as f64 + 0.5));
        let a_traco = cobertura(d, esp);
        let a_halo = cobertura(d, esp + 3.0) * 0.75;
        let a = a_traco + a_halo * (1.0 - a_traco);
        if a > 0.0 {
            let c = |v: u8| ((v as f64 * a_traco) / a).round() as u8;
            *px = image::Rgba([c(rgb[0]), c(rgb[1]), c(rgb[2]), (a * 255.0).round() as u8]);
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    let _ = img.write_to(&mut out, image::ImageFormat::Png);
    out.into_inner()
}

fn rgb_de(c: &str) -> Result<[u8; 3]> {
    let h = cor(c)?;
    let v = u32::from_str_radix(&h[2..], 16).unwrap_or(0);
    Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

pub(crate) fn png_circulo(lado: u32, rgb: [u8; 3], esp: f64) -> Vec<u8> {
    let c = lado as f64 / 2.0;
    let r = c - esp / 2.0 - 3.0;
    pintar(lado, rgb, esp, move |p| (((p.0 - c).powi(2) + (p.1 - c).powi(2)).sqrt() - r).abs())
}

/// Seta com a ponta no canto inferior esquerdo da imagem.
pub(crate) fn png_seta(lado: u32, rgb: [u8; 3], esp: f64) -> Vec<u8> {
    let s = lado as f64;
    let m = esp / 2.0 + 3.0;
    let (ponta, cauda) = ((m, s - m), (s - m, m));
    let ang = (cauda.1 - ponta.1).atan2(cauda.0 - ponta.0);
    let l = s * 0.4;
    let asa = |sinal: f64| {
        let a = ang + sinal * 0.52;
        (ponta.0 + l * a.cos(), ponta.1 + l * a.sin())
    };
    let (a1, a2) = (asa(1.0), asa(-1.0));
    pintar(lado, rgb, esp, move |p| {
        dist_segmento(p, ponta, cauda)
            .min(dist_segmento(p, ponta, a1))
            .min(dist_segmento(p, ponta, a2))
    })
}

struct Cadeia {
    partes: Vec<String>,
    atual: String,
    prefixo: String,
    n: usize,
}

impl Cadeia {
    fn f(&mut self, filtro: &str) {
        self.atual.push(',');
        self.atual.push_str(filtro);
    }
    /// Fecha o trecho da cadeia com um rótulo novo e devolve o rótulo.
    fn fechar(&mut self) -> String {
        let l = format!("{}{}", self.prefixo, self.n);
        self.n += 1;
        self.partes.push(format!("{}[{l}]", self.atual));
        l
    }
}

struct Efeitos<'a> {
    w: u32,
    h: u32,
    fps: u32,
    dur: f64,
    zoom: Option<&'a MontagemZoom>,
    destaques: &'a [MontagemDestaque],
    desfoques: &'a [MontagemDesfoque],
}

fn aplicar_efeitos(
    ch: &mut Cadeia,
    e: &Efeitos,
    imagens: &mut Vec<(String, Vec<u8>)>,
    n_trecho: usize,
) -> Result<()> {
    let (wf, hf) = (e.w as f64, e.h as f64);
    for (j, d) in e.desfoques.iter().enumerate() {
        let q = quadros_validos(&d.quadros, e.dur, true, "desfoque")?;
        let cw = (clamp(q.iter().filter_map(|k| k.w).fold(0.0, f64::max) * wf, 16.0, wf) / 2.0).round() as u32 * 2;
        let chh = (clamp(q.iter().filter_map(|k| k.h).fold(0.0, f64::max) * hf, 16.0, hf) / 2.0).round() as u32 * 2;
        let raio = clamp((20.0 * hf / 1080.0).round(), 2.0, (cw.min(chh) / 4) as f64) as u32;
        let px = |f: fn(&MontagemQuadro) -> f64, max: f64| -> Vec<(f64, f64)> {
            q.iter().map(|k| (k.t, clamp(f(k) * max, 0.0, max - 0.0))).collect()
        };
        let xs: Vec<(f64, f64)> = px(|k| k.x, wf).into_iter().map(|(t, v)| (t, v.min(wf - cw as f64))).collect();
        let ys: Vec<(f64, f64)> = px(|k| k.y, hf).into_iter().map(|(t, v)| (t, v.min(hf - chh as f64))).collect();
        let (xe, ye) = (interp(&xs), interp(&ys));
        let l = ch.fechar();
        let t0 = segundos(q[0].t);
        ch.partes.push(format!("[{l}]split[{l}a][{l}b]"));
        ch.partes.push(format!(
            "[{l}b]crop={cw}:{chh}:x='{xe}':y='{ye}',boxblur={raio}:3[{l}c]"
        ));
        ch.atual = format!("[{l}a][{l}c]overlay=x='{xe}':y='{ye}':enable='gte(t,{t0})'");
        let _ = j;
    }

    for (j, d) in e.destaques.iter().enumerate() {
        let rgb = rgb_de(&d.cor)?;
        let hex = cor(&d.cor)?;
        let esp = (hf * 0.006).round().max(3.0);
        let q = quadros_validos(&d.quadros, e.dur, d.forma == "retangulo", "destaque")?;
        match d.forma.as_str() {
            "circulo" | "seta" => {
                let circulo = d.forma == "circulo";
                let lado = ((hf * if circulo { 0.12 } else { 0.14 }).round() as u32).max(24) & !1;
                let nome = format!("d{n_trecho}_{j}.png");
                let png = if circulo { png_circulo(lado, rgb, esp) } else { png_seta(lado, rgb, esp) };
                imagens.push((nome.clone(), png));
                let (dx, dy) = if circulo {
                    (-(lado as f64) / 2.0, -(lado as f64) / 2.0)
                } else {
                    (-(esp / 2.0 + 3.0), -(lado as f64) + esp / 2.0 + 3.0)
                };
                let xs: Vec<_> = q.iter().map(|k| (k.t, k.x * wf + dx)).collect();
                let ys: Vec<_> = q.iter().map(|k| (k.t, k.y * hf + dy)).collect();
                let l = ch.fechar();
                let ov = format!("{}i{}", ch.prefixo, ch.n);
                ch.n += 1;
                ch.partes.push(format!("movie={nome},format=rgba[{ov}]"));
                ch.atual = format!(
                    "[{l}][{ov}]overlay=x='{}':y='{}':enable='gte(t,{})'",
                    interp(&xs),
                    interp(&ys),
                    segundos(q[0].t)
                );
            }
            "retangulo" => {
                // drawbox não aceita `t` (tempo) nas expressões: segue o movimento em degraus.
                let caixa = |k: &MontagemQuadro| {
                    let (w, h) = (k.w.unwrap_or(0.0) * wf, k.h.unwrap_or(0.0) * hf);
                    (k.x * wf, k.y * hf, w, h)
                };
                let span = q.last().map(|k| k.t).unwrap_or(0.0) - q[0].t;
                let passo = PASSO_RETANGULO_S.max(span / MAX_SEGMENTOS_RETANGULO as f64);
                let mut seg: Vec<(f64, Option<f64>, (f64, f64, f64, f64))> = Vec::new();
                for par in q.windows(2) {
                    let (a, b) = (&par[0], &par[1]);
                    let n = (((b.t - a.t) / passo).ceil() as usize).max(1);
                    let (ca, cb) = (caixa(a), caixa(b));
                    for i in 0..n {
                        let (f0, f1) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
                        let l = |u: f64, v: f64, f: f64| u + (v - u) * f;
                        seg.push((
                            a.t + (b.t - a.t) * f0,
                            Some(a.t + (b.t - a.t) * f1),
                            (l(ca.0, cb.0, f0), l(ca.1, cb.1, f0), l(ca.2, cb.2, f0), l(ca.3, cb.3, f0)),
                        ));
                    }
                }
                let ult = &q[q.len() - 1];
                seg.push((ult.t, None, caixa(ult)));
                for (a, b, (x, y, w, h)) in seg {
                    let en = match b {
                        Some(b) => format!("gte(t,{})*lt(t,{})", segundos(a), segundos(b)),
                        None => format!("gte(t,{})", segundos(a)),
                    };
                    ch.f(&format!(
                        "drawbox=x={}:y={}:w={}:h={}:color={hex}:t={}:enable='{en}'",
                        x.round() as i64,
                        y.round() as i64,
                        (w.round() as i64).max(2),
                        (h.round() as i64).max(2),
                        esp as u32
                    ));
                }
            }
            _ => return Err(SicroError::Validation("forma de destaque inválida".into())),
        }
    }
    // Zoom por último: desfoques e destaques acompanham o conteúdo da imagem original.
    if let Some(z) = e.zoom {
        let reg = |r: &crate::models::Regiao| -> Result<(f64, f64, f64)> {
            if ![r.x, r.y, r.w, r.h].iter().all(|v| v.is_finite()) || r.w <= 0.0 {
                return Err(SicroError::Validation("região de zoom inválida".into()));
            }
            let w = clamp(r.w, 0.1, 1.0);
            Ok((clamp(r.x, 0.0, 1.0 - w), clamp(r.y, 0.0, 1.0 - w * hf / wf), w))
        };
        let ((x0, y0, w0), (x1, y1, w1)) = (reg(&z.inicio)?, reg(&z.fim)?);
        // `time` do zoompan = tempo do quadro no trecho; conforma o fps antes (d=1 gera 1 quadro por quadro).
        let p = format!("min(time/{},1)", segundos(e.dur.max(0.04)));
        ch.f(&format!(
            "fps={},zoompan=z='1/({}+({})*{p})':x='({}+({})*{p})*iw':y='({}+({})*{p})*ih':d=1:s={}x{}:fps={}",
            e.fps,
            segundos(w0),
            segundos(w1 - w0),
            segundos(x0),
            segundos(x1 - x0),
            segundos(y0),
            segundos(y1 - y0),
            e.w,
            e.h,
            e.fps
        ));
    }
    Ok(())
}

fn duracao_item(it: &MontagemItem) -> f64 {
    match it {
        MontagemItem::Trecho { inicio_s, fim_s, velocidade, .. } => (fim_s - inicio_s) / velocidade,
        MontagemItem::Cartela { duracao_s, .. } | MontagemItem::Congelar { duracao_s, .. } => *duracao_s,
    }
}

fn fade_in(it: &MontagemItem) -> bool {
    match it {
        MontagemItem::Trecho { transicao_entrada, .. } | MontagemItem::Cartela { transicao_entrada, .. } => {
            transicao_entrada.as_deref() == Some("fade")
        }
        MontagemItem::Congelar { .. } => false,
    }
}

pub fn construir(
    m: &Montagem,
    fonte: &str,
    resolver: &dyn Fn(&str) -> Result<Fonte>,
    audio: &dyn Fn(&str) -> Result<PathBuf>,
) -> Result<Plano> {
    if m.itens.is_empty() {
        return Err(SicroError::Validation("a montagem está vazia".into()));
    }
    let (w, h, fps) = (m.saida.largura, m.saida.altura, m.saida.fps);
    if w < 64 || h < 64 || w > 7680 || h > 4320 || w % 2 != 0 || h % 2 != 0 || !(1..=120).contains(&fps) {
        return Err(SicroError::Validation("saída da montagem inválida".into()));
    }
    let mut ctx = Ctx { h, fonte, textos: Vec::new() };
    let mut imagens: Vec<(String, Vec<u8>)> = Vec::new();
    let mut inputs: Vec<PathBuf> = Vec::new();
    let mut partes: Vec<String> = Vec::new();
    let mut concat_in = String::new();
    let mut total = 0.0;
    let n = m.itens.len();

    for (i, it) in m.itens.iter().enumerate() {
        let dur = duracao_item(it);
        if !dur.is_finite() || dur < 0.04 {
            return Err(SicroError::Validation(format!("item {} com duração inválida", i + 1)));
        }
        total += dur;
        let fi = fade_in(it);
        let fo = m.itens.get(i + 1).map(fade_in).unwrap_or(false);
        let d_fade = FADE_S.min(dur / 2.0);
        let mut fades_v = String::new();
        let mut fades_a = String::new();
        if fi {
            fades_v += &format!(",fade=t=in:st=0:d={}", segundos(d_fade));
            fades_a += &format!(",afade=t=in:st=0:d={}", segundos(d_fade));
        }
        if fo {
            let st = segundos(dur - d_fade);
            fades_v += &format!(",fade=t=out:st={st}:d={}", segundos(d_fade));
            fades_a += &format!(",afade=t=out:st={st}:d={}", segundos(d_fade));
        }
        let (vl, al) = (format!("v{i}"), format!("a{i}"));
        let enquadra = format!(
            "scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1"
        );

        match it {
            MontagemItem::Trecho { media_id, inicio_s, fim_s, velocidade, texto, zoom, destaques, desfoques, .. } => {
                if !(0.1..=8.0).contains(velocidade) {
                    return Err(SicroError::Validation(format!("velocidade inválida no item {}", i + 1)));
                }
                if *inicio_s < 0.0 || fim_s <= inicio_s {
                    return Err(SicroError::Validation(format!("intervalo inválido no item {}", i + 1)));
                }
                let src = resolver(media_id)?;
                let k = inputs.len();
                inputs.push(src.path);
                let mut ch = Cadeia {
                    partes: Vec::new(),
                    atual: format!(
                        "[{k}:v]trim=start={}:end={},setpts=PTS-STARTPTS,{enquadra}",
                        segundos(*inicio_s),
                        segundos(*fim_s)
                    ),
                    prefixo: format!("e{i}_"),
                    n: 0,
                };
                // Efeitos no tempo do trecho (velocidade original), antes do `setpts` da velocidade.
                let ef = Efeitos {
                    w,
                    h,
                    fps,
                    dur: fim_s - inicio_s,
                    zoom: zoom.as_ref(),
                    destaques,
                    desfoques,
                };
                aplicar_efeitos(&mut ch, &ef, &mut imagens, i)?;
                if let Some(t) = texto {
                    if t.carimbo_tempo {
                        ch.f(&ctx.drawtext_carimbo(&format!("%{{pts\\:hms\\:{}}}", segundos(*inicio_s))));
                    }
                    if let Some(d) = ctx.drawtext_usuario(t) {
                        ch.f(&d);
                    }
                }
                // Velocidade depois do texto: o carimbo mostra o tempo do original.
                ch.f(&format!("setpts=PTS/{},fps={fps},format=yuv420p{fades_v}", segundos(*velocidade)));
                ch.atual.push_str(&format!("[{vl}]"));
                partes.extend(ch.partes);
                partes.push(ch.atual);
                let a = if src.tem_audio && m.saida.incluir_audio {
                    let tempo = if (velocidade - 1.0).abs() > 1e-9 { format!(",{}", atempo(*velocidade)) } else { String::new() };
                    format!(
                        "[{k}:a]atrim=start={}:end={},asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo{tempo}{fades_a}[{al}]",
                        segundos(*inicio_s),
                        segundos(*fim_s)
                    )
                } else {
                    silencio(dur, &fades_a, &al)
                };
                partes.push(a);
            }
            MontagemItem::Cartela { duracao_s, texto, cor_fundo, cor_texto, tamanho, .. } => {
                let f = ctx.arquivo_texto(texto);
                let mut v = format!(
                    "color=c={}:s={w}x{h}:d={}:r={fps},format=yuv420p",
                    cor(cor_fundo)?,
                    segundos(*duracao_s)
                );
                if !texto.trim().is_empty() {
                    v += &format!(
                        ",drawtext={}:textfile={f}:expansion=none:fontsize={}:fontcolor={}:x=(w-text_w)/2:y=(h-text_h)/2",
                        ctx.fonte,
                        tam_px(tamanho, h),
                        cor(cor_texto)?
                    );
                }
                v += &format!(",setsar=1{fades_v}[{vl}]");
                partes.push(v);
                partes.push(silencio(dur, &fades_a, &al));
            }
            MontagemItem::Congelar { media_id, instante_s, duracao_s, texto, .. } => {
                if *instante_s < 0.0 {
                    return Err(SicroError::Validation(format!("instante inválido no item {}", i + 1)));
                }
                let src = resolver(media_id)?;
                let k = inputs.len();
                inputs.push(src.path);
                let mut v = format!(
                    "[{k}:v]trim=start={},setpts=PTS-STARTPTS,trim=end_frame=1,{enquadra},fps={fps},format=yuv420p,tpad=stop_mode=clone:stop_duration={},trim=duration={},setpts=PTS-STARTPTS",
                    segundos(*instante_s),
                    segundos(*duracao_s),
                    segundos(*duracao_s)
                );
                if let Some(t) = texto {
                    if t.carimbo_tempo {
                        v += &format!(",{}", ctx.drawtext_carimbo(&hms(*instante_s)));
                    }
                    if let Some(d) = ctx.drawtext_usuario(t) {
                        v += &format!(",{d}");
                    }
                }
                v += &format!("{fades_v}[{vl}]");
                partes.push(v);
                partes.push(silencio(dur, &fades_a, &al));
            }
        }
        concat_in += &format!("[{vl}][{al}]");
    }
    let vol = m.volume_original;
    if !(0.0..=2.0).contains(&vol) {
        return Err(SicroError::Validation("volume do áudio original fora de 0 a 2".into()));
    }
    let mexe_audio = m.saida.incluir_audio && (!m.narracoes.is_empty() || (vol - 1.0).abs() > 1e-9);
    if !mexe_audio {
        partes.push(format!("{concat_in}concat=n={n}:v=1:a=1[v][a]"));
    } else {
        partes.push(format!("{concat_in}concat=n={n}:v=1:a=1[v][as]"));
        let orig = if (vol - 1.0).abs() > 1e-9 {
            partes.push(format!("[as]volume={}[ao]", segundos(vol)));
            "ao"
        } else {
            "as"
        };
        if m.narracoes.is_empty() {
            partes.push(format!("[{orig}]anull[a]"));
        } else {
            let mut mix = format!("[{orig}]");
            for (j, nr) in m.narracoes.iter().enumerate() {
                if !(0.0..=2.0).contains(&nr.volume) || !nr.inicio_s.is_finite() || nr.inicio_s < 0.0 {
                    return Err(SicroError::Validation(format!("narração {} inválida", j + 1)));
                }
                let k = inputs.len();
                inputs.push(audio(&nr.arquivo_relativo)?);
                let ms = (nr.inicio_s * 1000.0).round() as u64;
                partes.push(format!(
                    "[{k}:a]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,volume={},adelay={ms}|{ms}[n{j}]",
                    segundos(nr.volume)
                ));
                mix += &format!("[n{j}]");
            }
            partes.push(format!(
                "{mix}amix=inputs={}:normalize=0:duration=first[a]",
                m.narracoes.len() + 1
            ));
        }
    }
    Ok(Plano { inputs, filter_complex: partes.join(";"), textos: ctx.textos, duracao_s: total, imagens })
}

fn silencio(dur: f64, fades_a: &str, rotulo: &str) -> String {
    format!(
        "anullsrc=r=48000:cl=stereo,atrim=duration={},asetpts=PTS-STARTPTS{fades_a}[{rotulo}]",
        segundos(dur)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MontagemNarracao, MontagemSaida, Regiao};

    fn base(itens: Vec<MontagemItem>) -> Montagem {
        Montagem {
            id: "m1".into(),
            titulo: "t".into(),
            created_at: String::new(),
            updated_at: String::new(),
            itens,
            saida: MontagemSaida { largura: 1920, altura: 1080, fps: 30, incluir_audio: true },
            narracoes: Vec::new(),
            volume_original: 1.0,
        }
    }

    fn resolver(id: &str) -> Result<Fonte> {
        Ok(Fonte { path: PathBuf::from(format!("/v/{id}.mp4")), tem_audio: id != "mudo" })
    }

    fn audio(rel: &str) -> Result<PathBuf> {
        Ok(PathBuf::from(format!("/a/{rel}")))
    }

    fn trecho(media: &str, i: f64, f: f64, vel: f64) -> MontagemItem {
        MontagemItem::Trecho {
            id: "x".into(),
            media_id: media.into(),
            inicio_s: i,
            fim_s: f,
            velocidade: vel,
            texto: None,
            transicao_entrada: None,
            zoom: None,
            destaques: Vec::new(),
            desfoques: Vec::new(),
        }
    }

    fn cartela(dur: f64, fade: bool) -> MontagemItem {
        MontagemItem::Cartela {
            id: "c".into(),
            duracao_s: dur,
            texto: "Título".into(),
            cor_fundo: "#000000".into(),
            cor_texto: "#ffffff".into(),
            tamanho: "m".into(),
            transicao_entrada: fade.then(|| "fade".to_string()),
        }
    }

    #[test]
    fn trecho_simples() {
        let p = construir(&base(vec![trecho("a", 10.0, 14.5, 1.0)]), "font=Sans", &resolver, &audio).unwrap();
        assert_eq!(p.inputs, vec![PathBuf::from("/v/a.mp4")]);
        let f = &p.filter_complex;
        assert!(f.contains("[0:v]trim=start=10:end=14.5,setpts=PTS-STARTPTS,scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,setsar=1,setpts=PTS/1,fps=30,format=yuv420p[v0]"), "{f}");
        assert!(f.contains("[0:a]atrim=start=10:end=14.5,asetpts=PTS-STARTPTS"));
        assert!(!f.contains("atempo"));
        assert!(f.ends_with("[v0][a0]concat=n=1:v=1:a=1[v][a]"));
        assert!((p.duracao_s - 4.5).abs() < 1e-9);
    }

    #[test]
    fn camera_lenta_com_audio() {
        let p = construir(&base(vec![trecho("a", 0.0, 2.0, 0.5)]), "font=Sans", &resolver, &audio).unwrap();
        assert!(p.filter_complex.contains("setpts=PTS/0.5"));
        assert!(p.filter_complex.contains("atempo=0.5[a0]"));
        assert!((p.duracao_s - 4.0).abs() < 1e-9);
        let p = construir(&base(vec![trecho("a", 0.0, 2.0, 0.25)]), "font=Sans", &resolver, &audio).unwrap();
        assert!(p.filter_complex.contains("atempo=0.5,atempo=0.5[a0]"));
    }

    #[test]
    fn sem_audio_na_origem_ou_desligado() {
        let p = construir(&base(vec![trecho("mudo", 0.0, 1.0, 1.0)]), "font=Sans", &resolver, &audio).unwrap();
        assert!(p.filter_complex.contains("anullsrc=r=48000:cl=stereo,atrim=duration=1"));
        let mut m = base(vec![trecho("a", 0.0, 1.0, 1.0)]);
        m.saida.incluir_audio = false;
        let p = construir(&m, "font=Sans", &resolver, &audio).unwrap();
        assert!(!p.filter_complex.contains("[0:a]"));
    }

    #[test]
    fn cartela_preta() {
        let p = construir(&base(vec![cartela(3.0, false)]), "font=Sans", &resolver, &audio).unwrap();
        assert!(p.inputs.is_empty());
        assert!(p.filter_complex.contains("color=c=0x000000:s=1920x1080:d=3:r=30"));
        assert!(p.filter_complex.contains("textfile=t0.txt:expansion=none"));
        assert_eq!(p.textos, vec![("t0.txt".to_string(), "Título".to_string())]);
        assert!(p.filter_complex.contains("anullsrc"));
    }

    #[test]
    fn congelar_quadro() {
        let it = MontagemItem::Congelar {
            id: "g".into(),
            media_id: "a".into(),
            instante_s: 7.25,
            duracao_s: 2.0,
            texto: Some(MontagemTexto { conteudo: "Impacto".into(), posicao: "rodape".into(), tamanho: "m".into(), carimbo_tempo: true }),
        };
        let p = construir(&base(vec![it]), "font=Sans", &resolver, &audio).unwrap();
        let f = &p.filter_complex;
        assert!(f.contains("trim=start=7.25,setpts=PTS-STARTPTS,trim=end_frame=1"), "{f}");
        assert!(f.contains("tpad=stop_mode=clone:stop_duration=2,trim=duration=2"));
        assert!(f.contains("text='00\\:00\\:07.250'"));
        assert!((p.duracao_s - 2.0).abs() < 1e-9);
    }

    #[test]
    fn fade_entra_e_sai_no_anterior() {
        let m = base(vec![trecho("a", 0.0, 4.0, 1.0), cartela(2.0, true)]);
        let p = construir(&m, "font=Sans", &resolver, &audio).unwrap();
        let f = &p.filter_complex;
        assert!(f.contains("format=yuv420p,fade=t=out:st=3.5:d=0.5[v0]"), "{f}");
        assert!(f.contains("afade=t=out:st=3.5:d=0.5[a0]"));
        assert!(f.contains("setsar=1,fade=t=in:st=0:d=0.5[v1]"));
        assert!(f.contains("concat=n=2:v=1:a=1"));
    }

    #[test]
    fn carimbo_de_tempo_no_trecho() {
        let mut it = trecho("a", 12.5, 14.0, 1.0);
        if let MontagemItem::Trecho { texto, .. } = &mut it {
            *texto = Some(MontagemTexto { conteudo: "A".into(), posicao: "topo".into(), tamanho: "p".into(), carimbo_tempo: true });
        }
        let p = construir(&base(vec![it]), "font=Sans", &resolver, &audio).unwrap();
        assert!(p.filter_complex.contains("text='%{pts\\:hms\\:12.5}'"));
        assert!(p.filter_complex.contains("textfile=t0.txt"));
    }

    /// Imprime um exemplo completo (`cargo test exemplo -- --nocapture`).
    #[test]
    fn exemplo() {
        let mut t = trecho("a", 2.0, 4.0, 0.5);
        if let MontagemItem::Trecho { texto, transicao_entrada, .. } = &mut t {
            *texto = Some(MontagemTexto { conteudo: "Cena 1".into(), posicao: "rodape".into(), tamanho: "m".into(), carimbo_tempo: true });
            *transicao_entrada = Some("fade".into());
        }
        let g = MontagemItem::Congelar { id: "g".into(), media_id: "a".into(), instante_s: 3.0, duracao_s: 1.5, texto: None };
        let p = construir(&base(vec![cartela(2.0, false), t, g]), "font=Sans", &resolver, &audio).unwrap();
        println!("{}", p.filter_complex);
    }

    #[test]
    fn rejeita_invalidos() {
        assert!(construir(&base(vec![]), "font=Sans", &resolver, &audio).is_err());
        assert!(construir(&base(vec![trecho("a", 5.0, 5.0, 1.0)]), "font=Sans", &resolver, &audio).is_err());
        assert!(construir(&base(vec![trecho("a", 0.0, 5.0, 0.0)]), "font=Sans", &resolver, &audio).is_err());
    }

    fn q(t: f64, x: f64, y: f64, w: Option<f64>, h: Option<f64>) -> MontagemQuadro {
        MontagemQuadro { t, x, y, w, h }
    }

    fn com_efeitos(
        zoom: Option<MontagemZoom>,
        destaques: Vec<MontagemDestaque>,
        desfoques: Vec<MontagemDesfoque>,
    ) -> MontagemItem {
        let mut it = trecho("a", 2.0, 5.0, 1.0);
        if let MontagemItem::Trecho { zoom: z, destaques: d, desfoques: f, .. } = &mut it {
            *z = zoom;
            *d = destaques;
            *f = desfoques;
        }
        it
    }

    fn zoom_exemplo() -> MontagemZoom {
        MontagemZoom {
            inicio: Regiao { x: 0.0, y: 0.0, w: 1.0, h: 1.0 },
            fim: Regiao { x: 0.25, y: 0.25, w: 0.5, h: 0.5 },
        }
    }

    fn circulo() -> MontagemDestaque {
        MontagemDestaque {
            id: "d".into(),
            forma: "circulo".into(),
            cor: "#ff0000".into(),
            quadros: vec![q(0.5, 0.25, 0.5, None, None), q(2.0, 0.75, 0.5, None, None)],
        }
    }

    fn desfoque() -> MontagemDesfoque {
        MontagemDesfoque {
            id: "f".into(),
            quadros: vec![q(0.0, 0.1, 0.1, Some(0.2), Some(0.2)), q(1.0, 0.3, 0.2, Some(0.2), Some(0.2))],
        }
    }

    #[test]
    fn zoom_interpola_no_tempo_do_trecho() {
        let it = com_efeitos(Some(zoom_exemplo()), vec![], vec![]);
        let p = construir(&base(vec![it]), "font=Sans", &resolver, &audio).unwrap();
        let f = &p.filter_complex;
        // D = 3 s (tempo original); antes do setpts da velocidade.
        assert!(f.contains("fps=30,zoompan=z='1/(1+(-0.5)*min(time/3,1))'"), "{f}");
        assert!(f.contains(":x='(0+(0.25)*min(time/3,1))*iw':y='(0+(0.25)*min(time/3,1))*ih':d=1:s=1920x1080:fps=30"), "{f}");
        assert!(f.find("zoompan").unwrap() < f.find("setpts=PTS/1").unwrap());
    }

    #[test]
    fn destaque_com_dois_quadros() {
        let it = com_efeitos(None, vec![circulo()], vec![]);
        let p = construir(&base(vec![it]), "font=Sans", &resolver, &audio).unwrap();
        let f = &p.filter_complex;
        assert!(f.contains("movie=d0_0.png,format=rgba["), "{f}");
        assert!(f.contains("overlay=x='if(lt(t,2),"), "{f}");
        assert!(f.contains("enable='gte(t,0.5)'"), "{f}");
        assert_eq!(p.imagens.len(), 1);
        assert_eq!(&p.imagens[0].1[1..4], b"PNG");
    }

    #[test]
    fn retangulo_vira_degraus_de_drawbox() {
        let mut d = circulo();
        d.forma = "retangulo".into();
        d.quadros = vec![q(0.0, 0.1, 0.1, Some(0.2), Some(0.2)), q(1.0, 0.3, 0.1, Some(0.2), Some(0.2))];
        let p = construir(&base(vec![com_efeitos(None, vec![d], vec![])]), "font=Sans", &resolver, &audio).unwrap();
        let f = &p.filter_complex;
        assert!(f.contains("drawbox=x=192:y=108:w=384:h=216:color=0xff0000"), "{f}");
        assert!(f.contains("enable='gte(t,1)'"));
        assert!(f.matches("drawbox=").count() >= 10);
        let mut sem = circulo();
        sem.forma = "retangulo".into();
        assert!(construir(&base(vec![com_efeitos(None, vec![sem], vec![])]), "font=Sans", &resolver, &audio).is_err());
    }

    #[test]
    fn desfoque_recorta_borra_e_sobrepoe() {
        let it = com_efeitos(None, vec![], vec![desfoque()]);
        let p = construir(&base(vec![it]), "font=Sans", &resolver, &audio).unwrap();
        let f = &p.filter_complex;
        assert!(f.contains("split[e0_0a][e0_0b]"), "{f}");
        assert!(f.contains("[e0_0b]crop=384:216:x='if(lt(t,1),"), "{f}");
        assert!(f.contains("boxblur=20:3[e0_0c]"));
        assert!(f.contains("[e0_0a][e0_0c]overlay=x='if(lt(t,1),"));
        assert!(f.contains("enable='gte(t,0)'"));
    }

    #[test]
    fn narracao_com_adelay_e_amix() {
        let mut m = base(vec![trecho("a", 0.0, 4.0, 1.0)]);
        m.volume_original = 0.5;
        m.narracoes = vec![MontagemNarracao {
            id: "n".into(),
            arquivo_relativo: "n.wav".into(),
            inicio_s: 1.25,
            volume: 1.5,
            nome: "n".into(),
        }];
        let p = construir(&m, "font=Sans", &resolver, &audio).unwrap();
        let f = &p.filter_complex;
        assert_eq!(p.inputs.last().unwrap(), &PathBuf::from("/a/n.wav"));
        assert!(f.contains("concat=n=1:v=1:a=1[v][as]"), "{f}");
        assert!(f.contains("[as]volume=0.5[ao]"));
        assert!(f.contains("[1:a]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,volume=1.5,adelay=1250|1250[n0]"));
        assert!(f.ends_with("[ao][n0]amix=inputs=2:normalize=0:duration=first[a]"));
        m.volume_original = 3.0;
        assert!(construir(&m, "font=Sans", &resolver, &audio).is_err());
    }

    /// Imprime um filter_complex com zoom + destaque + desfoque (`cargo test exemplo_efeitos -- --nocapture`).
    #[test]
    fn exemplo_efeitos() {
        let it = com_efeitos(Some(zoom_exemplo()), vec![circulo()], vec![desfoque()]);
        let p = construir(&base(vec![it]), "font=Sans", &resolver, &audio).unwrap();
        println!("{}", p.filter_complex);
    }

    fn tem(cmd: &str) -> bool {
        std::process::Command::new(cmd)
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Roda o ffmpeg de verdade num vídeo sintético de 3 s com os quatro efeitos.
    #[test]
    fn ffmpeg_real_com_todos_os_efeitos() {
        if !tem("ffmpeg") || !tem("ffprobe") {
            eprintln!("ffmpeg ausente: ignorado");
            return;
        }
        let dir = std::env::temp_dir().join(format!("sicro-montagem-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (fonte_mp4, nar) = (dir.join("fonte.mp4"), dir.join("nar.wav"));
        let ok = |args: &[&str]| {
            let o = std::process::Command::new("ffmpeg")
                .args(["-y", "-hide_banner", "-loglevel", "error"])
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap();
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        };
        ok(&["-f", "lavfi", "-i", "testsrc=s=1280x720:d=3:r=30", "-f", "lavfi", "-i", "sine=f=440:d=3",
             "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", fonte_mp4.to_str().unwrap()]);
        ok(&["-f", "lavfi", "-i", "sine=f=880:d=2", nar.to_str().unwrap()]);

        let mut it = com_efeitos(Some(zoom_exemplo()), vec![circulo()], vec![desfoque()]);
        if let MontagemItem::Trecho { inicio_s, fim_s, destaques, .. } = &mut it {
            *inicio_s = 0.0;
            *fim_s = 3.0;
            let mut ret = circulo();
            ret.forma = "retangulo".into();
            ret.quadros = vec![q(0.0, 0.1, 0.6, Some(0.2), Some(0.2)), q(2.0, 0.5, 0.6, Some(0.3), Some(0.2))];
            let mut seta = circulo();
            seta.forma = "seta".into();
            destaques.extend([ret, seta]);
        }
        let mut m = base(vec![it, cartela(1.0, true)]);
        m.saida.largura = 854;
        m.saida.altura = 480;
        m.narracoes = vec![MontagemNarracao {
            id: "n".into(),
            arquivo_relativo: "nar.wav".into(),
            inicio_s: 0.5,
            volume: 1.0,
            nome: "n".into(),
        }];
        let fonte = fonte_mp4.clone();
        let nar2 = nar.clone();
        let p = construir(
            &m,
            "font=Sans",
            &move |_| Ok(Fonte { path: fonte.clone(), tem_audio: true }),
            &move |_| Ok(nar2.clone()),
        )
        .unwrap();
        for (n, b) in &p.imagens {
            std::fs::write(dir.join(n), b).unwrap();
        }
        for (n, c) in &p.textos {
            std::fs::write(dir.join(n), c).unwrap();
        }
        let mut args: Vec<String> = Vec::new();
        for i in &p.inputs {
            args.push("-i".into());
            args.push(i.to_string_lossy().into_owned());
        }
        let saida = dir.join("saida.mp4");
        args.extend(
            ["-filter_complex", &p.filter_complex, "-map", "[v]", "-map", "[a]", "-c:v", "libx264", "-preset", "ultrafast",
             "-pix_fmt", "yuv420p", "-c:a", "aac", saida.to_str().unwrap()]
                .map(String::from),
        );
        let a: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        ok(&a);
        let o = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", saida.to_str().unwrap()])
            .output()
            .unwrap();
        let dur: f64 = String::from_utf8_lossy(&o.stdout).trim().parse().unwrap();
        assert!((dur - p.duracao_s).abs() < 0.25, "duração {dur} esperada {}", p.duracao_s);
        if std::env::var_os("SICRO_MANTER").is_none() {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
