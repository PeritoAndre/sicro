//! Relatório da exportação (§10): `tecnico.json`, `mapa.csv`, dados do PDF do júri (`juri.rs`) e o catálogo das frases
//! dele (`frases.rs`). Dono: o pacote R da fase (P1.R).

pub mod frases;
pub mod juri;
pub mod mapa;
pub mod tecnico;

/// Plano, resultado e ambiente montados à mão (o motor e o executor ainda não existem), e a varredura do texto.
#[cfg(test)]
pub(crate) mod apoio {
    use std::path::PathBuf;
    use std::sync::Arc;

    use serde_json::{json, Value};

    use crate::edicao::camada::ArquivoCamada;
    use crate::edicao::conferir::ConferenciaSaida;
    use crate::edicao::executar::{Ambiente, Autor, BlocoFeito, Ciencia, Cpu, Identidade, LidosClipe, Resultado, RetratoCaso, Sistema, VersaoSicro};
    use crate::edicao::ffmpeg::exec::Termino;
    use crate::edicao::ffmpeg::sonda::Origem;
    use crate::edicao::modelo::{
        Confirmacao, ConteudoOculto, CorMidia, Derivacao, Faixa, FormaRegiao, Matriz, ModoOcultar, ModoSilencio, OrigemRastreio, Predefinicao,
        Racional, Recebido, TipoDerivacao, Velocidade,
    };
    use crate::edicao::motor::plano::*;
    use crate::edicao::motor::Alvo;
    use crate::edicao::quadros::{apoio as q, Declarado, TabelaQuadros};
    use crate::edicao::tempo::{self, MapaQuadros};

    /// Nome do rastreio no projeto: não pode aparecer no `tecnico.json`.
    pub const NOME_RASTREIO: &str = "Suspeito de camisa azul";
    pub const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    /// Base 1/15360 a 30 qps.
    pub const TB: (i64, i64) = (1, 15360);

    /// Pastas de um caso e do cache, numa pasta temporária própria (apagada no fim).
    pub struct Cena {
        tmp: tempfile::TempDir,
        pub caso: PathBuf,
        pub cache: PathBuf,
        pub pasta_projeto: PathBuf,
    }

    impl Cena {
        pub fn nova(nome: &str) -> Self {
            let tmp = tempfile::Builder::new().prefix("p0r-relatorio-").tempdir().unwrap();
            let caso = tmp.path().join(format!("caso {nome}"));
            let cache = tmp.path().join("cache").join("edicao").join("caso-1");
            let pasta_projeto = caso.join("Sistema/videos/edicao/p1");
            std::fs::create_dir_all(&caso).unwrap();
            Cena { tmp, caso, cache, pasta_projeto }
        }

        pub fn trabalho(&self, tarefa: &str) -> PathBuf {
            self.cache.join("render").join(tarefa)
        }

        pub fn ambiente(&self, tarefa: &str) -> Ambiente {
            let mut cap = q::cap_falsa();
            cap.binario = self.tmp.path().join("bin").join("ffmpeg");
            cap.ffprobe = self.tmp.path().join("bin").join("ffprobe");
            Ambiente {
                cap: Arc::new(cap),
                trabalho: self.trabalho(tarefa),
                cache: self.cache.clone(),
                caso: self.caso.clone(),
                pasta_projeto: self.pasta_projeto.clone(),
                paralelos: 1,
                identidade: Identidade {
                    autor: Autor { nome: "Perito Teste".into(), matricula: "000".into(), cargo: "Perito Criminal".into() },
                    caso: RetratoCaso { numero: "Ocorrência 0000/2026".into(), referencia: "Laudo 1".into(), orgao: "PCI".into(), requisitante: "Vara 1".into() },
                    sicro: VersaoSicro { versao: "6.0.0".into(), commit: "abc".into(), alvo: "x86_64-unknown-linux-gnu".into() },
                    sistema: Sistema { so: "Linux".into(), cpu: Cpu { modelo: "CPU".into(), instrucoes: vec!["avx2".into()] }, gpu: None, driver: None },
                },
                ciencias: vec![Ciencia { codigo: "taxa_variavel".into(), ciente_em: "2026-11-14T10:00:00Z".into() }],
            }
        }

