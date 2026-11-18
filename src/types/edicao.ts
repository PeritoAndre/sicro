/**
 * Edição profissional: modelo v2 e contratos da Fase 0.
 * A primeira parte é o §2.1 da arquitetura, literal. Depois vêm os tipos que cruzam com o Rust:
 * resultado de `edicao_analisar`, entradas e saídas dos comandos `edicao_*`, eventos e os contratos do palco (§8.3).
 * Onde este arquivo e o Rust divergirem, vale este (é o que o front grava): ver contratos-fase0.md no dossiê.
 */

/** Espelho de src-tauri/src/edicao/modelo.rs (nomes = fio do serde, snake_case). */
export const ESQUEMA_EDICAO = 2 as const;
// CONTRATO: 1 desde a abertura da Fase 4 (contratos-fase4.md B1): campos com efeito no render (sombra, faixa,
// animação do sobreposto, enquadramento que segue). Só o documento que USA um deles grava 1 (armazem::salvar).
export const ESQUEMA_MENOR = 1;          // sobe a cada campo novo com efeito no render; arquivo com valor maior abre só para leitura

export type Id = string;                 // UUID v4
export type Seg = number;                // segundos (f64 inteiro, sem arredondar)
export type Pts = number;                // inteiro, na base de tempo da mídia (TabelaQuadros do decodificador)
export type Cor = string;                // "#rrggbb" ou "#rrggbbaa"
export interface Racional { num: number; den: number }
export interface Ponto { x: number; y: number }                          // 0..1
export interface Caixa { x: number; y: number; w: number; h: number }    // 0..1; (x, y) = canto superior esquerdo; ponto: w = h = 0
export type Interp = "linear" | "suave" | "degrau";
/** Quadro-chave; `interp` vale desta chave até a próxima (padrão linear). `pts` só em chave presa a quadro de mídia (âncora). */
export interface Chave<T> { t: Seg; pts?: Pts | null; v: T; interp?: Interp }
/** Valor fixo ou animado. Chaves em ordem de t; fora delas vale a mais próxima (rastreio tem regra própria, §6.1). */
export type Animavel<T> = T | Chave<T>[];
/** Intervalo na base de tempo do clipe (§3.1); campo ausente = até a borda do clipe. */
export interface Intervalo { de?: Seg | null; ate?: Seg | null }
/** Item de versão futura: preservado e mostrado apagado (guarda `conhecido()` em modelo/projeto.ts). */
export type Desconhecido = { tipo: string; id?: Id; [k: string]: unknown };

export interface ProjetoEdicao {
  esquema: 2; esquema_menor: number;
  id: Id; titulo: string;
  created_at: string; updated_at: string;          // RFC 3339
  gravado_por: string;                             // versão do SICRO que salvou
  saida: Saida;
  midias: MidiaRef[];
  principal: (ClipePrincipal | Desconhecido)[];    // trilha magnética: ordem = ordem no vídeo
  fecho: Transicao | null;                         // saída do último clipe para o fundo (dentro da duração dele)
  sobreposicoes: TrilhaSobreposicao[];             // de baixo para cima
  audios: TrilhaAudio[];
  legendas: TrilhaLegendas;
  rastreios: Rastreio[];
  marcadores: Marcador[];
  sincronias: SincroniaRef[];
  mixagem: Mixagem;
}

// ---- saída ----
export interface Saida {
  largura: number; altura: number;       // pares, 64..7680 × 64..4320 (H.264: lado maior até 4096)
  fps: Racional;                         // 1..120 (30000/1001 aceito)
  auto: boolean;                         // true = tamanho e taxa seguem as fontes (§8.1) até o perito mexer em "Saída"
  cor_fundo: Cor;                        // barras e fundo
  audio: boolean;
  selos: { velocidade: boolean; ampliacao: boolean; realce: boolean };   // nascem ligados; média de quadros e imagem por IA têm selo fixo
  exportacao: Exportacao;                // última escolha do diálogo Exportar
}
export interface Exportacao {
  predefinicao: "juri" | "whatsapp" | "preservacao" | "personalizada";
  codificador: "auto" | "cpu";           // cpu = "Arquivo reprodutível" (§5.7)
  legendas: "nenhuma" | "queimar" | "arquivo" | "ambos";
  pacote: null | { originais: boolean }; // pacote do júri; originais só para o requisitante (18-M §12.2)
  pdf: "a3" | "a2_sem_anexos";           // PDF/A-3a com anexos, ou PDF/A-2b simples para protocolo que recusa anexo
  revisado_por: string;                  // revisão técnica (18-V §4.4.3); vazio = não houve
  personalizada: null | { codec: "h264" | "hevc" | "av1" | "ffv1"; conteiner: "mp4" | "mkv" | "mov";
                          qualidade: number; altura_max: number | null; audio_kbps: number };
}

// ---- mídias usadas (retrato: valida sem banco, detecta troca, carrega a proveniência) ----
export interface MidiaRef {
  id: Id;                                // local ao projeto; os clipes apontam para cá
  tipo: "video" | "audio" | "imagem";
  origem: "caso" | "projeto";            // registro do caso ou arquivo da pasta do projeto
  media_id: string | null;               // video_media.id / audio_media.id na hora em que entrou
  sha256: string;                        // chave real
  nome: string; relativo: string;        // caminho relativo ao caso
  duracao_s: number | null; inicio_s: number | null;   // start_time do contêiner (pode ser época, em gravador)
  largura: number | null; altura: number | null;       // tamanho exibido (giro e proporção do pixel aplicados)
  taxa: Racional | null;                 // nominal, tirada da tabela de pts (nunca avg_frame_rate)
  vfr: boolean; tem_audio: boolean;
  cor: null | { matriz: "bt601" | "bt709"; faixa: "tv" | "pc" };   // null = automático (§3.3); preenchido quando o perito troca
  derivacao: null | Derivacao;           // retrato de derived_from_hash + derivation_json, até a raiz
  recebido: { nativo: boolean | null; obs: string };   // veio no formato do gravador? (18-V §4.3.2)
  relogio_desvio_s: number | null;       // desvio do relógio do gravador informado pelo perito (OSAC §6.1.7)
  gravador: string | null;               // identifica o equipamento (método "relógio" só entre mídias do mesmo gravador, §9)
  decodificacao: null | { erros: number };             // passada de integridade (§5.2); null = ainda não feita
  ocultacoes: EfOcultar[];               // ocultações da MÍDIA: valem em todo uso dela; `ativo` em tempo de fonte, obrigatório
}
export interface Derivacao { tipo: "trecho" | "estabilizado" | "lente" | "copia_dvr" | "superres" | "outro";
                             de_sha256: string; raiz_sha256: string; comando: string; com_perdas: boolean;
                             // CONTRATO: acréscimo (P4.6). start_time do original quando é data do relógio do gravador
                             // (s desde 1970); a cópia de trabalho começa em zero. Ausente = não há.
                             inicio_epoca_s?: number;
                             // CONTRATO: acréscimo (P4.6b). Fluxo cru sem temporização no SPS: a taxa que a cópia supôs
                             // (a padrão do demuxer, 25). Ausente = não supôs.
                             taxa_suposta?: Racional;
                             // CONTRATO: acréscimo (P4.R). Cópia de gravador: nome do arquivo de origem e versão do
                             // ffmpeg que fez a cópia (o PDF cita os dois). Ausente = não é cópia de gravador.
                             de_nome?: string;
                             ffmpeg_versao?: string }

