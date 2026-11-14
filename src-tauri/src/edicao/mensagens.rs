//! Catálogo fixo dos códigos e das frases de `Problema` (§2.3). Dono: P0.R.
//! Toda frase que chega à tela sai daqui: nenhuma é montada fora do catálogo.

use super::modelo::{Nivel, Problema};
use crate::error::SicroError;

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
    // acréscimos do integrador depois da leva 3
    pub const NAO_SUPORTADO: &str = "nao_suportado";
    pub const TABELA_GRANDE: &str = "tabela_grande";
    pub const PTS_INVALIDO: &str = "pts_invalido";
    pub const SHA256_INVALIDO: &str = "sha256_invalido";
    pub const LEITURA_DIVERGENTE: &str = "leitura_divergente";
    pub const PROJETO_NAO_ENCONTRADO: &str = "projeto_nao_encontrado";
    pub const PERITO_AUSENTE: &str = "perito_ausente";
    pub const ORIGINAL: &str = "original";
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

use codigo::*;

/// Uma entrada do catálogo. `um`/`varios`: frase com a contagem (`{n}`), no singular e no plural;
/// `detalhe`: a frase aceita texto variável depois de ": ".
struct Entrada {
    codigo: &'static str,
    nivel: Nivel,
    frase: &'static str,
    um: Option<&'static str>,
    varios: Option<&'static str>,
    detalhe: bool,
}

const fn e(codigo: &'static str, nivel: Nivel, frase: &'static str) -> Entrada {
    Entrada { codigo, nivel, frase, um: None, varios: None, detalhe: false }
}

const fn com_n(codigo: &'static str, nivel: Nivel, frase: &'static str, um: &'static str, varios: &'static str) -> Entrada {
    Entrada { codigo, nivel, frase, um: Some(um), varios: Some(varios), detalhe: false }
}

const fn com_detalhe(codigo: &'static str, nivel: Nivel, frase: &'static str) -> Entrada {
    Entrada { codigo, nivel, frase, um: None, varios: None, detalhe: true }
}

use Nivel::{Aviso, Erro, Pericial};

