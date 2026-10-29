//! Rastreio no vídeo: detectar (YOLOX, 80 classes), acompanhar (ByteTrack), seguir manchas
//! marcadas à mão (correlação), numerar e desenhar.
//! Tudo é derivado: o vídeo original nunca é alterado e as trilhas ficam no banco do caso.

pub mod bytetrack;
pub mod classes;
pub mod decode;
pub mod mancha;
pub mod pipeline;
pub mod render;
pub mod yolox;

use serde::{Deserialize, Serialize};

/// Cores das pessoas (ciclo pelo número): distinguíveis entre si e sobre vídeo.
pub const PALETA: [&str; 12] = [
    "#e6194b", "#3cb44b", "#ffe119", "#4363d8", "#f58231", "#911eb4",
    "#42d4f4", "#f032e6", "#bfef45", "#fabed4", "#469990", "#dcbeff",
];

pub fn cor_padrao(numero: i64) -> String {
    PALETA[((numero - 1).max(0) as usize) % PALETA.len()].to_string()
}

/// Caixa automática num instante (s), em pixels do vídeo.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Amostra {
    pub t: f64,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Correção manual: diferença entre a caixa desenhada pelo perito e a automática naquele instante.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ajuste {
    pub t: f64,
    pub dx: f32,
    pub dy: f32,
    pub dw: f32,
    pub dh: f32,
}

/// Regras de tempo de uma análise (as mesmas no desenho do app e no vídeo exportado).
#[derive(Debug, Clone, Copy)]
pub struct Cadencia {
    /// Intervalo típico entre quadros do vídeo (s).
    pub frame_dur: f64,
    /// Um quadro analisado a cada `passo`.
    pub passo: u32,
}

impl Cadencia {
    fn folga(&self) -> f64 {
        self.frame_dur * self.passo as f64 * 1.5
    }
}

