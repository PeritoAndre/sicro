//! Monta o `-filter_complex` da Montagem (sequência de trechos, cartelas e quadros
//! congelados). Função pura: não toca em disco nem roda o ffmpeg.

use std::path::PathBuf;

use crate::error::{Result, SicroError};
use crate::models::{Montagem, MontagemItem, MontagemTexto};

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
) -> Result<Plano> {
    if m.itens.is_empty() {
        return Err(SicroError::Validation("a montagem está vazia".into()));
    }
    let (w, h, fps) = (m.saida.largura, m.saida.altura, m.saida.fps);
    if w < 64 || h < 64 || w > 7680 || h > 4320 || w % 2 != 0 || h % 2 != 0 || !(1..=120).contains(&fps) {
        return Err(SicroError::Validation("saída da montagem inválida".into()));
    }
    let mut ctx = Ctx { h, fonte, textos: Vec::new() };
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
            MontagemItem::Trecho { media_id, inicio_s, fim_s, velocidade, texto, .. } => {
                if !(0.1..=8.0).contains(velocidade) {
                    return Err(SicroError::Validation(format!("velocidade inválida no item {}", i + 1)));
                }
                if *inicio_s < 0.0 || fim_s <= inicio_s {
                    return Err(SicroError::Validation(format!("intervalo inválido no item {}", i + 1)));
                }
                let src = resolver(media_id)?;
                let k = inputs.len();
                inputs.push(src.path);
                let mut v = format!(
                    "[{k}:v]trim=start={}:end={},setpts=PTS-STARTPTS,{enquadra}",
                    segundos(*inicio_s),
                    segundos(*fim_s)
                );
                if let Some(t) = texto {
                    if t.carimbo_tempo {
                        v += &format!(",{}", ctx.drawtext_carimbo(&format!("%{{pts\\:hms\\:{}}}", segundos(*inicio_s))));
                    }
                    if let Some(d) = ctx.drawtext_usuario(t) {
                        v += &format!(",{d}");
                    }
                }
                // Velocidade depois do texto: o carimbo mostra o tempo do original.
                v += &format!(",setpts=PTS/{},fps={fps},format=yuv420p{fades_v}[{vl}]", segundos(*velocidade));
                partes.push(v);
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
    partes.push(format!("{concat_in}concat=n={n}:v=1:a=1[v][a]"));
    Ok(Plano { inputs, filter_complex: partes.join(";"), textos: ctx.textos, duracao_s: total })
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
    use crate::models::MontagemSaida;

    fn base(itens: Vec<MontagemItem>) -> Montagem {
        Montagem {
            id: "m1".into(),
            titulo: "t".into(),
            created_at: String::new(),
            updated_at: String::new(),
            itens,
            saida: MontagemSaida { largura: 1920, altura: 1080, fps: 30, incluir_audio: true },
        }
    }

    fn resolver(id: &str) -> Result<Fonte> {
        Ok(Fonte { path: PathBuf::from(format!("/v/{id}.mp4")), tem_audio: id != "mudo" })
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
        let p = construir(&base(vec![trecho("a", 10.0, 14.5, 1.0)]), "font=Sans", &resolver).unwrap();
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
        let p = construir(&base(vec![trecho("a", 0.0, 2.0, 0.5)]), "font=Sans", &resolver).unwrap();
        assert!(p.filter_complex.contains("setpts=PTS/0.5"));
        assert!(p.filter_complex.contains("atempo=0.5[a0]"));
        assert!((p.duracao_s - 4.0).abs() < 1e-9);
        let p = construir(&base(vec![trecho("a", 0.0, 2.0, 0.25)]), "font=Sans", &resolver).unwrap();
        assert!(p.filter_complex.contains("atempo=0.5,atempo=0.5[a0]"));
    }

    #[test]
    fn sem_audio_na_origem_ou_desligado() {
        let p = construir(&base(vec![trecho("mudo", 0.0, 1.0, 1.0)]), "font=Sans", &resolver).unwrap();
        assert!(p.filter_complex.contains("anullsrc=r=48000:cl=stereo,atrim=duration=1"));
        let mut m = base(vec![trecho("a", 0.0, 1.0, 1.0)]);
        m.saida.incluir_audio = false;
        let p = construir(&m, "font=Sans", &resolver).unwrap();
        assert!(!p.filter_complex.contains("[0:a]"));
    }

    #[test]
    fn cartela_preta() {
        let p = construir(&base(vec![cartela(3.0, false)]), "font=Sans", &resolver).unwrap();
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
        let p = construir(&base(vec![it]), "font=Sans", &resolver).unwrap();
        let f = &p.filter_complex;
        assert!(f.contains("trim=start=7.25,setpts=PTS-STARTPTS,trim=end_frame=1"), "{f}");
        assert!(f.contains("tpad=stop_mode=clone:stop_duration=2,trim=duration=2"));
        assert!(f.contains("text='00\\:00\\:07.250'"));
        assert!((p.duracao_s - 2.0).abs() < 1e-9);
    }

    #[test]
    fn fade_entra_e_sai_no_anterior() {
        let m = base(vec![trecho("a", 0.0, 4.0, 1.0), cartela(2.0, true)]);
        let p = construir(&m, "font=Sans", &resolver).unwrap();
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
        let p = construir(&base(vec![it]), "font=Sans", &resolver).unwrap();
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
        let p = construir(&base(vec![cartela(2.0, false), t, g]), "font=Sans", &resolver).unwrap();
        println!("{}", p.filter_complex);
    }

    #[test]
    fn rejeita_invalidos() {
        assert!(construir(&base(vec![]), "font=Sans", &resolver).is_err());
        assert!(construir(&base(vec![trecho("a", 5.0, 5.0, 1.0)]), "font=Sans", &resolver).is_err());
        assert!(construir(&base(vec![trecho("a", 0.0, 5.0, 0.0)]), "font=Sans", &resolver).is_err());
    }
}
