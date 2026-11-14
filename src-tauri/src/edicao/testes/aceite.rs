//! Aceites do motor de vídeo (P0.6a): o compilador contra o ffmpeg de verdade, nos dois binários
//! (`SICRO_FFMPEG` escolhe; `SICRO_EXIGE_FFMPEG=1` proíbe a dispensa calada). Os completos são `#[ignore]`.
#![cfg(test)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};

use super::executor::{self, Opcoes};
use super::fontes::{self, Fonte};
use super::medir::{self, QuadroRgb};
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::modelo::ProjetoEdicao;
use crate::edicao::motor::plano::{Juncao, Papel, Plano, TipoCamada};
use crate::edicao::motor::{compilar_video, Alvo, Contexto, FonteInfo};
use crate::edicao::quadros::apoio::{self, Medida, SemRastreios};
use crate::edicao::quadros::TabelaQuadros;
use crate::edicao::tempo;
use crate::error::Result;

// ------------------------------------------------------------------ projeto de teste

pub(crate) fn exportacao(codificador: &str) -> Value {
    json!({ "predefinicao": "juri", "codificador": codificador, "legendas": "nenhuma", "pacote": null, "pdf": "a3", "revisado_por": "", "personalizada": null })
}

pub(crate) fn alvo_final(codificador: &str) -> Alvo {
    serde_json::from_value(json!({ "tipo": "final", "exportacao": exportacao(codificador) })).unwrap()
}

pub(crate) fn midia(id: &str, tipo: &str, ocultacoes: Value) -> Value {
    json!({ "id": id, "tipo": tipo, "origem": "caso", "media_id": null, "sha256": "0".repeat(64), "nome": format!("{id}.mp4"), "relativo": format!("Midias/{id}.mp4"),
            "duracao_s": null, "inicio_s": null, "largura": null, "altura": null, "taxa": null, "vfr": false, "tem_audio": false, "cor": null,
            "derivacao": null, "recebido": { "nativo": null, "obs": "" }, "relogio_desvio_s": null, "gravador": null, "decodificacao": null,
            "ocultacoes": ocultacoes })
}

/// Projeto mínimo; `extra` = objeto com `rastreios`, `sobreposicoes`, `fecho`… que entra por cima.
pub(crate) fn projeto(w: u32, h: u32, fps: (i64, i64), midias: Vec<Value>, principal: Vec<Value>, extra: Value) -> ProjetoEdicao {
    let mut v = json!({
        "esquema": 2, "esquema_menor": 0, "id": "p-teste", "titulo": "Teste", "created_at": "2026-11-14T00:00:00Z", "updated_at": "2026-11-14T00:00:00Z",
        "saida": { "largura": w, "altura": h, "fps": { "num": fps.0, "den": fps.1 }, "auto": false, "cor_fundo": "#202020", "audio": false,
                   "selos": { "velocidade": true, "ampliacao": true, "realce": true }, "exportacao": exportacao("cpu") },
        "midias": midias, "principal": principal
    });
    if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
        for (k, x) in e {
            o.insert(k.clone(), x.clone());
        }
    }
    serde_json::from_value(v).expect("projeto de teste")
}

pub(crate) fn trecho(id: &str, midia: &str, tab: &TabelaQuadros, j_in: usize, j_out: usize, v: f64, efeitos: Value) -> Value {
    let fim_s = if j_out < tab.pts.len() { tab.tempo(j_out) } else { tab.fim_s() };
    json!({ "tipo": "trecho", "id": id, "rotulo": id, "desligado": false, "transicao_entrada": null, "efeitos": efeitos, "midia": midia,
            "inicio_s": tab.tempo(j_in), "fim_s": fim_s, "inicio_pts": tab.pts[j_in], "fim_pts": tab.pts.get(j_out),
            "velocidade": { "tipo": "constante", "fator": v }, "audio": { "volume": 1.0, "mudo": false, "manter_tom": true, "silencios": [] } })
}

pub(crate) fn rastreio(id: &str, midia: Option<&str>, base: &str, ancoras: Vec<Value>) -> Value {
    json!({ "id": id, "nome": id, "cor": "#ff0000", "midia": midia, "base": base, "tipo": "caixa", "espaco": "original", "ancoras": ancoras,
            "denso": null, "silhuetas": null, "ausencias": [], "origem": { "tipo": "manual" }, "confirmacao": null })
}

pub(crate) fn ancora(t: f64, pts: Option<i64>, c: [f64; 4]) -> Value {
    json!({ "t": t, "pts": pts, "v": { "x": c[0], "y": c[1], "w": c[2], "h": c[3] }, "interp": "linear" })
}

/// `EfOcultar` no fio (com `tipo`, como em `MidiaRef.ocultacoes` e em `clipe.efeitos`).
pub(crate) fn ocultar(id: &str, rastreio: &str, modo: &str, extra: Value) -> Value {
    let mut v = json!({ "tipo": "ocultar", "id": id, "ligado": true, "nome": "", "ativo": null, "modo": modo,
        "alvo": { "rastreio": rastreio, "forma": "retangulo", "vertices": null, "margem": 0.1, "borda_suave": 0.0, "inverter": false },
        "intensidade": 1.0, "cor": "#000000", "conteudo": "outro", "motivo": "teste" });
    if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
        for (k, x) in e {
            o.insert(k.clone(), x.clone());
        }
    }
    v
}

pub(crate) fn efeito(tipo: &str, id: &str, campos: Value) -> Value {
    let mut v = json!({ "tipo": tipo, "id": id, "ligado": true, "nome": "", "ativo": null });
    if let (Some(o), Some(e)) = (v.as_object_mut(), campos.as_object()) {
        for (k, x) in e {
            o.insert(k.clone(), x.clone());
        }
    }
    v
}

/// As mídias e o ffmpeg de um teste; `compilar` resolve a linha e chama `compilar_video`.
pub(crate) struct Mundo {
    pub fontes: HashMap<String, FonteInfo>,
    pub cap: Arc<Capacidades>,
}

impl Mundo {
    pub fn novo(cap: &Arc<Capacidades>) -> Self {
        Self { fontes: HashMap::new(), cap: cap.clone() }
    }

    pub fn com(mut self, id: &str, info: &FonteInfo) -> Self {
        self.fontes.insert(id.to_string(), info.clone());
        self
    }

    pub fn compilar(&self, p: &ProjetoEdicao, alvo: &Alvo) -> Result<Plano> {
        let ctx = Contexto { fontes: &self.fontes, cap: &self.cap, rastreios: &SemRastreios, pasta_projeto: Path::new("/nao/existe/projeto"), midias: &[] };
        let linha = tempo::resolver(p, &ctx);
        compilar_video(p, &linha, &ctx, alvo)
    }
}

/// Âncoras (uma por quadro) da marca vermelha da fonte nos quadros `[j_in, j_out)`.
pub(crate) fn ancoras_da_marca(f: Fonte, m: &Medida, j_in: usize, j_out: usize) -> Vec<Value> {
    let tab = &m.info.quadros;
    (j_in..j_out).map(|j| ancora(tab.tempo(j), Some(tab.pts[j]), fontes::marca_esperada(f, m.numeros[j]))).collect()
}

pub(crate) fn vermelho(q: &QuadroRgb) -> u64 {
    q.px.chunks_exact(3).filter(|p| p[0] as i32 - p[1] as i32 > 90 && p[0] as i32 - p[2] as i32 > 90).count() as u64
}

fn pasta(nome: &str) -> PathBuf {
    fontes::pasta_temporaria(nome)
}

fn sem_perdas() -> Opcoes {
    Opcoes { sem_perdas: true, ..Opcoes::default() }
}

/// Números desenhados que o plano prevê em cada slot do clipe `n` do roteiro (pelo mapa da proveniência).
fn previstos(plano: &Plano, n: usize, m: &Medida) -> Vec<u32> {
    let o = plano.proveniencia.roteiro[n].origem.as_ref().expect("clipe com fonte");
    o.quadro_do_slot.iter().map(|&j| m.numeros[j as usize]).collect()
}

// ------------------------------------------------------------------ sem ffmpeg

