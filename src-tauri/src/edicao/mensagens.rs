//! Catálogo fixo dos códigos e das frases de `Problema` (§2.3). Dono: P0.R.

use super::modelo::{Nivel, Problema};

/// Códigos do §2.3. O texto é o próprio código no fio.
pub mod codigo {
    // erros
    pub const ESQUEMA: &str = "esquema";
    pub const ESQUEMA_MENOR: &str = "esquema_menor";
    pub const SAIDA: &str = "saida";
    pub const VAZIO: &str = "vazio";
    pub const ITEM_DESCONHECIDO: &str = "item_desconhecido";
    pub const ITEM_INVALIDO: &str = "item_invalido";
    pub const MIDIA_AUSENTE: &str = "midia_ausente";
    pub const MIDIA_TROCADA: &str = "midia_trocada";
    pub const TABELA_PENDENTE: &str = "tabela_pendente";
    pub const INTERVALO: &str = "intervalo";
    pub const MAPA_GRANDE: &str = "mapa_grande";
    pub const DURACAO: &str = "duracao";
    pub const VELOCIDADE: &str = "velocidade";
    pub const TRANSICAO: &str = "transicao";
    pub const REGIAO: &str = "regiao";
    pub const RASTREIO_AUSENTE: &str = "rastreio_ausente";
    pub const RASTREIO_BASE: &str = "rastreio_base";
    pub const RASTREIO_ESPACO: &str = "rastreio_espaco";
    pub const RASTREIO_NAO_CONFIRMADO: &str = "rastreio_nao_confirmado";
    pub const SEM_COBERTURA: &str = "sem_cobertura";
    pub const OCULTAR_COR_OPACA: &str = "ocultar_cor_opaca";
    pub const LEGENDA: &str = "legenda";
    pub const LEGENDA_SOB_SILENCIO: &str = "legenda_sob_silencio";
    pub const LEGENDA_NAO_CONFERIDA: &str = "legenda_nao_conferida";
    pub const ESTABILIZAR_FORA_DA_ANALISE: &str = "estabilizar_fora_da_analise";
    pub const REGUA_ESPACO: &str = "regua_espaco";
    pub const REGUA_FORA_DA_CALIBRACAO: &str = "regua_fora_da_calibracao";
    pub const FILTRO_AUSENTE: &str = "filtro_ausente";
    pub const AUDIO: &str = "audio";
    pub const COR: &str = "cor";
    pub const PIXEL_NAO_SUPORTADO: &str = "pixel_nao_suportado";
    pub const SEM_GRAVACAO: &str = "sem_gravacao";
    // erros de comando (mídia do projeto): `Validation("<código>[: detalhe]")`
    pub const ARQUIVO_VAZIO: &str = "arquivo_vazio";
    pub const EXTENSAO: &str = "extensao";
    pub const MIME: &str = "mime";
    // periciais
    pub const TAXA_VARIAVEL: &str = "taxa_variavel";
    pub const QUADROS_DESCARTADOS: &str = "quadros_descartados";
    pub const FONTE_DANIFICADA: &str = "fonte_danificada";
    pub const MIDIA_DERIVADA_COM_PERDAS: &str = "midia_derivada_com_perdas";
    pub const OCULTACAO_REVERSIVEL: &str = "ocultacao_reversivel";
    pub const OCULTACAO_FRACA: &str = "ocultacao_fraca";
    pub const OCULTAR_INTERPOLADO: &str = "ocultar_interpolado";
    pub const OCULTAR_INCERTO: &str = "ocultar_incerto";
    pub const SILHUETA_EM_OCULTAR: &str = "silhueta_em_ocultar";
    pub const MEDIA_DE_QUADROS: &str = "media_de_quadros";
    pub const IMAGEM_IA: &str = "imagem_ia";
    pub const SINCRONIA_SEM_CONFERENCIA: &str = "sincronia_sem_conferencia";
    pub const SINCRONIA_EXTRAPOLADA: &str = "sincronia_extrapolada";
    pub const RELOGIO_DISTANTE: &str = "relogio_distante";
    // avisos
    pub const FPS_DIFERENTE: &str = "fps_diferente";
    pub const AMPLIACAO_ALTA: &str = "ampliacao_alta";
    pub const FONTE_10BITS: &str = "fonte_10bits";
    pub const ENTRELACADO: &str = "entrelacado";
    pub const TEXTO_FORA: &str = "texto_fora";
    pub const QUADRO_AJUSTADO: &str = "quadro_ajustado";
    pub const RETRATO_DESATUALIZADO: &str = "retrato_desatualizado";
    pub const SELO_DESLIGADO: &str = "selo_desligado";
}

/// Nível de um código do catálogo; None = código desconhecido.
pub fn nivel(codigo: &str) -> Option<Nivel> {
    todo!()
}

/// Frase curta do código (uma linha, para o cartão ou o campo). `n` = contagem; `detalhe` = texto variável
/// que a frase aceita (a mensagem do serde em `item_invalido`).
pub fn frase(codigo: &str, n: Option<u64>, detalhe: Option<&str>) -> String {
    todo!()
}

/// `Problema` do código, com nível e frase do catálogo e os demais campos vazios.
pub fn problema(codigo: &'static str, n: Option<u64>, detalhe: Option<&str>) -> Problema {
    todo!()
}
