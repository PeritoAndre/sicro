//! Leitura de quadros do vídeo pelo ffmpeg em pipe (rawvideo RGB). O tempo de cada quadro
//! vem do pts do decodificador (`LeitorDatado`), nunca de uma conta com fps nem do pacote de
//! mesmo índice (`frame_times`, dos pacotes, só serve à busca do seguidor de antes).

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, ChildStdout, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::error::{Result, SicroError};
use crate::video::frame_export::detect_ffmpeg;
use crate::video::probe::detect_ffprobe;

/// Tamanho com que o player mostra o vídeo (giro e proporção do pixel aplicados).
pub fn display_size(path: &Path) -> Result<(u32, u32)> {
    let ffprobe = detect_ffprobe()?;
    let out = crate::tools::command(&ffprobe)
        .args(["-v", "error", "-select_streams", "v:0", "-show_streams", "-of", "json"])
        .arg(path)
        .output()
        .map_err(|e| SicroError::Workspace(format!("could not spawn ffprobe: {e}")))?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let s = v["streams"].get(0).ok_or_else(|| SicroError::Validation("vídeo sem imagem".into()))?;
    let (w, h) = (s["width"].as_f64().unwrap_or(0.0), s["height"].as_f64().unwrap_or(0.0));
    if w < 2.0 || h < 2.0 {
        return Err(SicroError::Validation("dimensões do vídeo ilegíveis".into()));
    }
    let sar = s["sample_aspect_ratio"]
        .as_str()
        .and_then(|r| {
            let (a, b) = r.split_once(':')?;
            let (a, b) = (a.parse::<f64>().ok()?, b.parse::<f64>().ok()?);
            (a > 0.0 && b > 0.0).then_some(a / b)
        })
        .unwrap_or(1.0);
    let rot = s["side_data_list"]
        .as_array()
        .and_then(|l| l.iter().find_map(|d| d["rotation"].as_f64()))
        .or_else(|| s["tags"]["rotate"].as_str().and_then(|r| r.parse::<f64>().ok()))
        .unwrap_or(0.0);
    let (mut w, h) = ((w * sar).round(), h);
    let (w2, h2) = if (rot.abs() as i64) % 180 == 90 { (h, w) } else { (w, h) };
    w = w2;
    let even = |v: f64| ((v as u32) / 2 * 2).max(2);
    Ok((even(w), even(h2)))
}

/// Instantes (s) de todos os quadros de vídeo, em ordem de apresentação.
pub fn frame_times(path: &Path) -> Result<Vec<f64>> {
    let t = crate::video::clip::all_frame_times(path)?;
    if t.is_empty() {
        return Err(SicroError::Validation("nenhum quadro de vídeo encontrado".into()));
    }
    Ok(t)
}

/// Intervalo típico entre quadros (mediana).
pub fn frame_dur(times: &[f64]) -> f64 {
    let mut d: Vec<f64> = times.windows(2).map(|w| w[1] - w[0]).filter(|d| *d > 0.0).collect();
    d.sort_by(|a, b| a.total_cmp(b));
    d.get(d.len() / 2).copied().unwrap_or(1.0 / 30.0)
}

pub struct FrameReader {
    child: Child,
    out: ChildStdout,
    buf: Vec<u8>,
}