#[test]
fn compilar_e_deterministica_e_nao_toca_o_disco() {
    // tabelas sintéticas, arquivos que não existem e capacidades sem binário: compilar não pode precisar de nada disso
    let cap = Arc::new(apoio::cap_falsa());
    let tab = apoio::tabela((1, 15360), apoio::pts_regulares(18_000, (1, 30), (1, 15360), 0));
    let mut info = apoio::fonte(tab, (1920, 1080));
    info.caminho = PathBuf::from("/nao/existe/video.mp4");
    let mut foto = apoio::fonte(apoio::tabela((1, 1), vec![0]), (800, 600));
    foto.caminho = PathBuf::from("/nao/existe/foto.png");
    let mundo = Mundo::novo(&cap).com("m1", &info).com("foto", &foto);
    let t = &info.quadros;
    let ancoras: Vec<Value> = (0..500).map(|i| ancora(t.tempo(330 + i), Some(t.pts[330 + i]), [0.1 + 0.001 * i as f64, 0.3, 0.1, 0.2])).collect();
    let p = projeto(
        1920,
        1080,
        (30, 1),
        vec![midia("m1", "video", json!([ocultar("o1", "r1", "desfoque", json!({ "ativo": { "de": t.tempo(330), "ate": t.tempo(829) } }))])), midia("foto", "imagem", json!([]))],
        vec![
            trecho("c1", "m1", t, 330, 930, 0.5, json!([
                efeito("destaque", "d1", json!({ "forma": "retangulo", "rastreio": "r1", "traco": { "cor": "#00ff00", "espessura": 0.006, "contorno": true }, "tamanho": 0.1, "angulo_graus": 0.0, "margem": 0.0, "rotulo": null })),
                efeito("enquadramento", "e1", json!({ "regiao": [{ "t": t.tempo(330), "v": { "x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0 } }, { "t": t.tempo(930), "v": { "x": 0.25, "y": 0.25, "w": 0.5, "h": 0.5 } }], "ajuste": "conter", "interpolacao": "bicubica" })),
                efeito("carimbo", "k1", json!({ "mostra": "tempo_fonte", "formato": "hms_ms", "relogio": null, "lugar": { "x": 0.02, "y": 0.98, "h": "esq", "v": "base" },
                    "estilo": { "fonte": "mono", "negrito": false, "italico": false, "tamanho": 0.04, "cor": "#ffffff", "alinhamento": "esq", "caixa": null, "contorno": null } })),
            ])),
            json!({ "tipo": "congelar", "id": "c2", "rotulo": "", "midia": "m1", "instante_s": t.tempo(450), "instante_pts": t.pts[450], "duracao_s": 2.0,
                    "transicao_entrada": { "tipo": "escurecer", "duracao_s": 0.5, "audio": "acompanhar" } }),
            json!({ "tipo": "imagem", "id": "c3", "rotulo": "", "midia": "foto", "duracao_s": 1.5 }),
            json!({ "tipo": "titulo", "id": "c4", "rotulo": "", "duracao_s": 1.0, "titulo": { "modelo": "cartela", "fundo": { "tipo": "cor", "cor": "#000000" }, "blocos": [] } }),
        ],
        json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras)], "fecho": { "tipo": "escurecer", "duracao_s": 0.5, "audio": "acompanhar" } }),
    );
    for alvo in [alvo_final("cpu"), Alvo::Previa { altura: 540, de_q: 100, ate_q: 400 }, Alvo::Quadro { q: 777, largura: 960 }] {
        let a = mundo.compilar(&p, &alvo).expect("compila");
        let b = mundo.compilar(&p, &alvo).expect("compila de novo");
        assert_eq!(a, b);
        assert_eq!(serde_json::to_vec(&a).unwrap(), serde_json::to_vec(&b).unwrap());
        for (x, y) in a.blocos.iter().zip(&b.blocos) {
            assert_eq!(x.grafo.as_bytes(), y.grafo.as_bytes());
            assert_eq!(x.chave, y.chave);
            assert!(!x.grafo.contains("/nao/existe") && !x.grafo.contains('#'), "só nomes simples e nenhum comentário no grafo");
        }
        assert_eq!(a.alvo, alvo);
    }
    let f = mundo.compilar(&p, &alvo_final("cpu")).unwrap();
    assert_eq!((f.canvas.w, f.canvas.h, f.total_q), (1920, 1080, 1200 + 60 + 45 + 30));
    assert_eq!(f.blocos.len(), 1);
    let b = &f.blocos[0];
    assert_eq!((b.saida.as_str(), b.framehash.as_str()), ("b000.mp4", "b000.framehash"));
    // o trecho começa a 11 s: busca no quadro-chave anterior, com o refazer sem -ss previsto
    assert_eq!(b.entradas[0].antes[0], "-ss");
    assert_eq!(b.entradas[0].antes_sem_busca, Some(Vec::new()));
    assert!(matches!(&b.entradas[0].papel, Papel::Midia { id, clipe } if id == "m1" && clipe == "c1"));
    // conferência: rótulo do fps@, lidos e os pts esperados (só na exportação final)
    assert_eq!((b.conferir[0].rotulo.as_str(), b.conferir[0].lidos, b.conferir[0].pts.len()), ("c000", 600, 600));
    assert_eq!(b.conferir[0].pts, t.pts[330..930].to_vec());
    assert_eq!((b.conferir[1].rotulo.as_str(), b.conferir[1].lidos, &b.conferir[1].pts), ("c001", 1, &vec![t.pts[450]]));
    assert!(b.grafo.contains("fps@c000=30/1:start_time=0") && b.grafo.contains("showinfo@c000=checksum=0") && b.grafo.contains("settb=1/"));
    // ocultar é o primeiro efeito depois da cor; depois dele o clone e o mapa
    let (cor, alpha, mapa, persp) = (b.grafo.find("setparams=").unwrap(), b.grafo.find("alphamerge").unwrap(), b.grafo.find("setpts=expr=ceil").unwrap(), b.grafo.find("perspective=").unwrap());
    assert!(cor < alpha && alpha < mapa && mapa < persp);
    // camadas pedidas, nunca desenhadas: máscara por quadro lido (2 clipes da mesma mídia), anotação, foto, título
    let tipos: Vec<(TipoCamada, u32)> = f.camadas.iter().map(|c| (c.tipo, c.n)).collect();
    assert_eq!(tipos, vec![(TipoCamada::MascaraFonte, 600), (TipoCamada::Anotacao, 1200), (TipoCamada::MascaraFonte, 1), (TipoCamada::Anotacao, 1), (TipoCamada::Anotacao, 1)]);
    for c in &f.camadas {
        assert_eq!(c.hash.len(), 64);
        assert!(c.caixa_px.iter().all(|v| v % 2 == 0), "{:?}", c.caixa_px);
        assert!(b.entradas.iter().any(|e| matches!(&e.papel, Papel::Camada { id } if id == &c.id)), "camada {} sem entrada", c.id);
    }
    // máscara no espaço da fonte exibida; 500 âncoras, uma por quadro
    assert!(f.camadas[0].caixa_px[2] <= 1920 && f.camadas[0].itens[0].quadros[0].quadro.canvas.w == 1920);
    // prévia: canvas reduzido, sem framehash e sem pts a conferir; quadro: BMP e junção direta
    let pv = mundo.compilar(&p, &Alvo::Previa { altura: 540, de_q: 100, ate_q: 400 }).unwrap();
    assert_eq!((pv.canvas.w, pv.canvas.h, pv.total_q, pv.blocos[0].de_q, pv.blocos[0].n_q), (960, 540, 300, 100, 300));
    assert!(pv.blocos[0].framehash.is_empty() && pv.blocos[0].conferir.iter().all(|c| c.pts.is_empty()));
    assert!(!pv.blocos[0].grafo.contains("showinfo"));
    let q = mundo.compilar(&p, &Alvo::Quadro { q: 777, largura: 960 }).unwrap();
    assert_eq!((q.blocos[0].saida.as_str(), q.total_q, &q.juncao), ("b000.bmp", 1, &Juncao::Direta));
    // a janela lê só o quadro do slot: 777 a 0,5× = quadro 330 + 388
    assert_eq!(q.blocos[0].conferir[0].lidos, 1);
    assert!(q.blocos[0].grafo.contains(&format!("trim=start_pts={}:end_pts={}", t.pts[718], t.pts[719])), "{}", &q.blocos[0].grafo[..200]);
    // chave do bloco: muda com o grafo e com a mídia
    let mut outra = info.clone();
    outra.sha256 = "1".repeat(64);
    let g = Mundo::novo(&cap).com("m1", &outra).com("foto", &foto).compilar(&p, &alvo_final("cpu")).unwrap();
    assert_eq!(g.blocos[0].grafo, b.grafo);
    assert_ne!(g.blocos[0].chave, b.chave);
    // proveniência completa: mídias, roteiro com o mapa, ocultação, rastreio, ffmpeg
    let pr = &f.proveniencia;
    assert_eq!(pr.midias.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["m1", "foto"]);
    assert_eq!(pr.roteiro.iter().map(|i| (i.tipo.as_str(), i.na_saida.de_q, i.na_saida.n_q)).collect::<Vec<_>>(), vec![("trecho", 0, 1200), ("congelar", 1200, 60), ("imagem", 1260, 45), ("titulo", 1305, 30)]);
    let o = pr.roteiro[0].origem.as_ref().unwrap();
    assert_eq!((o.lidos, o.repetidos, o.descartados, o.j_in, o.quadro_do_slot.len(), o.pts.len()), (600, 600, 0, 330, 1200, 600));
    assert!(o.mapa.c > 0 && o.mapa.kd > 0);
    assert_eq!(pr.roteiro[0].processamentos.iter().map(|x| x.nome.as_str()).collect::<Vec<_>>(), vec!["destaque", "enquadramento", "carimbo"]);
    assert_eq!(pr.ocultacoes.len(), 1);
    let oc = &pr.ocultacoes[0];
    assert_eq!((oc.midia.as_str(), &oc.clipes, oc.quadros.ancora, oc.quadros.interpolado), ("m1", &vec!["c1".to_string(), "c2".to_string()], 501, 0));
    assert_eq!((pr.rastreios.len(), pr.rastreios[0].ancoras), (1, 500));
    assert_eq!((pr.saida.n_quadros, pr.saida.com_perdas, pr.ffmpeg.sha256.as_str()), (1335, true, cap.sha256.as_str()));
    let texto = serde_json::to_string(pr).unwrap();
    assert!(!texto.contains("/nao/existe"), "caminho absoluto na proveniência");
    assert!((pr.midias[0].usado_total_s - 20.0).abs() < 1e-6, "{:?}", pr.midias[0].usado);
}

#[test]
fn item_de_outra_fase_devolve_nao_suportado_com_o_id() {
    let cap = Arc::new(apoio::cap_falsa());
    let info = apoio::fonte(apoio::tabela((1, 30), (0..300).collect()), (320, 180));
    let mundo = Mundo::novo(&cap).com("m1", &info);
    let t = &info.quadros;
    let casos: Vec<(&str, Vec<Value>, Value)> = vec![
        ("efeito lupa L9", vec![trecho("c1", "m1", t, 0, 30, 1.0, json!([efeito("lupa", "L9", json!({ "rastreio": "r1", "janela": 0.1, "fator": 2.0, "forma": "circulo", "interpolacao": "bilinear", "onde": { "modo": "fixa", "x": 0.8, "y": 0.2 }, "moldura": { "cor": "#ffffff", "espessura": 0.004, "contorno": false }, "ligacao": false }))]))], json!({})),
        ("efeito nitidez N1", vec![trecho("c1", "m1", t, 0, 30, 1.0, json!([efeito("nitidez", "N1", json!({ "quantidade": 0.5 }))]))], json!({})),
        ("efeito do_futuro F7", vec![trecho("c1", "m1", t, 0, 30, 1.0, json!([{ "tipo": "do_futuro", "id": "F7", "ligado": true }]))], json!({})),
        ("clipe holograma H1", vec![trecho("c1", "m1", t, 0, 30, 1.0, json!([])), json!({ "tipo": "holograma", "id": "H1" })], json!({})),
        ("efeito ocultar O4 em clipe sem fonte", vec![json!({ "tipo": "titulo", "id": "c1", "duracao_s": 1.0, "titulo": { "modelo": "livre", "fundo": { "tipo": "transparente" }, "blocos": [] }, "efeitos": [ocultar("O4", "r1", "solido", json!({}))] })], json!({})),
        (
            "sobreposto video V3",
            vec![trecho("c1", "m1", t, 0, 30, 1.0, json!([]))],
            json!({ "sobreposicoes": [{ "id": "t1", "nome": "", "oculta": false, "clipes": [{ "tipo": "video", "id": "V3", "rotulo": "", "pos": { "ref": "programa", "t": 0.0 }, "duracao_s": 0.5,
                "transformacao": { "x": 0.5, "y": 0.5, "escala": 0.3, "rotacao": 0.0, "opacidade": 1.0 }, "entrada_s": 0.0, "saida_s": 0.0, "efeitos": [],
                "midia": "m1", "inicio_s": 0.0, "inicio_pts": 0, "velocidade": { "tipo": "constante", "fator": 1.0 }, "audio": null, "moldura": null, "sincronia": null }] }] }),
        ),
    ];
    for (esperado, principal, extra) in casos {
        let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], principal, extra);
        let e = mundo.compilar(&p, &alvo_final("cpu")).expect_err(esperado).to_string();
        assert!(e.contains("nao_suportado") && e.contains(esperado), "{esperado}: {e}");
    }
    // efeito desligado de outra fase não impede
    let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", t, 0, 30, 1.0, json!([{ "tipo": "nitidez", "id": "N1", "ligado": false, "nome": "", "ativo": null, "quantidade": 0.5 }]))], json!({}));
    assert!(mundo.compilar(&p, &alvo_final("cpu")).is_ok());
    // predefinição de outra fase
    let wa: Alvo = serde_json::from_value(json!({ "tipo": "final", "exportacao": { "predefinicao": "whatsapp", "codificador": "auto", "legendas": "nenhuma", "pacote": null, "pdf": "a3", "revisado_por": "", "personalizada": null } })).unwrap();
    assert!(mundo.compilar(&p, &wa).unwrap_err().to_string().contains("nao_suportado: predefinicao whatsapp"));
    // ocultar ativo sem caixa: nunca sai quadro descoberto calado
    let r = rastreio("r1", Some("m1"), "fonte", vec![ancora(t.tempo(10), Some(10), [0.1, 0.1, 0.2, 0.2])]);
    let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([ocultar("o1", "r1", "solido", json!({}))]))], vec![trecho("c1", "m1", t, 0, 30, 1.0, json!([]))], json!({ "rastreios": [r] }));
    assert!(mundo.compilar(&p, &alvo_final("cpu")).unwrap_err().to_string().contains("sem_cobertura: o1"));
}

// ------------------------------------------------------------------ com ffmpeg

/// [A2] E1 pelo compilador: trecho com `ocultar` sólido por quadro lido, a `v`× na taxa `fps`, inteiro ou em janela.
/// Devolve os slots conferidos.
fn caso_e1(cap: &Arc<Capacidades>, fonte: Fonte, v: f64, fps: (i64, i64), janela: Option<(u64, u64)>) -> std::result::Result<u64, String> {
    let m = apoio::medida(cap, fonte);
    let (j_in, j_out) = (40, 130);
    let rotulo = format!("{fonte:?} {v}× em {}/{} janela {janela:?} ({})", fps.0, fps.1, cap.versao);
    let p = projeto(
        320,
        180,
        fps,
        vec![midia("m1", "video", json!([ocultar("o1", "r1", "solido", json!({}))]))],
        vec![trecho("c1", "m1", &m.info.quadros, j_in, j_out, v, json!([]))],
        json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras_da_marca(fonte, &m, j_in, j_out))] }),
    );
    let alvo = match janela {
        Some((de, ate)) => Alvo::Previa { altura: 180, de_q: de, ate_q: ate },
        None => alvo_final("cpu"),
    };
    let plano = Mundo::novo(cap).com("m1", &m.info).compilar(&p, &alvo).map_err(|e| format!("{rotulo}: {e}"))?;
    let r = executor::rodar(cap, &plano, &pasta("e1"), &sem_perdas()).map_err(|e| format!("{rotulo}: {e}"))?;
    let todos = previstos(&plano, 0, &m);
    let esperado = match janela {
        Some((de, ate)) => todos[de as usize..ate as usize].to_vec(),
        None => todos,
    };
    let numeros = medir::quadros_de_origem(cap, &r.blocos[0]).map_err(|e| format!("{rotulo}: {e}"))?;
    if numeros != esperado {
        let q = numeros.iter().zip(&esperado).position(|(a, b)| a != b).unwrap_or(numeros.len().min(esperado.len()));
        return Err(format!("{rotulo}: {} slots × {} previstos; primeiro diferente {q}: {:?} × {:?}", numeros.len(), esperado.len(), numeros.get(q), esperado.get(q)));
    }
    let sobra = medir::quadros(cap, &r.blocos[0]).map_err(|e| format!("{rotulo}: {e}"))?.iter().map(vermelho).max().unwrap_or(0);
    if sobra != 0 {
        return Err(format!("{rotulo}: {sobra} px vermelhos de sobra"));
    }
    if janela.is_none() && r.compostos.len() != esperado.len() {
        return Err(format!("{rotulo}: {} hashes do composto", r.compostos.len()));
    }
    Ok(esperado.len() as u64)
}

fn rodar_e1(cap: &Arc<Capacidades>, fontes: &[Fonte]) {
    let casos: [(f64, (i64, i64), Option<(u64, u64)>); 6] =
        [(0.5, (30, 1), None), (1.0, (30, 1), None), (4.0, (30, 1), None), (0.5, (30000, 1001), None), (1.0, (25, 1), None), (0.5, (30, 1), Some((50, 80)))];
    let (mut slots, mut falhas) = (0, Vec::new());
    for &f in fontes {
        for (v, fps, janela) in casos {
            match caso_e1(cap, f, v, fps, janela) {
                Ok(n) => slots += n,
                Err(e) => falhas.push(e),
            }
        }
    }
    eprintln!("E1 pelo compilador ({}): {} casos, {slots} slots, {} falhas", cap.versao, fontes.len() * casos.len(), falhas.len());
    assert!(falhas.is_empty(), "{}", falhas.join("\n"));
    // controle: a fonte sem máscara tem o vermelho que o teste procura
    let m = apoio::medida(cap, fontes[0]);
    assert!(vermelho(&medir::quadro(cap, &m.info.caminho, 40).unwrap()) > 500);
}

#[test]
fn e1_pelo_compilador_mp4_e_mkv() {
    let Some(cap) = fontes::cap() else { return };
    rodar_e1(&cap, &[Fonte::Mp4, Fonte::Mkv30]);
}

// ------------------------------------------------------------------ medidores e apoio

