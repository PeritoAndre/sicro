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
    /// Título sobreposto (texto, terço inferior, marca): `feito.sobrepostos[]` de tipo `titulo`.
    pub const TEXTO_SOBREPOSTO: &str = "texto_sobreposto";
    // CONTRATO: acréscimo (revisão do fecho da Fase 1, contratos-fase1 (g) R2).
    /// Selo: o aviso que o programa escreve na imagem (`feito.roteiro[].selos`).
    pub const AVISO_NA_IMAGEM: &str = "aviso_na_imagem";
    // CONTRATO: acréscimo (verificação da Fase 1).
    /// Ocultação por mosaico ou desfoque: só o sólido opaco é irreversível (D17); a frase não promete o que não é.
    pub const OCULTAR_PARCIAL: &str = "ocultar_parcial";
    // CONTRATO: acréscimo (P4.R, zoom que segue).
    /// Enquadramento cujo centro acompanha um rastreio (`EfEnquadramento.segue`): a parte mostrada se desloca.
    pub const AMPLIACAO_QUE_SEGUE: &str = "ampliacao_que_segue";
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
    // CONTRATO: acréscimo (verificação da Fase 1; §5.7).
    /// Predefinição WhatsApp: o código do arquivo só confere no envio como documento.
    pub const SO_COMO_DOCUMENTO: &str = "so_como_documento";
    // CONTRATO: acréscimo (fecho da Fase 2).
    /// `ocultar_interpolado` quando alguma ponta da interpolação não é marcação do perito ("Simplificar", denso).
    pub const OCULTAR_INTERPOLADO_CALCULADO: &str = "ocultar_interpolado_calculado";
}