impl FrameReader {
    /// Quadros a partir do instante `inicio_s` (busca exata), no tamanho `w`×`h`.
    /// `inicio_s` é absoluto (pts do ffprobe): `-seek_timestamp 1` impede o `-ss` de
    /// entrada de somar o `start_time` do arquivo.
    pub fn open_from(path: &Path, inicio_s: f64, w: u32, h: u32) -> Result<Self> {
        let ffmpeg = detect_ffmpeg()?;
        let vf = format!("scale={w}:{h}:flags=area,setsar=1");
        let mut child = crate::tools::command(&ffmpeg)
            .args(["-v", "error", "-nostdin", "-seek_timestamp", "1"])
            .args(["-ss", &format!("{:.4}", inicio_s.max(0.0)), "-i"])
            .arg(path)
            .args(["-map", "0:v:0", "-vf", &vf, "-fps_mode", "passthrough"])
            .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| SicroError::Workspace(format!("could not spawn ffmpeg: {e}")))?;
        let out = child.stdout.take().ok_or_else(|| SicroError::Workspace("ffmpeg sem saída".into()))?;
        Ok(Self { child, out, buf: vec![0u8; (w * h * 3) as usize] })
    }

    /// `passo` > 1 entrega só um quadro a cada `passo`; `w`×`h` é o tamanho de saída.
    pub fn open(path: &Path, passo: u32, w: u32, h: u32) -> Result<Self> {
        let ffmpeg = detect_ffmpeg()?;
        let mut vf = String::new();
        if passo > 1 {
            vf.push_str(&format!("select='not(mod(n\\,{passo}))',"));
        }
        vf.push_str(&format!("scale={w}:{h}:flags=area,setsar=1"));
        let mut child = crate::tools::command(&ffmpeg)
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(path)
            .args(["-map", "0:v:0", "-vf", &vf, "-fps_mode", "passthrough"])
            .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| SicroError::Workspace(format!("could not spawn ffmpeg: {e}")))?;
        let out = child.stdout.take().ok_or_else(|| SicroError::Workspace("ffmpeg sem saída".into()))?;
        Ok(Self { child, out, buf: vec![0u8; (w * h * 3) as usize] })
    }

    /// Próximo quadro RGB, ou `None` no fim.
    pub fn next(&mut self) -> Result<Option<&[u8]>> {
        let mut got = 0;
        while got < self.buf.len() {
            let n = self.out.read(&mut self.buf[got..])?;
            if n == 0 {
                return Ok(None);
            }
            got += n;
        }
        Ok(Some(&self.buf))
    }
}

impl Drop for FrameReader {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Linha do `showinfo` lida do stderr: base de tempo ou pts de um quadro (`todos` = antes da seleção).
enum Linha {
    Tb(i64, i64),
    Pts { todos: bool, pts: Option<i64> },
}

fn ler_linha(l: &str) -> Option<Linha> {
    let (todos, resto) = if let Some(r) = l.strip_prefix("[showinfo@todos @ ") {
        (true, r)
    } else {
        (false, l.strip_prefix("[showinfo@pq @ ")?)
    };
    let resto = resto.split_once("] ")?.1.trim_start();
    let resto = resto.strip_prefix("[info]").unwrap_or(resto).trim_start();
    if let Some(tb) = resto.strip_prefix("config in time_base:") {
        let (n, d) = tb.split(',').next()?.trim().split_once('/')?;
        let (n, d) = (n.trim().parse::<i64>().ok()?, d.trim().parse::<i64>().ok()?);
        return (n > 0 && d > 0).then_some(Linha::Tb(n, d));
    }
    if !resto.starts_with("n:") {
        return None;
    }
    let pts = resto.split_once(" pts:")?.1.split_whitespace().next()?;
    Some(Linha::Pts { todos, pts: pts.parse().ok() })
}

/// Tempos de todos os quadros decodificados (s), na ordem, ao fim de um `LeitorDatado`.
pub struct Decodificados {
    pub tempos: Vec<f64>,
}

/// Quadros RGB com o tempo de cada um pelo próprio decodificador (`-copyts` e `showinfo` lido do stderr, casado pela
/// ordem): o n-ésimo quadro decodificado leva o pts dele, nunca o do n-ésimo pacote (TS, PS ou gravador que começa
/// no meio do GOP, MP4 com lista de edição). `passo` > 1: um segundo `showinfo`, antes da seleção, conta todos.
pub struct LeitorDatado {
    leitor: FrameReader,
    linhas: Receiver<Linha>,
    leitura: Option<JoinHandle<()>>,
    tb: Option<(i64, i64)>,
    entregues: Vec<Option<i64>>,
    todos: Vec<Option<i64>>,
    passo: u32,
    lidos: usize,
}

/// A linha de um quadro sai antes dos bytes dele: a espera só cobre a leitura do stderr.
const ESPERA_LINHA: Duration = Duration::from_secs(60);

impl LeitorDatado {
    /// `ffmpeg` = o binário da Edição (`sonda::escolher`), o mesmo da tabela de quadros; `w`×`h` = tamanho de saída.
    pub fn abrir(ffmpeg: &Path, path: &Path, passo: u32, w: u32, h: u32) -> Result<Self> {
        let passo = passo.max(1);
        let mut vf = String::new();
        if passo > 1 {
            vf.push_str(&format!("showinfo@todos=checksum=0,select='not(mod(n\\,{passo}))',"));
        }
        vf.push_str(&format!("showinfo@pq=checksum=0,scale={w}:{h}:flags=area,setsar=1"));
        let mut child = crate::tools::command(ffmpeg)
            .args(["-hide_banner", "-nostdin", "-nostats", "-loglevel", "info", "-copyts", "-i"])
            .arg(path)
            .args(["-map", "0:v:0", "-vf", &vf, "-fps_mode", "passthrough"])
            .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| SicroError::Workspace(format!("could not spawn ffmpeg: {e}")))?;
        let out = child.stdout.take().ok_or_else(|| SicroError::Workspace("ffmpeg sem saída".into()))?;
        let err = child.stderr.take().ok_or_else(|| SicroError::Workspace("ffmpeg sem saída".into()))?;
        let leitor = FrameReader { child, out, buf: vec![0u8; (w * h * 3) as usize] };
        let (tx, linhas) = mpsc::channel();
        let leitura = std::thread::spawn(move || {
            let mut r = BufReader::new(err);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match r.read_until(b'\n', &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                if let Some(l) = ler_linha(&String::from_utf8_lossy(&buf)) {
                    if tx.send(l).is_err() {
                        break;
                    }
                }
            }
        });
        Ok(Self { leitor, linhas, leitura: Some(leitura), tb: None, entregues: Vec::new(), todos: Vec::new(), passo, lidos: 0 })
    }