// ---- posição de clipe livre ----
export type Posicao =
  | { ref: "programa"; t: Seg }
  | { ref: "clipe"; clipe: Id; desloc: Seg };          // preso ao início de um clipe da principal

// ---- transição (catálogo em motor/juncao.rs; id desconhecido = erro de validação, preservado) ----
export interface Transicao {
  tipo: string;        // "escurecer" (sai e entra pelo fundo, sem sobrepor) | "fundir" | "cortina_esq" | "cortina_dir" | "deslizar_esq" | "deslizar_dir" | "circulo" (xfade)
  duracao_s: Seg;
  audio: "acompanhar" | "cortar";        // cortar = som de quem sai até o meio da sobreposição, de quem entra depois (afade de 10 ms)
}

// ---- velocidade ----
export type Velocidade =
  | { tipo: "constante"; fator: number }                       // 0,05..16, decimal com até 3 casas (vira racional exato)
  | { tipo: "rampa"; pontos: Chave<number>[] };                // Fase 3; t em tempo de fonte
export interface AudioClipe {
  volume: Animavel<number>;              // 0..2
  mudo: boolean;
  manter_tom: boolean;                   // true = atempo (pode adiantar o som até 26 ms a 0,5× e 74 ms a 0,25×, §5.9); false = exato
  silencios: Silencio[];
}
export interface Silencio { id: Id; de: Seg; ate: Seg; modo: "tom" | "mudo"; motivo: string }   // tempo de fonte

// ---- clipes da principal ----
interface ClipeBase {
  id: Id; rotulo: string;                // "Nome" na tela; é a descrição do item no PDF
  desligado: boolean;
  transicao_entrada: Transicao | null;   // no primeiro clipe: a partir do fundo, dentro da duração dele
  efeitos: (Efeito | Desconhecido)[];    // ordem dentro de cada família = ordem de aplicação
}
export interface ClipeTrecho extends ClipeBase {
  tipo: "trecho"; midia: Id;
  inicio_s: Seg; fim_s: Seg;             // [quadro exibido em inicio_s, quadro exibido em fim_s)
  inicio_pts: Pts | null; fim_pts: Pts | null;   // identidade dos dois quadros (fim_pts null = fim da mídia); preenchidos pelo Rust
  velocidade: Velocidade; audio: AudioClipe;
}
export interface ClipeCongelar extends ClipeBase { tipo: "congelar"; midia: Id; instante_s: Seg; instante_pts: Pts | null; duracao_s: Seg }
export interface ClipeImagem extends ClipeBase { tipo: "imagem"; midia: Id; duracao_s: Seg }
export interface ClipeTitulo extends ClipeBase { tipo: "titulo"; duracao_s: Seg; titulo: Titulo }
/** Fase 3: tela dividida e picture-in-picture como item. O tempo e o áudio vêm da célula `referencia`. */
export interface ClipeTela extends ClipeBase {
  tipo: "tela"; arranjo: "lado_a_lado" | "empilhado" | "grade" | "pip";
  celulas: Celula[]; referencia: number;
  inicio_s: Seg; fim_s: Seg; inicio_pts: Pts | null; fim_pts: Pts | null;
  velocidade: Velocidade; audio: AudioClipe;
  pip: Caixa | null;                     // onde fica a célula 1 no arranjo "pip" (0..1 do canvas)
}
export interface Celula {
  midia: Id; sincronia: Id | null;       // vínculo usado (null na referência)
  rotulo: string; efeitos: (Efeito | Desconhecido)[];   // no espaço da própria célula
}
export type ClipePrincipal = ClipeTrecho | ClipeCongelar | ClipeImagem | ClipeTitulo | ClipeTela;

// ---- efeitos ----
interface EfBase { id: Id; ligado: boolean; nome: string; ativo: Intervalo | null }
export interface Traco { cor: Cor; espessura: number; contorno: boolean }   // espessura em fração da altura do canvas (v1 = 0,006)
export interface EstiloTexto {
  fonte: "sans" | "serifa" | "mono"; negrito: boolean; italico: boolean;
  tamanho: number;                       // fração da altura do canvas (v1: p 0,033 · m 0,052 · g 0,078)
  cor: Cor; alinhamento: "esq" | "centro" | "dir";
  // CONTRATO: `faixa` (Fase 4): a caixa ocupa a largura inteira do bloco (`largura_max`), não só a das linhas.
  caixa: null | { cor: Cor; margem: number; raio: number; faixa?: boolean };
  contorno: null | { cor: Cor; largura: number };
  sombra?: SombraTexto;                  // CONTRATO: Fase 4; ausente = sem sombra
}
/** Ponto do canvas (0..1) e que ponto da caixa do texto cai nele. */
export interface Lugar { x: number; y: number; h: "esq" | "centro" | "dir"; v: "topo" | "meio" | "base" }
export interface AlvoRegiao {
  rastreio: Id; forma: "retangulo" | "elipse" | "poligono" | "silhueta";
  vertices: Ponto[] | null;              // polígono: 0..1 dentro da caixa
  margem: number;                        // fração do lado; em `ocultar` nunca negativa
  borda_suave: number;                   // 0..0,5 do lado menor; cresce PARA FORA (a caixa mais a margem fica toda coberta)
  inverter: boolean;                     // tudo menos a região
}
export type Interpolacao = "vizinho" | "bilinear" | "bicubica";   // padrão bilinear (não cria valores fora do original); vai para o relatório
// origem: geometria e tempo (formam o "espaço de origem" do clipe, §3.2)
export interface EfEstabilizar extends EfBase {
  tipo: "estabilizar"; suavizar: number; borda: "preta" | "ampliar";      // preta = padrão; ampliar = zoom fixo
  analise: { de_s: Seg; ate_s: Seg; de_pts: Pts; ate_pts: Pts };          // faixa do .trf; não segue a borda do clipe ("Refazer estabilização")
  zoom_pct: number;                      // calculado uma vez quando borda = "ampliar"; o motor usa optzoom=0:zoom=<zoom_pct>
  trf_sha256: string | null;             // vidstab/<sha256>.trf na pasta do projeto; null = ainda não analisado
}
export interface EfLente extends EfBase { tipo: "lente"; k1: number; k2: number; cx: number; cy: number; perfil: string }
export interface EfDesentrelacar extends EfBase { tipo: "desentrelacar"; modo: "quadro" | "campo"; ordem: "auto" | "tff" | "bff" }
export interface EfMediaTemporal extends EfBase { tipo: "media_temporal"; quadros: number }   // 2..15, câmera fixa; selo fixo e aviso com ciência
// origem: tom (não mudam o espaço)
export interface EfAjustes extends EfBase { tipo: "ajustes"; brilho: number; contraste: number; gama: number; saturacao: number }
export interface EfNitidez extends EfBase { tipo: "nitidez"; quantidade: number }
// enquadre (fazem parte de A(τ), §3.2)
export interface EfGirar extends EfBase { tipo: "girar"; graus: 0 | 90 | 180 | 270; espelhar_h: boolean; espelhar_v: boolean }
export interface EfEnquadramento extends EfBase { tipo: "enquadramento"; regiao: Animavel<Caixa>; ajuste: "conter" | "cobrir";
                                                  interpolacao: Interpolacao;    // "vizinho" só com região fixa
                                                  segue?: SegueRastreio }        // CONTRATO: Fase 4; ausente = a região é a gravada