/// Frases com número ou nome no meio: `{n}`, `{item}`… são trocados por quem monta os dados (`juri::dados`).
pub mod modelo {
    pub const IA_OCULTACOES: &str = "Ocultações propostas por detecção automática e confirmadas pelo perito: {n}.";
    pub const IA_MARCACOES: &str = "Marcações que acompanham alvo achado por detecção automática e confirmado pelo perito: {n}.";
    /// Só rastreio com corrida de motor neural registrada (`feito.rastreios[].corridas[].motor`).
    pub const IA_ASSISTIDO: &str = "Alvos acompanhados por programa de inteligência artificial a partir de marcação do perito: {n}.";
    /// Rastreio assistido sem corrida legível: o `feito` não diz se o programa era de inteligência artificial.
    pub const IA_ASSISTIDO_SEM_REGISTRO: &str = "Alvos acompanhados por programa a partir de marcação do perito, sem registro do programa usado: {n}.";
    pub const IA_IMAGENS: &str = "Imagens ampliadas por programa de inteligência artificial: {n}.";
    pub const IA_MODELOS: &str = "Os programas de inteligência artificial usados estão identificados no anexo técnico, item H.";
    pub const MEDIDA_REGUA: &str = "Régua de escala de {m} m (item {item}): erro médio da calibração de {residuo} m.";
    pub const MEDIDA_VELOCIDADE: &str = "Velocidade calculada (item {item}): {kmh} km/h, na faixa de {de} a {ate} km/h.";
    pub const MEDIDA_VELOCIDADE_SEM_FAIXA: &str = "Velocidade calculada (item {item}): aproximadamente {kmh} km/h, sem faixa de erro calculada.";
    pub const DERIVADA: &str = "Este arquivo foi feito pela perícia a partir de outra gravação do caso ({como}).";
    pub const CONFERIDA_POR: &str = "{frase} {por}, em {quando}.";
    /// Anexo E: ausências do alvo aceitas pelo perito, como ele as registrou. Onde o rastreio dá região dentro delas
    /// a ocultação cobre (§6.1): o intervalo não é dito "sem ocultação" (`QUADROS_AUSENTES` conta os quadros sem região).
    pub const AUSENCIAS: &str = "ausências do alvo aceitas pelo perito: {lista}";
    pub const IA_LEGENDAS: &str = "Legendas escritas a partir de degravação automática e conferidas pelo perito.";
    /// Só quando o `feito` conta legenda automática que saiu sem a conferência registrada.
    pub const IA_LEGENDAS_SEM_CONFERENCIA: &str = "Legendas escritas a partir de degravação automática; sem conferência registrada pelo perito: {n}.";
    // Anexos E e H (técnicos): cada pedaço sai de um campo de `feito.rastreios[]`.
    /// Uma corrida do rastreio (`Corrida` do §6.1).
    pub const CORRIDA: &str = "{modelo} · {execucao} · {sentido}, {faixa} · parada: {parou} · limiares {ok} e {perdido}, paciência de {paciencia} quadros · {quando}";
    pub const CONFIRMADO: &str = "confirmado por {por} em {quando}, de {de} a {ate}";
    /// `feito.rastreios[].ancoras`: só as do perito (as do "Simplificar" que ele não mexeu ficam de fora).
    pub const MARCACOES: &str = "marcações do perito: {n}";
    /// `feito.ocultacoes[].quadros`: como cada quadro coberto obteve a região (`avaliar::Como`), uma parte por
    /// contagem, separadas por " · ". Os incertos estão dentro dos medidos e dos interpolados (a frase diz, e não se
    /// somam); os sem região ficam fora dos cobertos (na invertida, cobertos por inteiro).
    pub const QUADROS_OCULTOS: &str = "quadros — {lista}";
    // CONTRATO: fecho da Fase 2 (as partes de `QUADROS_OCULTOS`).
    pub const QUADROS_MARCADOS: &str = "na marcação do perito: {n}";
    /// Âncora do "Simplificar" (`Rastreio.ancoras_do_programa`): escolhida pelo programa, nunca "do perito".
    pub const QUADROS_DO_PROGRAMA: &str = "em ponto escolhido pelo programa entre as amostras: {n}";
    /// `Como::Segurado`: depois da última marcação de rastreio manual.
    pub const QUADROS_SEGURADOS: &str = "com a última marcação do perito mantida: {n}";
    pub const QUADROS_MEDIDOS: &str = "medidos: {n}";
    pub const QUADROS_INTERPOLADOS: &str = "interpolados: {n}";
    pub const QUADROS_INCERTOS: &str = "dos medidos e interpolados, incertos: {n}";
    pub const QUADROS_AUSENTES: &str = "sem região, em ausência aceita pelo perito: {n}";
    // CONTRATO: acréscimo (revisão do fecho da Fase 2).
    /// Invertida: sem o alvo, tudo está fora da forma e o quadro sai todo coberto.
    pub const QUADROS_AUSENTES_INVERTIDA: &str = "quadro inteiro coberto, em ausência aceita pelo perito: {n}";
    /// Rastreio com corrida que não lê como `Corrida` (gravada antes da Fase 2): o anexo só diz quantas.
    pub const CORRIDAS_ILEGIVEIS: &str = "corridas sem registro legível: {n}";
    // CONTRATO: acréscimo (fecho da Fase 2).
    /// Corrida que não deu amostra a nenhum quadro usado (fora do trecho, ou substituída ali por outra): fica só a conta.
    pub const CORRIDAS_FORA: &str = "corridas sem amostra nos trechos usados: {n}";
    // CONTRATO: acréscimos do P4.R (bloco 6.4).
    /// Ocultações de origem automática (`IA_OCULTACOES`): como cada imagem coberta obteve a posição, pelas contagens de
    /// `feito.ocultacoes[].quadros` (achada = linha medida do programa; calculada = interpolada ou estendida).
    pub const IA_OCULTACOES_QUADROS: &str = "Nessas ocultações, imagens com a posição achada pelo programa: {achadas}; calculada a partir das posições vizinhas: {calculadas}.";
    pub const IA_OCULTACOES_QUADROS_PERITO: &str =
        "Nessas ocultações, imagens com a posição achada pelo programa: {achadas}; calculada a partir das posições vizinhas: {calculadas}; marcada pelo perito: {marcadas}.";
    /// Zoom que acompanha alvo de origem automática (contado à parte das marcações).
    pub const IA_AMPLIACOES: &str = "Ampliações que acompanham alvo achado por detecção automática e confirmado pelo perito: {n}.";
    /// Mídia derivada de arquivo com nome conhecido (`Derivacao.de_nome`).
    pub const DERIVADA_DE: &str = "Este arquivo foi feito pela perícia a partir do arquivo «{nome}», também do caso ({como}).";
    /// Anexo C do enquadramento que segue (`EfEnquadramento.segue`).
    pub const ACOMPANHANDO: &str = "acompanhando o rastreio {r} (média de {s} s)";
    pub const ACOMPANHANDO_SEM_MEDIA: &str = "acompanhando o rastreio {r}, sem média";
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
    // a taxa típica é a mediana; a faixa, a dos intervalos do meio (percentis 10 e 90) nos trechos usados
    ("ritmo.variavel", "cerca de {fps} imagens por segundo, em intervalos irregulares"),
    ("ritmo.variavel_faixa", "cerca de {fps} imagens por segundo, em intervalos irregulares (na maior parte, entre {de} e {ate} por segundo)"),
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
    // P4.R: cópia de gravador recodificada sem perda (FFV1: a prova da cópia confere imagem por imagem)
    ("derivacao.sem_perdas_recodificada", "regravada em formato sem perda: cada imagem é igual à do original"),
    // P4.R: relógio do gravador no começo do original (`Derivacao.inicio_epoca_s`), o 0:00,000 dos tempos da cópia (a
    // primeira imagem pode vir depois: no Dahua o som começa 1 s antes)
    ("relogio.inicio", "{quando} no início do arquivo (0:00,000)"),
    // P4.R: fluxo cru sem temporização (`Derivacao.taxa_suposta`)
    ("ritmo.suposto", "o arquivo do gravador não informa; a perícia supôs {fps} imagens por segundo"),
    // P4.R: entrada e saída do texto sobreposto quando não são o padrão ("aparecer" nos dois lados)
    ("surgir.entrada.corte", "aparece de uma vez"),
    ("surgir.entrada.aparecer", "aparece aos poucos em {d} s"),
    ("surgir.entrada.deslizar", "entra deslizando {lado} em {d} s"),
    ("surgir.saida.corte", "some de uma vez"),
    ("surgir.saida.aparecer", "some aos poucos em {d} s"),
    ("surgir.saida.deslizar", "sai deslizando {lado} em {d} s"),
    ("surgir.de.cima", "de cima"),
    ("surgir.de.baixo", "de baixo"),
    ("surgir.de.esq", "da esquerda"),
    ("surgir.de.dir", "da direita"),
    ("surgir.para.cima", "para cima"),
    ("surgir.para.baixo", "para baixo"),
    ("surgir.para.esq", "para a esquerda"),
    ("surgir.para.dir", "para a direita"),
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
    ("onde.de_ate", "de {de} a {ate}"),
    ("legendas.queimar", "na imagem, ao longo do vídeo"),
    ("legendas.arquivo", "em arquivo que acompanha o vídeo"),
    ("legendas.ambos", "na imagem e em arquivo que acompanha o vídeo"),
    ("codificacao.cpu", "esperada com os mesmos programas, na mesma classe de máquina (codificador de CPU)"),
    ("codificacao.gpu", "não esperada (codificador de GPU); a comparação é pelo SHA-256 dos quadros"),
    ("anexo.tecnico", "Relatório técnico completo, legível por programa (JSON)"),
    ("anexo.projeto", "Projeto da edição, para refazer a montagem (JSON)"),
    ("papel.sicro", "edição, cálculo dos códigos de verificação, este relatório"),
    ("papel.ffmpeg", "leitura das gravações e produção do vídeo"),
    ("papel.typst", "textos da imagem e composição deste relatório"),
    ("corte", "Corte no tempo"),
    ("enquadramento.inteiro", "Ajuste de tamanho"),
    // Fase 2: origem da região (anexo E) e do rastreio (anexo H); `Rastreio.nome` nunca sai
    ("rastreio.manual", "marcação do perito"),
    ("rastreio.assistido", "acompanhamento por programa a partir de marcação do perito"),
    ("rastreio.deteccao", "detecção automática ({classe})"),
    ("rastreio.pessoas.auto", "trilha automática da análise de pessoas"),
    ("rastreio.pessoas.revisada", "trilha da análise de pessoas, revisada pelo perito na análise"),
    ("rastreio.ref", "{r} (anexo H)"),
    ("classe.rosto", "rosto"),
    ("classe.placa", "placa"),
    ("classe.cabeca", "cabeça"),
    ("classe.pessoa", "pessoa"),
    ("classe.veiculo", "veículo"),
    ("motor.edgetam", "EdgeTAM"),
    ("motor.vittrack", "VitTrack"),
    ("motor.mancha", "correlação, sem inteligência artificial"),
    ("sentido.frente", "para a frente"),
    ("sentido.tras", "para trás"),
    ("parou.fim", "fim do trecho"),
    ("parou.perdido", "alvo perdido"),
    ("parou.saiu_do_quadro", "alvo saiu da imagem"),
    ("parou.limite", "limite de quadros"),
    ("parou.ancora", "marcação do perito"),
    ("parou.cancelado", "interrompida pelo perito"),
    ("provedor.cpu", "CPU"),
    ("provedor.cuda", "CUDA"),
    ("uso.rastreio", "acompanhamento de alvo"),
    ("uso.legendas", "degravação das legendas"),
    // P4.5: trilha de detecção da ocultação assistida (`ia::uso`)
    ("uso.deteccao", "proposta de ocultação por detecção automática"),
    ("arquivo.modelo", "arquivo do modelo"),
    ("papel.runtime", "execução dos modelos de inteligência artificial"),
    ("onde.ocultacao", "ocultação {n}"),
    ("onde.ocultacoes", "ocultações {n}"),
    // texto das telas de texto, transcrito no item do roteiro (`PapelTexto`)
    ("bloco.titulo", "Título"),
    ("bloco.subtitulo", "Subtítulo"),
    ("bloco.corpo", "Texto"),
    ("bloco.rodape", "Rodapé"),
    // marcações no item do roteiro: "3 setas: PP1 (vermelha), sem rótulo (verde) ×2"
    ("marca.grupo", "{n} {forma}"),
    ("marca.item", "{rotulo} ({cor})"),
    ("marca.sem_rotulo", "sem rótulo"),
    ("marca.repetida", "{marca} ×{n}"),
    ("marca.secao", "Marcação ({formas})"),
    ("lupa.uma", "Lupa (aumento de {f}×)"),
    ("lupa.varias", "{n} lupas (aumento de {f})"),
    ("holofote.varios", "{n} holofotes"),
    // anexo C: valores do modelo
    ("lado.cima", "acima"),
    ("lado.baixo", "abaixo"),
    ("lado.esq", "à esquerda"),
    ("lado.dir", "à direita"),
    ("interpolacao.vizinho", "vizinho mais próximo"),
    ("interpolacao.bilinear", "bilinear"),
    ("interpolacao.bicubica", "bicúbica"),
    ("ajuste.conter", "imagem inteira na tela"),
    ("ajuste.cobrir", "preenche a tela"),
    ("fonte.sans", "sem serifa"),
    ("fonte.serifa", "com serifa"),
    ("fonte.mono", "monoespaçada"),
    ("mostra.tempo_fonte", "tempo da gravação original"),
    ("mostra.relogio", "relógio da câmera"),
    ("mostra.quadro", "número do quadro"),
    ("mostra.tempo_programa", "tempo do vídeo"),
    ("formato_carimbo.hms_ms", "h:mm:ss,mmm"),
    ("formato_carimbo.hms", "h:mm:ss"),
    ("formato_carimbo.data_hora", "data e hora"),
    ("borda.preta", "bordas pretas"),
    ("borda.ampliar", "bordas cortadas por ampliação"),
    ("desentrelace.quadro", "uma imagem por quadro"),
    ("desentrelace.campo", "uma imagem por campo"),
    ("campos.auto", "ordem dos campos automática"),
    ("campos.tff", "campo superior primeiro"),
    ("campos.bff", "campo inferior primeiro"),
    ("calibracao.line", "por linha"),
    ("calibracao.plane", "por plano"),
    ("calibracao.cross_ratio", "por razão cruzada"),
    ("ffmpeg.path", "do sistema (PATH)"),
    ("ffmpeg.embutido", "embutido no SICRO"),
    ("ffmpeg.configurado", "escolhido nas Configurações"),
];