    fn receber(&mut self, l: Linha) {
        match l {
            Linha::Tb(n, d) => self.tb = self.tb.or(Some((n, d))),
            Linha::Pts { todos: true, pts } => self.todos.push(pts),
            Linha::Pts { todos: false, pts } => self.entregues.push(pts),
        }
    }

    fn tempo(&self, j: usize, pts: Option<i64>) -> Result<f64> {
        let (n, d) = self.tb.ok_or_else(|| SicroError::Validation("pts_invalido: sem base de tempo".into()))?;
        let p = pts.ok_or_else(|| SicroError::Validation(format!("pts_invalido: quadro {j} sem pts")))?;
        Ok(p as f64 * n as f64 / d as f64)
    }

    /// Próximo quadro e o tempo dele (s, tempo de fonte), ou `None` no fim.
    pub fn proximo(&mut self) -> Result<Option<(&[u8], f64)>> {
        let k = self.lidos;
        if self.leitor.next()?.is_none() {
            return Ok(None);
        }
        self.lidos += 1;
        while self.entregues.len() <= k {
            match self.linhas.recv_timeout(ESPERA_LINHA) {
                Ok(l) => self.receber(l),
                Err(_) => return Err(SicroError::Validation(format!("leitura_divergente: quadro {k} sem pts"))),
            }
        }
        let t = self.tempo(k, self.entregues[k])?;
        Ok(Some((&self.leitor.buf, t)))
    }