/// O catálogo. Texto revisado aqui e só aqui.
const CATALOGO: &[Entrada] = &[
    // erros
    e(ESQUEMA, Erro, "Arquivo de projeto inválido"),
    e(ESQUEMA_MENOR, Erro, "Projeto de versão mais nova do SICRO"),
    e(SAIDA, Erro, "Tamanho ou taxa da saída fora dos limites"),
    e(VAZIO, Erro, "Nenhum clipe na sequência"),
    e(ITEM_DESCONHECIDO, Erro, "Item de versão mais nova do SICRO"),
    com_detalhe(ITEM_INVALIDO, Erro, "Item inválido"),
    e(MIDIA_AUSENTE, Erro, "Mídia ausente"),
    e(MIDIA_TROCADA, Erro, "Mídia trocada"),
    e(TABELA_PENDENTE, Erro, "Tabela de quadros em preparo"),
    e(INTERVALO, Erro, "Trecho inválido"),
    e(MAPA_GRANDE, Erro, "Trecho longo demais"),
    e(DURACAO, Erro, "Duração inválida"),
    e(VELOCIDADE, Erro, "Velocidade inválida"),
    e(TRANSICAO, Erro, "Transição inválida"),
    e(REGIAO, Erro, "Região inválida"),
    e(RASTREIO_AUSENTE, Erro, "Rastreio ausente"),
    e(RASTREIO_BASE, Erro, "Rastreio de outra base de tempo"),
    e(RASTREIO_ESPACO, Erro, "Rastreio marcado em outro enquadramento"),
    e(RASTREIO_NAO_CONFIRMADO, Erro, "Rastreio automático sem confirmação"),
    com_n(SEM_COBERTURA, Erro, "Ocultação sem região", "1 quadro sem região na ocultação", "{n} quadros sem região na ocultação"),
    e(OCULTAR_COR_OPACA, Erro, "Cor do sólido não opaca"),
    e(LEGENDA, Erro, "Legenda inválida"),
    e(LEGENDA_SOB_SILENCIO, Erro, "Legenda sobre trecho silenciado"),
    e(LEGENDA_NAO_CONFERIDA, Erro, "Legenda automática não conferida"),
    e(ESTABILIZAR_FORA_DA_ANALISE, Erro, "Clipe fora da faixa estabilizada"),
    e(REGUA_ESPACO, Erro, "Régua com estabilização ou lente"),
    e(REGUA_FORA_DA_CALIBRACAO, Erro, "Régua fora da área calibrada"),
    com_detalhe(FILTRO_AUSENTE, Erro, "Filtro ausente no ffmpeg"),
    e(AUDIO, Erro, "Áudio inválido"),
    e(COR, Erro, "Cor inválida"),
    e(PIXEL_NAO_SUPORTADO, Erro, "Formato de pixel não suportado"),
    e(SEM_GRAVACAO, Erro, "Pasta do caso só para leitura"),
    e(ARQUIVO_VAZIO, Erro, "Arquivo vazio"),
    com_detalhe(EXTENSAO, Erro, "Extensão não aceita"),
    com_detalhe(MIME, Erro, "Tipo de arquivo não aceito"),
    com_detalhe(NAO_SUPORTADO, Erro, "Ainda não suportado"),
    e(TABELA_GRANDE, Erro, "Tabela de quadros grande demais"),
    e(PTS_INVALIDO, Erro, "Tempo dos quadros inválido"),
    e(SHA256_INVALIDO, Erro, "SHA-256 inválido"),
    e(LEITURA_DIVERGENTE, Erro, "Leitura diferente da prevista"),
    e(PROJETO_NAO_ENCONTRADO, Erro, "Projeto não encontrado"),
    e(PERITO_AUSENTE, Erro, "Caso sem perito"),
    e(ORIGINAL, Erro, "Prévia pelo original"),
    // periciais
    com_n(TAXA_VARIAVEL, Pericial, "Taxa variável", "Taxa variável: 1 intervalo irregular", "Taxa variável: {n} intervalos irregulares"),
    com_n(QUADROS_DESCARTADOS, Pericial, "Quadros descartados", "1 quadro descartado", "{n} quadros descartados"),
    com_n(FONTE_DANIFICADA, Pericial, "Gravação danificada", "Gravação danificada: 1 erro", "Gravação danificada: {n} erros"),
    e(MIDIA_DERIVADA_COM_PERDAS, Pericial, "Mídia derivada com perdas"),
    e(OCULTACAO_REVERSIVEL, Pericial, "Ocultação reversível em texto"),
    e(OCULTACAO_FRACA, Pericial, "Ocultação fraca"),
    com_n(OCULTAR_INTERPOLADO, Pericial, "Ocultação por interpolação", "1 quadro ocultado por interpolação", "{n} quadros ocultados por interpolação"),
    com_n(OCULTAR_INCERTO, Pericial, "Ocultação com rastreio incerto", "1 quadro ocultado com rastreio incerto", "{n} quadros ocultados com rastreio incerto"),
    e(SILHUETA_EM_OCULTAR, Pericial, "Ocultação por silhueta"),
    e(MEDIA_DE_QUADROS, Pericial, "Média de quadros"),
    e(IMAGEM_IA, Pericial, "Imagem tratada por IA"),
    e(SINCRONIA_SEM_CONFERENCIA, Pericial, "Sincronia sem conferência visual"),
    e(SINCRONIA_EXTRAPOLADA, Pericial, "Sincronia fora do intervalo medido"),
    e(RELOGIO_DISTANTE, Pericial, "Relógio a mais de 10 min do vínculo"),
    // avisos
    com_n(FPS_DIFERENTE, Aviso, "Taxa da gravação diferente da saída", "Taxa diferente da saída: 1 quadro repetido ou descartado", "Taxa diferente da saída: {n} quadros repetidos ou descartados"),
    e(AMPLIACAO_ALTA, Aviso, "Ampliação acima de 4×"),
    e(FONTE_10BITS, Aviso, "Gravação reduzida a 8 bits 4:2:0"),
    e(ENTRELACADO, Aviso, "Gravação entrelaçada"),
    e(TEXTO_FORA, Aviso, "Texto fora da área segura"),
    com_n(QUADRO_AJUSTADO, Aviso, "Quadro ajustado", "1 quadro ajustado", "{n} quadros ajustados"),
    e(RETRATO_DESATUALIZADO, Aviso, "Retrato desatualizado"),
    com_n(SELO_DESLIGADO, Aviso, "Selo desligado", "1 selo desligado", "{n} selos desligados"),
];