/// Centro (px) pesado pela cor e o peso total. O peso é o quanto o canal passa dos outros dois: na borda ele cai
/// em proporção à cobertura, sobre fundo claro ou escuro, e o centro não é puxado para um lado (contar pixels por
/// limiar puxava 3 px).
fn centro_de(q: &QuadroRgb, peso: impl Fn(&[u8]) -> f64) -> Option<(f64, f64, u64)> {
    let (mut sx, mut sy, mut sp) = (0.0, 0.0, 0.0);
    for (i, p) in q.px.chunks_exact(3).enumerate() {
        let w = peso(p);
        if w > 0.0 {
            sx += w * ((i as u32 % q.w) as f64 + 0.5);
            sy += w * ((i as u32 / q.w) as f64 + 0.5);
            sp += w;
        }
    }
    (sp >= 255.0 * 50.0).then(|| (sx / sp, sy / sp, (sp / 255.0) as u64))
}

fn excesso(c: u8, a: u8, b: u8) -> f64 {
    let d = c as f64 - a.max(b) as f64;
    if d > 40.0 {
        d
    } else {
        0.0
    }
}

/// Marca vermelha (o `vermelho` conta sobra de ocultação; este acha o lugar).
fn eh_vermelho(p: &[u8]) -> f64 {
    excesso(p[0], p[1], p[2])
}

/// Miolo do traço verde do destaque.
fn eh_verde(p: &[u8]) -> bool {
    p[1] > 170 && p[0] < 100 && p[2] < 100
}

/// Centro (px) da caixa envolvente dos pixels que passam no teste, e quantos são. Para o contorno do destaque:
/// um lado sobre fundo claro pesa mais que o outro sobre fundo escuro (o croma 4:2:0 borra diferente) e a média
/// pesada anda; a caixa do miolo não.
fn centro_da_caixa(q: &QuadroRgb, f: impl Fn(&[u8]) -> bool) -> Option<(f64, f64, u64)> {
    let (mut x0, mut y0, mut x1, mut y1, mut n) = (u32::MAX, u32::MAX, 0u32, 0u32, 0u64);
    for (i, p) in q.px.chunks_exact(3).enumerate() {
        if f(p) {
            let (x, y) = (i as u32 % q.w, i as u32 / q.w);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            n += 1;
        }
    }
    (n > 0).then(|| ((x0 + x1 + 1) as f64 / 2.0, (y0 + y1 + 1) as f64 / 2.0, n))
}

/// Caixa prevista (px do canvas) de uma caixa da origem por `A(τ)`, e se ela cabe, com a margem do destaque,
/// na parte da imagem que aparece (no `conter` com barras e no `cobrir`, o retângulo de destino).
fn no_canvas(efeitos: &[crate::edicao::modelo::Tolerante<crate::edicao::modelo::Efeito>], tau: f64, c: [f64; 4], origem: (u32, u32), canvas: (u32, u32)) -> ([f64; 4], bool) {
    use crate::edicao::modelo::{Caixa, TamanhoPx};
    let (o, k) = (TamanhoPx { w: origem.0, h: origem.1 }, TamanhoPx { w: canvas.0, h: canvas.1 });
    let a = crate::edicao::geometria::a_de(efeitos, tau, o, k);
    let cc = crate::edicao::geometria::caixa_no_canvas(&a, Caixa { x: c[0], y: c[1], w: c[2], h: c[3] });
    let (px, py, pw, ph) = (cc.x * k.w as f64, cc.y * k.h as f64, cc.w * k.w as f64, cc.h * k.h as f64);
    let d = crate::edicao::motor::efeitos::enquadramento::enquadre(efeitos, tau, o, k).destino;
    let folga = MARGEM_DESTAQUE * pw.max(ph) + 0.03 * k.h as f64 + 4.0;
    let cabe = px >= d[0] + folga && py >= d[1] + folga && px + pw <= d[0] + d[2] - folga && py + ph <= d[1] + d[3] - folga;
    ([px, py, pw, ph], cabe)
}

fn efeitos_do_clipe(p: &ProjetoEdicao, n: usize) -> &[crate::edicao::modelo::Tolerante<crate::edicao::modelo::Efeito>] {
    match &p.principal[n] {
        crate::edicao::modelo::Tolerante::Conhecido(crate::edicao::modelo::ClipePrincipal::Trecho(t)) => &t.base.efeitos,
        _ => panic!("trecho"),
    }
}

fn pixel(q: &QuadroRgb, x: u32, y: u32) -> [u8; 3] {
    let i = (y * q.w + x) as usize * 3;
    [q.px[i], q.px[i + 1], q.px[i + 2]]
}

/// Margem do destaque dos testes (fração do lado da caixa): o traço fica fora da marca.
const MARGEM_DESTAQUE: f64 = 0.4;

fn destaque(id: &str, rastreio: &str) -> Value {
    efeito("destaque", id, json!({ "forma": "retangulo", "rastreio": rastreio, "traco": { "cor": "#00ff00", "espessura": 0.02, "contorno": false },
        "tamanho": 0.1, "angulo_graus": 0.0, "margem": MARGEM_DESTAQUE, "rotulo": null }))
}

fn enquadramento(regiao: Value, ajuste: &str) -> Value {
    efeito("enquadramento", "enq", json!({ "regiao": regiao, "ajuste": ajuste, "interpolacao": "bicubica" }))
}

fn caixa(c: [f64; 4]) -> Value {
    json!({ "x": c[0], "y": c[1], "w": c[2], "h": c[3] })
}

/// Roda sem perdas e devolve os quadros de todos os blocos, na ordem.
fn renderizar(cap: &Arc<Capacidades>, plano: &Plano, nome: &str) -> (executor::Rodado, Vec<QuadroRgb>) {
    let r = executor::rodar(cap, plano, &pasta(nome), &sem_perdas()).unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
    let mut q = Vec::new();
    for b in &r.blocos {
        q.extend(medir::quadros(cap, b).unwrap_or_else(|e| panic!("{nome}: {e}")));
    }
    assert_eq!(q.len() as u64, plano.total_q, "{nome}: quadros × slots");
    // depuração: SICRO_GUARDAR_QUADROS=<pasta> grava alguns quadros em PNG
    if let Ok(d) = std::env::var("SICRO_GUARDAR_QUADROS") {
        for i in [0, q.len() / 2, q.len() - 1] {
            let _ = image::save_buffer(Path::new(&d).join(format!("{nome}-{i}.png")), &q[i].px, q[i].w, q[i].h, image::ExtendedColorType::Rgb8);
        }
        let _ = std::fs::write(Path::new(&d).join(format!("{nome}.txt")), &plano.blocos[0].grafo);
    }
    (r, q)
}

/// `FonteInfo` de um arquivo qualquer, com a tabela do decodificador.
fn fonte_de(cap: &Capacidades, video: &Path) -> FonteInfo {
    let tab = TabelaQuadros::do_decodificador(cap, video, &std::sync::atomic::AtomicBool::new(false), &mut |_| {}).expect("tabela do decodificador");
    crate::edicao::quadros::fonte_info(cap, video, &"0".repeat(64), Arc::new(tab), None).expect("fonte_info")
}

/// Gera um vídeo de teste com o ffmpeg do sistema de testes (uma linha `lavfi`) e devolve o caminho.
fn gerar(cap: &Capacidades, pasta: &Path, nome: &str, lavfi: &str, codificar: &[&str]) -> PathBuf {
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-f", "lavfi", "-i", lavfi].map(String::from).into();
    a.extend(codificar.iter().map(|s| s.to_string()));
    a.push(nome.to_string());
    let cmd = crate::edicao::ffmpeg::exec::Comando { nome: "gerar".into(), programa: cap.binario.clone(), args: a, pasta: pasta.to_path_buf(), ambiente: Vec::new(), sem_ambiente: Vec::new(), stderr: pasta.join("gerar.stderr") };
    let fim = crate::edicao::ffmpeg::exec::rodar(&cmd, &std::sync::atomic::AtomicBool::new(false)).expect("ffmpeg");
    crate::edicao::ffmpeg::exec::exigir_sucesso(&cmd, &fim).expect("fonte de teste");
    pasta.join(nome)
}

/// O outro ffmpeg da máquina (o do sistema ou o 7.0.2 do AppImage); com `SICRO_EXIGE_FFMPEG=1` a falta dele é falha.
fn outro_ffmpeg(cap: &Capacidades) -> Option<Arc<Capacidades>> {
    let candidatos = [Path::new(env!("CARGO_MANIFEST_DIR")).join("ffmpeg-linux/ffmpeg"), PathBuf::from("/usr/bin/ffmpeg")];
    let mesmo = |p: &Path| std::fs::canonicalize(p).ok() == std::fs::canonicalize(&cap.binario).ok();
    let achado = candidatos.iter().find(|p| p.exists() && !mesmo(p)).and_then(|p| crate::edicao::ffmpeg::sonda::capacidades_de(p, crate::edicao::ffmpeg::sonda::Origem::Configurado, None, false).ok());
    let achado = achado.filter(|c| c.faltam.is_empty() && c.sha256 != cap.sha256);
    if achado.is_none() {
        assert!(!std::env::var("SICRO_EXIGE_FFMPEG").is_ok_and(|v| v == "1"), "SICRO_EXIGE_FFMPEG=1 e o segundo ffmpeg não foi achado");
        eprintln!("aviso: teste dispensado, sem o segundo ffmpeg");
    }
    achado
}

// ------------------------------------------------------------------ matriz de fontes e busca

/// Um trecho pela cadeia inteira do compilador, no canvas do tamanho da fonte: número desenhado em cada slot = previsto.
fn conferir_trecho(cap: &Arc<Capacidades>, nome: &str, m: &Medida, j_in: usize, j_out: usize, v: f64, fps: (i64, i64), janela: Option<(u64, u64)>, com_busca: Option<bool>) -> std::result::Result<(u64, bool), String> {
    let rotulo = format!("{nome} [{j_in}, {j_out}) a {v}× em {}/{} janela {janela:?} ({})", fps.0, fps.1, cap.versao);
    let (w, h) = m.info.exibido;
    let p = projeto(w, h, fps, vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", &m.info.quadros, j_in, j_out, v, json!([]))], json!({}));
    let alvo = match janela {
        Some((de, ate)) => Alvo::Previa { altura: h, de_q: de, ate_q: ate },
        None => alvo_final("cpu"),
    };
    let plano = Mundo::novo(cap).com("m1", &m.info).compilar(&p, &alvo).map_err(|e| format!("{rotulo}: {e}"))?;
    let buscou = !plano.blocos[0].entradas[0].antes.is_empty();
    if com_busca.is_some_and(|b| b != buscou) {
        return Err(format!("{rotulo}: busca {buscou}, esperada {com_busca:?}"));
    }
    let r = executor::rodar(cap, &plano, &pasta("matriz"), &sem_perdas()).map_err(|e| format!("{rotulo}: {e}"))?;
    let todos = previstos(&plano, 0, m);
    let esperado = match janela {
        Some((de, ate)) => todos[de as usize..(ate as usize).min(todos.len())].to_vec(),
        None => todos,
    };
    let numeros = match &m.por_conteudo {
        None => medir::quadros_de_origem(cap, &r.blocos[0]).map_err(|e| format!("{rotulo}: {e}"))?,
        Some(tabela) => {
            // gravador real: identidade pelo conteúdo do quadro reduzido
            let a: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error", "-i", &r.blocos[0].to_string_lossy(), "-vf", &format!("{},extractplanes=y", apoio::REDUZIDO.0), "-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "gray", "-"].map(String::from).into();
            let pasta = r.blocos[0].parent().unwrap().to_path_buf();
            let cmd = crate::edicao::ffmpeg::exec::Comando { nome: "reduzir".into(), programa: cap.binario.clone(), args: a, pasta: pasta.clone(), ambiente: Vec::new(), sem_ambiente: Vec::new(), stderr: pasta.join("reduzir.stderr") };
            let (_, out) = crate::edicao::ffmpeg::exec::rodar_com_saida(&cmd, &std::sync::atomic::AtomicBool::new(false)).map_err(|e| format!("{rotulo}: {e}"))?;
            out.chunks_exact(apoio::REDUZIDO.1 * apoio::REDUZIDO.2).map(|y| tabela.get(&apoio::conteudo(y)).copied().unwrap_or(medir::NAO_IDENTIFICADO)).collect()
        }
    };
    let _ = std::fs::remove_dir_all(r.blocos[0].parent().unwrap());
    if numeros != esperado {
        let q = numeros.iter().zip(&esperado).position(|(a, b)| a != b).unwrap_or(numeros.len().min(esperado.len()));
        return Err(format!("{rotulo}: {} slots × {} previstos; primeiro diferente {q}: {:?} × {:?}", numeros.len(), esperado.len(), numeros.get(q), esperado.get(q)));
    }
    Ok((esperado.len() as u64, buscou))
}

#[derive(Default)]
struct Placar {
    casos: usize,
    slots: u64,
    com_busca: usize,
    falhas: Vec<String>,
}

impl Placar {
    fn soma(&mut self, r: std::result::Result<(u64, bool), String>) {
        self.casos += 1;
        match r {
            Ok((n, b)) => {
                self.slots += n;
                self.com_busca += usize::from(b);
            }
            Err(e) => self.falhas.push(e),
        }
    }
    fn fechar(self, nome: &str, cap: &Capacidades) {
        eprintln!("{nome} ({}): {} casos, {} slots conferidos, {} com busca (-ss), {} falhas", cap.versao, self.casos, self.slots, self.com_busca, self.falhas.len());
        assert!(self.falhas.is_empty(), "{}", self.falhas.join("\n"));
    }
}

