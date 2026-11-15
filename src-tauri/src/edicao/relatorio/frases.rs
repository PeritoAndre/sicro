//! Catálogo fixo das frases do relatório (§10.3): uma por tipo de efeito e por condição detectada, para o texto não
//! variar de caso para caso. Nenhuma frase do PDF é montada fora daqui. Dono: P1.R.
//! Redação para leigo: frases curtas, sem sigla. Nunca "conforme" nem "certificado".

use crate::edicao::mensagens::codigo;

/// O que o item é para quem assiste: algo posto pela perícia ou a gravação mostrada de outro jeito (seção 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classe {
    Acrescimo,
    Mudanca,
}

/// Tipos que não são efeito do modelo, mas entram nas mesmas tabelas (velocidade, quadro parado, cartela, som…).
pub mod tipo {
    pub const CAMERA_LENTA: &str = "camera_lenta";
    pub const ACELERADO: &str = "acelerado";
    pub const CONGELAR: &str = "congelar";
    pub const TITULO: &str = "titulo";
    pub const IMAGEM_CLIPE: &str = "imagem_clipe";
    pub const TRANSICAO: &str = "transicao";
    pub const NARRACAO: &str = "narracao";
    pub const MUSICA: &str = "musica";
    pub const EFEITOS_SONOROS: &str = "efeitos_sonoros";
    pub const LEGENDAS: &str = "legendas";
    pub const SILENCIO: &str = "silencio";
    pub const MUDO: &str = "mudo";
    pub const VOLUME: &str = "volume";
    /// Enquadramento da imagem inteira em "Preencher": nada é ampliado por escolha de região, mas as bordas saem.
    pub const PREENCHER: &str = "preencher";
}

/// Condições que não são código de validação (seções 6 a 8 e anexos).
pub mod condicao {
    pub const COM_PERDAS: &str = "com_perdas";
    pub const SELO_DESLIGADO: &str = "selo_desligado";
    pub const OCULTACAO_NAO_CONFERIDA: &str = "ocultacao_nao_conferida";
    pub const OCULTACAO_PENDENTE: &str = "ocultacao_pendente";
    pub const OCULTACAO_CONFERIDA: &str = "ocultacao_conferida";
    pub const RECEBIDA_CONVERTIDA: &str = "recebida_convertida";
    pub const RECEBIMENTO_NAO_INFORMADO: &str = "recebimento_nao_informado";
    pub const APARENCIA: &str = "aparencia";
    pub const QUEDA_PARA_CPU: &str = "queda_para_cpu";
    pub const IA_NENHUMA: &str = "ia_nenhuma";
    pub const SEM_RELOGIO: &str = "sem_relogio";
    pub const RELOGIO_TEMPO_FONTE: &str = "relogio_tempo_fonte";
    pub const RELOGIO_CAMERA: &str = "relogio_camera";
    pub const RELOGIO_QUADRO: &str = "relogio_quadro";
    pub const RELOGIO_TEMPO_PROGRAMA: &str = "relogio_tempo_programa";
    pub const INCERTEZA_NAO_DETERMINADA: &str = "incerteza_nao_determinada";
    pub const PROVA_ARQUIVO: &str = "prova_arquivo";
    pub const PROVA_QUADROS: &str = "prova_quadros";
    pub const REFAZER_CPU: &str = "refazer_cpu";
    pub const REFAZER_GPU: &str = "refazer_gpu";
    pub const CIENTE: &str = "ciente";
    pub const SINCRONIA: &str = "sincronia";
}

