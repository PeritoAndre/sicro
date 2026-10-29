//! Desenho da sobreposição (caixa fina, seta e número) sobre quadros RGB. O número usa
//! uma fonte de traços própria — a mesma do app (`digitFont.ts`) — para o vídeo
//! exportado ficar igual ao que se vê na tela, sem depender de fontes do sistema.

use super::Amostra;

/// O que desenhar por pessoa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Modo {
    Numero,
    NumeroSeta,
    Completo,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct Opcoes {
    pub modo: Modo,
    /// Multiplicador do tamanho do número/seta (1 = padrão).
    pub fonte: f32,
    /// Escrever o nome da classe ao lado do número.
    #[serde(default)]
    pub classe: bool,
}

impl Default for Opcoes {
    fn default() -> Self {
        Self { modo: Modo::Completo, fonte: 1.0, classe: false }
    }
}

/// Letras e números 5×7 (maiúsculas sem acento) para a etiqueta de classe; mesma tabela do app (`fonteEtiqueta.ts`).
pub const GLIFOS: [(char, [&str; 7]); 37] = [
    ('A', ["01110", "10001", "10001", "11111", "10001", "10001", "10001"]),
    ('B', ["11110", "10001", "10001", "11110", "10001", "10001", "11110"]),
    ('C', ["01110", "10001", "10000", "10000", "10000", "10001", "01110"]),
    ('D', ["11110", "10001", "10001", "10001", "10001", "10001", "11110"]),
    ('E', ["11111", "10000", "10000", "11110", "10000", "10000", "11111"]),
    ('F', ["11111", "10000", "10000", "11110", "10000", "10000", "10000"]),
    ('G', ["01110", "10001", "10000", "10111", "10001", "10001", "01111"]),
    ('H', ["10001", "10001", "10001", "11111", "10001", "10001", "10001"]),
    ('I', ["01110", "00100", "00100", "00100", "00100", "00100", "01110"]),
    ('J', ["00111", "00010", "00010", "00010", "00010", "10010", "01100"]),
    ('K', ["10001", "10010", "10100", "11000", "10100", "10010", "10001"]),
    ('L', ["10000", "10000", "10000", "10000", "10000", "10000", "11111"]),
    ('M', ["10001", "11011", "10101", "10101", "10001", "10001", "10001"]),
    ('N', ["10001", "11001", "10101", "10011", "10001", "10001", "10001"]),
    ('O', ["01110", "10001", "10001", "10001", "10001", "10001", "01110"]),
    ('P', ["11110", "10001", "10001", "11110", "10000", "10000", "10000"]),
    ('Q', ["01110", "10001", "10001", "10001", "10101", "10010", "01101"]),
    ('R', ["11110", "10001", "10001", "11110", "10100", "10010", "10001"]),
    ('S', ["01111", "10000", "10000", "01110", "00001", "00001", "11110"]),
    ('T', ["11111", "00100", "00100", "00100", "00100", "00100", "00100"]),
    ('U', ["10001", "10001", "10001", "10001", "10001", "10001", "01110"]),
    ('V', ["10001", "10001", "10001", "10001", "10001", "01010", "00100"]),
    ('W', ["10001", "10001", "10001", "10101", "10101", "10101", "01010"]),
    ('X', ["10001", "10001", "01010", "00100", "01010", "10001", "10001"]),
    ('Y', ["10001", "10001", "01010", "00100", "00100", "00100", "00100"]),
    ('Z', ["11111", "00001", "00010", "00100", "01000", "10000", "11111"]),
    ('0', ["01110", "10001", "10011", "10101", "11001", "10001", "01110"]),
    ('1', ["00100", "01100", "00100", "00100", "00100", "00100", "01110"]),
    ('2', ["01110", "10001", "00001", "00010", "00100", "01000", "11111"]),
    ('3', ["11110", "00001", "00001", "01110", "00001", "00001", "11110"]),
    ('4', ["00010", "00110", "01010", "10010", "11111", "00010", "00010"]),
    ('5', ["11111", "10000", "11110", "00001", "00001", "10001", "01110"]),
    ('6', ["00110", "01000", "10000", "11110", "10001", "10001", "01110"]),
    ('7', ["11111", "00001", "00010", "00100", "01000", "01000", "01000"]),
    ('8', ["01110", "10001", "10001", "01110", "10001", "10001", "01110"]),
    ('9', ["01110", "10001", "10001", "01111", "00001", "00010", "01100"]),
    ('-', ["00000", "00000", "00000", "11111", "00000", "00000", "00000"]),
];