/// Casos de uma fonte: 1×, câmera lenta em 29,97, 4× em 25, trecho até o fim da mídia e janela no meio.
fn casos_da_fonte(cap: &Arc<Capacidades>, placar: &mut Placar, f: Fonte, completo: bool) {
    let m = apoio::medida(cap, f);
    let n = m.limite.min(m.info.quadros.pts.len());
    let d = fontes::descricao(f);
    let nome = format!("{f:?}");
    placar.soma(conferir_trecho(cap, &nome, &m, 20, 80, 1.0, d.fps, None, None));
    placar.soma(conferir_trecho(cap, &nome, &m, n - 40, n, 1.0, (30, 1), None, None));
    if completo {
        placar.soma(conferir_trecho(cap, &nome, &m, 31, 91, 0.5, (30000, 1001), None, None));
        placar.soma(conferir_trecho(cap, &nome, &m, 10, 110, 4.0, (25, 1), None, None));
        placar.soma(conferir_trecho(cap, &nome, &m, 5, 125, 0.25, (60, 1), Some((101, 131)), None));
        // janela no fim do arquivo inteiro (os slots saem do mapa: a 59,94 qps a fonte tem o dobro de quadros)
        let fim = tempo::mapa_trecho(&m.info.quadros, 0, n, &crate::edicao::modelo::Velocidade::Constante { fator: 1.0 }, crate::edicao::modelo::Racional { num: 30, den: 1 }).map(|x| x.n_q as u64).unwrap_or(40);
        placar.soma(conferir_trecho(cap, &nome, &m, 0, n, 1.0, (30, 1), Some((fim - 31, fim - 1)), None));
    }
}

#[test]
fn matriz_rapida_quadro_de_origem_por_slot() {
    let Some(cap) = fontes::cap() else { return };
    let mut p = Placar::default();
    for f in [Fonte::Mp4, Fonte::Mkv30, Fonte::Avi25, Fonte::TaxaVariavel, Fonte::FaixaCheia, Fonte::PixelNaoQuadrado] {
        casos_da_fonte(&cap, &mut p, f, false);
    }
    p.fechar("matriz rápida pelo compilador", &cap);
}

#[test]
#[ignore = "completo: as 17 fontes da matriz"]
fn matriz_completa_quadro_de_origem_por_slot() {
    let Some(cap) = fontes::cap() else { return };
    let mut p = Placar::default();
    for &f in fontes::MATRIZ {
        // a entrelaçada sem `desentrelacar` passa como está; o giro de 90° chega aplicado pelo contêiner
        casos_da_fonte(&cap, &mut p, f, true);
    }
    p.fechar("matriz completa pelo compilador", &cap);
}

/// Casos de busca: trecho que começa 2 quadros antes de um quadro-chave (depois dos 10 s de guarda) e trecho
/// dentro dos 3 primeiros segundos.
fn casos_de_busca(cap: &Arc<Capacidades>, placar: &mut Placar, nome: &str, m: &Medida) {
    let tab = &m.info.quadros;
    let n = m.limite.min(tab.pts.len());
    // segundo quadro-chave com o alvo da busca além da guarda: 2 quadros antes dele a busca cai no anterior
    let chave = (2..n).filter(|&j| tab.chave[j] && tab.tempo(j) - m.info.inicio_s - 0.5 >= 10.0).nth(1);
    if let Some(k) = chave.filter(|&k| k + 20 <= n) {
        let espera = tab.busca_em(m.info.inicio_s, k - 2).is_some();
        placar.soma(conferir_trecho(cap, nome, m, k - 2, (k + 30).min(n), 1.0, (30, 1), None, Some(espera)));
        placar.soma(conferir_trecho(cap, nome, m, k, (k + 30).min(n), 0.5, (30, 1), None, None));
    }
    let tres = (0..n).take_while(|&j| tab.tempo(j) - tab.tempo(0) < 3.0).count();
    if tres > 12 {
        placar.soma(conferir_trecho(cap, nome, m, 3, tres - 2, 1.0, (30, 1), None, Some(false)));
        placar.soma(conferir_trecho(cap, nome, m, 0, tres.min(40), 1.0, (25, 1), Some((2, 12)), Some(false)));
    }
}

#[test]
fn busca_antes_do_quadro_chave_e_no_inicio_em_ts_e_ps() {
    let Some(cap) = fontes::cap() else { return };
    let mut p = Placar::default();
    for f in [Fonte::Ts5994, Fonte::TsMeioDoGop, Fonte::MpegPs, Fonte::Mp4QuadrosB] {
        casos_de_busca(&cap, &mut p, &format!("{f:?}"), &apoio::medida(&cap, f));
    }
    assert!(p.com_busca >= 3, "nenhum caso usou -ss: {}", p.com_busca);
    p.fechar("busca em TS e PS", &cap);
}

#[test]
#[ignore = "completo: amostras reais de gravador, quando presentes"]
fn busca_em_amostras_de_gravador() {
    let Some(cap) = fontes::cap() else { return };
    let amostras = fontes::amostras_de_gravador();
    if amostras.is_empty() {
        eprintln!("dispensado: nenhuma amostra de gravador presente");
        return;
    }
    let mut p = Placar::default();
    let mut lidas = 0;
    for a in &amostras {
        let nome = a.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match apoio::medir_por_conteudo(&cap, a) {
            Ok(m) => {
                lidas += 1;
                casos_de_busca(&cap, &mut p, &nome, &m);
            }
            Err(e) => eprintln!("amostra {nome} fora: {e}"),
        }
    }
    eprintln!("amostras de gravador: {lidas} de {} lidas", amostras.len());
    p.fechar("busca em gravadores", &cap);
}

#[test]
fn busca_errada_injetada_faz_o_bloco_falhar_na_conferencia() {
    let Some(cap) = fontes::cap() else { return };
    let m = apoio::medida(&cap, Fonte::Mp4);
    let tab = &m.info.quadros;
    let (j_in, j_out) = (362, 422);
    let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", tab, j_in, j_out, 1.0, json!([]))], json!({}));
    let mundo = Mundo::novo(&cap).com("m1", &m.info);
    for alvo in [alvo_final("cpu"), Alvo::Previa { altura: 180, de_q: 0, ate_q: 60 }] {
        let plano = mundo.compilar(&p, &alvo).unwrap();
        assert_eq!(plano.blocos[0].entradas[0].antes[0], "-ss", "o caso tem de usar busca");
        // certo como está
        executor::rodar(&cap, &plano, &pasta("busca-certa"), &sem_perdas()).expect("a busca da regra lê o trecho inteiro");
        // busca 1,5 s adiante: o decodificador entrega do quadro-chave seguinte, com código 0
        let mut errado = plano.clone();
        let ss: f64 = errado.blocos[0].entradas[0].antes[1].parse().unwrap();
        errado.blocos[0].entradas[0].antes[1] = format!("{:.6}", ss + 1.5);
        let e = executor::rodar(&cap, &errado, &pasta("busca-errada"), &sem_perdas()).expect_err("a leitura divergente tem de falhar").to_string();
        assert!(e.contains("leitura_divergente"), "{e}");
        // refeito sem -ss: passa e mostra os quadros certos
        let r = executor::rodar(&cap, &errado, &pasta("busca-refeita"), &Opcoes { sem_perdas: true, sem_busca: true, ..Opcoes::default() }).expect("sem -ss lê certo");
        assert_eq!(medir::quadros_de_origem(&cap, &r.blocos[0]).unwrap(), previstos(&plano, 0, &m));
    }
    // só a lista de pts acusa quando a contagem fecha: o trecho certo com os pts esperados de outro
    let mut plano = mundo.compilar(&p, &alvo_final("cpu")).unwrap();
    plano.blocos[0].conferir[0].pts[7] += 1;
    assert!(executor::rodar(&cap, &plano, &pasta("pts-errado"), &sem_perdas()).unwrap_err().to_string().contains("leitura_divergente"));
}

// ------------------------------------------------------------------ ocultação

#[test]
fn ocultar_com_giro_de_90_sem_pixel_de_sobra() {
    let Some(cap) = fontes::cap() else { return };
    // giro do contêiner (a fonte chega em pé) e mais um `girar` de 90° com espelho no clipe: a máscara anda no espaço original
    let f = Fonte::Giro90;
    let m = apoio::medida(&cap, f);
    let (j_in, j_out) = (30, 120);
    let girar = efeito("girar", "g1", json!({ "graus": 90, "espelhar_h": true, "espelhar_v": false }));
    let montar = |ocultacoes: Value, efeitos: Value, v: f64| {
        projeto(320, 180, (30, 1), vec![midia("m1", "video", ocultacoes)], vec![trecho("c1", "m1", &m.info.quadros, j_in, j_out, v, efeitos)], json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras_da_marca(f, &m, j_in, j_out))] }))
    };
    let mundo = Mundo::novo(&cap).com("m1", &m.info);
    // controle: sem ocultar o vermelho está lá, girado
    let (_, q) = renderizar(&cap, &mundo.compilar(&montar(json!([]), json!([girar.clone()]), 1.0), &alvo_final("cpu")).unwrap(), "giro-controle");
    assert!(q.iter().all(|x| vermelho(x) > 300), "controle sem vermelho");
    assert_eq!((q[0].w, q[0].h), (320, 180));
    for (ocultacoes, efeitos, v) in [
        (json!([ocultar("o1", "r1", "solido", json!({}))]), json!([girar.clone()]), 1.0),
        (json!([ocultar("o1", "r1", "solido", json!({}))]), json!([girar.clone()]), 0.5),
        // a do clipe (O5), no mesmo ponto da cadeia
        (json!([]), json!([girar.clone(), ocultar("o2", "r1", "solido", json!({ "cor": "#0000ff" }))]), 1.0),
    ] {
        let plano = mundo.compilar(&montar(ocultacoes, efeitos, v), &alvo_final("cpu")).unwrap();
        let (_, q) = renderizar(&cap, &plano, "giro-ocultar");
        let sobra: u64 = q.iter().map(vermelho).max().unwrap();
        assert_eq!(sobra, 0, "{sobra} px de sobra a {v}× ({})", cap.versao);
    }
}

#[test]
fn ocultar_do_clipe_no_congelado_usa_o_ativo_local() {
    let Some(cap) = fontes::cap() else { return };
    let f = Fonte::Mp4;
    let m = apoio::medida(&cap, f);
    let t = &m.info.quadros;
    let j = 100;
    let r = rastreio("r1", Some("m1"), "fonte", ancoras_da_marca(f, &m, j, j + 1));
    let mundo = Mundo::novo(&cap).com("m1", &m.info);
    let montar = |ativo: Value| {
        let oc = ocultar("o1", "r1", "solido", json!({ "ativo": ativo }));
        let c = json!({ "tipo": "congelar", "id": "c1", "rotulo": "", "midia": "m1", "instante_s": t.tempo(j), "instante_pts": t.pts[j], "duracao_s": 1.0, "efeitos": [oc] });
        projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], vec![c], json!({ "rastreios": [r.clone()] }))
    };
    // `ativo` local que pega o congelado inteiro: vale (lido em tempo de fonte, 3,3 s, saía descoberto)
    for ativo in [json!(null), json!({ "de": 0.0, "ate": 1.0 }), json!({ "de": null, "ate": 5.0 })] {
        let plano = mundo.compilar(&montar(ativo.clone()), &alvo_final("cpu")).unwrap();
        let (_, q) = renderizar(&cap, &plano, "congelado-oculto");
        assert_eq!(q.iter().map(vermelho).max(), Some(0), "ativo {ativo} ({})", cap.versao);
        assert_eq!(plano.proveniencia.ocultacoes.len(), 1, "ativo {ativo}");
    }
    // só parte do congelado: recusado com o id, nunca meio oculto
    let e = mundo.compilar(&montar(json!({ "de": 0.2, "ate": 0.6 })), &alvo_final("cpu")).unwrap_err().to_string();
    assert!(e.contains("nao_suportado") && e.contains("o1"), "{e}");
    // nenhum slot: não vale, e o relatório não conta ocultação
    let plano = mundo.compilar(&montar(json!({ "de": 3.0, "ate": 4.0 })), &alvo_final("cpu")).unwrap();
    assert!(plano.proveniencia.ocultacoes.is_empty());
}

#[test]
fn peca_de_um_slot_na_juncao_nao_perde_quadro() {
    let Some(cap) = fontes::cap() else { return };
    let m = apoio::medida(&cap, Fonte::Mp4);
    let t = &m.info.quadros;
    // o `concat` empurra a peça seguinte pela duração média da anterior: com 1 quadro ela caía em cima
    for (meio, numero) in [
        (json!({ "tipo": "congelar", "id": "c1", "rotulo": "", "midia": "m1", "instante_s": t.tempo(100), "instante_pts": t.pts[100], "duracao_s": 1.0 / 30.0 }), 100),
        (trecho("c1", "m1", t, 150, 151, 1.0, json!([])), 150),
        (trecho("c1", "m1", t, 150, 154, 4.0, json!([])), 150),
    ] {
        let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c0", "m1", t, 40, 70, 1.0, json!([])), meio.clone(), trecho("c2", "m1", t, 200, 230, 1.0, json!([]))], json!({}));
        let plano = Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &alvo_final("cpu")).unwrap();
        assert_eq!(plano.total_q, 61);
        let r = executor::rodar(&cap, &plano, &pasta("um-slot"), &sem_perdas()).unwrap_or_else(|e| panic!("{meio} ({}): {e}", cap.versao));
        let mut esperado: Vec<u32> = (40..70).collect();
        esperado.push(numero);
        esperado.extend(200..230);
        assert_eq!(medir::quadros_de_origem(&cap, &r.blocos[0]).unwrap(), esperado, "{meio} ({})", cap.versao);
    }
}