// regiões
export interface EfOcultar extends EfBase {
  tipo: "ocultar"; modo: "desfoque" | "mosaico" | "solido"; alvo: AlvoRegiao;
  intensidade: number;                   // 0..1 (desfoque: raio = intensidade × 0,25 × lado menor; mosaico: round(12 − 11 × intensidade) células no lado menor)
  cor: Cor;                              // modo sólido; alfa tem de ser ff
  conteudo: "rosto" | "placa" | "corpo" | "tela" | "documento" | "outro"; motivo: string;   // vão para o relatório
}
export interface EfHolofote extends EfBase { tipo: "holofote"; alvo: AlvoRegiao; escurecer: number; dessaturar: number }
// anotações
export interface EfDestaque extends EfBase {
  tipo: "destaque"; forma: "circulo" | "elipse" | "retangulo" | "seta" | "colchetes";
  rastreio: Id;                          // caixa ou ponto
  traco: Traco;
  tamanho: number;                       // círculo e seta sobre ponto: lado em fração da altura do canvas (v1: 0,12 e 0,14)
  angulo_graus: number;                  // seta: de onde vem a cauda (v1 = 45)
  margem: number;                        // formas sobre caixa
  rotulo: null | { texto: string; estilo: EstiloTexto; lado: "cima" | "baixo" | "esq" | "dir" };
}
export interface EfLupa extends EfBase {
  tipo: "lupa"; rastreio: Id;            // centro da janela de origem
  janela: number;                        // lado da janela, fração da altura da fonte
  fator: number;                         // 1,5..8
  forma: "circulo" | "retangulo"; interpolacao: Interpolacao;
  onde: { modo: "fixa"; x: number; y: number } | { modo: "segue"; dx: number; dy: number };   // centro da lupa no canvas
  moldura: Traco; ligacao: boolean;
}
export interface EfRegua extends EfBase {
  tipo: "regua";
  calibracao: { id: string; metodo: "line" | "plane" | "cross_ratio"; h: number[]; espaco: { w: number; h: number };
                pontos_sha256: string; residuo_m: number; regiao: Ponto[]; midia_sha256?: string };   // retrato: homografia, resíduo e polígono dos pontos de controle
  ancora: Ponto; direcao_graus: number;  // no plano do chão
  comprimento_m: number; divisoes: number; traco: Traco; rotulo: boolean;
}
export interface EfVelocidadeExibida extends EfBase {
  tipo: "velocidade_exibida";
  calculo: { id: string; kmh: number; faixa: [number, number] | null; de_s: Seg; ate_s: Seg; metodo: string; midia_sha256?: string };   // retrato (CONTRATO: midia_sha256, fecho da Fase 1)
  onde: { modo: "fixa"; lugar: Lugar } | { modo: "segue"; rastreio: Id; dx: number; dy: number };
  estilo: EstiloTexto;                   // texto automático: "42 km/h (39 a 45)" com faixa, "≈ 42 km/h" sem; `ativo` dentro de [de_s, ate_s]
}
export interface EfTexto extends EfBase { tipo: "texto"; conteudo: string; estilo: EstiloTexto; lugar: Lugar; largura_max: number }
export interface EfCarimbo extends EfBase {
  tipo: "carimbo"; mostra: "tempo_fonte" | "relogio" | "quadro" | "tempo_programa";   // prefixo fixo na imagem: "Original", "Câmera", "Quadro", "Vídeo"
  formato: "hms_ms" | "hms" | "data_hora";
  relogio: null | { media_time_s: number; clock_seconds: number; clock_date: string | null };   // retrato do vínculo de relógio (um ponto)
  estilo: EstiloTexto; lugar: Lugar;
}
export interface EfImagem extends EfBase { tipo: "imagem"; midia: Id; lugar: Lugar; largura: number; opacidade: number }
export type Efeito =
  | EfEstabilizar | EfLente | EfDesentrelacar | EfMediaTemporal | EfAjustes | EfNitidez | EfGirar | EfEnquadramento
  | EfOcultar | EfHolofote
  | EfDestaque | EfLupa | EfRegua | EfVelocidadeExibida | EfTexto | EfCarimbo | EfImagem;

// ---- títulos (cartela, abertura, terço inferior, encerramento) ----
export interface Titulo {
  modelo: "cartela" | "abertura" | "evento" | "terco_inferior" | "encerramento" | "livre";   // só rótulo: o conteúdo abaixo é explícito
  fundo: { tipo: "cor"; cor: Cor } | { tipo: "transparente" } | { tipo: "imagem"; midia: Id };
  blocos: BlocoTexto[];
}
export interface BlocoTexto { id: Id; papel: "titulo" | "subtitulo" | "corpo" | "rodape"; conteudo: string;
                              estilo: EstiloTexto; lugar: Lugar; largura_max: number }

// ---- sobreposições (clipes livres sobre a principal) ----
export interface TrilhaSobreposicao { id: Id; nome: string; oculta: boolean; clipes: (ClipeSobreposto | Desconhecido)[] }
export interface Transformacao {          // tempo local do clipe
  x: Animavel<number>; y: Animavel<number>;          // centro no canvas, 0..1
  escala: Animavel<number>;                           // 1 = largura do canvas
  rotacao: Animavel<number>; opacidade: Animavel<number>;
}
interface SobBase { id: Id; rotulo: string; pos: Posicao;
                    duracao_s: Seg | null;            // null = até o fim do clipe de âncora (ou do programa): marca fixa, terço inferior do clipe
                    transformacao: Transformacao; entrada_s: Seg; saida_s: Seg; efeitos: (Efeito | Desconhecido)[];
                    animacao?: AnimacaoSob }          // CONTRATO: Fase 4; ausente = aparecer na entrada e na saída