/// Frases com número ou nome no meio: `{n}`, `{item}`… são trocados por quem monta os dados (`juri::dados`).
pub mod modelo {
    pub const IA_OCULTACOES: &str = "Ocultações propostas por detecção automática e confirmadas pelo perito: {n}.";
    pub const IA_MARCACOES: &str = "Marcações que acompanham alvo achado por detecção automática e confirmado pelo perito: {n}.";
    pub const IA_ASSISTIDO: &str = "Alvos acompanhados por programa a partir de marcação do perito: {n}.";
    pub const IA_IMAGENS: &str = "Imagens ampliadas por programa de inteligência artificial: {n}.";
    pub const IA_MODELOS: &str = "Os programas de inteligência artificial usados estão identificados no anexo técnico, item H.";
    pub const MEDIDA_REGUA: &str = "Régua de escala de {m} m (item {item}): erro médio da calibração de {residuo} m.";
    pub const MEDIDA_VELOCIDADE: &str = "Velocidade calculada (item {item}): {kmh} km/h, na faixa de {de} a {ate} km/h.";
    pub const MEDIDA_VELOCIDADE_SEM_FAIXA: &str = "Velocidade calculada (item {item}): aproximadamente {kmh} km/h, sem faixa de erro calculada.";
    pub const DERIVADA: &str = "Este arquivo foi feito pela perícia a partir de outra gravação do caso ({como}).";
    pub const CONFERIDA_POR: &str = "{frase} {por}, em {quando}.";
    /// Anexo E: trechos em que o perito aceitou a falta do alvo (o rastreio não dá região e nada é coberto).
    pub const AUSENCIAS: &str = "trechos sem ocultação, aceitos pelo perito: {lista}";
    pub const IA_LEGENDAS: &str = "Legendas escritas a partir de degravação automática e conferidas pelo perito.";
}

/// (chave, texto): rótulos de valores do modelo (`grupo.valor`) usados nas tabelas.
const ROTULOS: &[(&str, &str)] = &[
    ("clipe.trecho", "Trecho de gravação"),
    ("clipe.congelar", "Imagem parada"),
    ("clipe.imagem", "Fotografia ou figura"),
    ("clipe.titulo", "Tela de texto"),
    ("clipe.tela", "Câmeras lado a lado"),
    ("especie.video", "Gravação"),
    ("especie.imagem", "Imagem"),
    ("especie.som", "Som"),
    ("velocidade.normal", "normal (1×)"),
    ("velocidade.lenta", "câmera lenta ({v}×)"),
    ("velocidade.rapida", "acelerada ({v}×)"),
    ("velocidade.parada", "parada"),
    ("velocidade.variavel", "variável ao longo do trecho"),
    ("recebido.nativo", "no formato próprio do aparelho gravador"),
    ("recebido.convertido", "já convertida, e não no formato próprio do aparelho gravador"),
    ("recebido.nao_informado", "não informado"),
    ("relogio.sem", "sem informação"),
    ("relogio.desvio", "desvio informado pelo perito: {d}"),
    ("ritmo.constante", "{fps} imagens por segundo, constante"),
    ("ritmo.variavel", "entre {de} e {ate} imagens por segundo (variável)"),
    ("ritmo.saida", "{fps} imagens por segundo"),
    ("ritmo.imagem", "imagem única"),
    ("ritmo.som", "só som"),
    ("derivacao.trecho", "trecho recortado"),
    ("derivacao.estabilizado", "cópia estabilizada"),
    ("derivacao.lente", "cópia com a lente corrigida"),
    ("derivacao.copia_dvr", "cópia convertida do formato do gravador"),
    ("derivacao.superres", "ampliação por inteligência artificial"),
    ("derivacao.outro", "outro processamento"),
    ("derivacao.sem_perdas", "sem recompressão"),
    ("derivacao.com_perdas", "com recompressão"),
    ("conteudo.rosto", "rosto"),
    ("conteudo.placa", "placa"),
    ("conteudo.corpo", "corpo"),
    ("conteudo.tela", "tela"),
    ("conteudo.documento", "documento"),
    ("conteudo.outro", "outro"),
    ("conteudo.som", "som"),
    ("modo.solido", "sólido opaco"),
    ("modo.mosaico", "mosaico"),
    ("modo.desfoque", "desfoque"),
    ("modo.tom", "tom no lugar do som"),
    ("modo.mudo", "silêncio no lugar do som"),
    ("forma.retangulo", "retângulo"),
    ("forma.elipse", "elipse"),
    ("forma.poligono", "polígono"),
    ("forma.silhueta", "silhueta"),
    ("forma.invertida", "invertida (cobre tudo o que está fora da forma)"),
    ("ausencia.oculto", "alvo encoberto"),
    ("ausencia.fora_de_cena", "alvo fora de cena"),
    ("ausencia.perdido", "alvo perdido pelo acompanhamento"),
    ("predefinicao.juri", "Júri"),
    ("predefinicao.whatsapp", "WhatsApp"),
    ("predefinicao.preservacao", "Preservação"),
    ("predefinicao.personalizada", "Personalizada"),
    ("som.com", "com som"),
    ("som.sem", "sem som"),
    ("sincronia.audio", "pelo som"),
    ("sincronia.evento", "por acontecimento visível nas duas câmeras, marcado pelo perito"),
    ("sincronia.relogio", "pelo relógio dos gravadores"),
    ("sincronia.manual", "ajuste do perito"),
    ("sincronia.sem_conferencia", "sem conferência por acontecimento visível"),
    ("sincronia.conferida", "conferido em acontecimento visível nas duas câmeras ({n})"),
    ("sincronia.sem_evento", "sem acontecimento visível em comum (registrado pelo perito)"),
    ("selo.velocidade", "Selo de velocidade"),
    ("selo.ampliacao", "Selo de ampliação"),
    ("selo.realce", "Selo de realce"),
    ("onde.item", "item {n}"),
    ("onde.itens", "itens {n}"),
    ("onde.video", "ao longo do vídeo"),
    ("codificacao.cpu", "esperada com os mesmos programas, na mesma classe de máquina (codificador de CPU)"),
    ("codificacao.gpu", "não esperada (codificador de GPU); a comparação é pelo SHA-256 dos quadros"),
    ("anexo.tecnico", "Relatório técnico completo, legível por programa (JSON)"),
    ("anexo.projeto", "Projeto da edição, para refazer a montagem (JSON)"),
    ("papel.sicro", "edição, cálculo dos códigos de verificação, este relatório"),
    ("papel.ffmpeg", "leitura das gravações e produção do vídeo"),
    ("papel.typst", "textos da imagem e composição deste relatório"),
    ("corte", "Corte no tempo"),
    ("enquadramento.inteiro", "Ajuste de tamanho"),
];