/// Rótulo do que `nao_suportado` recusa (o detalhe dos erros); palavra desconhecida = só a frase.
const NAO_SUPORTADOS: &[(&str, &str)] = &[
    ("lupa", "lupa"),
    ("regua", "régua"),
    ("velocidade_exibida", "velocidade"),
    ("silhueta", "silhueta"),
    ("rampa", "rampa de velocidade"),
    ("tela", "tela dividida"),
    ("video", "vídeo sobreposto"),
    ("imagem", "imagem"),
    ("ocultar_imagem", "ocultação em imagem"),
    ("fonte sem vídeo", "mídia sem vídeo"),
];

fn entrada(codigo: &str) -> Option<&'static Entrada> {
    CATALOGO.iter().find(|x| x.codigo == codigo)
}

/// Nível de um código do catálogo; None = código desconhecido.
pub fn nivel(codigo: &str) -> Option<Nivel> {
    entrada(codigo).map(|x| x.nivel)
}

/// Frase curta do código (uma linha, para o cartão ou o campo). `n` = contagem; `detalhe` = texto variável
/// que a frase aceita (a mensagem do serde em `item_invalido`). Código fora do catálogo volta como está.
pub fn frase(codigo: &str, n: Option<u64>, detalhe: Option<&str>) -> String {
    let Some(x) = entrada(codigo) else { return codigo.to_string() };
    let base = match (n, x.um, x.varios) {
        (Some(1), Some(um), _) => um.to_string(),
        (Some(n), _, Some(varios)) => varios.replace("{n}", &n.to_string()),
        _ => x.frase.to_string(),
    };
    let detalhe = detalhe.map(str::trim).filter(|d| x.detalhe && !d.is_empty());
    match detalhe {
        Some(d) if codigo == NAO_SUPORTADO => match NAO_SUPORTADOS.iter().find(|(k, _)| *k == d) {
            Some((_, rotulo)) => format!("{base}: {rotulo}"),
            None => base,
        },
        Some(d) => format!("{base}: {d}"),
        None => base,
    }
}

/// `Problema` do código, com nível e frase do catálogo e os demais campos vazios.
/// Código fora do catálogo sai como erro, com o próprio código na mensagem.
pub fn problema(codigo: &'static str, n: Option<u64>, detalhe: Option<&str>) -> Problema {
    Problema {
        nivel: nivel(codigo).unwrap_or(Nivel::Erro),
        codigo,
        mensagem: frase(codigo, n, detalhe),
        clipe: None,
        efeito: None,
        trilha: None,
        midia: None,
        campo: None,
        n,
    }
}