        /// Tudo o que não pode aparecer no texto.
        pub fn raizes(&self) -> Vec<String> {
            vec![self.tmp.path().to_string_lossy().to_string(), self.caso.to_string_lossy().to_string(), self.cache.to_string_lossy().to_string()]
        }
    }

    /// 300 quadros (10 s) a 30 qps, base 1/15360.
    pub fn tabela() -> TabelaQuadros {
        q::tabela(TB, q::pts_regulares(300, (1, 30), TB, 0))
    }

    /// Origem de um trecho a partir do mapa real do `tempo`.
    pub fn origem(tab: &TabelaQuadros, m: &MapaQuadros) -> ProvOrigem {
        let fim = if m.j_out < tab.pts.len() { tab.tempo(m.j_out) } else { tab.fim_s() };
        ProvOrigem {
            campos: false,
            midia: "m1".into(),
            de_s: tab.tempo(m.j_in),
            ate_s: fim,
            pts_primeiro: tab.pts[m.j_in],
            pts_ultimo: tab.pts[m.j_out - 1],
            lidos: (m.j_out - m.j_in) as u32,
            repetidos: m.repetidos,
            descartados: m.descartados,
            mapa: ProvMapa { c: m.c, kn: m.kn, kd: m.kd, e: m.e },
            j_in: m.j_in as u32,
            quadro_do_slot: m.quadro_do_slot.clone(),
            pts: tab.pts[m.j_in..m.j_out].to_vec(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn item(n: u32, clipe: &str, tipo: &str, de_q: u64, n_q: u32, fps: Racional, origem: Option<ProvOrigem>, velocidade: Option<Velocidade>) -> ProvItem {
        let s = |q: u64| tempo::instante_do_slot(q, fps);
        ProvItem {
            n,
            clipe: clipe.into(),
            nome: format!("item {n}"),
            tipo: tipo.into(),
            na_saida: ProvNaSaida { de_q, n_q, de_s: s(de_q), ate_s: s(de_q + n_q as u64) },
            origem,
            velocidade,
            transicao: None,
            processamentos: vec![ProvProcessamento { ordem: 1, nome: "recorte".into(), parametros: json!({ "de_pts": 15360 }) }],
            selos: vec![],
            audio: None,
        }
    }

    /// Título (30 slots), trecho dos quadros 30..90 na velocidade `v` e quadro 100 congelado (15 slots), na taxa `fps`.
    /// O trecho e o congelado usam o mapa de quadros real (`tempo::mapa_trecho`).
    pub fn plano(c: &Cena, fps: Racional, v: f64) -> Plano {
        let tab = tabela();
        let vel = Velocidade::Constante { fator: v };
        let m = tempo::mapa_trecho(&tab, 30, 90, &vel, fps).unwrap();
        let tr = origem(&tab, &m);
        let mut cg = origem(&tab, &tempo::mapa_trecho(&tab, 100, 101, &Velocidade::Constante { fator: 1.0 }, fps).unwrap());
        cg.quadro_do_slot = vec![100; 15];
        cg.repetidos = 14;
        let roteiro = vec![
            item(1, "c1", "titulo", 0, 30, fps, None, None),
            item(2, "c2", "trecho", 30, m.n_q, fps, Some(tr.clone()), Some(vel)),
            item(3, "c3", "congelar", 30 + m.n_q as u64, 15, fps, Some(cg.clone()), None),
        ];
        let total_q = 30 + m.n_q as u64 + 15;
        let caso = c.caso.to_string_lossy().to_string();
        let midia = ProvMidia {
            id: "m1".into(),
            nome: "cam 1.mp4".into(),
            sha256: SHA.into(),
            derivacao: Some(Derivacao {
                tipo: TipoDerivacao::CopiaDvr,
                de_sha256: "b".repeat(64),
                raiz_sha256: "b".repeat(64),
                comando: format!("ffmpeg -i {caso}/Mídias/orig.dav -c copy /mnt/dvr 1/out.mp4"),
                com_perdas: false,
            }),
            recebido: Recebido { nativo: Some(false), obs: "veio por pendrive".into() },
            relogio_desvio_s: Some(-2.5),
            gravador: Some("DVR 1".into()),
            cor: CorMidia { matriz: Matriz::Bt709, faixa: Faixa::Tv },
            conteiner: "mov,mp4,m4a,3gp,3g2,mj2".into(),
            codec: "h264".into(),
            pix_fmt: "yuv420p".into(),
            sar: Racional { num: 1, den: 1 },
            giro: 0,
            inicio_s: 0.0,
            declarado: Declarado { r_frame_rate: "30/1".into(), avg_frame_rate: "30/1".into(), nb_frames: Some(300), duracao: Some(10.0) },
            medido: ProvMedido { tb: Racional { num: TB.0, den: TB.1 }, de_s: 0.0, ate_s: 10.0, pacotes: 300, quadros: 300, passo_min: 512, passo_med: 512, passo_max: 512, lacunas: vec![] },
            discrepancias: vec![],
            erros_decodificacao: 0,
            usado: vec![ProvFaixa { de_s: 1.0, ate_s: 3.0 }],
            usado_total_s: 2.0,
            largura: 1280,
            altura: 720,
            tamanho: 123_456_789,
        };
        let bloco = Bloco {
            indice: 0,
            de_q: 0,
            n_q: total_q as u32,
            chave: "c".repeat(64),
            entradas: vec![Entrada {
                antes: vec!["-t".into(), "4".into()],
                antes_sem_busca: None,
                caminho: c.caso.join("Mídias/Vídeos/cam 1.mp4"),
                papel: Papel::Midia { id: "m1".into(), clipe: "c2".into() },
            }],
            grafo: "[0:v]null[v]".into(),
            auxiliares: vec![],
            codificador: vec!["-c:v".into(), "libx264".into()],
            saida: "b000.mp4".into(),
            framehash: "b000.framehash".into(),
            conferir: vec![
                ConferirClipe { rotulo: "c001".into(), clipe: "c2".into(), lidos: tr.lidos, pts: tr.pts.clone() },
                ConferirClipe { rotulo: "c002".into(), clipe: "c3".into(), lidos: 1, pts: cg.pts.clone() },
            ],
        };
        Plano {
            alvo: Alvo::Final {
                exportacao: serde_json::from_value(json!({ "predefinicao": "juri", "codificador": "cpu", "legendas": "nenhuma", "pacote": null,
                                                          "pdf": "a3", "revisado_por": "", "personalizada": null })).unwrap(),
            },
            canvas: Canvas { w: 1280, h: 720, fps, cor_fundo: "#000000".into() },
            total_q,
            previas: vec![],
            camadas: vec![],
            blocos: vec![bloco],
            audio: None,
            legendas: vec![],
            juncao: Juncao::Concat { lista: ArquivoTexto { nome: "blocos.ffconcat".into(), conteudo: "ffconcat version 1.0\n".into() }, args: vec![], extensao: "mp4".into() },
            proveniencia: Proveniencia {
                projeto: ProvProjeto { id: "p1".into(), titulo: "Caso".into(), esquema: 2, esquema_menor: 0 },
                saida: ProvSaida { largura: 1280, altura: 720, fps, duracao_s: tempo::instante_do_slot(total_q, fps), n_quadros: total_q, predefinicao: Some(Predefinicao::Juri), com_perdas: true },
                midias: vec![midia],
                roteiro,
                ocultacoes: vec![ProvOcultacao {
                    midia: "m1".into(),
                    clipes: vec!["c2".into()],
                    de_s: 1.0,
                    ate_s: 2.0,
                    modo: ModoOcultar::Solido,
                    forma: FormaRegiao::Retangulo,
                    intensidade: 1.0,
                    conteudo: ConteudoOculto::Rosto,
                    motivo: "terceiro sem relação".into(),
                    quadros: ProvQuadrosOcultos { ancora: 2, medido: 0, interpolado: 0, incerto: 0 },
                    inverter: false,
                    margem: 0.0,
                    borda_suave: 0.0,
                    cor: "#000000ff".into(),
                    rastreio: "r1".into(),
                }],
                audio: ProvAudio { silencios: vec![ProvSilencio { midia: "m1".into(), de_s: 1.5, ate_s: 2.0, modo: ModoSilencio::Tom, motivo: "nome".into() }], tolerancia_atempo_ms: 0.0, mixagem: None, trilhas: vec![] },
                legendas: ProvLegendas { origem: None, n: 0, conferidas: 0, modo: crate::edicao::modelo::ModoLegendas::Nenhuma, no_projeto: 0, ia: 0, ia_conferidas: 0 },
                sincronias: vec![],
                rastreios: vec![ProvRastreio {
                    id: "r1".into(),
                    origem: OrigemRastreio::Manual,
                    ancoras: 2,
                    denso_sha256: None,
                    silhuetas_sha256: None,
                    corridas: vec![json!({ "motor": "edgetam", "modelo": { "nome": "EdgeTAM vídeo", "arquivo": format!("{caso}/../modelos/x.onnx") } })],
                    confirmacao: Some(Confirmacao { por: "Perito Teste".into(), em: "2026-11-14T09:00:00Z".into(), denso_sha256: None, de_s: 1.0, ate_s: 2.0 }),
                    ausencias: vec![],
                }],
                ia: vec![ProvIa {
                    modelo: "EdgeTAM".into(),
                    arquivos: vec![ProvArquivo { nome: "vision_encoder.onnx".into(), sha256: "e".repeat(64) }],
                    origem: "https://huggingface.co/x/y".into(),
                    commit: "8ca3d3e4".into(),
                    desvio: String::new(),
                    runtime: "onnxruntime 1.31.1".into(),
                    // no fio, `cpu` | `cuda` (contratos-fase2 B2); o PDF mostra "CPU" / "CUDA"
                    provedor: "cpu".into(),
                    uso: "rastreio".into(),
                }],
                estabilizacoes: vec![],
                avisos: vec![ProvAviso { codigo: "taxa_variavel".into(), n: 3 }, ProvAviso { codigo: "fonte_danificada".into(), n: 2 }],
                selos_desligados: vec!["realce".into()],
                revisado_por: String::new(),
                ffmpeg: ProvFfmpeg {
                    versao: "n9.0.1".into(),
                    configuracao: "--prefix=/usr --enable-gpl --extra-cflags=-I/opt/include".into(),
                    origem: Origem::Path,
                    sha256: "f".repeat(64),
                },
                fecho: None,
                sobrepostos: vec![],
            },
        }
    }

    /// O que o executor devolveria rodando o plano na pasta `render/<tarefa>/`.
    pub fn resultado(c: &Cena, p: &Plano, tarefa: &str) -> Resultado {
        let t = c.trabalho(tarefa);
        let s = |x: &PathBuf| x.to_string_lossy().to_string();
        let hora = if tarefa == "t1" { "10" } else { "11" };
        let lidos = p.blocos.iter().flat_map(|b| &b.conferir).map(|k| LidosClipe { clipe: k.clipe.clone(), pts: k.pts.clone() }).collect();
        Resultado {
            arquivo: t.join("saida.mp4"),
            sha256: "1".repeat(64),
            tamanho: 123_456,
            codificador: "libx264".into(),
            argumentos: vec!["-c:v".into(), "libx264".into(), "-crf".into(), "18".into()],
            queda_para_cpu: None,
            composto_sha256: Some("2".repeat(64)),
            compostos: Some(t.join("quadros.compostos.sha256")),
            audio_sha256: Some("3".repeat(64)),
            loudnorm: None,
            ganho_db: None,
            blocos: p.blocos.iter().map(|b| BlocoFeito { indice: b.indice, de_q: b.de_q, n_q: b.n_q, chave: b.chave.clone(), reaproveitado: tarefa != "t1", sha256: "4".repeat(64) }).collect(),
            camadas: vec![ArquivoCamada { arquivo: "q00001.png".into(), sha256: "5".repeat(64) }],
            passos: vec![Termino {
                nome: "bloco 0".into(),
                argumentos: vec![
                    "-i".into(),
                    s(&c.caso.join("Mídias/Vídeos/cam 1.mp4")),
                    "-/filter_complex".into(),
                    s(&t.join("b000.txt")),
                    "-i".into(),
                    s(&c.cache.join("camadas/abc/lista.ffconcat")),
                    s(&t.join("b000.mp4")),
                ],
                inicio: format!("2026-11-14T{hora}:00:00Z"),
                fim: format!("2026-11-14T{hora}:00:03Z"),
                codigo: Some(0),
                cancelado: false,
            }],
            conferencia: Some(ConferenciaSaida { quadros: p.total_q, erros: 0, decodificado_sha256: "6".repeat(64), decodificados: t.join("quadros.decodificados.sha256") }),
            lidos,
            legendas: vec![],
            trf: vec![],
            renderizado_em: format!("2026-11-14T{hora}:00:04Z"),
        }
    }

    /// Plano a 30 qps e 1× e o resultado na tarefa "t1".
    pub fn plano_e_resultado(c: &Cena) -> (Plano, Resultado) {
        let p = plano(c, Racional { num: 30, den: 1 }, 1.0);
        let r = resultado(c, &p, "t1");
        (p, r)
    }

    /// Trechos do texto (JSON) com caminho absoluto, raiz da máquina ou nome proibido.
    pub fn absolutos_no_texto(texto: &str, raizes: &[String], nomes: &[&str]) -> Vec<String> {
        let mut achados: Vec<String> = raizes.iter().chain(["/home/", "/tmp/"].map(String::from).iter()).filter(|r| texto.contains(r.as_str())).cloned().collect();
        achados.extend(nomes.iter().filter(|n| texto.contains(**n)).map(|n| n.to_string()));
        fn strings<'a>(v: &'a Value, saida: &mut Vec<&'a str>) {
            match v {
                Value::String(s) => saida.push(s),
                Value::Array(a) => a.iter().for_each(|x| strings(x, saida)),
                Value::Object(m) => m.values().for_each(|x| strings(x, saida)),
                _ => {}
            }
        }
        let v: Value = serde_json::from_str(texto).unwrap();
        let mut todas = Vec::new();
        strings(&v, &mut todas);
        let sep: &[char] = &[' ', '\t', '\n', '"', '\'', '=', ',', ';', '(', ')', '[', ']', '|', '<', '>'];
        for s in todas {
            for tok in s.split(sep) {
                let b = tok.as_bytes();
                let resto = if b.len() > 2 && b[0] == b'-' && b[1].is_ascii_alphabetic() { &b[2..] } else { b };
                let abs = matches!(resto, [b'/', c, ..] if *c != b'/')
                    || matches!(resto, [b'~', b'/', ..] | [b'\\', b'\\', ..])
                    || matches!(resto, [l, b':', b'\\' | b'/', ..] if l.is_ascii_alphabetic());
                if abs {
                    achados.push(s.to_string());
                }
            }
        }
        achados
    }
}