/// Cores das marcações: (masculino, feminino, sRGB de referência). Vale o nome da referência mais próxima no espaço
/// Lab (`legivel::cor`); um nome pode ter mais de uma referência.
pub const CORES: &[(&str, &str, [u8; 3])] = &[
    ("vermelho", "vermelha", [220, 30, 30]),
    ("laranja", "laranja", [245, 130, 0]),
    ("amarelo", "amarela", [250, 215, 50]),
    ("verde", "verde", [40, 170, 70]),
    ("verde", "verde", [0, 230, 0]),
    ("azul-claro", "azul-clara", [110, 180, 240]),
    ("azul-claro", "azul-clara", [0, 210, 230]),
    ("azul", "azul", [30, 90, 200]),
    ("azul", "azul", [0, 0, 255]),
    ("roxo", "roxa", [120, 50, 160]),
    ("rosa", "rosa", [240, 120, 180]),
    ("rosa", "rosa", [230, 30, 190]),
    ("rosa", "rosa", [255, 180, 200]),
    ("marrom", "marrom", [130, 80, 40]),
    ("branco", "branca", [250, 250, 250]),
    ("cinza-claro", "cinza-clara", [195, 195, 195]),
    ("cinza", "cinza", [128, 128, 128]),
    ("preto", "preta", [15, 15, 15]),
];