// CONTRATO: acréscimo do integrador depois da leva 3 (dono: P0.R; quem usa: P0.7).
/// Erro de armazém, fila, motor ou executor (`Validation("<código>[: detalhe]")`) → `Validation("<código>: <frase do catálogo>")`.
/// Erro sem código do catálogo passa como chegou.
pub fn erro_de_comando(e: SicroError) -> SicroError {
    let SicroError::Validation(texto) = &e else { return e };
    let (cod, detalhe) = match texto.split_once(':') {
        Some((c, d)) => (c.trim(), Some(d.trim())),
        None => (texto.trim(), None),
    };
    if entrada(cod).is_none() {
        return e;
    }
    // já passou por aqui (fila que guarda a mensagem, comando que chama comando): fica como está
    let base = frase(cod, None, None);
    if detalhe.is_some_and(|d| d == base || d.strip_prefix(base.as_str()).is_some_and(|r| r.starts_with(": "))) {
        return e;
    }
    SicroError::Validation(format!("{cod}: {}", frase(cod, None, detalhe)))
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::collections::BTreeSet;

    /// Todas as constantes de `codigo`, lidas do próprio arquivo (nenhuma fica sem nível e frase).
    fn constantes() -> Vec<String> {
        let fonte = include_str!("mensagens.rs");
        let inicio = fonte.find("pub mod codigo {").unwrap();
        let fim = inicio + fonte[inicio..].find("\n}\n").unwrap();
        fonte[inicio..fim]
            .lines()
            .filter_map(|l| l.trim().strip_prefix("pub const "))
            .map(|l| l.split('"').nth(1).unwrap().to_string())
            .collect()
    }

    #[test]
    fn todo_codigo_tem_nivel_e_frase() {
        let cods = constantes();
        assert_eq!(cods.len(), 65, "constantes de `codigo`");
        let unicos: BTreeSet<_> = cods.iter().collect();
        assert_eq!(unicos.len(), cods.len(), "código repetido");
        for c in &cods {
            assert!(nivel(c).is_some(), "{c} sem nível");
            let f = frase(c, None, None);
            assert!(!f.is_empty() && f != *c, "{c} sem frase");
            assert!(!f.ends_with('.'), "{c}: frase com ponto final");
            assert!(f.chars().next().unwrap().is_uppercase(), "{c}: frase começa em maiúscula");
        }
        assert_eq!(CATALOGO.len(), cods.len(), "catálogo com entrada fora de `codigo`");
        let frases: BTreeSet<_> = CATALOGO.iter().map(|x| x.frase).collect();
        assert_eq!(frases.len(), CATALOGO.len(), "frase repetida entre códigos");
    }

    #[test]
    fn niveis_do_paragrafo_2_3() {
        for c in [ESQUEMA, SAIDA, VAZIO, ITEM_INVALIDO, SEM_COBERTURA, COR, PIXEL_NAO_SUPORTADO, NAO_SUPORTADO, SEM_GRAVACAO, ARQUIVO_VAZIO] {
            assert_eq!(nivel(c), Some(Erro), "{c}");
        }
        for c in [TAXA_VARIAVEL, QUADROS_DESCARTADOS, FONTE_DANIFICADA, MIDIA_DERIVADA_COM_PERDAS, OCULTACAO_REVERSIVEL, OCULTACAO_FRACA,
                  OCULTAR_INTERPOLADO, OCULTAR_INCERTO, SILHUETA_EM_OCULTAR, MEDIA_DE_QUADROS, IMAGEM_IA, SINCRONIA_SEM_CONFERENCIA,
                  SINCRONIA_EXTRAPOLADA, RELOGIO_DISTANTE] {
            assert_eq!(nivel(c), Some(Pericial), "{c}");
        }
        for c in [FPS_DIFERENTE, AMPLIACAO_ALTA, FONTE_10BITS, ENTRELACADO, TEXTO_FORA, QUADRO_AJUSTADO, RETRATO_DESATUALIZADO, SELO_DESLIGADO] {
            assert_eq!(nivel(c), Some(Aviso), "{c}");
        }
        assert_eq!(nivel("velocidade_fora_do_intervalo"), None, "fica para a Fase 1 (O5)");
        assert_eq!(nivel("x"), None);
    }

    #[test]
    fn contagem_no_singular_e_no_plural() {
        assert_eq!(frase(QUADROS_DESCARTADOS, Some(1), None), "1 quadro descartado");
        assert_eq!(frase(QUADROS_DESCARTADOS, Some(12), None), "12 quadros descartados");
        assert_eq!(frase(QUADROS_DESCARTADOS, None, None), "Quadros descartados");
        assert_eq!(frase(SEM_COBERTURA, Some(0), None), "0 quadros sem região na ocultação");
        // código sem contagem ignora o n
        assert_eq!(frase(VAZIO, Some(3), None), "Nenhum clipe na sequência");
        let p = problema(FONTE_DANIFICADA, Some(2), None);
        assert_eq!((p.nivel, p.codigo, p.mensagem.as_str(), p.n), (Pericial, FONTE_DANIFICADA, "Gravação danificada: 2 erros", Some(2)));
        assert!(p.clipe.is_none() && p.efeito.is_none() && p.trilha.is_none() && p.midia.is_none() && p.campo.is_none());
    }

    #[test]
    fn detalhe_so_onde_a_frase_aceita() {
        assert_eq!(frase(ITEM_INVALIDO, None, Some("missing field `fator`")), "Item inválido: missing field `fator`");
        assert_eq!(frase(ITEM_INVALIDO, None, Some("  ")), "Item inválido");
        assert_eq!(frase(MIDIA_AUSENTE, None, Some("/home/x/video.mp4 fora do caso")), "Mídia ausente");
        assert_eq!(frase(NAO_SUPORTADO, None, Some("lupa")), "Ainda não suportado: lupa");
        assert_eq!(frase(NAO_SUPORTADO, None, Some("rampa")), "Ainda não suportado: rampa de velocidade");
        assert_eq!(frase(NAO_SUPORTADO, None, Some("cópia de hevc yuv420p10le")), "Ainda não suportado");
        assert_eq!(frase(FILTRO_AUSENTE, None, Some("vidstabtransform")), "Filtro ausente no ffmpeg: vidstabtransform");
        assert_eq!(frase("cancelado", None, None), "cancelado");
    }

    #[test]
    fn erro_de_comando_troca_o_detalhe_pela_frase() {
        let msg = |e: SicroError| match e {
            SicroError::Validation(m) => m,
            outro => panic!("{outro:?}"),
        };
        let v = |s: &str| SicroError::Validation(s.into());
        assert_eq!(msg(erro_de_comando(v("tabela_pendente"))), "tabela_pendente: Tabela de quadros em preparo");
        assert_eq!(msg(erro_de_comando(v("midia_ausente: /caso/x.mp4 fora do caso"))), "midia_ausente: Mídia ausente");
        assert_eq!(msg(erro_de_comando(v("item_invalido: texto longo demais"))), "item_invalido: Item inválido: texto longo demais");
        assert_eq!(msg(erro_de_comando(v("nao_suportado: imagem"))), "nao_suportado: Ainda não suportado: imagem");
        assert_eq!(msg(erro_de_comando(v("extensao: exe"))), "extensao: Extensão não aceita: exe");
        assert_eq!(msg(erro_de_comando(v("leitura_divergente: nenhum quadro"))), "leitura_divergente: Leitura diferente da prevista");
        // fora do catálogo: como chegou
        assert_eq!(msg(erro_de_comando(v("cancelado: prévia"))), "cancelado: prévia");
        assert_eq!(msg(erro_de_comando(v("ffmpeg_ausente: SICRO_FFMPEG=/x"))), "ffmpeg_ausente: SICRO_FFMPEG=/x");
        assert_eq!(msg(erro_de_comando(v("y4m inválido"))), "y4m inválido");
        // outro tipo de erro: intacto
        assert!(matches!(erro_de_comando(SicroError::Filesystem("x".into())), SicroError::Filesystem(m) if m == "x"));
        // aplicar duas vezes não estraga, com e sem detalhe
        for cru in ["vazio", "item_invalido: texto longo demais", "nao_suportado: lupa", "nao_suportado: efeito nitidez e2", "extensao: exe", "sem_cobertura: e1"] {
            let uma = msg(erro_de_comando(v(cru)));
            assert_eq!(msg(erro_de_comando(v(&uma))), uma, "{cru}");
        }
        assert_eq!(msg(erro_de_comando(v("nao_suportado: ocultar_imagem"))), "nao_suportado: Ainda não suportado: ocultação em imagem");
    }
}