#[test]
fn clipe_cortado_entre_blocos_junta_sem_pts_repetido() {
    let Some(cap) = fontes::cap() else { return };
    let dir = pasta("clipe-longo");
    // 5 min 30 s de 160×90: o primeiro bloco é uma peça só (o clipe cortado nos 5 min), o segundo junta o resto e outro clipe
    let video = gerar(&cap, &dir, "f.mp4", "testsrc2=s=160x90:r=30:d=330", &["-c:v", "libx264", "-preset", "ultrafast", "-g", "60", "-pix_fmt", "yuv420p"]);
    let info = fonte_de(&cap, &video);
    let m = apoio::medida(&cap, Fonte::Mp4);
    let p = projeto(160, 90, (30, 1), vec![midia("m1", "video", json!([])), midia("m2", "video", json!([]))],
        vec![trecho("c0", "m1", &info.quadros, 0, 9800, 1.0, json!([])), trecho("c1", "m2", &m.info.quadros, 40, 70, 1.0, json!([]))], json!({}));
    let plano = Mundo::novo(&cap).com("m1", &info).com("m2", &m.info).compilar(&p, &alvo_final("cpu")).unwrap();
    assert_eq!(plano.blocos.iter().map(|b| (b.de_q, b.n_q)).collect::<Vec<_>>(), vec![(0, 9000), (9000, 830)]);
    let r = executor::rodar(&cap, &plano, &dir.join("r"), &Opcoes { juntar: true, ..Opcoes::default() }).unwrap_or_else(|e| panic!("{}: {e}", cap.versao));
    // sem a duração do último quadro do bloco, o 7.0.2 punha o primeiro do bloco seguinte no mesmo pts (passo 0):
    // a contagem fecha e o vídeo dali em diante fica 1 quadro adiantado
    let pts = medir::pts(&cap, r.junto.as_ref().unwrap()).unwrap();
    assert_eq!(pts.len() as u64, plano.total_q);
    assert!(pts.windows(2).all(|w| w[1] - w[0] == 512), "passos do arquivo junto ({})", cap.versao);
    // o clipe curto do fim, no lugar dele
    let fim = medir::quadros_de_origem(&cap, r.junto.as_ref().unwrap()).unwrap();
    assert_eq!(&fim[9800..], &(40..70).collect::<Vec<u32>>()[..], "{}", cap.versao);
}

#[test]
fn desfoque_e_mosaico_mudam_so_a_regiao() {
    let Some(cap) = fontes::cap() else { return };
    let f = Fonte::Mp4;
    let m = apoio::medida(&cap, f);
    let (j_in, j_out) = (40, 70);
    let montar = |ocultacoes: Value| {
        projeto(320, 180, (30, 1), vec![midia("m1", "video", ocultacoes)], vec![trecho("c1", "m1", &m.info.quadros, j_in, j_out, 1.0, json!([]))], json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras_da_marca(f, &m, j_in, j_out))] }))
    };
    let mundo = Mundo::novo(&cap).com("m1", &m.info);
    let (_, limpo) = renderizar(&cap, &mundo.compilar(&montar(json!([])), &alvo_final("cpu")).unwrap(), "limpo");
    for (modo, extra) in [("desfoque", json!({})), ("mosaico", json!({})), ("desfoque", json!({ "intensidade": 0.4 })), ("mosaico", json!({ "intensidade": 0.3 }))] {
        // duas ocultações seguidas: a segunda (sólida, azul) num canto fixo
        let fixa = rastreio("r2", Some("m1"), "fonte", vec![ancora(m.info.quadros.tempo(j_in), Some(m.info.quadros.pts[j_in]), [0.05, 0.1, 0.1, 0.15])]);
        let mut p = montar(json!([ocultar("o1", "r1", modo, extra), ocultar("o2", "r2", "solido", json!({ "cor": "#0000ff", "alvo": { "rastreio": "r2", "forma": "retangulo", "vertices": null, "margem": 0.0, "borda_suave": 0.0, "inverter": false } }))]));
        p.rastreios.push(serde_json::from_value(fixa).unwrap());
        let plano = mundo.compilar(&p, &alvo_final("cpu")).unwrap();
        let (r, q) = renderizar(&cap, &plano, modo);
        assert_eq!(r.lidos[0].get("c000"), Some(&31));
        for (i, (a, b)) in limpo.iter().zip(&q).enumerate() {
            let c = fontes::marca_esperada(f, m.numeros[j_in + i]);
            let (x0, y0, x1, y1) = (c[0] * 320.0, c[1] * 180.0, (c[0] + c[2]) * 320.0, (c[1] + c[3]) * 180.0);
            // margem de 10 % + blocos 2×2 + 1 px de folga
            // a medida é em RGB: o croma ampliado espalha a mudança por mais 4 px além da máscara
            let fora_da_marca = |x: f64, y: f64| x < x0 - 12.0 || x > x1 + 12.0 || y < y0 - 12.0 || y > y1 + 12.0;
            let no_canto = |x: f64, y: f64| (10.0..54.0).contains(&x) && (12.0..51.0).contains(&y);
            let (mut mudou_fora, mut mudou_dentro, mut azul) = (0, 0, 0);
            for y in 0..180u32 {
                for x in 0..320u32 {
                    let (pa, pb) = (pixel(a, x, y), pixel(b, x, y));
                    let dif = pa.iter().zip(&pb).any(|(u, v)| u.abs_diff(*v) > 1);
                    let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
                    if no_canto(fx, fy) {
                        azul += u64::from(pb[2] as i32 - pb[0] as i32 > 150 && pb[2] as i32 - pb[1] as i32 > 150);
                    } else if fora_da_marca(fx, fy) {
                        mudou_fora += u64::from(dif);
                    } else {
                        mudou_dentro += u64::from(dif);
                    }
                }
            }
            assert_eq!(mudou_fora, 0, "{modo}: quadro {i} mudou fora da região ({})", cap.versao);
            assert!(mudou_dentro > 200, "{modo}: quadro {i} quase não mudou dentro ({mudou_dentro})");
            assert!(azul > 28 * 22, "{modo}: sólido azul do canto com {azul} px");
        }
    }
}

#[test]
fn holofote_escurece_e_dessatura_fora_do_alvo() {
    let Some(cap) = fontes::cap() else { return };
    let f = Fonte::Mp4;
    let m = apoio::medida(&cap, f);
    let (j_in, j_out) = (40, 70);
    let holofote = efeito("holofote", "h1", json!({ "alvo": { "rastreio": "r1", "forma": "retangulo", "vertices": null, "margem": 0.2, "borda_suave": 0.0, "inverter": false }, "escurecer": 0.5, "dessaturar": 1.0 }));
    let render = |efeitos: Value, nome: &str| {
        let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", &m.info.quadros, j_in, j_out, 1.0, efeitos)], json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras_da_marca(f, &m, j_in, j_out))] }));
        let plano = Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &alvo_final("cpu")).unwrap();
        let r = executor::rodar(&cap, &plano, &pasta(nome), &sem_perdas()).unwrap();
        quadros_709(&cap, &r.blocos[0], 320, 180)
    };
    let (limpo, com) = (render(json!([]), "holofote-limpo"), render(json!([holofote]), "holofote"));
    for (i, (a, b)) in limpo.iter().zip(&com).enumerate() {
        // dentro do alvo nada muda: a marca segue vermelha, inteira
        assert_eq!(vermelho(a), vermelho(b), "quadro {i}: a marca mudou");
        let c = fontes::marca_esperada(f, m.numeros[j_in + i]);
        let (cx, cy) = (((c[0] + c[2] / 2.0) * 320.0) as u32, ((c[1] + c[3] / 2.0) * 180.0) as u32);
        assert_eq!(pixel(a, cx, cy), pixel(b, cx, cy));
        // fora: metade da luz (em código de vídeo: 16 + (Y − 16)/2), sem cor
        let mut conferidos = 0;
        for (x, y) in [(8u32, 8u32), (100, 20), (200, 40), (310, 170), (160, 10)] {
            let (pa, pb) = (pixel(a, x, y), pixel(b, x, y));
            let esperado = |v: u8| ((v as f64) / 2.0).round();
            assert!(pb.iter().zip(&pa).all(|(u, v)| (*u as f64 - esperado(*v)).abs() <= 3.0), "quadro {i} em ({x}, {y}): {pa:?} → {pb:?}");
            assert!(pb[0].abs_diff(pb[1]) <= 2 && pb[1].abs_diff(pb[2]) <= 2, "quadro {i} em ({x}, {y}): com cor {pb:?}");
            conferidos += 1;
        }
        assert_eq!(conferidos, 5);
    }
}

// ------------------------------------------------------------------ anotações e enquadramento

#[test]
fn destaque_de_uma_ancora_fica_ate_o_ultimo_slot() {
    let Some(cap) = fontes::cap() else { return };
    let m = apoio::medida(&cap, Fonte::Mp4);
    let tab = &m.info.quadros;
    let (j_in, j_out) = (40, 130);
    // uma âncora só, no 11º quadro do trecho: antes dela nada; dali até o fim, a última caixa (rastreio manual)
    let r = rastreio("r1", Some("m1"), "fonte", vec![ancora(tab.tempo(j_in + 10), Some(tab.pts[j_in + 10]), [0.4, 0.2, 0.2, 0.3])]);
    for (v, fps) in [(1.0, (30, 1)), (0.5, (30000, 1001))] {
        let p = projeto(320, 180, fps, vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", tab, j_in, j_out, v, json!([destaque("d1", "r1")]))], json!({ "rastreios": [r.clone()] }));
        let plano = Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &alvo_final("cpu")).unwrap();
        let (_, q) = renderizar(&cap, &plano, "uma-ancora");
        let mapa = &plano.proveniencia.roteiro[0].origem.as_ref().unwrap().quadro_do_slot;
        for (s, x) in q.iter().enumerate() {
            let verde = centro_da_caixa(x, eh_verde);
            if (mapa[s] as usize) < j_in + 10 {
                assert!(verde.is_none(), "slot {s}: destaque antes da âncora");
            } else {
                let (cx, cy, n) = verde.unwrap_or_else(|| panic!("slot {s} de {}: destaque sumiu ({})", q.len(), cap.versao));
                assert!(n > 300 && (cx - 160.0).abs() <= 2.0 && (cy - 63.0).abs() <= 2.0, "slot {s}: centro ({cx:.1}, {cy:.1}), {n} px");
                // por cima do vídeo certo: o quadro do slot
                assert!(vermelho(x) > 300);
            }
        }
        assert!(centro_da_caixa(q.last().unwrap(), eh_verde).is_some());
    }
}

/// Maior distância (px) entre o centro do destaque (verde) e o da marca (vermelho) nos quadros renderizados.
/// `inteira(i)` = no quadro i a caixa do destaque cabe na imagem (na borda ela é limitada e o centro anda).
fn desvio_da_marcacao(q: &[QuadroRgb], inteira: impl Fn(usize) -> bool) -> (f64, usize) {
    let (mut pior, mut medidos) = (0.0f64, 0);
    for (i, x) in q.iter().enumerate() {
        if !inteira(i) {
            continue;
        }
        let (v, r) = (centro_da_caixa(x, eh_verde), centro_de(x, eh_vermelho));
        let (Some(v), Some(r)) = (v, r) else { panic!("quadro {i}: sem destaque ou sem marca ({:?}, {:?})", v.map(|a| a.2), r.map(|a| a.2)) };
        pior = pior.max((v.0 - r.0).abs()).max((v.1 - r.1).abs());
        medidos += 1;
    }
    (pior, medidos)
}

#[test]
fn marcacao_a_ate_2_px_em_4_3_vertical_pixel_nao_quadrado_e_giro() {
    let Some(cap) = fontes::cap() else { return };
    const LIMITE_PX: f64 = 2.0;
    let girar = efeito("girar", "g1", json!({ "graus": 90, "espelhar_h": true, "espelhar_v": false }));
    let casos: Vec<(&str, Fonte, (u32, u32), (usize, usize), Value)> = vec![
        ("4:3", Fonte::Mp4, (640, 480), (45, 85), json!([])),
        ("vertical", Fonte::Mp4, (360, 640), (45, 85), json!([])),
        ("pixel 10:11", Fonte::PixelNaoQuadrado, (1280, 720), (45, 85), json!([])),
        ("giro do contêiner", Fonte::Giro90, (640, 360), (20, 100), json!([])),
        ("girar + espelho", Fonte::Mp4, (360, 640), (45, 85), json!([girar])),
        ("cobrir", Fonte::Mp4, (400, 300), (72, 108), json!([enquadramento(caixa([0.1, 0.4, 0.8, 0.6]), "cobrir")])),
        ("região fixa", Fonte::Mp4, (640, 360), (20, 100), json!([enquadramento(caixa([0.1, 0.45, 0.8, 0.5]), "conter")])),
    ];
    for (nome, f, (w, h), (j_in, j_out), mut efeitos) in casos {
        let m = apoio::medida(&cap, f);
        efeitos.as_array_mut().unwrap().push(destaque("d1", "r1"));
        let p = projeto(w, h, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", &m.info.quadros, j_in, j_out, 1.0, efeitos)], json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras_da_marca(f, &m, j_in, j_out))] }));
        let plano = Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &alvo_final("cpu")).unwrap();
        let (_, q) = renderizar(&cap, &plano, "marcacao");
        assert_eq!((q[0].w, q[0].h), (w, h));
        let ef = efeitos_do_clipe(&p, 0);
        let (d, medidos) = desvio_da_marcacao(&q, |i| no_canvas(ef, m.info.quadros.tempo(j_in + i), fontes::marca_esperada(f, m.numeros[j_in + i]), m.info.exibido, (w, h)).1);
        eprintln!("marcação em {nome} ({}): desvio máximo {d:.2} px em {medidos} quadros", cap.versao);
        assert!(medidos >= 15, "{nome}: {medidos} quadros medidos");
        assert!(d <= LIMITE_PX, "{nome}: {d:.2} px");
        // barras na cor do fundo (#202020), a até 2/255
        if nome == "4:3" {
            for (x, y) in [(10, 10), (320, 30), (630, 470), (320, 450)] {
                let c = pixel(&q[0], x, y);
                assert!(c.iter().all(|v| v.abs_diff(0x20) <= 2), "barra em ({x}, {y}): {c:?}");
            }
        }
    }
}