export interface SobVideo extends SobBase { tipo: "video"; midia: Id; inicio_s: Seg; inicio_pts: Pts | null; velocidade: Velocidade;
                                            audio: AudioClipe | null; moldura: Traco | null; sincronia: Id | null }   // Fase 3
export interface SobImagem extends SobBase { tipo: "imagem"; midia: Id }
export interface SobTitulo extends SobBase { tipo: "titulo"; titulo: Titulo }
export type ClipeSobreposto = SobVideo | SobImagem | SobTitulo;

// ---- áudio ----
export interface TrilhaAudio { id: Id; nome: string; papel: "narracao" | "musica" | "efeitos";
                               volume: number; mudo: boolean; abaixa_original: boolean; clipes: ClipeAudio[] }
export interface ClipeAudio { id: Id; nome: string; midia: Id; pos: Posicao;
                              de_s: Seg; ate_s: Seg | null;          // recorte dentro do arquivo
                              volume: Animavel<number>;              // tempo local
                              entrada_s: Seg; saida_s: Seg }
export interface Mixagem {
  volume_original: number;               // 0..2
  abaixar: { db: number; ataque_s: Seg; soltura_s: Seg };            // original sob as trilhas com abaixa_original
  normalizar: null | { lufs: number; pico_db: number };              // EBU R128, medição + ganho linear
  limitador: boolean;
}

// ---- legendas ----
export interface TrilhaLegendas {
  estilo: EstiloTexto; lugar: Lugar; linhas_max: number;
  origem: null | { audio_sha256: string; importado_em: string; motor: string | null; modelo_sha256: string | null };   // degravação de onde vieram
  itens: Legenda[];
}
export interface Legenda {
  id: Id; texto: string; locutor: string | null;
  origem: "perito" | "ia";               // "ia" = trecho de transcrição automática ainda em rascunho
  conferida: boolean; locutor_auto: boolean;           // legenda "ia" não conferida barra a exportação
  ref: { tipo: "fonte"; midia: Id; de: Seg; ate: Seg }               // acompanha o trecho onde esse tempo aparece
     | { tipo: "programa"; de: Seg; ate: Seg };
}

// ---- rastreios (§6.1) ----
export interface Rastreio {
  id: Id; nome: string; cor: Cor;        // o nome não sai no relatório
  midia: Id | null;                      // null = coordenadas do canvas (clipe sem mídia)
  base: "fonte" | "local";               // fonte: âncoras em tempo de fonte (em `congelar` é amostrado em instante_s); local: desenho sobre imagem parada ou título
  tipo: "caixa" | "ponto";
  espaco: string;                        // "original" ou "origem:<hash>" dos efeitos geométricos sob os quais foi marcado (§3.2)
  ancoras: Chave<Caixa>[];               // marcadas pelo perito
  denso: null | { sha256: string; n: number; de_s: Seg; ate_s: Seg;  // rastreios/<id>/<sha256>.json, imutável
                  ok: number; incertos: number; ausentes: number };
  silhuetas: null | { sha256: string; n: number; modelo: string };   // silhuetas/<id>/<sha256>.sil (Fase 2 guarda, Fase 5 usa)
  ausencias: { de_s: Seg; ate_s: Seg; motivo: "oculto" | "fora_de_cena" | "perdido"; aceita_em: string; por: string }[];   // lacunas aceitas pelo perito
  origem: { tipo: "manual" } | { tipo: "assistido" }
        | { tipo: "pessoas"; pessoa_id: string; estado: "auto" | "revisada"; copiado_em: string }
        | { tipo: "deteccao"; classe: "rosto" | "placa" | "cabeca" | "pessoa" | "veiculo" };
  confirmacao: null | { por: string; em: string; denso_sha256: string | null; de_s: Seg; ate_s: Seg };   // vale só para esse conteúdo e intervalo
  ancoras_do_programa?: Seg[];           // t das âncoras do "Simplificar" que o perito não mexeu
  // CONTRATO: acréscimo (17/11). Corridas dos densos que o "Simplificar" trocou por essas âncoras (null = sem registro,
  // como em ArquivoDenso.corridas): o registro do programa usado fica no projeto.
  corridas_do_programa?: Corrida[];
}

// ---- marcadores e sincronias ----
export interface Marcador { id: Id; rotulo: string; cor: Cor;
                            pos: Posicao | { ref: "fonte"; midia: Id; t: Seg } }
/** Retrato de um vínculo de `video_sync_links` (§9). t_outra = t_ref + desloc_s + deriva_ppm·1e-6·(t_ref − t0_s). */
export interface SincroniaRef {
  id: Id; vinculo_id: string | null; referencia: Id; outra: Id;
  desloc_s: number; deriva_ppm: number; deriva_incerteza_ppm: number; t0_s: number;
  valido_de_s: number; valido_ate_s: number;           // intervalo medido (tempo da referência); fora dele = extrapolação
  metodo: "audio" | "evento" | "relogio" | "manual";
  incerteza_s: number | null;            // null = não determinada (som ou relógio sem conferência visual)
  confianca: number | null;
  conferencias: Conferencia[];
}
export type Conferencia = { modo: "evento_visivel"; t_ref_s: number; diferenca_quadros: number; por: string; em: string }
                        | { modo: "sem_evento"; por: string; em: string };

// =====================================================================================================
// Contratos da Fase 0 (além do §2.1). Espelhos de src-tauri/src/edicao/**; nomes = fio do serde.
// =====================================================================================================

// ---- resultado de edicao_analisar (modelo.rs, só saída) ----
export type Nivel = "erro" | "aviso" | "pericial";
/** `mensagem` vem do catálogo fixo do Rust; `n` = contagem quando o código tem uma.
 *  clipe = id do item da linha do tempo (clipe da principal, livre ou de áudio, legenda, marcador); trilha = a dele, quando há. */
export interface Problema {
  nivel: Nivel; codigo: string; mensagem: string;
  clipe: Id | null; efeito: Id | null; trilha: Id | null; midia: Id | null; campo: string | null; n: number | null;
}
export type OrigemTabela = "pacotes" | "decodificador";
export type Busca = "confiavel" | "sem_busca";
/** Quadros j_in..j_out da tabela; pts_out null = fim da mídia. */
export interface FaixaFonte { midia: Id; j_in: number; j_out: number; pts_in: Pts; pts_out: Pts | null;
                              lidos: number; repetidos: number; descartados: number; tabela: OrigemTabela }
