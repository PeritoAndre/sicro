//! As 80 classes do COCO (as que o YOLOX reconhece), em português e agrupadas para o seletor.

use serde::Serialize;

/// Classe da trilha desenhada à mão (não vem da rede).
pub const CLASSE_MANUAL: i32 = -1;

pub const GRUPOS: [&str; 8] = [
    "Pessoas",
    "Veículos",
    "Trânsito e rua",
    "Animais",
    "Objetos de mão",
    "Esporte e lazer",
    "Comida",
    "Casa e mobiliário",
];

/// (nome, grupo) na ordem do COCO: o índice é o id da classe.
const COCO: [(&str, u8); 80] = [
    ("pessoa", 0), ("bicicleta", 1), ("carro", 1), ("moto", 1), ("avião", 1), ("ônibus", 1),
    ("trem", 1), ("caminhão", 1), ("barco", 1), ("semáforo", 2), ("hidrante", 2),
    ("placa de pare", 2), ("parquímetro", 2), ("banco de rua", 2), ("pássaro", 3), ("gato", 3),
    ("cachorro", 3), ("cavalo", 3), ("ovelha", 3), ("vaca", 3), ("elefante", 3), ("urso", 3),
    ("zebra", 3), ("girafa", 3), ("mochila", 4), ("guarda-chuva", 4), ("bolsa", 4), ("gravata", 4),
    ("mala", 4), ("frisbee", 5), ("esquis", 5), ("snowboard", 5), ("bola", 5), ("pipa", 5),
    ("taco de beisebol", 5), ("luva de beisebol", 5), ("skate", 5), ("prancha de surfe", 5),
    ("raquete de tênis", 5), ("garrafa", 4), ("taça de vinho", 4), ("copo", 4), ("garfo", 4),
    ("faca", 4), ("colher", 4), ("tigela", 6), ("banana", 6), ("maçã", 6), ("sanduíche", 6),
    ("laranja", 6), ("brócolis", 6), ("cenoura", 6), ("cachorro-quente", 6), ("pizza", 6),
    ("rosquinha", 6), ("bolo", 6), ("cadeira", 7), ("sofá", 7), ("planta", 7), ("cama", 7),
    ("mesa de jantar", 7), ("vaso sanitário", 7), ("televisão", 7), ("notebook", 4), ("mouse", 4),
    ("controle remoto", 4), ("teclado", 4), ("celular", 4), ("micro-ondas", 7), ("forno", 7),
    ("torradeira", 7), ("pia", 7), ("geladeira", 7), ("livro", 4), ("relógio", 4),
    ("vaso de planta", 7), ("tesoura", 4), ("urso de pelúcia", 7), ("secador de cabelo", 4),
    ("escova de dentes", 4),
];

pub const N_CLASSES: usize = 80;

pub fn nome(classe: i32) -> &'static str {
    if classe == CLASSE_MANUAL {
        return "mancha";
    }
    COCO.get(classe as usize).map_or("?", |c| c.0)
}

#[derive(Debug, Serialize)]
pub struct ClasseInfo {
    pub id: u16,
    pub nome: &'static str,
    pub grupo: &'static str,
}

pub fn catalogo() -> Vec<ClasseInfo> {
    COCO.iter()
        .enumerate()
        .map(|(i, c)| ClasseInfo { id: i as u16, nome: c.0, grupo: GRUPOS[c.1 as usize] })
        .collect()
}

/// Máscara das classes escolhidas (ids fora de 0..80 são ignorados).
pub fn mascara(ids: &[u16]) -> [bool; N_CLASSES] {
    let mut m = [false; N_CLASSES];
    for &i in ids {
        if (i as usize) < N_CLASSES {
            m[i as usize] = true;
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogo_tem_80_e_nomes_unicos() {
        let c = catalogo();
        assert_eq!(c.len(), 80);
        let mut n: Vec<_> = c.iter().map(|x| x.nome).collect();
        n.sort();
        n.dedup();
        assert_eq!(n.len(), 80);
        assert_eq!(nome(0), "pessoa");
        assert_eq!(nome(2), "carro");
        assert_eq!(nome(CLASSE_MANUAL), "mancha");
    }
}