#[test]
fn zoom_animado_a_ate_1_px_e_desenho_por_cima_a_ate_1_5_px() {
    let Some(cap) = fontes::cap() else { return };
    const ZOOM_PX: f64 = 1.0;
    const DESENHO_PX: f64 = 1.5;
    let (j_in, j_out) = (30, 120);
    // (fonte, ajuste, região final, canvas): zoom 1 → 2 com pan; no segundo a região é 4:3 num canvas 16:9 (barras
    // fixas); no pixel 10:11 (654×480 exibido) o destino não cai em px pares
    for (f, ajuste, r0, r1, (w, h)) in [
        (Fonte::Mp4, "conter", [0.0, 0.0, 1.0, 1.0], [0.3, 0.44, 0.5, 0.5], (640u32, 360u32)),
        (Fonte::Mp4, "conter", [0.125, 0.0, 0.75, 1.0], [0.3, 0.4, 0.375, 0.5], (640, 360)),
        (Fonte::Mp4, "cobrir", [0.0, 0.0, 1.0, 1.0], [0.3, 0.4, 0.45, 0.6], (480, 480)),
        (Fonte::PixelNaoQuadrado, "conter", [0.0, 0.0, 1.0, 1.0], [0.25, 0.3, 0.5, 0.5], (640, 360)),
    ] {
        let m = apoio::medida(&cap, f);
        let tab = &m.info.quadros;
        let chaves = json!([{ "t": tab.tempo(j_in), "v": caixa(r0), "interp": "suave" }, { "t": tab.tempo(j_out - 1), "v": caixa(r1) }]);
        let efeitos = json!([enquadramento(chaves, ajuste), destaque("d1", "r1")]);
        let p = projeto(w, h, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", tab, j_in, j_out, 1.0, efeitos)], json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras_da_marca(f, &m, j_in, j_out))] }));
        let plano = Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &alvo_final("cpu")).unwrap();
        assert!(plano.blocos[0].grafo.contains("perspective="));
        let (_, q) = renderizar(&cap, &plano, "zoom");
        let ef = efeitos_do_clipe(&p, 0);
        let mapa = &plano.proveniencia.roteiro[0].origem.as_ref().unwrap().quadro_do_slot;
        let (mut pior_zoom, mut pior_desenho, mut medidos) = (0.0f64, 0.0f64, 0);
        let (mut soma_x, mut soma_y) = (0.0, 0.0);
        for (s, x) in q.iter().enumerate() {
            let j = mapa[s] as usize;
            let ([px, py, pw, ph], cabe) = no_canvas(ef, tab.tempo(j), fontes::marca_esperada(f, m.numeros[j]), m.info.exibido, (w, h));
            if !cabe {
                continue;
            }
            let (previsto_x, previsto_y) = (px + pw / 2.0, py + ph / 2.0);
            let r = centro_de(x, eh_vermelho).unwrap_or_else(|| panic!("slot {s}: marca fora"));
            let v = centro_da_caixa(x, eh_verde).unwrap_or_else(|| panic!("slot {s}: destaque fora"));
            pior_zoom = pior_zoom.max((r.0 - previsto_x).abs()).max((r.1 - previsto_y).abs());
            (soma_x, soma_y) = (soma_x + r.0 - previsto_x, soma_y + r.1 - previsto_y);
            pior_desenho = pior_desenho.max((v.0 - r.0).abs()).max((v.1 - r.1).abs());
            medidos += 1;
        }
        eprintln!(
            "zoom animado {f:?} {ajuste} {w}×{h} ({}): {medidos} slots, marca a {pior_zoom:.2} px do previsto (média {:+.2}, {:+.2}), destaque a {pior_desenho:.2} px da marca",
            cap.versao, soma_x / medidos.max(1) as f64, soma_y / medidos.max(1) as f64
        );
        assert!(medidos >= 25, "{medidos} slots medidos");
        assert!(pior_zoom <= ZOOM_PX, "zoom {f:?} {ajuste}: {pior_zoom:.2} px ({})", cap.versao);
        assert!(pior_desenho <= DESENHO_PX, "desenho {f:?} {ajuste}: {pior_desenho:.2} px ({})", cap.versao);
        // conter com região de outra proporção: as barras são fundo do começo ao fim, nunca a imagem vizinha
        if r0[2] != 1.0 || f == Fonte::PixelNaoQuadrado {
            for x in [&q[0], &q[45], &q[89]] {
                for (bx, by) in [(20, 40), (60, 300), (600, 180), (630, 20)] {
                    let c = pixel(x, bx, by);
                    assert!(c.iter().all(|v| v.abs_diff(0x20) <= 2), "barra em ({bx}, {by}): {c:?}");
                }
            }
        }
    }
}

#[test]
fn animado_com_chaves_iguais_da_o_quadro_do_fixo() {
    let Some(cap) = fontes::cap() else { return };
    let m = apoio::medida(&cap, Fonte::Mp4);
    let tab = &m.info.quadros;
    let (j_in, j_out) = (40, 60);
    // conter com região de proporção diferente da do canvas: barras dos lados
    let r = [0.1, 0.2, 0.5, 0.6];
    let render = |regiao: Value| {
        let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", tab, j_in, j_out, 1.0, json!([enquadramento(regiao, "conter")]))], json!({}));
        let plano = Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &alvo_final("cpu")).unwrap();
        renderizar(&cap, &plano, "chaves-iguais").1
    };
    let fixo = render(caixa(r));
    let animado = render(json!([{ "t": tab.tempo(j_in), "v": caixa(r) }, { "t": tab.tempo(j_out), "v": caixa(r), "interp": "suave" }]));
    for (i, (a, b)) in fixo.iter().zip(&animado).enumerate() {
        assert_eq!(medir::pixels_diferentes(a, b, 2), 0, "quadro {i}");
    }
    // e as barras estão lá, na cor do fundo
    for (x, y) in [(4, 90), (316, 90)] {
        let c = pixel(&fixo[0], x, y);
        assert!(c.iter().all(|v| v.abs_diff(0x20) <= 2), "barra em ({x}, {y}): {c:?}");
    }
    assert!(pixel(&fixo[0], 160, 90).iter().any(|v| v.abs_diff(0x20) > 10), "o meio é imagem");
}

#[test]
fn quinhentas_ancoras_renderizam() {
    let Some(cap) = fontes::cap() else { return };
    let f = Fonte::Ts5994;
    let m = apoio::medida(&cap, f);
    let tab = &m.info.quadros;
    let (j_in, j_out) = (100, 600);
    let ancoras = ancoras_da_marca(f, &m, j_in, j_out);
    assert_eq!(ancoras.len(), 500);
    // 500 chaves também no enquadramento (uma por quadro, tremendo): o v1 caía com 98
    let chaves: Vec<Value> = (j_in..j_out).map(|j| json!({ "t": tab.tempo(j), "v": caixa([0.05 + 0.02 * ((j * 7) % 5) as f64, 0.1, 0.8, 0.8]) })).collect();
    let efeitos = json!([enquadramento(json!(chaves), "cobrir"), destaque("d1", "r1")]);
    let p = projeto(320, 180, (60000, 1001), vec![midia("m1", "video", json!([ocultar("o1", "r1", "solido", json!({}))]))], vec![trecho("c1", "m1", tab, j_in, j_out, 1.0, efeitos)], json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras)] }));
    let plano = Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &alvo_final("cpu")).unwrap();
    assert_eq!(plano.total_q, 500);
    let (r, q) = renderizar(&cap, &plano, "500-ancoras");
    assert_eq!((q.len(), r.compostos.len()), (500, 500));
    assert_eq!(q.iter().map(vermelho).max(), Some(0), "ocultação sólida com 500 âncoras");
    assert!(q.iter().filter(|x| centro_da_caixa(x, eh_verde).is_some()).count() >= 450, "destaque com 500 âncoras");
    eprintln!("500 âncoras ({}): grafo de {} bytes", cap.versao, plano.blocos[0].grafo.len());
}

// ------------------------------------------------------------------ ensaio integrado ([V] A17) e alvos

/// Quadros de um vídeo em RGB pela MESMA conversão do quadro final (BT.709 limitada → RGB), para comparar cor.
fn quadros_709(cap: &Capacidades, video: &Path, w: u32, h: u32) -> Vec<QuadroRgb> {
    let a: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error", "-i", &video.to_string_lossy(), "-vf",
        "scale=flags=bicubic+accurate_rnd+bitexact+full_chroma_int:in_color_matrix=bt709:in_range=tv:out_range=pc,format=rgb24", "-f", "rawvideo", "-"].map(String::from).into();
    let pasta = video.parent().unwrap().to_path_buf();
    let cmd = crate::edicao::ffmpeg::exec::Comando { nome: "rgb709".into(), programa: cap.binario.clone(), args: a, pasta: pasta.clone(), ambiente: Vec::new(), sem_ambiente: Vec::new(), stderr: pasta.join("rgb709.stderr") };
    let (fim, out) = crate::edicao::ffmpeg::exec::rodar_com_saida(&cmd, &std::sync::atomic::AtomicBool::new(false)).expect("ffmpeg");
    crate::edicao::ffmpeg::exec::exigir_sucesso(&cmd, &fim).expect("decodificar");
    out.chunks_exact((w * h * 3) as usize).map(|px| QuadroRgb { w, h, px: px.to_vec() }).collect()
}

/// Imagem PNG de teste (quadrantes de cor, canto transparente opcional) e o `FonteInfo` dela.
fn imagem_de_teste(pasta: &Path, nome: &str, w: u32, h: u32, cores: [[u8; 4]; 4]) -> FonteInfo {
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            px.extend_from_slice(&cores[(usize::from(y >= h / 2) << 1) | usize::from(x >= w / 2)]);
        }
    }
    let png = crate::edicao::camada::Bitmap { w, h, canais: 4, px }.png().unwrap();
    let caminho = pasta.join(nome);
    std::fs::write(&caminho, &png).unwrap();
    let mut info = apoio::fonte(apoio::tabela((1, 1), vec![0]), (w, h));
    info.caminho = caminho;
    info.pix_fmt = "rgba".into();
    info
}

const OPACO: [[u8; 4]; 4] = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 255, 255]];

/// Âncoras "que pulam": uma por quadro, em 30 lugares distintos (6,4 px entre vizinhos): o lugar diz o quadro.
fn x_que_pula(j: usize) -> f64 {
    0.1 + 0.6 * ((j * 7) % 30) as f64 / 30.0
}

struct Integrado {
    p: ProjetoEdicao,
    mundo: Mundo,
    mp4: Arc<Medida>,
    mkv: Arc<Medida>,
}