fn origem_auto() -> String {
    "auto".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pessoa {
    pub id: String,
    pub media_hash: String,
    pub numero: i64,
    pub nome: String,
    pub cor: String,
    /// "auto" | "editado"
    pub estado: String,
    /// Classe COCO (0 = pessoa); -1 = mancha marcada à mão.
    #[serde(default)]
    pub classe: i32,
    /// "auto" (detector) | "manual" (mancha seguida por correlação).
    #[serde(default = "origem_auto")]
    pub origem: String,
    pub amostras: Vec<Amostra>,
    pub ajustes: Vec<Ajuste>,
}

impl Pessoa {
    /// Texto da etiqueta de classe: o nome da classe; na mancha, o nome dado pelo perito.
    pub fn rotulo(&self) -> String {
        if self.classe == classes::CLASSE_MANUAL && !self.nome.trim().is_empty() {
            self.nome.trim().to_string()
        } else {
            classes::nome(self.classe).to_string()
        }
    }

    /// Deslocamento da correção manual no instante `t`: interpolado entre duas correções,
    /// constante depois da última, nulo antes da primeira.
    fn deslocamento(&self, t: f64) -> [f32; 4] {
        let a = &self.ajustes;
        if a.is_empty() || t < a[0].t {
            return [0.0; 4];
        }
        let k = a.partition_point(|x| x.t <= t) - 1;
        let p = a[k];
        match a.get(k + 1) {
            Some(n) if n.t > p.t => {
                let f = ((t - p.t) / (n.t - p.t)) as f32;
                [
                    p.dx + (n.dx - p.dx) * f,
                    p.dy + (n.dy - p.dy) * f,
                    p.dw + (n.dw - p.dw) * f,
                    p.dh + (n.dh - p.dh) * f,
                ]
            }
            _ => [p.dx, p.dy, p.dw, p.dh],
        }
    }

    /// Caixa efetiva (automática + correção) de uma amostra.
    pub fn efetiva(&self, a: &Amostra) -> Amostra {
        let d = self.deslocamento(a.t);
        Amostra { t: a.t, x: a.x + d[0], y: a.y + d[1], w: (a.w + d[2]).max(2.0), h: (a.h + d[3]).max(2.0) }
    }

    pub fn caixas(&self) -> Vec<Amostra> {
        self.amostras.iter().map(|a| self.efetiva(a)).collect()
    }

    /// Caixa mostrada no instante `t` (o quadro que o player exibe: último com pts <= t).
    pub fn caixa_em(&self, t: f64, c: Cadencia) -> Option<Amostra> {
        let s = &self.amostras;
        let k = s.partition_point(|a| a.t <= t + 1e-6);
        if k == 0 {
            return None;
        }
        let a = s[k - 1];
        if t - a.t > c.folga() {
            return None;
        }
        let ea = self.efetiva(&a);
        // Passo > 1: entre dois quadros analisados o desenho caminha de um ao outro.
        if c.passo > 1 {
            if let Some(n) = s.get(k) {
                if n.t - a.t <= c.folga() && n.t > a.t {
                    let en = self.efetiva(n);
                    let f = ((t - a.t) / (n.t - a.t)).clamp(0.0, 1.0) as f32;
                    return Some(Amostra {
                        t,
                        x: ea.x + (en.x - ea.x) * f,
                        y: ea.y + (en.y - ea.y) * f,
                        w: ea.w + (en.w - ea.w) * f,
                        h: ea.h + (en.h - ea.h) * f,
                    });
                }
            }
        }
        Some(ea)
    }

    pub fn primeiro_s(&self) -> f64 {
        self.amostras.first().map_or(0.0, |a| a.t)
    }

    pub fn ultimo_s(&self) -> f64 {
        self.amostras.last().map_or(0.0, |a| a.t)
    }

    /// Registra a caixa que o perito desenhou no instante da amostra mais próxima de `t`.
    pub fn ajustar(&mut self, t: f64, x: f32, y: f32, w: f32, h: f32) -> bool {
        let Some(a) = self
            .amostras
            .iter()
            .min_by(|a, b| (a.t - t).abs().total_cmp(&(b.t - t).abs()))
            .copied()
        else {
            return false;
        };
        let novo = Ajuste { t: a.t, dx: x - a.x, dy: y - a.y, dw: w - a.w, dh: h - a.h };
        match self.ajustes.iter_mut().find(|j| (j.t - a.t).abs() < 1e-6) {
            Some(j) => *j = novo,
            None => {
                self.ajustes.push(novo);
                self.ajustes.sort_by(|a, b| a.t.total_cmp(&b.t));
            }
        }
        self.estado = "editado".into();
        true
    }

    pub fn limpar_ajuste(&mut self, t: f64) {
        self.ajustes.retain(|j| (j.t - t).abs() > 1e-6);
        self.estado = "editado".into();
    }

    /// Junta `outra` nesta (mesma pessoa): onde as duas têm caixa no mesmo instante, vale a desta.
    pub fn juntar(&mut self, outra: &Pessoa) {
        // As correções de cada uma viram a própria caixa antes de misturar.
        let mut todas: Vec<Amostra> = self.caixas();
        for e in outra.caixas() {
            if !todas.iter().any(|b| (b.t - e.t).abs() < 1e-6) {
                todas.push(e);
            }
        }
        todas.sort_by(|a, b| a.t.total_cmp(&b.t));
        self.amostras = todas;
        self.ajustes.clear();
        if self.nome.trim().is_empty() {
            self.nome = outra.nome.clone();
        }
        self.estado = "editado".into();
    }

    /// Separa a trilha em `t`: o que vem a partir daí vira outra pessoa (devolvida).
    pub fn separar(&mut self, t: f64, novo_id: String, numero: i64) -> Option<Pessoa> {
        let k = self.amostras.partition_point(|a| a.t < t - 1e-6);
        if k == 0 || k >= self.amostras.len() {
            return None;
        }
        let depois_a = self.amostras.split_off(k);
        let (ajustes_antes, ajustes_depois): (Vec<Ajuste>, Vec<Ajuste>) =
            self.ajustes.iter().partition(|j| j.t < t - 1e-6);
        self.ajustes = ajustes_antes;
        self.estado = "editado".into();
        Some(Pessoa {
            id: novo_id,
            media_hash: self.media_hash.clone(),
            numero,
            nome: String::new(),
            cor: cor_padrao(numero),
            estado: "editado".into(),
            classe: self.classe,
            origem: self.origem.clone(),
            amostras: depois_a,
            ajustes: ajustes_depois,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pessoa(ts: &[f64]) -> Pessoa {
        Pessoa {
            id: "a".into(),
            media_hash: "h".into(),
            numero: 1,
            nome: String::new(),
            cor: cor_padrao(1),
            estado: "auto".into(),
            classe: 0,
            origem: "auto".into(),
            amostras: ts.iter().map(|&t| Amostra { t, x: t as f32 * 10.0, y: 5.0, w: 40.0, h: 100.0 }).collect(),
            ajustes: vec![],
        }
    }
    const C1: Cadencia = Cadencia { frame_dur: 0.1, passo: 1 };

    #[test]
    fn caixa_segura_o_quadro_e_some_apos_a_folga() {
        let p = pessoa(&[1.0, 1.1, 1.2]);
        assert!(p.caixa_em(0.95, C1).is_none());
        assert_eq!(p.caixa_em(1.05, C1).unwrap().x, 10.0); // quadro de 1.0
        assert_eq!(p.caixa_em(1.2, C1).unwrap().x, 12.0);
        assert!(p.caixa_em(1.5, C1).is_none());
    }

    #[test]
    fn passo_maior_interpola_entre_analisados() {
        let p = pessoa(&[1.0, 1.4]);
        let c = Cadencia { frame_dur: 0.1, passo: 4 };
        let b = p.caixa_em(1.2, c).unwrap();
        assert!((b.x - 12.0).abs() < 1e-3);
    }

    #[test]
    fn ajuste_interpola_entre_correcoes_e_segura_depois_da_ultima() {
        let mut p = pessoa(&[1.0, 2.0, 3.0, 4.0]);
        assert!(p.ajustar(1.0, 20.0, 5.0, 40.0, 100.0)); // +10 em x
        assert!(p.ajustar(3.0, 50.0, 5.0, 40.0, 100.0)); // +20 em x (auto = 30)
        let c = p.caixas();
        assert_eq!(c[0].x, 20.0);
        assert!((c[1].x - (20.0 + 15.0)).abs() < 1e-3); // auto 20 + 15 (meio de 10 e 20)
        assert_eq!(c[2].x, 50.0);
        assert_eq!(c[3].x, 40.0 + 20.0); // depois da última: segura +20
        assert_eq!(p.estado, "editado");
        p.limpar_ajuste(3.0);
        assert_eq!(p.caixas()[3].x, 40.0 + 10.0);
    }

    #[test]
    fn juntar_e_separar() {
        let mut a = pessoa(&[1.0, 2.0]);
        let mut b = pessoa(&[3.0, 4.0]);
        b.nome = "Segunda".into();
        a.juntar(&b);
        assert_eq!(a.amostras.len(), 4);
        assert_eq!(a.nome, "Segunda");
        assert_eq!(a.estado, "editado");
        let c = a.separar(3.0, "n".into(), 7).unwrap();
        assert_eq!(a.amostras.len(), 2);
        assert_eq!(c.amostras.len(), 2);
        assert_eq!(c.numero, 7);
        assert!(a.separar(0.5, "x".into(), 8).is_none()); // nada antes
    }
}