/// Rótulo de um valor do modelo (`grupo.valor`); None = fora do catálogo.
pub fn rotulo(chave: &str) -> Option<&'static str> {
    ROTULOS.iter().find(|x| x.0 == chave).map(|x| x.1)
}

/// Troca `{nome}` pelo valor em `modelo`.
pub fn preencher(modelo: &str, campos: &[(&str, &str)]) -> String {
    let mut s = modelo.to_string();
    for (k, v) in campos {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

/// Referências do anexo J, com versão e data (lista padrão; a editável vem das Configurações).
pub const REFERENCIAS: &[&str] = &[
    "SWGDE Best Practices for Digital Forensic Video Analysis, 18-V-001, versão 2.0 (10/12/2025)",
    "SWGDE Considerations for the Use of Time-Based Analysis of Digital Video for Court, 19-V-003, versão 1.0 (17/09/2020)",
    "SWGDE Video and Audio Redaction Guidelines, 18-M-001, versão 2.3 (19/12/2025)",
    "SWGDE Considerations for Release of Synopsis Videos for Public Review, 21-V-002, versão 1.2 (02/12/2024)",
    "OSAC 2022-S-0031, Standard Guide for Forensic Digital Video Examination Workflow, versão 2.0 (janeiro de 2024)",
];

/// (tipo, classe, rótulo curto, frase). O rótulo vai na coluna "O quê" e na lista do item do roteiro.
const EFEITOS: &[(&str, Classe, &str, &str)] = &[
    ("estabilizar", Classe::Mudanca, "Estabilização", "Compensa a tremida da câmera para a cena ficar parada na tela. As bordas podem aparecer cortadas ou pretas."),
    ("lente", Classe::Mudanca, "Correção da lente", "Desfaz a curvatura que a lente da câmera dá às linhas retas. O conteúdo da imagem é o mesmo, com outra geometria."),
    ("desentrelacar", Classe::Mudanca, "Desentrelaçamento", "A gravação guarda cada imagem em duas metades tiradas em instantes diferentes. Elas foram separadas para o movimento não aparecer serrilhado."),
    ("media_temporal", Classe::Mudanca, "Média de imagens", "Cada imagem mostrada é a média de várias imagens seguidas da gravação. Acontecimentos rápidos podem sumir ou parecer durar mais do que duraram."),
    ("ajustes", Classe::Mudanca, "Brilho, contraste e cor", "Muda o claro e o escuro da imagem inteira para facilitar a visão. Não cria detalhe que a câmera não registrou."),
    ("nitidez", Classe::Mudanca, "Nitidez", "Reforça os contornos da imagem inteira. Não cria detalhe que a câmera não registrou."),
    ("girar", Classe::Mudanca, "Giro ou espelhamento", "A imagem foi girada ou espelhada para ficar na posição de leitura."),
    ("enquadramento", Classe::Mudanca, "Ampliação", "Mostra uma parte da imagem em tamanho maior. Ampliar não cria detalhe que a câmera não registrou."),
    ("ocultar", Classe::Mudanca, "Ocultação", "Parte da imagem foi coberta para preservar pessoa ou dado sem relação com o fato. No vídeo entregue não é possível desfazer; a gravação original continua íntegra."),
    ("holofote", Classe::Mudanca, "Holofote", "O entorno foi escurecido para guiar o olhar até uma parte da imagem. A parte realçada não foi alterada."),
    ("destaque", Classe::Acrescimo, "Marcação", "Desenho posto sobre a imagem para guiar o olhar. Não faz parte da gravação."),
    ("lupa", Classe::Acrescimo, "Lupa", "Repete, em tamanho maior, uma parte da própria imagem. Ampliar não cria detalhe que a câmera não registrou."),
    ("regua", Classe::Acrescimo, "Régua de escala", "Desenhada a partir de medidas tomadas pela perícia. Vale só no plano do chão em que foi calibrada."),
    ("velocidade_exibida", Classe::Acrescimo, "Velocidade calculada", "Resultado de cálculo da perícia, mostrado com a faixa de erro. Não é leitura de aparelho."),
    ("texto", Classe::Acrescimo, "Texto", "Escrito pelo perito para identificar o local, o momento ou o que se vê."),
    ("carimbo", Classe::Acrescimo, "Relógio na imagem", "Posto pela perícia. O que ele conta está explicado na seção sobre tempo."),
    ("imagem", Classe::Acrescimo, "Imagem sobreposta", "Figura posta pela perícia sobre a gravação."),
    (tipo::CAMERA_LENTA, Classe::Mudanca, "Câmera lenta", "O trecho passa mais devagar do que aconteceu. Nenhuma imagem foi criada: as da gravação ficam mais tempo na tela."),
    (tipo::ACELERADO, Classe::Mudanca, "Velocidade aumentada", "O trecho passa mais depressa do que aconteceu. Parte das imagens pode não ser mostrada."),
    (tipo::CONGELAR, Classe::Mudanca, "Imagem parada", "Uma única imagem da gravação fica na tela pelo tempo indicado."),
    (tipo::TITULO, Classe::Acrescimo, "Tela de texto", "Tela escrita pelo perito. Não faz parte das gravações."),
    (tipo::IMAGEM_CLIPE, Classe::Acrescimo, "Fotografia ou figura", "Arquivo de imagem juntado pela perícia, mostrado parado."),
    (tipo::TRANSICAO, Classe::Mudanca, "Passagem entre partes", "Efeito de passagem de uma parte para a outra. Não faz parte das gravações."),
    (tipo::NARRACAO, Classe::Acrescimo, "Narração", "Voz gravada depois pela perícia. Não é som do local."),
    (tipo::MUSICA, Classe::Acrescimo, "Música", "Som posto pela perícia. Não é som do local."),
    (tipo::EFEITOS_SONOROS, Classe::Acrescimo, "Som acrescentado", "Som posto pela perícia. Não é som do local."),
    (tipo::LEGENDAS, Classe::Acrescimo, "Legendas", "Texto do que é falado, escrito ou conferido pelo perito."),
    (tipo::SILENCIO, Classe::Mudanca, "Som ocultado", "Um trecho do som foi trocado por um tom ou por silêncio para preservar dado sem relação com o fato."),
    (tipo::MUDO, Classe::Mudanca, "Som da gravação retirado", "O som desta parte da gravação não foi incluído no vídeo."),
    (tipo::PREENCHER, Classe::Mudanca, "Imagem ajustada para preencher a tela", "A imagem foi aumentada até ocupar a tela inteira. O que não cabe nas bordas fica de fora."),
    (tipo::VOLUME, Classe::Mudanca, "Volume alterado", "O som da gravação foi deixado mais alto ou mais baixo. Nada foi acrescentado a ele."),
];

/// (código, frase).
const CONDICOES: &[(&str, &str)] = &[
    (codigo::TAXA_VARIAVEL, "Há gravação que não registra as imagens em intervalos iguais. Movimentos vistos nela podem parecer mais rápidos ou mais lentos do que foram."),
    (codigo::QUADROS_DESCARTADOS, "Parte das imagens da gravação não aparece no vídeo, porque ele mostra menos imagens por segundo do que a gravação tem."),
    (codigo::FONTE_DANIFICADA, "Há gravação com defeito de leitura. Partes da imagem podem ter sido reconstruídas pelo programa de leitura e não devem servir a exame de detalhe."),
    (codigo::MIDIA_DERIVADA_COM_PERDAS, "Há gravação usada que já é cópia recomprimida de outra gravação do caso. Para exame de detalhe vale a gravação de origem."),
    (codigo::OCULTACAO_REVERSIVEL, "Há texto ou placa coberto por mosaico ou desfoque. Esse tipo de cobertura pode permitir a leitura por quem tentar revertê-la."),
    (codigo::OCULTACAO_FRACA, "Há ocultação por mosaico ou desfoque de pouca intensidade, que pode não impedir o reconhecimento."),
    (codigo::OCULTAR_INTERPOLADO, "Em parte das imagens, a posição da ocultação foi calculada entre duas marcações do perito, e não marcada uma a uma."),
    (codigo::OCULTAR_INCERTO, "Em parte das imagens, a posição da ocultação veio de acompanhamento automático que o programa marcou como incerto."),
    (codigo::SILHUETA_EM_OCULTAR, "Há ocultação recortada pelo contorno do corpo. O contorno é calculado por programa e pode deixar bordas à mostra."),
    (codigo::MEDIA_DE_QUADROS, "Há trecho em que cada imagem é a média de várias imagens seguidas. Acontecimentos rápidos podem sumir ou parecer durar mais do que duraram."),
    (codigo::IMAGEM_IA, "Há imagem ampliada por programa de inteligência artificial. Os detalhes que ela mostra podem ter sido criados pelo programa e não servem a exame."),
    (codigo::SINCRONIA_SEM_CONFERENCIA, "As câmeras foram alinhadas pelo som ou pelo relógio, sem conferência por um acontecimento visível nas duas."),
    (codigo::SINCRONIA_EXTRAPOLADA, "O alinhamento entre as câmeras foi usado fora do intervalo em que foi medido."),
    (codigo::RELOGIO_DISTANTE, "O relógio da câmera mostrado na imagem foi acertado em momento distante do trecho exibido e pode ter se desviado."),
    (condicao::COM_PERDAS, "O vídeo entregue foi gravado em formato com compressão. Para exame técnico valem as gravações originais, e não este vídeo."),
    (condicao::SELO_DESLIGADO, "O perito desligou o aviso que o programa escreve na imagem quando ela é mostrada de outro jeito. As mudanças continuam listadas neste relatório."),
    (condicao::OCULTACAO_NAO_CONFERIDA, "As ocultações não foram conferidas pelo perito no arquivo final."),
    (condicao::OCULTACAO_PENDENTE, "A conferência das ocultações no arquivo final ainda não foi registrada."),
    (condicao::OCULTACAO_CONFERIDA, "As ocultações foram conferidas pelo perito no arquivo final."),
    (condicao::RECEBIDA_CONVERTIDA, "Há gravação recebida já convertida, e não no formato próprio do aparelho gravador. Parte das informações de tempo pode ter se perdido na conversão."),
    (condicao::RECEBIMENTO_NAO_INFORMADO, "Não foi informado se a gravação veio no formato próprio do aparelho gravador."),
    (condicao::APARENCIA, "A distância e o ângulo da câmera, a lente, a iluminação e a proporção da imagem mudam a aparência de tamanho e de velocidade do que se vê."),
    (condicao::QUEDA_PARA_CPU, "A placa de vídeo falhou durante a exportação e o trabalho foi refeito inteiro pelo processador."),
    (condicao::IA_NENHUMA, "Nenhuma etapa deste vídeo foi feita por programa de inteligência artificial."),
    (condicao::SEM_RELOGIO, "O vídeo não mostra relógio na imagem."),
    (condicao::RELOGIO_TEMPO_FONTE, "O relógio marcado \"Original\" conta o tempo desde o início da gravação original. Ele não mostra a hora do dia. Em câmera lenta ele anda mais devagar, na mesma proporção."),
    (condicao::RELOGIO_CAMERA, "O relógio marcado \"Câmera\" mostra a data e a hora do relógio do aparelho gravador, que pode estar adiantado ou atrasado."),
    (condicao::RELOGIO_QUADRO, "O número marcado \"Quadro\" é a posição da imagem na gravação original, contada a partir de zero."),
    (condicao::RELOGIO_TEMPO_PROGRAMA, "O relógio marcado \"Vídeo\" conta o tempo deste vídeo, desde o começo dele."),
    (condicao::INCERTEZA_NAO_DETERMINADA, "não determinada"),
    (condicao::PROVA_ARQUIVO, "Se o código calculado for igual ao impresso neste relatório, o arquivo é exatamente o que a perícia produziu, sem nenhuma alteração."),
    (condicao::PROVA_QUADROS, "Esta conferência calcula um código para cada imagem do vídeo. Se a lista for igual à do pacote, as imagens são as mesmas, ainda que o arquivo tenha sido copiado para outro formato de embalagem sem recompressão."),
    (condicao::REFAZER_CPU, "Refeita com as mesmas versões dos programas e na mesma classe de computador, a montagem resulta em arquivo idêntico, com o mesmo código de verificação. Em outro computador, as imagens antes da compressão resultam iguais e o arquivo pode diferir."),
    (condicao::REFAZER_GPU, "O vídeo foi comprimido pela placa de vídeo. Refeita, a montagem resulta nas mesmas imagens antes da compressão; o arquivo pode diferir."),
    (condicao::CIENTE, "O perito registrou ciência deste aviso antes de exportar."),
    (condicao::SINCRONIA, "Os relógios das câmeras não marcam a mesma hora. Para mostrá-las juntas, a perícia alinhou uma à outra."),
];

fn efeito(tipo: &str) -> Option<&'static (&'static str, Classe, &'static str, &'static str)> {
    EFEITOS.iter().find(|x| x.0 == tipo)
}

/// Frase de "O que foi acrescentado e modificado" para o tipo de efeito (o `tipo` do fio).
pub fn do_efeito(tipo: &str) -> Option<&'static str> {
    efeito(tipo).map(|x| x.3)
}