/// Projeto do ensaio integrado, a 30000/1001: trecho a 0,5× com camada que muda a cada quadro e carimbo; título
/// (ou foto) de 45 slots entrando do preto; trecho de outra mídia com `ocultar`; quadro congelado; faixa cheia com
/// desfoque e holofote; zoom animado; 9 trechos curtos (15 clipes: 2 blocos); imagem sobreposta; fecho escurecendo.
fn integrado(cap: &Arc<Capacidades>, com_titulo: bool) -> Integrado {
    let dir = pasta("integrado-img");
    let (mp4, mkv, cheia) = (apoio::medida(cap, Fonte::Mp4), apoio::medida(cap, Fonte::Mkv30), apoio::medida(cap, Fonte::FaixaCheia));
    let (t4, tk, tc) = (&mp4.info.quadros, &mkv.info.quadros, &cheia.info.quadros);
    let foto = imagem_de_teste(&dir, "foto.png", 400, 300, OPACO);
    let logo = imagem_de_teste(&dir, "logo.png", 64, 64, [[255, 255, 0, 255], [255, 255, 0, 255], [255, 255, 0, 255], [0, 0, 0, 0]]);
    let pula: Vec<Value> = (60..90).map(|j| ancora(t4.tempo(j), Some(t4.pts[j]), [x_que_pula(j), 0.3, 0.1, 0.2])).collect();
    let carimbo = efeito("carimbo", "k1", json!({ "mostra": "tempo_fonte", "formato": "hms_ms", "relogio": null, "lugar": { "x": 0.03, "y": 0.97, "h": "esq", "v": "base" },
        "estilo": { "fonte": "mono", "negrito": false, "italico": false, "tamanho": 0.07, "cor": "#ffffff", "alinhamento": "esq", "caixa": { "cor": "#000000", "margem": 0.01, "raio": 0.0 }, "contorno": null } }));
    let escurecer = json!({ "tipo": "escurecer", "duracao_s": 0.5, "audio": "acompanhar" });
    let segundo = if com_titulo {
        json!({ "tipo": "titulo", "id": "c1", "rotulo": "", "duracao_s": 1.5, "transicao_entrada": escurecer,
                "titulo": { "modelo": "cartela", "fundo": { "tipo": "cor", "cor": "#102040" }, "blocos": [{ "id": "b1", "papel": "titulo", "conteudo": "Câmera 2", "largura_max": 0.8,
                    "lugar": { "x": 0.5, "y": 0.5, "h": "centro", "v": "meio" },
                    "estilo": { "fonte": "sans", "negrito": true, "italico": false, "tamanho": 0.12, "cor": "#ffffff", "alinhamento": "centro", "caixa": null, "contorno": null } }] } })
    } else {
        json!({ "tipo": "imagem", "id": "c1", "rotulo": "", "midia": "foto", "duracao_s": 1.5, "transicao_entrada": escurecer })
    };
    let mut c2 = trecho("c2", "mk", tk, 40, 100, 1.0, json!([]));
    c2["transicao_entrada"] = escurecer.clone();
    let zoom = json!([{ "t": t4.tempo(200), "v": caixa([0.0, 0.0, 1.0, 1.0]), "interp": "suave" }, { "t": t4.tempo(229), "v": caixa([0.3, 0.3, 0.5, 0.5]) }]);
    let holofote = efeito("holofote", "h1", json!({ "alvo": { "rastreio": "rc", "forma": "elipse", "vertices": null, "margem": 0.3, "borda_suave": 0.2, "inverter": false }, "escurecer": 0.6, "dessaturar": 0.5 }));
    let mut principal = vec![
        trecho("c0", "m4", t4, 60, 90, 0.5, json!([destaque("d1", "pula"), carimbo])),
        segundo,
        c2,
        json!({ "tipo": "congelar", "id": "c3", "rotulo": "", "midia": "mk", "instante_s": tk.tempo(70), "instante_pts": tk.pts[70], "duracao_s": 0.5, "efeitos": [destaque("d3", "rk")] }),
        trecho("c4", "fc", tc, 30, 60, 1.0, json!([holofote])),
        trecho("c5", "m4", t4, 200, 230, 1.0, json!([enquadramento(zoom, "conter")])),
    ];
    for i in 0..9 {
        principal.push(trecho(&format!("c{}", 6 + i), "m4", t4, 240 + 10 * i, 250 + 10 * i, 1.0, json!([])));
    }
    let sob = json!({ "tipo": "imagem", "id": "s1", "rotulo": "", "pos": { "ref": "programa", "t": 0.5 }, "duracao_s": 2.0, "midia": "logo",
        "transformacao": { "x": 0.85, "y": 0.2, "escala": 0.15, "rotacao": 0.0, "opacidade": 1.0 }, "entrada_s": 0.2, "saida_s": 0.0, "efeitos": [] });
    let p = projeto(
        320,
        180,
        (30000, 1001),
        vec![
            midia("m4", "video", json!([])),
            midia("mk", "video", json!([ocultar("o1", "rk", "solido", json!({}))])),
            midia("fc", "video", json!([ocultar("o2", "rc", "desfoque", json!({ "ativo": { "de": tc.tempo(30), "ate": tc.tempo(44) } }))])),
            midia("foto", "imagem", json!([])),
            midia("logo", "imagem", json!([])),
        ],
        principal,
        json!({
            "rastreios": [
                rastreio("pula", Some("m4"), "fonte", pula),
                rastreio("rk", Some("mk"), "fonte", ancoras_da_marca(Fonte::Mkv30, &mkv, 40, 100)),
                rastreio("rc", Some("fc"), "fonte", ancoras_da_marca(Fonte::FaixaCheia, &cheia, 30, 60)),
            ],
            "sobreposicoes": [{ "id": "t1", "nome": "", "oculta": false, "clipes": [sob] }],
            "fecho": { "tipo": "escurecer", "duracao_s": 0.3, "audio": "acompanhar" }
        }),
    );
    let mundo = Mundo::novo(cap).com("m4", &mp4.info).com("mk", &mkv.info).com("fc", &cheia.info).com("foto", &foto).com("logo", &logo);
    Integrado { p, mundo, mp4, mkv }
}

/// Roda `f`; se o que falta é o `camada::titulo` (de outro pacote desta leva, ainda `todo!()`), registra e devolve None.
fn com_titulo_ou_dispensa<T>(f: impl FnOnce() -> T + std::panic::UnwindSafe) -> Option<T> {
    match std::panic::catch_unwind(f) {
        Ok(v) => Some(v),
        Err(e) => {
            let texto = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            if texto.contains("not yet implemented") {
                eprintln!("dispensado: `camada::titulo` ainda é todo!() nesta cópia (P0.5t); o ensaio roda com foto no lugar do título");
                None
            } else {
                std::panic::resume_unwind(e)
            }
        }
    }
}

fn conferir_integrado(cap: &Arc<Capacidades>, com_titulo: bool) {
    let it = integrado(cap, com_titulo);
    let plano = it.mundo.compilar(&it.p, &alvo_final("cpu")).expect("compila");
    assert_eq!(plano.blocos.len(), 2, "15 clipes: 12 entradas de mídia por bloco");
    assert!(plano.blocos[0].codificador.contains(&"libx264".to_string()));
    let r = executor::rodar(cap, &plano, &pasta("a17"), &Opcoes { juntar: true, ..Opcoes::default() }).unwrap_or_else(|e| panic!("A17 ({}): {e}", cap.versao));
    let junto = r.junto.as_ref().expect("junção");
    let total = plano.total_q as usize;
    // contagem, início em 0 e passos exatos de 1001/30000
    let pts = medir::pts(cap, junto).unwrap();
    assert_eq!((pts.len(), r.compostos.len(), pts[0]), (total, total, 0));
    assert!(pts.windows(2).all(|p| p[1] - p[0] == 1001), "passos do arquivo junto");
    let numeros = medir::quadros_de_origem(cap, junto).unwrap();
    let q = medir::quadros(cap, junto).unwrap();
    assert_eq!((q[0].w, q[0].h), (320, 180));
    let item = |n: usize| (plano.proveniencia.roteiro[n].na_saida.de_q as usize, plano.proveniencia.roteiro[n].na_saida.n_q as usize);
    // quadro de origem por slot nos trechos sem zoom (c0 tem a camada por cima; c2 tem a ocultação)
    let (mut conferidos, mut errados) = (0, 0);
    for (n, m, pula, corta) in [(0, &it.mp4, 0, 15), (2, &it.mkv, 15, 0), (6, &it.mp4, 0, 0), (10, &it.mp4, 0, 0), (13, &it.mp4, 0, 0)] {
        let (de, n_q) = item(n);
        let previsto = previstos(&plano, n, m);
        assert_eq!(previsto.len(), n_q);
        // `pula` e `corta` = slots do escurecimento (de entrada e de saída), em que o número não se lê
        conferidos += n_q - pula - corta;
        for s in pula..n_q - corta {
            if numeros[de + s] != previsto[s] {
                errados += 1;
                eprintln!("clipe {n} slot {s}: lido {} × previsto {}", numeros[de + s], previsto[s]);
            }
        }
    }
    assert_eq!(errados, 0, "quadro de origem errado em {errados} de {conferidos} slots ({})", cap.versao);
    // quadro congelado: o mesmo quadro (o 70 da fonte) em todos os slots dele
    let (de_c, n_c) = item(3);
    assert!(numeros[de_c..de_c + n_c].iter().all(|&x| x == it.mkv.numeros[70]), "congelado: {:?}", &numeros[de_c..de_c + n_c]);
    // camada no slot: o destaque que pula tem de estar no lugar do quadro que o slot mostra
    let (de, n_q) = item(0);
    let mapa = &plano.proveniencia.roteiro[0].origem.as_ref().unwrap().quadro_do_slot;
    let mut fora = 0;
    // sem os 15 slots em que o clipe escurece para o seguinte
    let n_q = n_q - 15;
    for s in 0..n_q {
        let esperado = (x_que_pula(mapa[s] as usize) + 0.05) * 320.0;
        let (cx, _, _) = centro_da_caixa(&q[de + s], eh_verde).unwrap_or_else(|| panic!("slot {s}: sem destaque"));
        fora += usize::from((cx - esperado).abs() > 2.5);
    }
    assert_eq!(fora, 0, "camada fora do slot em {fora} de {n_q} ({})", cap.versao);
    // ocultação: nenhum vermelho no trecho c2 nem no quadro congelado c3
    let (de2, n2) = item(2);
    let (de3, n3) = item(3);
    assert_eq!(q[de2..de2 + n2].iter().chain(&q[de3..de3 + n3]).map(vermelho).max(), Some(0));
    // entra do preto (escurecer no c1) e sai para o preto (fecho)
    let (de1, _) = item(1);
    let luz = |x: &QuadroRgb| x.px.iter().map(|&v| v as u64).sum::<u64>() / x.px.len() as u64;
    assert!(luz(&q[de1]) < 12 && luz(&q[de1 + 20]) > luz(&q[de1 + 5]) && luz(&q[total - 1]) < 30, "escurecer: {} {} {}", luz(&q[de1]), luz(&q[de1 + 20]), luz(&q[total - 1]));
    // imagem sobreposta (amarela, canto de cima à direita) só nos slots dela, de 0,5 s a 2,5 s
    let amarelo = |x: &QuadroRgb| x.px.chunks_exact(3).filter(|p| p[0] > 180 && p[1] > 180 && p[2] < 90).count();
    assert!(amarelo(&q[5]) == 0 && amarelo(&q[40]) > 800 && amarelo(&q[80]) == 0, "sobreposta: {} {} {}", amarelo(&q[5]), amarelo(&q[40]), amarelo(&q[80]));
    eprintln!("A17 pelo compilador ({}, {}): {total} quadros em 2 blocos, {conferidos} slots com o quadro previsto, camada fora do slot 0", cap.versao, if com_titulo { "com título" } else { "com foto" });
}

#[test]
fn a17_ensaio_integrado_pelo_compilador() {
    let Some(cap) = fontes::cap() else { return };
    let c = cap.clone();
    if com_titulo_ou_dispensa(move || conferir_integrado(&c, true)).is_none() {
        conferir_integrado(&cap, false);
    }
}

#[test]
fn quadro_final_e_previa_dao_o_slot_da_exportacao() {
    let Some(cap) = fontes::cap() else { return };
    let it = integrado(&cap, false);
    let plano = it.mundo.compilar(&it.p, &alvo_final("cpu")).unwrap();
    let r = executor::rodar(&cap, &plano, &pasta("final"), &sem_perdas()).unwrap();
    let mut todos = Vec::new();
    for b in &r.blocos {
        todos.extend(quadros_709(&cap, b, 320, 180));
    }
    assert_eq!(todos.len() as u64, plano.total_q);
    let item = |n: usize| plano.proveniencia.roteiro[n].na_saida.de_q;
    // camada + carimbo; escurecendo na entrada; ocultação; congelado; holofote; zoom animado (no começo, a janela
    // vê um zoom menor que o do clipe: a tabela tem de ser a mesma); emenda de bloco; fecho
    let slots = [7, 40, item(1) + 4, item(2) + 3, item(2) + 30, item(3) + 2, item(4) + 10, item(5) + 3, item(5) + 9, item(5) + 20, plano.blocos[1].de_q, plano.total_q - 3, plano.total_q - 1];
    for q in slots {
        let pq = it.mundo.compilar(&it.p, &Alvo::Quadro { q, largura: 320 }).unwrap();
        let rq = executor::rodar(&cap, &pq, &pasta("quadro"), &Opcoes::default()).unwrap_or_else(|e| panic!("quadro {q}: {e}"));
        let bmp = image::open(&rq.blocos[0]).expect("bmp").to_rgb8();
        let lido = QuadroRgb { w: bmp.width(), h: bmp.height(), px: bmp.into_raw() };
        assert_eq!((lido.w, lido.h), (320, 180));
        assert_eq!(medir::pixels_diferentes(&todos[q as usize], &lido, 1), 0, "quadro final do slot {q} ≠ exportação ({})", cap.versao);
    }
    // prévia do render na altura da saída: os mesmos quadros da exportação na janela
    let (de, ate) = (item(1) - 6, item(2) + 12);
    let pv = it.mundo.compilar(&it.p, &Alvo::Previa { altura: 180, de_q: de, ate_q: ate }).unwrap();
    let rv = executor::rodar(&cap, &pv, &pasta("previa"), &sem_perdas()).unwrap();
    let qv = quadros_709(&cap, &rv.blocos[0], 320, 180);
    assert_eq!(qv.len() as u64, ate - de);
    for (i, x) in qv.iter().enumerate() {
        assert_eq!(medir::pixels_diferentes(&todos[de as usize + i], x, 1), 0, "prévia: slot {}", de as usize + i);
    }
    // prévia em baixa: codifica de verdade (libx264 veryfast), metade do tamanho, mesma contagem
    let baixa = it.mundo.compilar(&it.p, &Alvo::Previa { altura: 90, de_q: 0, ate_q: 50 }).unwrap();
    assert_eq!((baixa.canvas.w, baixa.canvas.h), (160, 90));
    let rb = executor::rodar(&cap, &baixa, &pasta("previa-baixa"), &Opcoes { juntar: true, ..Opcoes::default() }).unwrap();
    assert_eq!(medir::pts(&cap, rb.junto.as_ref().unwrap()).unwrap().len(), 50);
}

