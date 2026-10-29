//! Leitura e realce de imagem no vídeo (5.3): média de quadros alinhados, super-resolução,
//! estabilização, correção de lente e retificação de perspectiva. Realce não é prova:
//! o original nunca é alterado e tudo que sai tem SHA-256 e registro.

pub mod align;
pub mod average;
pub mod perspective;
pub mod superres;