/// Rótulo curto do tipo (coluna "O quê" e lista do item do roteiro).
pub fn rotulo_do_efeito(tipo: &str) -> Option<&'static str> {
    efeito(tipo).map(|x| x.2)
}

/// Acrescentado pela perícia ou mudança na forma de mostrar.
pub fn classe_do_efeito(tipo: &str) -> Option<Classe> {
    efeito(tipo).map(|x| x.1)
}

/// Frase de "Limites" para o código do catálogo (`mensagens::codigo`) que aparece no `feito.avisos`, e das demais
/// condições do relatório (`condicao::*`).
pub fn da_condicao(codigo: &str) -> Option<&'static str> {
    CONDICOES.iter().find(|x| x.0 == codigo).map(|x| x.1)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::mensagens;
    use crate::edicao::modelo::Nivel;
    use crate::edicao::validar::TIPOS_EFEITO;
    use std::collections::BTreeSet;

    #[test]
    fn todo_efeito_do_modelo_tem_rotulo_classe_e_frase() {
        for t in TIPOS_EFEITO {
            assert!(do_efeito(t).is_some() && rotulo_do_efeito(t).is_some() && classe_do_efeito(t).is_some(), "{t}");
        }
        assert_eq!(do_efeito("efeito_do_futuro"), None);
        assert_eq!(classe_do_efeito("destaque"), Some(Classe::Acrescimo));
        assert_eq!(classe_do_efeito("ocultar"), Some(Classe::Mudanca));
    }

    /// Todo aviso pericial do catálogo de validação tem frase de limite: nenhum sai do PDF calado.
    #[test]
    fn todo_aviso_pericial_tem_frase() {
        let fonte = include_str!("../mensagens.rs");
        let periciais: Vec<&str> = fonte
            .lines()
            .filter_map(|l| l.trim().strip_prefix("pub const "))
            .filter_map(|l| l.split('"').nth(1))
            .filter(|c| mensagens::nivel(c) == Some(Nivel::Pericial))
            .collect();
        assert_eq!(periciais.len(), 14, "{periciais:?}");
        for c in periciais {
            assert!(da_condicao(c).is_some(), "{c} sem frase de limite");
        }
        assert_eq!(da_condicao("x"), None);
    }

    #[test]
    fn redacao_do_catalogo() {
        let tipos: BTreeSet<_> = EFEITOS.iter().map(|x| x.0).collect();
        assert_eq!(tipos.len(), EFEITOS.len(), "tipo repetido");
        let cods: BTreeSet<_> = CONDICOES.iter().map(|x| x.0).collect();
        assert_eq!(cods.len(), CONDICOES.len(), "condição repetida");
        let chaves: BTreeSet<_> = ROTULOS.iter().map(|x| x.0).collect();
        assert_eq!(chaves.len(), ROTULOS.len(), "rótulo repetido");
        assert_eq!(preencher(modelo::IA_OCULTACOES, &[("n", "3")]), "Ocultações propostas por detecção automática e confirmadas pelo perito: 3.");
        assert_eq!(rotulo("modo.solido"), Some("sólido opaco"));
        assert_eq!(rotulo("modo.x"), None);
        for r in REFERENCIAS.iter().chain(ROTULOS.iter().map(|x| &x.1)) {
            let baixo = r.to_lowercase();
            assert!(!baixo.contains("conforme") && !baixo.contains("certificad"), "{r}");
        }
        // toda referência com versão e data
        assert!(REFERENCIAS.iter().all(|r| r.contains("versão") && r.contains('(')));
        let modelos = [modelo::IA_OCULTACOES, modelo::IA_MARCACOES, modelo::IA_ASSISTIDO, modelo::IA_IMAGENS, modelo::IA_MODELOS, modelo::MEDIDA_REGUA,
                       modelo::MEDIDA_VELOCIDADE, modelo::MEDIDA_VELOCIDADE_SEM_FAIXA, modelo::DERIVADA, modelo::IA_LEGENDAS];
        let frases = EFEITOS.iter().map(|x| x.3).chain(CONDICOES.iter().map(|x| x.1)).chain(modelos);
        for f in frases {
            let baixo = f.to_lowercase();
            // política de citação das normas (§10.3): nunca endosso
            assert!(!baixo.contains("conforme") && !baixo.contains("certificad"), "{f}");
            // para leigo: sem sigla técnica
            for sigla in ["fps", "pts", "hash", "sha", "codec", "ffmpeg", "gpu", "cpu"] {
                assert!(!baixo.split(|c: char| !c.is_alphanumeric()).any(|p| p == sigla), "{sigla} em: {f}");
            }
            assert!(f == "não determinada" || f.ends_with('.'), "{f}");
        }
        for x in EFEITOS {
            assert!(!x.2.is_empty() && !x.2.ends_with('.'), "{}", x.0);
        }
        // velocidade: a frase vale em qualquer taxa. Câmera lenta de gravação com taxa maior que a da saída ainda
        // descarta imagens (0,75× de 60 em 30); acelerar uma de taxa menor pode mostrar todas (2× de 15 em 30)
        let (lenta, rapida) = (do_efeito(tipo::CAMERA_LENTA).unwrap(), do_efeito(tipo::ACELERADO).unwrap());
        assert!(!lenta.contains("retirada") && lenta.contains("Nenhuma imagem foi criada"), "{lenta}");
        assert!(rapida.contains("pode não ser mostrada"), "{rapida}");
    }
}