#[test]
fn clipe_de_imagem_cor_do_fundo_e_sobreposta() {
    let Some(cap) = fontes::cap() else { return };
    let dir = pasta("imagem");
    // foto 4:3 num canvas 16:9 de fundo colorido: barras na cor do fundo, quadrantes na cor da foto (a até 2/255)
    let foto = imagem_de_teste(&dir, "foto.png", 400, 300, OPACO);
    let mut p = projeto(320, 180, (25, 1), vec![midia("foto", "imagem", json!([]))], vec![json!({ "tipo": "imagem", "id": "c0", "rotulo": "", "midia": "foto", "duracao_s": 1.0 })], json!({}));
    p.saida.cor_fundo = "#804020".into();
    let plano = Mundo::novo(&cap).com("foto", &foto).compilar(&p, &alvo_final("cpu")).unwrap();
    assert_eq!((plano.total_q, plano.blocos[0].conferir.len()), (25, 0));
    let r = executor::rodar(&cap, &plano, &pasta("imagem-r"), &sem_perdas()).unwrap();
    let q = quadros_709(&cap, &r.blocos[0], 320, 180);
    assert_eq!(q.len(), 25);
    let perto = |c: [u8; 3], e: [u8; 3]| c.iter().zip(&e).all(|(a, b)| a.abs_diff(*b) <= 2);
    for x in [&q[0], &q[24]] {
        for (px, py, cor) in [(10, 90, [0x80, 0x40, 0x20]), (310, 20, [0x80, 0x40, 0x20]), (100, 45, [255, 0, 0]), (220, 45, [0, 255, 0]), (100, 135, [0, 0, 255]), (220, 135, [255, 255, 255])] {
            assert!(perto(pixel(x, px, py), cor), "({px}, {py}): {:?} × {cor:?} ({})", pixel(x, px, py), cap.versao);
        }
    }
    // a foto ocupa 240×180 no meio: a borda cai em x = 40 e x = 280
    assert!(perto(pixel(&q[0], 36, 90), [0x80, 0x40, 0x20]) && !perto(pixel(&q[0], 44, 90), [0x80, 0x40, 0x20]));
}

// ------------------------------------------------------------------ reprodutibilidade, GPU, memória e tempo

#[test]
fn hash_do_composto_igual_entre_binarios_e_cpus() {
    let Some(cap) = fontes::cap() else { return };
    let it = integrado(&cap, false);
    let plano = it.mundo.compilar(&it.p, &alvo_final("cpu")).unwrap();
    // inclui fonte de faixa cheia, desfoque, holofote, zoom em 4:4:4, escalas, camadas e escurecer
    assert!(plano.blocos[0].grafo.contains("mergeplanes") && plano.blocos[0].grafo.contains("boxblur") && plano.blocos[0].grafo.contains("perspective"));
    let base = executor::rodar(&cap, &plano, &pasta("hash-base"), &sem_perdas()).unwrap().compostos;
    assert_eq!(base.len() as u64, plano.total_q);
    let conferir = |nome: &str, outros: &[String]| {
        let n = base.iter().zip(outros).filter(|(a, b)| a != b).count();
        let primeiro = base.iter().zip(outros).position(|(a, b)| a != b);
        assert!(n == 0 && outros.len() == base.len(), "{nome}: {n} de {} quadros com outro hash (primeiro: {primeiro:?})", base.len());
    };
    // conjuntos de instruções reduzidos: CPU de 2012 (sem AVX2 nem AVX-512), só até SSE4.2 e C puro
    for flags in ["-avx2-avx512-avx512icl-fma3-bmi1-bmi2", "-avx-avx2-avx512-avx512icl-fma3-fma4-xop-bmi1-bmi2", "0"] {
        let r = executor::rodar(&cap, &plano, &pasta("hash-cpu"), &Opcoes { sem_perdas: true, globais: vec!["-cpuflags".into(), flags.into()], ..Opcoes::default() }).unwrap();
        conferir(&format!("-cpuflags {flags} ({})", cap.versao), &r.compostos);
    }
    // o outro ffmpeg, com o MESMO plano (as tabelas são as deste)
    let Some(outro) = outro_ffmpeg(&cap) else { return };
    assert_ne!(outro.versao, cap.versao);
    let r = executor::rodar(&outro, &plano, &pasta("hash-outro"), &sem_perdas()).unwrap_or_else(|e| panic!("{}: {e}", outro.versao));
    conferir(&format!("{} × {}", outro.versao, cap.versao), &r.compostos);
    eprintln!("hash do composto: {} quadros iguais entre {} e {}, e com -cpuflags reduzido (até C puro)", base.len(), cap.versao, outro.versao);
}

#[test]
#[ignore = "completo: codificação real por GPU (dispensado sem NVENC aprovado)"]
fn nvenc_codifica_1080p60_e_4k() {
    let Some(cap) = fontes::cap() else { return };
    let com_gpu = Arc::new(crate::edicao::ffmpeg::sonda::sondar(&cap.binario).expect("sonda"));
    if com_gpu.gpu.as_ref().map(|g| g.codificador.as_str()) != Some("h264_nvenc") {
        eprintln!("dispensado: sem NVENC aprovado em {} (gpu: {:?}; reprovados: {:?})", cap.versao, com_gpu.gpu.as_ref().map(|g| &g.codificador), com_gpu.reprovados.iter().map(|r| (&r.codificador, r.classe)).collect::<Vec<_>>());
        return;
    }
    let m = apoio::medida(&cap, Fonte::Mp4);
    for (w, h, fps) in [(1920u32, 1080u32, (60i64, 1i64)), (3840, 2160, (30, 1)), (3840, 2160, (60, 1))] {
        if !crate::edicao::ffmpeg::sonda::gpu_serve(&com_gpu, w, h, crate::edicao::modelo::Racional { num: fps.0, den: fps.1 }) {
            eprintln!("dispensado: a sonda no tamanho do projeto reprovou {w}×{h} a {}", fps.0);
            continue;
        }
        let p = projeto(w, h, fps, vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", &m.info.quadros, 30, 60, 1.0, json!([]))], json!({}));
        let plano = Mundo::novo(&com_gpu).com("m1", &m.info).compilar(&p, &alvo_final("auto")).unwrap();
        let linha = plano.blocos[0].codificador.join(" ");
        assert!(linha.starts_with("-c:v h264_nvenc") && !linha.contains("-level"), "{linha}");
        let r = executor::rodar(&com_gpu, &plano, &pasta("nvenc"), &Opcoes { juntar: true, ..Opcoes::default() }).unwrap_or_else(|e| panic!("NVENC {w}×{h} a {}: {e}", fps.0));
        let junto = r.junto.unwrap();
        // conteúdo, não só a contagem: o quadro de origem de cada slot depois de codificar e decodificar
        assert_eq!(medir::quadros_de_origem(&cap, &junto).unwrap(), previstos(&plano, 0, &m), "NVENC {w}×{h}");
        let q0 = medir::quadro(&cap, &junto, 0).unwrap();
        assert_eq!((q0.w, q0.h, r.compostos.len() as u64), (w, h, plano.total_q));
        eprintln!("NVENC ({}): {w}×{h} a {} qps, {} quadros, {} bytes", cap.versao, fps.0, plano.total_q, std::fs::metadata(&junto).map(|m| m.len()).unwrap_or(0));
    }
}

/// Pico de memória aceito num ffmpeg do render de 60 trechos (GiB).
const PICO_GIB: f64 = 1.5;

#[test]
#[ignore = "completo: 60 trechos de 1080p, memória medida"]
fn sessenta_trechos_com_pico_abaixo_de_1_5_gib() {
    let Some(cap) = fontes::cap() else { return };
    let dir = pasta("60-trechos");
    let video = gerar(&cap, &dir, "f1080.mp4", "testsrc2=s=1920x1080:r=30:d=45", &["-c:v", "libx264", "-preset", "ultrafast", "-g", "60", "-pix_fmt", "yuv420p"]);
    let info = fonte_de(&cap, &video);
    let tab = &info.quadros;
    let clipes: Vec<Value> = (0..60).map(|i| trecho(&format!("c{i}"), "m1", tab, 20 * i + 5, 20 * i + 20, 1.0, json!([]))).collect();
    let p = projeto(1920, 1080, (30, 1), vec![midia("m1", "video", json!([]))], clipes, json!({}));
    let plano = Mundo::novo(&cap).com("m1", &info).compilar(&p, &alvo_final("cpu")).unwrap();
    assert_eq!((plano.blocos.len(), plano.total_q), (5, 900));
    // 12 entradas por bloco, cada uma com o decodificador limitado a 1 thread (é o que segura a memória no 7.0.2)
    assert!(plano.blocos.iter().all(|b| b.entradas.len() == 12 && b.entradas.iter().all(|e| e.antes[..2] == ["-threads".to_string(), "1".to_string()])));
    let r = executor::rodar(&cap, &plano, &dir.join("r"), &Opcoes { juntar: true, ..Opcoes::default() }).unwrap();
    let gib = r.pico_kb as f64 / (1024.0 * 1024.0);
    eprintln!("60 trechos de 1080p ({}): 5 blocos de 12 entradas, pico de {gib:.2} GiB num ffmpeg", cap.versao);
    assert!(r.pico_kb > 0, "pico de memória não medido");
    assert!(gib < PICO_GIB, "{gib:.2} GiB");
    assert_eq!(medir::pts(&cap, r.junto.as_ref().unwrap()).unwrap().len(), 900);
    // 15 quadros de cada trecho, na ordem: os pts lidos de cada clipe foram conferidos pelo executor (showinfo)
    assert_eq!(r.lidos.iter().map(|l| l.len()).sum::<usize>(), 60);
}

/// Tempo aceito para o quadro final no fim de um clipe de 10 min (s) e a folga com a máquina ocupada.
const QUADRO_FINAL_S: f64 = 1.0;
const FOLGA: f64 = 3.0;

#[test]
#[ignore = "completo: quadro final no fim de um clipe de 10 min de 1080p"]
fn quadro_final_no_fim_de_10_min_em_menos_de_1_s() {
    let Some(cap) = fontes::cap() else { return };
    let dir = pasta("10-min");
    // 10 min de 1080p a 30 qps, quadro-chave a cada 2 s
    let video = gerar(&cap, &dir, "f10min.mp4", "testsrc2=s=1920x1080:r=30:d=600", &["-c:v", "libx264", "-preset", "ultrafast", "-crf", "30", "-g", "60", "-pix_fmt", "yuv420p"]);
    let info = fonte_de(&cap, &video);
    let tab = &info.quadros;
    assert_eq!(tab.pts.len(), 18_000);
    assert_eq!(tab.busca, crate::edicao::quadros::Busca::Confiavel, "a sonda de busca tem de aprovar a fonte");
    let p = projeto(1920, 1080, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", tab, 0, 18_000, 1.0, json!([]))], json!({}));
    let mundo = Mundo::novo(&cap).com("m1", &info);
    let mut pior = 0.0f64;
    for q in [17_999u64, 17_941, 14_000] {
        let t0 = std::time::Instant::now();
        let plano = mundo.compilar(&p, &Alvo::Quadro { q, largura: 1920 }).unwrap();
        let compilar_s = t0.elapsed().as_secs_f64();
        let r = executor::rodar(&cap, &plano, &dir.join(format!("q{q}")), &Opcoes::default()).unwrap();
        let total = t0.elapsed().as_secs_f64();
        pior = pior.max(total);
        // o quadro certo: o mesmo que decodificar o arquivo até lá
        let bmp = image::open(&r.blocos[0]).unwrap().to_rgb8();
        assert_eq!((bmp.width(), bmp.height()), (1920, 1080));
        assert_eq!(plano.blocos[0].conferir[0].lidos, 1);
        assert_eq!(plano.blocos[0].entradas[0].antes[0], "-ss");
        eprintln!("quadro final no slot {q} ({}): {total:.3} s (compilar {compilar_s:.3} s)", cap.versao);
    }
    // identidade: o slot 14 000 pelo quadro final × o mesmo quadro lido direto, pela mesma conversão
    let plano = mundo.compilar(&p, &Alvo::Quadro { q: 14_000, largura: 1920 }).unwrap();
    let r = executor::rodar(&cap, &plano, &dir.join("id"), &Opcoes::default()).unwrap();
    let bmp = image::open(&r.blocos[0]).unwrap().to_rgb8();
    let direto = gerar_quadro_direto(&cap, &dir, &video, tab.pts[14_000]);
    let lido = QuadroRgb { w: 1920, h: 1080, px: bmp.into_raw() };
    assert_eq!(medir::pixels_diferentes(&direto, &lido, 1), 0, "o quadro final não é o quadro 14 000 da fonte");
    assert!(pior < QUADRO_FINAL_S * FOLGA, "{pior:.3} s (limite {QUADRO_FINAL_S} s × folga {FOLGA})");
}

/// O quadro de pts `pts` lido sem busca nenhuma, pela cor do motor e pela conversão do quadro final.
fn gerar_quadro_direto(cap: &Capacidades, dir: &Path, video: &Path, pts: i64) -> QuadroRgb {
    let vf = format!(
        "trim=start_pts={pts}:end_pts={},scale=1920:1080:flags=bicubic+accurate_rnd+bitexact:in_color_matrix=bt709:in_range=tv:out_color_matrix=bt709:out_range=tv,format=yuv420p,\
         scale=flags=bicubic+accurate_rnd+bitexact+full_chroma_int:in_color_matrix=bt709:in_range=tv:out_range=pc,format=rgb24",
        pts + 1
    );
    let a: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error", "-copyts", "-i", &video.to_string_lossy(), "-vf", &vf, "-frames:v", "1", "-f", "rawvideo", "-"].map(String::from).into();
    let cmd = crate::edicao::ffmpeg::exec::Comando { nome: "direto".into(), programa: cap.binario.clone(), args: a, pasta: dir.to_path_buf(), ambiente: Vec::new(), sem_ambiente: Vec::new(), stderr: dir.join("direto.stderr") };
    let (_, out) = crate::edicao::ffmpeg::exec::rodar_com_saida(&cmd, &std::sync::atomic::AtomicBool::new(false)).expect("ffmpeg");
    assert_eq!(out.len(), 1920 * 1080 * 3, "quadro direto");
    QuadroRgb { w: 1920, h: 1080, px: out }
}