/** Em slots; sobrepoe_q = transição de entrada. */
export interface ClipeResolvido { id: Id; inicio_q: number; n_q: number; sobrepoe_q: number; fonte: FaixaFonte | null }
export interface LivreResolvido { id: Id; trilha: Id; inicio_q: number; n_q: number }
export interface LinhaResolvida { fps: Racional; total_q: number; clipes: ClipeResolvido[]; livres: LivreResolvido[] }
/** id = MidiaRef.id; tabela, busca e erros null enquanto não há tabela. */
export interface EstadoMidia { id: Id; estado: "ok" | "ausente" | "trocada"; tabela: OrigemTabela | null; busca: Busca | null; erros: number | null }
/** Campo do documento que guarda um tempo preso a quadro. */
export type OndePrender =
  | { tipo: "inicio"; clipe: Id }                       // inicio_s / inicio_pts de trecho, tela ou vídeo sobreposto
  | { tipo: "fim"; clipe: Id }                          // fim_s / fim_pts de trecho ou tela
  | { tipo: "instante"; clipe: Id }                     // quadro congelado
  | { tipo: "ancora"; rastreio: Id; indice: number }
  | { tipo: "analise_de"; clipe: Id; efeito: Id }       // analise.de_s / de_pts de um estabilizar
  | { tipo: "analise_ate"; clipe: Id; efeito: Id };
/** O front grava cada um por alterar(). pts null só em "fim" no fim da mídia. */
export interface Prender { onde: OndePrender; pts: Pts | null; t: Seg }
export interface Analise {
  problemas: Problema[]; linha: LinhaResolvida; midias: EstadoMidia[]; prender: Prender[];
  saida_auto: [number, number, Racional] | null;       // largura, altura, fps enquanto saida.auto
}

// ---- projeto: listar, abrir, salvar, usos (armazem.rs) ----
/** duracao_s do programa; miniatura_rel relativo ao caso; problema != null = arquivo ilegível. */
export interface ResumoProjeto { id: Id; titulo: string; updated_at: string; esquema: number; duracao_s: number; n_clipes: number;
                                 miniatura_rel: string | null; problema: string | null }
/** projeto = o JSON como está no disco (pode trazer item de versão futura). */
export interface ProjetoAberto { projeto: ProjetoEdicao; importado_de: number | null; so_leitura: boolean; avisos: Problema[]; raiz_cache: string }
// CONTRATO: `raiz_cache` (contratos (g) 11, feito na abertura da Fase 1): a mesma de `ProjetoAberto.raiz_cache`.
export interface Salvo { updated_at: string; raiz_cache: string }
/** saidas = exportações registradas desse projeto que usaram a mídia. */
export interface UsoMidia { projeto_id: Id; titulo: string; n_clipes: number; saidas: number }

// ---- quadros (quadros.rs) ----
/** Tabela compacta: pts do quadro i = pts0 + soma de passos[0..i), em ticks de tb; passo_mediano = duração do último quadro
 *  (ticks), calculada no Rust; chaves = índices dos quadros-chave. */
export interface TabelaQuadros { tb: Racional; pts0: Pts; passos: number[]; passo_mediano: number; chaves: number[]; vfr: boolean; origem: OrigemTabela }
export type AlvoQuadro = { t: Seg } | { indice: number } | { pts: Pts };
/** pts_s = tempo de fonte do quadro; dur_s = duração dele. */
export interface QuadroEm { indice: number; pts: Pts; pts_s: Seg; dur_s: Seg }

// ---- prévia e mídia (previa.rs; §7.1, §8.3) ----
/** relativo: ao caso em "original"; nos demais, ao cache da Edição do caso. desloc_s = pts0 da fonte − pts0 do arquivo tocado. */
export interface PreviaMidia { relativo: string; tipo: "original" | "copia" | "proxy" | "origem";
                               desloc_s: Seg; de_s: Seg; ate_s: Seg;      // faixa em tempo de fonte
                               largura: number; altura: number; pronta: boolean }
/** id = video_media.id do caso. */
export interface MidiaInfo { id: Id; sha256: string; nome: string; exibido: { w: number; h: number }; taxa: Racional; vfr: boolean; passo: { min: Seg; max: Seg };
                             de_s: Seg; ate_s: Seg; n: number; tem_audio: boolean; previa: PreviaMidia; tabela: OrigemTabela; erros: number | null;
                             // CONTRATO: contratos (h) 17. Falso enquanto a tabela é a de pacotes em contêiner sem tempo confiável
                             // nos pacotes (h264, hevc, mpeg, mpegvideo): única regra para marcar entrada, saída e quadro-chave.
                             marcavel: boolean;
                             // CONTRATO: abertura da Fase 1 (O4/F5). Cor da FONTE (etiqueta ou presunção do §3.3), sem a escolha do projeto.
                             cor: { matriz: "bt601" | "bt709"; faixa: "tv" | "pc" } }

// ---- texto (camada/texto.rs) ----
export interface PedidoSprite { blocos: BlocoTexto[]; largura_max: number; altura_canvas: number; fundo: boolean }
/** relativo ao cache da Edição do caso; w e h em px. */
export interface SpriteTexto { relativo: string; w: number; h: number }
/** glifos na ordem das células do atlas (em linha). */
export interface AtlasCarimbo { relativo: string; celula: { w: number; h: number }; glifos: string }

// ---- capacidades (ffmpeg/sonda.rs) ----
export interface GpuAprovada { nome: string; driver: string; codificador: string; degrau: string; args: string[]; nvdec: boolean; scale_cuda: boolean }
export interface CodificadorReprovado {
  codificador: string;
  classe: "sem_gpu" | "sem_driver" | "driver_antigo" | "limite_de_sessoes" | "recurso_nao_suportado" | "ffmpeg_sem_codificador" | "saida_invalida" | "outra";
  dica: string; driver_minimo: string | null; api_exigida: string | null; api_do_driver: string | null;
}
/** codificador = o que a exportação "auto" usaria agora; sondando = sonda de GPU em andamento. */
export interface ResumoCapacidades {
  versao: string; origem: "configurado" | "embutido" | "path"; sha256: string;
  faltam: string[]; opcionais_ausentes: string[];
  codificador: string; gpu: GpuAprovada | null; reprovados: CodificadorReprovado[]; sondando: boolean;
}

// ---- render: alvo, fila e eventos (motor/mod.rs, fila.rs, fundo.rs) ----
export type AlvoRender =
  | { tipo: "final"; exportacao: Exportacao }
  | { tipo: "previa"; altura: number; de_q: number; ate_q: number }     // slots [de_q, ate_q)
  | { tipo: "quadro"; q: number; largura: number };
/** BMP nomeado pelo hash do pedido, relativo ao cache da Edição do caso. */
export interface QuadroFinal { relativo: string; q: number }
export type Fase =
  | { tipo: "previas" } | { tipo: "camadas" } | { tipo: "blocos"; feitos: number; total: number } | { tipo: "audio" }
  | { tipo: "juncao" } | { tipo: "conferindo" } | { tipo: "relatorio" } | { tipo: "pacote" };