    /// Lê até o fim e devolve os tempos de todos os quadros decodificados. Linha a mais ou a menos que os quadros
    /// entregues, ou ffmpeg que não terminou bem, é `leitura_divergente`.
    pub fn fim(mut self) -> Result<Decodificados> {
        while self.leitor.next()?.is_some() {
            self.lidos += 1;
        }
        let status = self.leitor.child.wait();
        if let Some(h) = self.leitura.take() {
            let _ = h.join();
        }
        while let Ok(l) = self.linhas.try_recv() {
            self.receber(l);
        }
        if self.entregues.len() != self.lidos {
            return Err(SicroError::Validation(format!("leitura_divergente: {} quadros, {} pts", self.lidos, self.entregues.len())));
        }
        // morto no meio (sinal, falta de memória) dava um vídeo cortado contado como inteiro; 69 = desistiu por erros
        // de decodificação e vale até onde foi (a regra da tabela do decodificador)
        let ok = status.as_ref().is_ok_and(|s| s.success() || (s.code() == Some(69) && self.lidos > 0));
        if !ok {
            let como = status.map_or_else(|e| e.to_string(), |s| s.to_string());
            return Err(SicroError::Validation(format!("leitura_divergente: o ffmpeg terminou com {como}")));
        }
        let todos = if self.passo > 1 { std::mem::take(&mut self.todos) } else { std::mem::take(&mut self.entregues) };
        let tempos = todos.iter().enumerate().map(|(j, p)| self.tempo(j, *p)).collect::<Result<Vec<f64>>>()?;
        Ok(Decodificados { tempos })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Vídeo sintético 320×240, 25 qps, 6 s, que começa em 3 s (`-output_ts_offset 3`).
    fn gerar(dir: &Path, nome: &str, entrada: &[&str], filtro: Option<&str>) -> Option<PathBuf> {
        let ffmpeg = detect_ffmpeg().ok()?;
        let p = dir.join(nome);
        let mut cmd = crate::tools::command(ffmpeg);
        cmd.args(["-hide_banner", "-loglevel", "error"]).args(entrada);
        if let Some(f) = filtro {
            cmd.args(["-filter_complex", f]);
        }
        let ok = cmd
            .args(["-t", "6", "-c:v", "libx264", "-qp", "0", "-g", "25", "-pix_fmt", "yuv420p"])
            .args(["-output_ts_offset", "3", "-y"])
            .arg(&p)
            .status()
            .ok()?
            .success();
        ok.then_some(p)
    }

    /// Com `SICRO_EXIGE_FFMPEG=1` a falta do ffmpeg reprova em vez de pular (§11).
    fn sem_ffmpeg() {
        assert!(std::env::var_os("SICRO_EXIGE_FFMPEG").is_none(), "ffmpeg ausente ou sem libx264");
        eprintln!("ffmpeg ausente — teste pulado");
    }

    #[test]
    fn open_from_em_arquivo_que_comeca_em_3s_le_o_quadro_pedido() {
        let dir = tempfile::tempdir().unwrap();
        let Some(src) = gerar(dir.path(), "d.mp4", &["-f", "lavfi", "-i", "testsrc=size=320x240:rate=25"], None) else {
            sem_ffmpeg();
            return;
        };
        let times = frame_times(&src).unwrap();
        assert_eq!(times.len(), 150);
        assert!((times[0] - 3.0).abs() < 1e-3, "início {}", times[0]);

        let mut seq = FrameReader::open(&src, 1, 320, 240).unwrap();
        let mut todos = Vec::new();
        while let Some(f) = seq.next().unwrap() {
            todos.push(f.to_vec());
        }
        assert_eq!(todos.len(), 150);
        for idx in [0usize, 1, 57, 149] {
            let mut r = FrameReader::open_from(&src, times[idx] - 0.0005, 320, 240).unwrap();
            let f = r.next().unwrap().expect("quadro");
            assert!(f == todos[idx].as_slice(), "busca em {:.4} não deu o quadro {idx}", times[idx]);
        }
    }

    #[test]
    fn mancha_em_arquivo_que_comeca_em_3s_parte_do_quadro_certo() {
        use crate::video::pessoas::mancha::{seguir, OpcoesMancha};
        use std::sync::atomic::AtomicBool;
        // Bloco texturizado 40×60 em x = 30 + 3n, y = 80, só nos quadros n < 40.
        let dir = tempfile::tempdir().unwrap();
        let Some(src) = gerar(
            dir.path(),
            "m.mp4",
            &[
                "-f", "lavfi", "-i", "color=c=0x404040:s=320x240:r=25",
                "-f", "lavfi", "-i", "color=c=black:s=40x60:r=25",
            ],
            Some("[1:v]format=gray,geq=lum='mod(X*X*31+Y*Y*17+X*Y*7\\,256)'[b];[0:v][b]overlay=x='30+3*n':y=80:enable='lt(n\\,40)',format=yuv420p"),
        ) else {
            sem_ffmpeg();
            return;
        };
        let times = frame_times(&src).unwrap();
        assert!((times[0] - 3.0).abs() < 1e-3, "início {}", times[0]);
        let idx0 = 10;
        let r = seguir(
            &src,
            &times,
            320,
            240,
            idx0,
            (30.0 + 3.0 * idx0 as f32, 80.0, 40.0, 60.0),
            true,
            OpcoesMancha { min_conf: 0.55, max_quadros: 20 },
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .expect("rastreio a partir do quadro 10");
        assert_eq!(r.amostras.len(), 21);
        for (k, a) in r.amostras.iter().enumerate() {
            let n = idx0 + k;
            assert!((a.t - times[n]).abs() < 1e-9);
            let x = 30.0 + 3.0 * n as f32;
            assert!((a.x - x).abs() <= 1.0 && (a.y - 80.0).abs() <= 1.0, "quadro {n}: ({}, {}) vs ({x}, 80)", a.x, a.y);
        }
    }

    /// `ffmpeg` falso: 3 quadros 8×8 com as linhas do `showinfo` e sai com `fim` (código ou sinal).
    #[cfg(unix)]
    fn ffmpeg_falso(dir: &Path, fim: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("ffmpeg falso");
        let script = format!(
            "#!/bin/sh\nprintf '[showinfo@pq @ 0x1] config in time_base: 1/25, frame_rate: 25/1\\n' >&2\ni=0\nwhile [ $i -lt 3 ]; do\n  printf '[showinfo@pq @ 0x1] n:   %d pts:   %d pts_time:0\\n' $i $i >&2\n  head -c 192 /dev/zero\n  i=$((i+1))\ndone\n{fim}\n"
        );
        std::fs::write(&p, script).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    /// ffmpeg que não termina bem (código de erro, morto por sinal) depois de entregar quadros: `fim` é erro, nunca um
    /// vídeo cortado contado como inteiro; 0 e 69 (desistiu por erros de decodificação, como a tabela) valem.
    #[cfg(unix)]
    #[test]
    fn leitor_datado_cobra_o_fim_do_ffmpeg() {
        let d = tempfile::tempdir().unwrap();
        let video = d.path().join("v.mp4");
        for (fim, vale) in [("exit 0", true), ("exit 69", true), ("exit 1", false), ("kill -9 $$", false)] {
            let mut l = LeitorDatado::abrir(&ffmpeg_falso(d.path(), fim), &video, 1, 8, 8).unwrap();
            let (_, t) = l.proximo().unwrap().expect("quadro");
            assert_eq!(t, 0.0);
            match l.fim() {
                Ok(dec) => assert!(vale && dec.tempos == vec![0.0, 0.04, 0.08], "{fim}: {:?}", dec.tempos),
                Err(SicroError::Validation(m)) => assert!(!vale && m.starts_with("leitura_divergente: o ffmpeg"), "{fim}: {m}"),
                Err(e) => panic!("{fim}: {e}"),
            }
        }
    }

    /// O ffmpeg de verdade morto no meio da leitura: erro.
    #[test]
    fn leitor_datado_morto_no_meio_e_erro() {
        use crate::edicao::testes::fontes::{self, Fonte};
        let Some(cap) = fontes::cap() else { return };
        let video = fontes::gerar(Fonte::Mp4).expect("fonte");
        let inteiro = LeitorDatado::abrir(&cap.binario, &video, 1, 64, 48).unwrap().fim().unwrap();
        assert!(inteiro.tempos.len() > 20, "{}", inteiro.tempos.len());
        let mut l = LeitorDatado::abrir(&cap.binario, &video, 1, 64, 48).unwrap();
        for _ in 0..3 {
            l.proximo().unwrap().expect("quadro");
        }
        l.leitor.child.kill().unwrap();
        let r = l.fim();
        assert!(matches!(&r, Err(SicroError::Validation(m)) if m.starts_with("leitura_divergente")), "{:?}", r.map(|x| x.tempos.len()));
    }
}