/// Maiúscula sem acento (só o que a tabela tem; o resto vira espaço).
fn simplificar(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' => 'A',
        'é' | 'ê' | 'è' | 'É' | 'Ê' => 'E',
        'í' | 'Í' => 'I',
        'ó' | 'ô' | 'õ' | 'Ó' | 'Ô' | 'Õ' => 'O',
        'ú' | 'ü' | 'Ú' => 'U',
        'ç' | 'Ç' => 'C',
        c => c.to_ascii_uppercase(),
    }
}

/// Dígitos em grade 2×4, linhas poligonais.
pub const DIGITOS: [&[&[(f32, f32)]]; 10] = [
    &[&[(0.5, 0.0), (1.5, 0.0), (2.0, 0.5), (2.0, 3.5), (1.5, 4.0), (0.5, 4.0), (0.0, 3.5), (0.0, 0.5), (0.5, 0.0)]],
    &[&[(0.4, 0.9), (1.2, 0.0), (1.2, 4.0)]],
    &[&[(0.0, 0.6), (0.6, 0.0), (1.4, 0.0), (2.0, 0.6), (2.0, 1.4), (0.0, 4.0), (2.0, 4.0)]],
    &[
        &[(0.0, 0.4), (0.5, 0.0), (1.5, 0.0), (2.0, 0.5), (2.0, 1.5), (1.5, 2.0), (0.7, 2.0)],
        &[(1.5, 2.0), (2.0, 2.5), (2.0, 3.5), (1.5, 4.0), (0.5, 4.0), (0.0, 3.6)],
    ],
    &[&[(1.5, 4.0), (1.5, 0.0), (0.0, 2.8), (2.0, 2.8)]],
    &[&[(2.0, 0.0), (0.2, 0.0), (0.0, 1.9), (1.4, 1.7), (2.0, 2.3), (2.0, 3.5), (1.5, 4.0), (0.5, 4.0), (0.0, 3.6)]],
    &[&[(1.8, 0.2), (1.2, 0.0), (0.5, 0.0), (0.0, 0.6), (0.0, 3.5), (0.5, 4.0), (1.5, 4.0), (2.0, 3.5), (2.0, 2.5), (1.5, 2.0), (0.5, 2.0), (0.0, 2.5)]],
    &[&[(0.0, 0.0), (2.0, 0.0), (0.8, 4.0)]],
    &[
        &[(0.5, 0.0), (1.5, 0.0), (2.0, 0.5), (2.0, 1.5), (1.5, 2.0), (0.5, 2.0), (0.0, 1.5), (0.0, 0.5), (0.5, 0.0)],
        &[(0.5, 2.0), (0.0, 2.5), (0.0, 3.5), (0.5, 4.0), (1.5, 4.0), (2.0, 3.5), (2.0, 2.5), (1.5, 2.0)],
    ],
    &[&[(0.2, 3.8), (0.8, 4.0), (1.5, 4.0), (2.0, 3.4), (2.0, 0.5), (1.5, 0.0), (0.5, 0.0), (0.0, 0.5), (0.0, 1.5), (0.5, 2.0), (1.5, 2.0), (2.0, 1.5)]],
];

pub fn parse_hex(c: &str) -> [u8; 3] {
    let h = c.trim_start_matches('#');
    if h.len() == 6 {
        if let Ok(v) = u32::from_str_radix(h, 16) {
            return [(v >> 16) as u8, (v >> 8) as u8, v as u8];
        }
    }
    [230, 25, 75]
}

/// Preto ou branco, o que contrastar mais com a cor da pessoa.
pub fn contraste(c: [u8; 3]) -> [u8; 3] {
    let l = 0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32;
    if l > 150.0 { [20, 20, 20] } else { [255, 255, 255] }
}

pub struct Tela<'a> {
    pub buf: &'a mut [u8],
    pub w: i32,
    pub h: i32,
}