export type EstadoTarefa =
  | { tipo: "na_fila" } | { tipo: "preparando" }
  | { tipo: "rodando"; fase: Fase; feito_q: number; total_q: number; qps: number; restante_s: number }
  | { tipo: "a_conferir"; saida_id: Id } | { tipo: "concluida"; saida_id: Id }
  | { tipo: "pronta"; relativo: string }                                // previa e quadro_final: arquivo no cache da Edição do caso
  | { tipo: "falhou"; mensagem: string; log_rel: string } | { tipo: "cancelada" };
/** caso = id do caso. */
export interface Tarefa { id: Id; tipo: "exportar" | "previa" | "quadro_final" | "refazer"; caso: string; projeto_id: Id; titulo: string;
                          estado: EstadoTarefa; criado_em: string;
                          // CONTRATO: P4.4. Predefinição da exportação (null em prévia e quadro final); reaproveitada = todos os
                          // blocos de vídeo vieram do cache.
                          predefinicao?: Exportacao["predefinicao"] | null; reaproveitada?: boolean }
/** Carga do evento `edicao:progresso` (até 5 por segundo). */
export interface Progresso { tarefa_id: Id; fase: Fase; feito_q: number; total_q: number; qps: number; restante_s: number }
export type EstadoFundo = { tipo: "na_fila" } | { tipo: "rodando" } | { tipo: "pronta" } | { tipo: "falhou"; mensagem: string } | { tipo: "cancelada" };
/** chave = "<tipo>:<sha256 da mídia ou hash do pedido>"; feito em 0..1. */
export interface TarefaFundo { id: Id; tipo: "tabela" | "copia_previa" | "proxy" | "origem" | "onda" | "tira" | "sonda"; chave: string; feito: number; estado: EstadoFundo }
/** Eventos do backend: `edicao:fila` leva Tarefa[]; `edicao:progresso`, Progresso; `edicao:fundo`, TarefaFundo[]. */
export const EVENTO_FILA = "edicao:fila";
export const EVENTO_PROGRESSO = "edicao:progresso";
export const EVENTO_FUNDO = "edicao:fundo";

// ---- saídas registradas (fila.rs; linha de video_edit_outputs) ----
export interface AutorSaida { nome: string; matricula: string; cargo: string }
/** Caminhos relativos ao caso; tamanho em bytes; tecnico_rel só existe depois de edicao_conferir. */
export interface SaidaRegistrada {
  id: Id; projeto_id: Id; titulo: string; pasta: string; arquivo: string;
  sha256: string; composto_sha256: string; decodificado_sha256: string | null; audio_sha256: string | null;
  tamanho: number; duracao_s: number; n_quadros: number;
  predefinicao: Exportacao["predefinicao"]; codificador: string;
  tecnico_rel: string | null; retrato_rel: string; projeto_sha256: string; autor: AutorSaida; refeita_de: Id | null;
  estado: "a_conferir" | "concluida"; exige_conferencia: boolean; conferida_por: string | null; conferida_em: string | null; criado_em: string;
  // CONTRATO: abertura da Fase 1 (§10.4). Só numa saída refeita (`refeita_de`); P1.8.
  comparacao: Comparacao | null;
}

// ---- Fase 1 (contratos-fase1.md) ----
/** Resultado do "Refazer e comparar" (refazer.rs, P1.8). `tempo` em s do programa. */
export type Comparacao =
  | { tipo: "igual" }
  | { tipo: "conteudo_igual"; diferencas: string[] }
  | { tipo: "diferente"; primeiro_q: number; tempo: Seg }
  | { tipo: "nao_comparavel"; motivo: string };
/** `.trf` feito agora (executar.rs): o front grava `trf_sha256` e `zoom_pct` no efeito `efeito` por alterar(). */
export interface TrfNovo { efeito: Id; sha256: string; zoom_pct: number }
/** Intermediário de origem do clipe (origem.rs, P1.4); `fazendo`: progresso também pelo evento `edicao:fundo`. */
export type EstadoOrigem =
  | { tipo: "fazendo"; tarefa: string; feito: number }
  | { tipo: "pronto"; previa: PreviaMidia; trf: TrfNovo | null };
/** Forma de onda (ondas.rs, P1.9): min/max s16 mono por balde; o balde i começa em inicio_s + i / por_s (tempo de fonte). */
export interface Onda { inicio_s: Seg; por_s: number; min: number[]; max: number[] }
/** Trecho de fala da degravação em tempo de fonte (legendas.rs, P1.6); ia = rascunho da IA não reescrito. */
export interface TrechoFala { de: Seg; ate: Seg; texto: string; locutor: string | null; ia: boolean }
export interface Degravacao { origem: { audio_sha256: string; importado_em: string; motor: string | null; modelo_sha256: string | null }; trechos: TrechoFala[] }
/** Retratos lidos das análises (medidas.rs, P1.3): o mesmo formato de `EfRegua.calibracao` e `EfVelocidadeExibida.calculo`. */
export interface RetratoCalibracao { id: string; metodo: "line" | "plane" | "cross_ratio"; h: number[]; espaco: { w: number; h: number };
                                     pontos_sha256: string; residuo_m: number; regiao: Ponto[];
                                     // CONTRATO: fecho da Fase 1. SHA-256 da mídia da calibração; ausente em retrato antigo.
                                     midia_sha256?: string }
// CONTRATO: fecho da Fase 1. `midia_sha256` = mídia em que a velocidade foi medida; ausente em retrato antigo.
export interface RetratoCalculo { id: string; kmh: number; faixa: [number, number] | null; de_s: Seg; ate_s: Seg; metodo: string; midia_sha256?: string }
/** Proveniência da mídia na análise (video_proveniencia_commands.rs, P1.10); null = não informado. */
export interface ProvenienciaMidia { recebido_nativo: boolean | null; recebido_obs: string; relogio_desvio_s: number | null; gravador: string | null }