/// Formas da marcação (`EfDestaque.forma`): (forma, singular, plural, feminino).
pub const FORMAS_DESTAQUE: &[(&str, &str, &str, bool)] = &[
    ("seta", "seta", "setas", true),
    ("circulo", "círculo", "círculos", false),
    ("elipse", "elipse", "elipses", true),
    ("retangulo", "retângulo", "retângulos", false),
    ("colchetes", "par de colchetes", "pares de colchetes", false),
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
    (tipo::OCULTAR_PARCIAL, Classe::Mudanca, "Ocultação", "Parte da imagem foi coberta por mosaico ou desfoque para preservar pessoa ou dado sem relação com o fato. Esse tipo de cobertura dificulta o reconhecimento, mas não garante impedi-lo. A gravação original continua íntegra."),
    ("holofote", Classe::Mudanca, "Holofote", "O entorno foi escurecido para guiar o olhar até uma parte da imagem. A parte realçada não foi alterada."),
    ("destaque", Classe::Acrescimo, "Marcação", "Desenho posto sobre a imagem para guiar o olhar. Não faz parte da gravação."),
    ("lupa", Classe::Acrescimo, "Lupa", "Repete, em tamanho maior, uma parte da própria imagem. Ampliar não cria detalhe que a câmera não registrou."),
    ("regua", Classe::Acrescimo, "Régua de escala", "Desenhada a partir de medidas tomadas pela perícia. Vale só no plano do chão em que foi calibrada."),
    ("velocidade_exibida", Classe::Acrescimo, "Velocidade calculada", "Resultado de cálculo da perícia, mostrado com a faixa de erro ou, quando o cálculo não tem faixa, como valor aproximado. Não é leitura de aparelho."),
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
    (tipo::TEXTO_SOBREPOSTO, Classe::Acrescimo, "Texto sobreposto", "Escrito pelo perito e mostrado sobre a imagem. Não faz parte das gravações."),
    (tipo::AVISO_NA_IMAGEM, Classe::Acrescimo, "Aviso escrito na imagem", "Escrito pelo programa no canto da imagem para avisar que ela é mostrada de outro jeito. Não faz parte das gravações."),
    (tipo::VOLUME, Classe::Mudanca, "Volume alterado", "O som da gravação foi deixado mais alto ou mais baixo. Nada foi acrescentado a ele."),
    (tipo::AMPLIACAO_QUE_SEGUE, Classe::Mudanca, "Ampliação que acompanha o alvo", "Mostra em tamanho maior uma parte da imagem que se desloca junto com um alvo acompanhado imagem a imagem. O deslocamento foi feito pela perícia e não é movimento da câmera. Ampliar não cria detalhe que a câmera não registrou."),
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
    (condicao::OCULTAR_INTERPOLADO_CALCULADO, "Em parte das imagens, a posição da ocultação foi calculada a partir das posições vizinhas, e não marcada nem medida uma a uma."),
    (codigo::OCULTAR_INCERTO, "Em parte das imagens, a posição da ocultação veio de acompanhamento automático que o programa marcou como incerto."),
    (codigo::SILHUETA_EM_OCULTAR, "Há ocultação recortada pelo contorno do corpo. O contorno é calculado por programa e pode deixar bordas à mostra."),
    (codigo::MEDIA_DE_QUADROS, "Há trecho em que cada imagem é a média de várias imagens seguidas. Acontecimentos rápidos podem sumir ou parecer durar mais do que duraram."),
    (codigo::IMAGEM_IA, "Há imagem ampliada por programa de inteligência artificial. Os detalhes que ela mostra podem ter sido criados pelo programa e não servem a exame."),
    (codigo::SINCRONIA_SEM_CONFERENCIA, "As câmeras foram alinhadas pelo som ou pelo relógio, sem conferência por um acontecimento visível nas duas."),
    (codigo::SINCRONIA_EXTRAPOLADA, "O alinhamento entre as câmeras foi usado fora do intervalo em que foi medido."),
    (codigo::RELOGIO_DISTANTE, "O relógio da câmera mostrado na imagem foi acertado em momento distante do trecho exibido e pode ter se desviado."),
    (codigo::TAXA_SUPOSTA, "Há gravação cujo arquivo não informa quantas imagens por segundo foram registradas (seção 3). A perícia leu essa gravação supondo um ritmo; os tempos e as velocidades vistos nela dependem dessa suposição."),
    (condicao::COM_PERDAS, "O vídeo entregue foi gravado em formato com compressão. Para exame técnico valem as gravações originais, e não este vídeo."),
    (condicao::SO_COMO_DOCUMENTO, "Este vídeo foi preparado para envio por aplicativo de mensagens. O código de verificação só confere se ele for enviado como documento. Enviado como vídeo, o aplicativo o comprime de novo e o arquivo recebido passa a ser outro."),
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
        assert_eq!(periciais.len(), 15, "{periciais:?}");
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
        // as da parte para leigo (as do anexo técnico, `CORRIDA` e afins, levam sigla e não terminam em ponto)
        let modelos = [modelo::IA_OCULTACOES, modelo::IA_MARCACOES, modelo::IA_ASSISTIDO, modelo::IA_ASSISTIDO_SEM_REGISTRO, modelo::IA_IMAGENS, modelo::IA_MODELOS,
                       modelo::MEDIDA_REGUA, modelo::MEDIDA_VELOCIDADE, modelo::MEDIDA_VELOCIDADE_SEM_FAIXA, modelo::DERIVADA, modelo::IA_LEGENDAS,
                       modelo::IA_LEGENDAS_SEM_CONFERENCIA, modelo::IA_OCULTACOES_QUADROS, modelo::IA_OCULTACOES_QUADROS_PERITO,
                       modelo::IA_AMPLIACOES, modelo::DERIVADA_DE];
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
        // a velocidade sem faixa sai na imagem como "≈ N km/h" (`medidas::texto_da_velocidade`): a frase não afirma
        // faixa que a imagem não tem
        let vel = do_efeito("velocidade_exibida").unwrap();
        assert!(vel.contains("faixa de erro") && vel.contains("aproximado"), "{vel}");
        // o que o programa escolheu nunca é dito do perito (fecho da Fase 2)
        for f in [modelo::QUADROS_DO_PROGRAMA, modelo::CORRIDAS_FORA, da_condicao(condicao::OCULTAR_INTERPOLADO_CALCULADO).unwrap()] {
            assert!(!f.contains("perito"), "{f}");
        }
        // só o sólido opaco é dado como irreversível (D17)
        assert!(do_efeito("ocultar").unwrap().contains("não é possível desfazer"));
        assert!(!do_efeito(tipo::OCULTAR_PARCIAL).unwrap().contains("desfazer"));
    }
}