impl Tela<'_> {
    fn misturar(&mut self, x: i32, y: i32, cor: [u8; 3], a: f32) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h || a <= 0.0 {
            return;
        }
        let a = a.min(1.0);
        let i = ((y * self.w + x) * 3) as usize;
        for k in 0..3 {
            self.buf[i + k] = (self.buf[i + k] as f32 * (1.0 - a) + cor[k] as f32 * a).round() as u8;
        }
    }

    fn retangulo(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, cor: [u8; 3], raio: f32) {
        let (ix0, iy0) = (x0.floor() as i32, y0.floor() as i32);
        let (ix1, iy1) = (x1.ceil() as i32, y1.ceil() as i32);
        for y in iy0..iy1 {
            for x in ix0..ix1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                // distância assinada a um retângulo arredondado
                let cx = (x0 + x1) / 2.0;
                let cy = (y0 + y1) / 2.0;
                let qx = (px - cx).abs() - ((x1 - x0) / 2.0 - raio);
                let qy = (py - cy).abs() - ((y1 - y0) / 2.0 - raio);
                let d = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - raio;
                self.misturar(x, y, cor, (0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    fn contorno(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, esp: f32, cor: [u8; 3]) {
        self.retangulo(x0, y0, x1, y0 + esp, cor, 0.0);
        self.retangulo(x0, y1 - esp, x1, y1, cor, 0.0);
        self.retangulo(x0, y0 + esp, x0 + esp, y1 - esp, cor, 0.0);
        self.retangulo(x1 - esp, y0 + esp, x1, y1 - esp, cor, 0.0);
    }

    /// Triângulo apontando para baixo: base em `y`, ponta em `y + alt`.
    fn seta(&mut self, cx: f32, y: f32, larg: f32, alt: f32, cor: [u8; 3]) {
        let (ix0, ix1) = ((cx - larg / 2.0 - 1.0).floor() as i32, (cx + larg / 2.0 + 1.0).ceil() as i32);
        for yy in (y.floor() as i32)..((y + alt).ceil() as i32 + 1) {
            for xx in ix0..ix1 {
                let mut cob = 0.0;
                for sy in 0..3 {
                    for sx in 0..3 {
                        let px = xx as f32 + (sx as f32 + 0.5) / 3.0;
                        let py = yy as f32 + (sy as f32 + 0.5) / 3.0;
                        let t = (py - y) / alt;
                        if (0.0..=1.0).contains(&t) && (px - cx).abs() <= larg / 2.0 * (1.0 - t) {
                            cob += 1.0 / 9.0;
                        }
                    }
                }
                self.misturar(xx, yy, cor, cob);
            }
        }
    }

    fn traco(&mut self, a: (f32, f32), b: (f32, f32), esp: f32, cor: [u8; 3]) {
        let r = esp / 2.0;
        let (x0, x1) = ((a.0.min(b.0) - r - 1.0).floor() as i32, (a.0.max(b.0) + r + 1.0).ceil() as i32);
        let (y0, y1) = ((a.1.min(b.1) - r - 1.0).floor() as i32, (a.1.max(b.1) + r + 1.0).ceil() as i32);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let l2 = (dx * dx + dy * dy).max(1e-6);
        for y in y0..y1 {
            for x in x0..x1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let t = (((px - a.0) * dx + (py - a.1) * dy) / l2).clamp(0.0, 1.0);
                let d = (px - (a.0 + t * dx)).hypot(py - (a.1 + t * dy));
                self.misturar(x, y, cor, (r + 0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// Texto 5×7 com o canto superior esquerdo em (`x`, `y`); `p` = lado de cada pixel.
    fn texto(&mut self, t: &str, x: f32, y: f32, p: f32, cor: [u8; 3]) {
        let mut cx = x;
        for ch in t.chars().map(simplificar) {
            if let Some((_, linhas)) = GLIFOS.iter().find(|(c, _)| *c == ch) {
                for (ly, linha) in linhas.iter().enumerate() {
                    for (lx, b) in linha.bytes().enumerate() {
                        if b == b'1' {
                            let (px, py) = (cx + lx as f32 * p, y + ly as f32 * p);
                            self.retangulo(px, py, px + p, py + p, cor, 0.0);
                        }
                    }
                }
            }
            cx += 6.0 * p;
        }
    }

    /// Número centrado em (`cx`, `cy`), altura de `alt` pixels.
    fn numero(&mut self, n: i64, cx: f32, cy: f32, alt: f32, cor: [u8; 3]) {
        let s = alt / 4.0;
        let texto = n.max(0).to_string();
        let avanco = 2.7 * s;
        let total = avanco * texto.len() as f32 - 0.7 * s;
        let mut x = cx - total / 2.0;
        for ch in texto.chars() {
            let d = ch.to_digit(10).unwrap_or(0) as usize;
            for linha in DIGITOS[d] {
                for p in linha.windows(2) {
                    self.traco(
                        (x + p[0].0 * s, cy - alt / 2.0 + p[0].1 * s),
                        (x + p[1].0 * s, cy - alt / 2.0 + p[1].1 * s),
                        0.62 * s,
                        cor,
                    );
                }
            }
            x += avanco;
        }
    }
}

/// Medidas da etiqueta, em pixels, para um vídeo de altura `h`.
pub struct Medidas {
    pub u: f32,
}

pub fn medidas(altura: i32, fonte: f32) -> Medidas {
    Medidas { u: altura as f32 / 720.0 * fonte.clamp(0.4, 4.0) }
}

/// Desenha uma pessoa: caixa fina, seta e número.
pub fn desenhar_pessoa(tela: &mut Tela, caixa: &Amostra, numero: i64, cor_hex: &str, op: Opcoes, rotulo: Option<&str>) {
    let cor = parse_hex(cor_hex);
    let m = medidas(tela.h, op.fonte);
    let u = m.u;
    let (x0, y0, x1, y1) = (caixa.x, caixa.y, caixa.x + caixa.w, caixa.y + caixa.h);
    if op.modo == Modo::Completo {
        tela.contorno(x0, y0, x1, y1, (1.6 * u).max(1.0), cor);
    }
    let cx = (x0 + x1) / 2.0;
    let (seta_larg, seta_alt) = (16.0 * u, 12.0 * u);
    let digitos = numero.max(0).to_string().len() as f32;
    let (bh, bw) = (24.0 * u, (digitos * 14.0 + 12.0) * u);
    let folga = 3.0 * u;
    let com_seta = op.modo != Modo::Numero;
    let topo_seta = y0 - folga - seta_alt;
    if com_seta {
        // contorno escuro por baixo para a seta aparecer em fundo claro
        tela.seta(cx, topo_seta + 0.8 * u, seta_larg + 3.0 * u, seta_alt + 1.0 * u, [20, 20, 20]);
        tela.seta(cx, topo_seta, seta_larg, seta_alt, cor);
    }
    let base_badge = if com_seta { topo_seta } else { y0 - folga };
    let by1 = base_badge;
    let by0 = by1 - bh;
    let bx0 = (cx - bw / 2.0).clamp(0.0, (tela.w as f32 - bw).max(0.0));
    tela.retangulo(bx0 - u, by0 - u, bx0 + bw + u, by1 + u, [20, 20, 20], 6.0 * u);
    tela.retangulo(bx0, by0, bx0 + bw, by1, cor, 5.0 * u);
    tela.numero(numero, bx0 + bw / 2.0, (by0 + by1) / 2.0, 14.0 * u, contraste(cor));
    if let Some(r) = rotulo.filter(|r| op.classe && !r.trim().is_empty()) {
        let n = r.chars().count() as f32;
        let (p, folga_e) = (2.0 * u, 3.0 * u);
        let lw = n * 6.0 * p - p + 2.0 * folga_e + 2.0 * p;
        // à direita do número; sem espaço na tela, à esquerda
        let lx0 = if bx0 + bw + u + lw <= tela.w as f32 { bx0 + bw + u } else { bx0 - u - lw };
        tela.retangulo(lx0, by0, lx0 + lw, by1, [20, 20, 20], 5.0 * u);
        tela.texto(r, lx0 + folga_e + p, (by0 + by1) / 2.0 - 3.5 * p, p, [255, 255, 255]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desenha_algo_perto_da_pessoa_e_nada_longe() {
        let (w, h) = (640usize, 720usize);
        let mut buf = vec![0u8; w * h * 3];
        let c = Amostra { t: 0.0, x: 300.0, y: 300.0, w: 80.0, h: 300.0 };
        desenhar_pessoa(&mut Tela { buf: &mut buf, w: w as i32, h: h as i32 }, &c, 12, "#4363d8", Opcoes::default(), None);
        let px = |x: usize, y: usize| &buf[(y * w + x) * 3..(y * w + x) * 3 + 3];
        assert_eq!(px(300, 450), &[0x43, 0x63, 0xd8]); // borda esquerda da caixa
        assert_eq!(px(5, 5), &[0, 0, 0]); // longe: intacto
        assert_eq!(px(340, 450), &[0, 0, 0]); // dentro da caixa: não preenche
        assert!(buf.chunks(3).any(|p| p.iter().all(|&b| b > 200))); // número branco sobre azul
    }

    #[test]
    fn etiqueta_de_classe_desenha_texto_ao_lado_do_numero() {
        let (w, h) = (640usize, 720usize);
        let c = Amostra { t: 0.0, x: 300.0, y: 300.0, w: 80.0, h: 300.0 };
        let op = Opcoes { classe: true, ..Opcoes::default() };
        let conta = |rotulo| {
            let mut buf = vec![0u8; w * h * 3];
            desenhar_pessoa(&mut Tela { buf: &mut buf, w: w as i32, h: h as i32 }, &c, 3, "#4363d8", op, rotulo);
            buf.chunks(3).filter(|p| p[0] != 0 || p[1] != 0 || p[2] != 0).count()
        };
        assert!(conta(Some("ônibus")) > conta(None));
        assert!(conta(Some("")) == conta(None));
    }
}