// ---- Fase 2 (contratos-fase2.md): rastreio assistido, conferência das ocultações ----
/** Ponteiro do denso gravado no projeto (`Rastreio.denso`; modelo.rs `Denso`). */
export interface Denso { sha256: string; n: number; de_s: Seg; ate_s: Seg; ok: number; incertos: number; ausentes: number }
/** Ponteiro das silhuetas (`Rastreio.silhuetas`; modelo.rs `Silhuetas`). */
export interface Silhuetas { sha256: string; n: number; modelo: string }
/** Colunas `estado` e `tipo` do denso (rastreio/mod.rs). */
export const ESTADO_OK = 0;
export const ESTADO_INCERTO = 1;
export const ESTADO_AUSENTE = 2;
export const TIPO_MEDIDO = 0;
export const TIPO_INTERPOLADO = 1;
export const TIPO_ESTENDIDO = 2;
// CONTRATO: "deteccao" acrescentado (P4.5): trilha de detecção da redação (modelos + ByteTrack)
export type MotorRastreio = "edgetam" | "vittrack" | "mancha" | "deteccao";
/** `auto` = edgetam com GPU pronta e item instalado, senão vittrack, senão mancha. */
export type EscolhaMotor = "auto" | "edgetam" | "vittrack" | "mancha";
export type Sentido = "frente" | "tras";
export type SentidoPedido = "frente" | "tras" | "ambos";
/** Por que a corrida parou (rastreio/mod.rs `Motivo`). */
export type MotivoParada = "fim" | "perdido" | "saiu_do_quadro" | "limite" | "ancora" | "cancelado";
/** Limiares gravados na corrida; `paciencia` em quadros. */
export interface Limiares { ok: number; perdido: number; paciencia: number }
export interface ArquivoModelo { nome: string; sha256: string }
export interface ModeloCorrida { nome: string; origem: string; commit: string; arquivos: ArquivoModelo[]; desvio: string }
/** `provedor` = "cpu" | "cuda"; `runtime` = "onnxruntime 1.31.1" (vazio na mancha). */
export interface MotorCorrida { motor: MotorRastreio; modelo: ModeloCorrida; runtime: string; provedor: string }
/** Uma corrida, como em `ArquivoDenso.corridas[]` (§6.1). */
export interface Corrida extends MotorCorrida { sentido: Sentido; ancora_pts: Pts; de_pts: Pts; ate_pts: Pts; limiares: Limiares;
                                                parou: MotivoParada; feito_em: string }
/** `rastreios/<rid>/<sha256>.json` (§6.1): uma linha por quadro da fonte, em colunas. */
export interface ArquivoDenso {
  esquema: number; rastreio: Id; midia_sha256: string; espaco: string; pixel: { w: number; h: number };
  pts: Pts[]; x: number[]; y: number[]; w: number[]; h: number[]; conf: number[]; estado: number[]; tipo: number[]; corrida: number[];
  corridas: Corrida[];
}
/** O que "Seguir" pede (`edicao_rastreio_seguir`); `ate_s` em tempo de fonte (null = até a borda da mídia). */
export interface PedidoSeguir { rastreio_id: Id; clipe_id: Id; ancora: Chave<Caixa>; sentido: SentidoPedido; ate_s: Seg | null; motor: EscolhaMotor }
export interface Parou { motivo: MotivoParada; pts: Pts | null }
export interface ParouSentido { sentido: Sentido; motivo: MotivoParada; pts: Pts | null }
/** Evento `edicao:rastreio` (até 5 por segundo; `miniatura` = data URL JPEG de até 320 px com a caixa). */
export interface AvancoRastreio { tipo: "avanco"; tarefa_id: string; rastreio_id: Id; sentido: Sentido; pts: Pts; caixa: Caixa | null;
                                  conf: number; estado: number; feitos: number; miniatura: string | null }
/** Fim: o front aplica `denso` e `silhuetas` por alterar(); `denso` null = nada novo. */
export interface FimRastreio { tipo: "fim"; tarefa_id: string; rastreio_id: Id; parou: ParouSentido[]; denso: Denso | null; silhuetas: Silhuetas | null }
/** `mensagem` = "<código>: <frase>" do catálogo. */
export interface FalhaRastreio { tipo: "falhou"; tarefa_id: string; rastreio_id: Id; mensagem: string }
export type EventoRastreio = AvancoRastreio | FimRastreio | FalhaRastreio;
export const EVENTO_RASTREIO = "edicao:rastreio";
/** `edicao_rastreio_editar`: faixas por pts do quadro (`ate_pts` null = até o fim). */
export type OpRastreio = { tipo: "apagar_faixa"; de_pts: Pts; ate_pts: Pts | null } | { tipo: "simplificar"; tolerancia: number; suavizar_s?: number };
/** O front grava `ancoras` quando vem e `denso` sempre (null = rastreio sem denso). */
export interface Editado { ancoras: Chave<Caixa>[] | null; denso: Denso | null;
                          corridas_do_programa?: Corrida[] }   // CONTRATO: acréscimo (17/11); ausente = fica como está
/** "Conferir ocultações" (conferencia.rs, P2.6): segundos do arquivo exportado, `[de_s, ate_s)`. */
export type ComoFraco = "incerto" | "interpolado" | "ausencia"; // "ausencia": sem região, em ausência aceita
// ---- Fase 4 (P4.5): ocultação assistida (`rastreio/detectar.rs`, `rastreio/tira.rs`) ----
export type ClasseDeteccao = "rosto" | "placa" | "cabeca" | "pessoa" | "veiculo";
/** O que "Sugestões" pede (`edicao_detectar`): o trecho do clipe, no espaço original; "rosto" = cabeça e rosto. */
export interface PedidoDetectar { clipe_id: Id; classes: ClasseDeteccao[] }
/** Evento `edicao:deteccao` (até 5 por segundo). */
export interface AvancoDeteccao { tipo: "avanco"; tarefa_id: string; feitos: number; total: number }
/** Fim: as propostas (rastreios SEM confirmação, denso já gravado) da mídia; vazio quando parou antes (`cancelada`). */
export interface FimDeteccao { tipo: "fim"; tarefa_id: string; clipe_id: Id; midia_id: Id; rastreios: Rastreio[]; cancelada: boolean }
export interface FalhaDeteccao { tipo: "falhou"; tarefa_id: string; mensagem: string }
export type EventoDeteccao = AvancoDeteccao | FimDeteccao | FalhaDeteccao;
export const EVENTO_DETECCAO = "edicao:deteccao";
/** Folha de contato (`edicao_rastreio_tira`): JPEG relativo ao cache do caso; `tempos` (tempo de fonte) por célula. */
export interface Tira { relativo: string; colunas: number; tempos: Seg[] }
export interface FaixaFraca { de_s: Seg; ate_s: Seg; como: ComoFraco }
export interface TrechoConferencia { efeito: Id; conteudo: EfOcultar["conteudo"]; de_s: Seg; ate_s: Seg; fracos: FaixaFraca[] }

// ---- Fase 4: tiras de miniaturas (tiras.rs, P4.3) ----
/** Faixa [de_s, ate_s) em tempo de fonte; `n` miniaturas ou `largura` da faixa na tela (px; n = largura ÷ largura da
 *  célula, até o teto de 256 miniaturas e 16384 px: vale o `n` da resposta); `altura` de cada miniatura (px); `canal` =
 *  quem pede (o clipe; o bloco do clipe, se a tela pedir a faixa em pedaços): o pedido novo do mesmo canal cancela o anterior. */
export interface PedidoTira { sha256: string; de_s: Seg; ate_s: Seg; n: number | null; largura: number | null; altura: number; canal: string }
/** Célula da imagem: o quadro-chave (pts na base da tabela de quadros; t em s de fonte). */
export interface CelulaTira { pts: Pts; t: Seg }
/** `relativo` = JPEG no cache (célula c em x = c·w); `miniaturas[k]` = célula da janela k; sem quadro-chave na faixa: `relativo` null e listas vazias. */
export interface TiraMiniaturas { relativo: string | null; w: number; h: number; n: number; de_s: Seg; ate_s: Seg; celulas: CelulaTira[]; miniaturas: number[] }
/** `tira` null = pedida à fila de fundo: pedir de novo quando `edicao:fundo` der `pronta` (ou `cancelada`, se ainda for
 *  a última do canal) para `chave`.
 *  MPEG-PS e H.264 cru sem a tabela do decodificador: erro `tabela_pendente` (pedir de novo quando `tabela:<sha256>` ficar pronta). */
export interface RespostaTira { chave: string; tira: TiraMiniaturas | null }

// ---- Fase 4 (contratos-fase4.md): texto livre e zoom que segue ----
/** Sombra do texto (`EstiloTexto.sombra`): deslocamento e desfoque em fração da altura do canvas. */
export interface SombraTexto { cor: Cor; dx: number; dy: number; desfoque: number }
/** "corte" ignora a duração; "aparecer" = opacidade linear; "deslizar" = opacidade e posição (`edicao/sobreposto.rs`). */
export type Surgir = "corte" | "aparecer" | "deslizar";
/** Entrada (em `entrada_s`) e saída (em `saida_s`) do sobreposto; `lado` = de onde vem e para onde vai ao deslizar. */
export interface AnimacaoSob { entrada: Surgir; saida: Surgir; lado: "cima" | "baixo" | "esq" | "dir" }
/** O centro da região do enquadramento segue o rastreio; `suavizar_s` = média móvel centrada, na base do rastreio. */
export interface SegueRastreio { rastreio: Id; suavizar_s: Seg }

// ---- palco e desenho (§8.3) ----
// Unidades: 0..1 do canvas. O pintor (front) e camada::formas (Rust) implementam TODOS os `k` uma vez; efeito novo só devolve primitivas.
type Traco2 = { cor: Cor; largura: number; contorno?: Cor };
export type Primitiva =
  | { k: "caminho"; d: (["M" | "L", number, number] | ["C", number, number, number, number, number, number] | ["Z"])[]; traco?: Traco2; preenche?: Cor; regra?: "evenodd" }
  | { k: "elipse"; cx: number; cy: number; rx: number; ry: number; traco?: Traco2; preenche?: Cor }
  | { k: "sprite"; src: string; caixa: Caixa; opacidade: number }      // src: relativo ao cache da Edição, ou "midia:<MidiaRef.id>"
  | { k: "atlas"; src: string; celula: { w: number; h: number }; glifos: string; texto: string; lugar: Lugar; altura: number }
  | { k: "regiao"; modo: "desfoque" | "mosaico" | "solido" | "escurecer"; forma: Primitiva /* caminho ou elipse */; intensidade: number; cor?: Cor;
      borda_suave: number; inverter: boolean; espaco: "fonte" | "canvas" }
  | { k: "lupa"; origem: Caixa /* fonte */; destino: Caixa /* canvas */; forma: "circulo" | "retangulo" };
/** A = afim origem → canvas em 0..1, na ordem do canvas 2D (a, b, c, d, e, f): x' = a·x + c·y + e; y' = b·x + d·y + f.
 *  sprite, atlas e midia não estão no §8.3: são o que `primitivas()` precisa e não cabe no efeito (no Rust, `FonteSprites`);
 *  null = ainda não chegou do backend (o efeito fica sem desenho até chegar). */
// CONTRATO: `indice` e `programa_s` (contratos (g) 8), espelho de `QuadroCamada.indice`/`.programa_s`:
// índice do quadro de fonte na tabela (null sem fonte; carimbo "Quadro") e instante do slot no programa (carimbo "Vídeo").
export interface QuadroPalco { q: number; t: Seg; pts: Pts | null; indice: number | null; programa_s: Seg;
                               A: [number, number, number, number, number, number];
                               canvas: { w: number; h: number }; rastreio(id: Id): Caixa | null;
                               ausente?(id: Id): boolean;   // ausência aceita sem caixa no quadro (ocultar invertida cobre tudo)
                               sprite(pedido: PedidoSprite): SpriteTexto | null;
                               atlas(estilo: EstiloTexto, alturaCanvas: number): AtlasCarimbo | null;
                               midia(id: Id): { w: number; h: number } | null }     // tamanho da imagem em px
// CONTRATO: `{ livre, trilha, sobreposto }` (Fase 4, B6): alça do texto livre (ou de outro sobreposto) no palco.
export type Mudanca = { efeito: Efeito } | { rastreio: Id; ancora: Chave<Caixa> } | { clipe: Id; campos: Partial<ClipeTrecho> }
                    | { livre: Id; trilha: Id; sobreposto: ClipeSobreposto };
// CONTRATO: "giro" (Fase 4, B6): alça de rotação do texto livre no palco.
export interface Alca { id: string; tipo: "mover" | "canto" | "borda" | "ponto" | "giro"; em: Ponto; cursor: string;
                        arrastar(de: Ponto, para: Ponto, mod: { shift: boolean; alt: boolean }): Mudanca }
export type ItemSel = { k: "clipe" | "livre" | "audio" | "legenda" | "marcador"; id: Id; trilha?: Id };
export interface Selecao { itens: ItemSel[]; foco: ItemSel | null; efeito: Id | null }
export type Cursor = { modo: "clipe"; clipe: Id; t: Seg /* fonte ou local; pode sair de [inicio_s, fim_s] para aparar */ } | { modo: "programa"; q: number };
export type PedidoMarcacao = { tipo: "caixa" | "ponto" | "regiao" | "lugar"; rastreio?: Id; efeito?: Id; proporcao?: number; celula?: number };
export interface ContextoClipe {
  ws: string; projeto: ProjetoEdicao; clipe: ClipePrincipal | ClipeSobreposto; midias: MidiaInfo[];   // `tela`: uma por célula
  linha: ClipeResolvido | LivreResolvido;
  tempo(): Seg;                      // tempo do efeito no cursor, lido do relógio do motor (não é propriedade React)
  rastreios: Rastreio[];
  marcar(p: PedidoMarcacao): void;   // liga a ferramenta no palco
  irPara(t: Seg): void;
}
export interface PalcoProps { modo: "clipe" | "programa"; ferramenta: PedidoMarcacao | null; onMarcado(v: Caixa | Ponto | Lugar, t: Seg, pts: Pts | null): void }
// `EditorEfeitoProps` e `DescritorEfeito` (§8.3) dependem de React, lucide e do `Gesto` da store: nascem em inspetor/registro.ts (P0.8).
