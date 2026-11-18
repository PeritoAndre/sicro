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
use crate::edicao::modelo::{ProjetoEdicao, Racional};
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

/// Selos de cada item do roteiro (`feito.roteiro[].selos`).
fn pr_selos(plano: &Plano) -> Vec<Vec<String>> {
    plano.proveniencia.roteiro.iter().map(|i| i.selos.clone()).collect()
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
    // camadas pedidas, nunca desenhadas: máscara por quadro lido (2 clipes da mesma mídia), anotação, foto, título e a
    // camada global do bloco inteiro, com os selos (câmera lenta e ampliação no c1, imagem parada no c2)
    let tipos: Vec<(TipoCamada, u32)> = f.camadas.iter().map(|c| (c.tipo, c.n)).collect();
    assert_eq!(tipos, vec![(TipoCamada::MascaraFonte, 600), (TipoCamada::Anotacao, 1200), (TipoCamada::MascaraFonte, 1), (TipoCamada::Anotacao, 1), (TipoCamada::Anotacao, 1), (TipoCamada::Anotacao, 1335)]);
    assert_eq!(pr_selos(&f), vec![vec!["Velocidade 0,5×".to_string(), "Imagem ampliada".to_string()], vec!["Imagem parada".to_string()], vec![], vec![]]);
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
    // na ordem da cadeia: o enquadramento vem antes das anotações
    assert_eq!(pr.roteiro[0].processamentos.iter().map(|x| x.nome.as_str()).collect::<Vec<_>>(), vec!["enquadramento", "destaque", "carimbo"]);
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
    let lupa = efeito("lupa", "L9", json!({ "rastreio": "r1", "janela": 0.1, "fator": 2.0, "forma": "circulo", "interpolacao": "bilinear", "onde": { "modo": "fixa", "x": 0.8, "y": 0.2 }, "moldura": { "cor": "#ffffff", "espessura": 0.004, "contorno": false }, "ligacao": false }));
    let nitidez = efeito("nitidez", "N1", json!({ "quantidade": 0.5 }));
    let mut casos: Vec<(&str, Vec<Value>, Value)> = Vec::new();
    // os da Fase 1 saem quando o pacote dono entrega (P1.2, P1.4)
    if ainda_recusa(&lupa, true) {
        casos.push(("efeito lupa L9", vec![trecho("c1", "m1", t, 0, 30, 1.0, json!([lupa]))], json!({})));
    }
    if ainda_recusa(&nitidez, true) {
        casos.push(("efeito nitidez N1", vec![trecho("c1", "m1", t, 0, 30, 1.0, json!([nitidez]))], json!({})));
    }
    casos.extend::<Vec<(&str, Vec<Value>, Value)>>(vec![

        ("efeito ocultar O4 em clipe sem fonte", vec![json!({ "tipo": "titulo", "id": "c1", "duracao_s": 1.0, "titulo": { "modelo": "livre", "fundo": { "tipo": "transparente" }, "blocos": [] }, "efeitos": [ocultar("O4", "r1", "solido", json!({}))] })], json!({})),
        (
            "sobreposto video V3",
            vec![trecho("c1", "m1", t, 0, 30, 1.0, json!([]))],
            json!({ "sobreposicoes": [{ "id": "t1", "nome": "", "oculta": false, "clipes": [{ "tipo": "video", "id": "V3", "rotulo": "", "pos": { "ref": "programa", "t": 0.0 }, "duracao_s": 0.5,
                "transformacao": { "x": 0.5, "y": 0.5, "escala": 0.3, "rotacao": 0.0, "opacidade": 1.0 }, "entrada_s": 0.0, "saida_s": 0.0, "efeitos": [],
                "midia": "m1", "inicio_s": 0.0, "inicio_pts": 0, "velocidade": { "tipo": "constante", "fator": 1.0 }, "audio": null, "moldura": null, "sincronia": null }] }] }),
        ),
    ]);
    for (esperado, principal, extra) in casos {
        let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], principal, extra);
        let e = mundo.compilar(&p, &alvo_final("cpu")).expect_err(esperado).to_string();
        assert!(e.contains("nao_suportado") && e.contains(esperado), "{esperado}: {e}");
    }
    // item de versão mais nova ligado: o código da validação (contratos (h) 2), nunca ignorado
    for (esperado, principal) in [
        ("item_desconhecido: do_futuro F7", vec![trecho("c1", "m1", t, 0, 30, 1.0, json!([{ "tipo": "do_futuro", "id": "F7", "ligado": true }]))]),
        ("item_desconhecido: holograma H1", vec![trecho("c1", "m1", t, 0, 30, 1.0, json!([])), json!({ "tipo": "holograma", "id": "H1" })]),
        ("item_invalido: nitidez N2", vec![trecho("c1", "m1", t, 0, 30, 1.0, json!([{ "tipo": "nitidez", "id": "N2" }]))]),
    ] {
        let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], principal, json!({}));
        let e = mundo.compilar(&p, &alvo_final("cpu")).expect_err(esperado).to_string();
        assert!(e.contains(esperado), "{esperado}: {e}");
    }
    // efeito desligado nunca é recusado, conhecido ou não (contratos (h) 2)
    for desligado in [json!({ "tipo": "do_futuro", "id": "F7", "ligado": false }), json!({ "tipo": "nitidez", "id": "N1", "ligado": false, "nome": "", "ativo": null, "quantidade": 0.5 })] {
        let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", t, 0, 30, 1.0, json!([desligado.clone()]))], json!({}));
        assert!(mundo.compilar(&p, &alvo_final("cpu")).is_ok(), "{desligado}");
        let sob = json!({ "sobreposicoes": [{ "id": "t1", "nome": "", "oculta": false, "clipes": [{ "tipo": "titulo", "id": "x1", "rotulo": "", "pos": { "ref": "programa", "t": 0.0 },
            "duracao_s": 0.5, "transformacao": { "x": 0.5, "y": 0.5, "escala": 1.0, "rotacao": 0.0, "opacidade": 1.0 }, "entrada_s": 0.0, "saida_s": 0.0,
            "efeitos": [desligado.clone()], "titulo": { "modelo": "livre", "fundo": { "tipo": "transparente" }, "blocos": [] } }] }] });
        let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", t, 0, 30, 1.0, json!([]))], sob);
        assert!(mundo.compilar(&p, &alvo_final("cpu")).is_ok(), "sobreposto com {desligado}");
    }
    // efeito desligado de outra fase não impede
    let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", t, 0, 30, 1.0, json!([{ "tipo": "nitidez", "id": "N1", "ligado": false, "nome": "", "ativo": null, "quantidade": 0.5 }]))], json!({}));
    assert!(mundo.compilar(&p, &alvo_final("cpu")).is_ok());
    // predefinição de outra fase
    let wa: Alvo = serde_json::from_value(json!({ "tipo": "final", "exportacao": { "predefinicao": "whatsapp", "codificador": "auto", "legendas": "nenhuma", "pacote": null, "pdf": "a3", "revisado_por": "", "personalizada": null } })).unwrap();
    if matches!(&wa, Alvo::Final { exportacao } if crate::edicao::motor::saida::recusa(exportacao).is_some()) {
        assert!(mundo.compilar(&p, &wa).unwrap_err().to_string().contains("nao_suportado: predefinicao whatsapp"));
    }
    // ocultar ativo sem caixa: nunca sai quadro descoberto calado
    let r = rastreio("r1", Some("m1"), "fonte", vec![ancora(t.tempo(10), Some(10), [0.1, 0.1, 0.2, 0.2])]);
    let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([ocultar("o1", "r1", "solido", json!({}))]))], vec![trecho("c1", "m1", t, 0, 30, 1.0, json!([]))], json!({ "rastreios": [r] }));
    assert!(mundo.compilar(&p, &alvo_final("cpu")).unwrap_err().to_string().contains("sem_cobertura: o1"));
}

// ------------------------------------------------------------------ validação = motor (contratos (h) 1 e 2)

const CONTRATO: &str = include_str!("../../../tests/fixtures/edicao/contrato-todas-variantes.json");

/// Pedaço da fixture de contrato (todas as variantes do modelo) pelo ponteiro JSON.
fn da_fixture(ponteiro: &str) -> Value {
    let v: Value = serde_json::from_str(CONTRATO).unwrap();
    v.pointer(ponteiro).unwrap_or_else(|| panic!("{ponteiro} fora da fixture")).clone()
}

fn com_id(mut v: Value, id: &str) -> Value {
    v["id"] = json!(id);
    v
}

/// Efeito da fixture, ligado, sem `ativo` e com o id dado.
/// O motor ainda recusa o efeito (`json`)? A regra é a do pacote dono (contratos-fase1 (c)): o caso some sozinho quando
/// o pacote entrega.
fn ainda_recusa(efeito: &Value, com_fonte: bool) -> bool {
    let e: crate::edicao::modelo::Efeito = serde_json::from_value(efeito.clone()).expect("efeito");
    crate::edicao::motor::clipe::recusa_do_efeito(&e, com_fonte).is_some()
}

fn efeito_da_fixture(ponteiro: &str, id: &str) -> Value {
    let mut e = da_fixture(ponteiro);
    e["id"] = json!(id);
    e["ligado"] = json!(true);
    e["ativo"] = Value::Null;
    e
}

/// Uma recusa do motor: o arquivo do sítio e o pedaço da mensagem dele; o código que a validação tem de dar antes,
/// e onde (clipe, efeito ou mídia do problema).
struct Recusa {
    arquivo: &'static str,
    motor: &'static str,
    codigo: &'static str,
    clipe: Option<&'static str>,
    efeito: Option<&'static str>,
    midia: Option<&'static str>,
    p: ProjetoEdicao,
    alvo: Alvo,
}

/// Mídias do ensaio: m1 (vídeo com som), pc (nv12 de faixa cheia: sem cadeia de cor), foto e n1 (só som).
pub(crate) fn mundo_das_recusas() -> (Mundo, Vec<Value>) {
    let cap = {
        let mut c = apoio::cap_falsa();
        for f in ["perspective", "vidstabdetect", "vidstabtransform", "lenscorrection", "bwdif", "tmix", "unsharp"] {
            c.filtros.insert(f.into());
        }
        Arc::new(c)
    };
    let mut m1 = apoio::fonte(apoio::tabela((1, 30), (0..300).collect()), (320, 180));
    m1.audio = Some(crate::edicao::motor::AudioInfo { inicio_s: 0.0, taxa: 48000, canais: 2 });
    let mut pc = m1.clone();
    (pc.pix_fmt, pc.faixa) = ("nv12".into(), crate::edicao::modelo::Faixa::Pc);
    let mut foto = apoio::fonte(apoio::tabela((1, 1), vec![0]), (400, 300));
    foto.pix_fmt = "rgb24".into();
    let mut n1 = apoio::fonte(apoio::tabela((1, 1), vec![]), (0, 0));
    n1.audio = m1.audio.clone();
    let midias = vec![midia("m1", "video", json!([])), midia("pc", "video", json!([])), midia("foto", "imagem", json!([])), midia("n1", "audio", json!([]))];
    (Mundo::novo(&cap).com("m1", &m1).com("pc", &pc).com("foto", &foto).com("n1", &n1), midias)
}

/// Um caso por sítio de recusa do motor (`nao_suportado`, `pixel_nao_suportado` e item desconhecido).
fn recusas(mundo: &Mundo, midias: &[Value]) -> Vec<Recusa> {
    let t = &mundo.fontes["m1"].quadros;
    let tr = |id: &str, efeitos: Value| trecho(id, "m1", t, 0, 30, 1.0, efeitos);
    let r1 = rastreio("r1", Some("m1"), "fonte", vec![ancora(0.0, Some(0), [0.1, 0.1, 0.2, 0.2])]);
    let base = |principal: Vec<Value>, extra: Value| {
        let mut extra = extra;
        if extra.get("rastreios").is_none() {
            extra["rastreios"] = json!([r1.clone()]);
        }
        projeto(320, 180, (30, 1), midias.to_vec(), principal, extra)
    };
    let com_som = |mut p: ProjetoEdicao| {
        p.saida.audio = true;
        p
    };
    let titulo_sob = |efeitos: Value| json!({ "sobreposicoes": [{ "id": "t1", "nome": "", "oculta": false, "clipes": [{ "tipo": "titulo", "id": "x1", "rotulo": "",
        "pos": { "ref": "programa", "t": 0.0 }, "duracao_s": 0.5, "transformacao": { "x": 0.5, "y": 0.5, "escala": 1.0, "rotacao": 0.0, "opacidade": 1.0 },
        "entrada_s": 0.0, "saida_s": 0.0, "efeitos": efeitos, "titulo": { "modelo": "livre", "fundo": { "tipo": "transparente" }, "blocos": [] } }] }] });
    let video_sob = |audio: Value| {
        let mut v = da_fixture("/sobreposicoes/0/clipes/0");
        for (k, x) in [("midia", json!("m1")), ("pos", json!({ "ref": "programa", "t": 0.0 })), ("duracao_s", json!(0.5)), ("sincronia", Value::Null), ("audio", audio), ("inicio_s", json!(0.0)), ("inicio_pts", json!(0))] {
            v[k] = x;
        }
        json!({ "sobreposicoes": [{ "id": "t1", "nome": "", "oculta": false, "clipes": [v] }] })
    };
    let mut tela = da_fixture("/principal/4");
    for (k, x) in [("inicio_s", json!(0.0)), ("fim_s", json!(1.0)), ("inicio_pts", json!(0)), ("fim_pts", json!(30)), ("velocidade", json!({ "tipo": "constante", "fator": 1.0 })),
                   ("celulas", json!([{ "midia": "m1", "sincronia": null, "rotulo": "", "efeitos": [] }, { "midia": "m1", "sincronia": null, "rotulo": "", "efeitos": [] }])),
                   ("audio", json!({ "volume": 1.0, "mudo": false, "manter_tom": true, "silencios": [] }))] {
        tela[k] = x;
    }
    let congelado = |efeitos: Value| json!({ "tipo": "congelar", "id": "c2", "rotulo": "", "midia": "m1", "instante_s": t.tempo(10), "instante_pts": t.pts[10], "duracao_s": 1.0, "efeitos": efeitos });
    let fundir = json!({ "tipo": "fundir", "duracao_s": 0.5, "audio": "acompanhar" });
    let mut c2_fundir = tr("c2", json!([]));
    c2_fundir["transicao_entrada"] = fundir.clone();
    let zoom = json!([{ "t": 0.0, "v": caixa([0.0, 0.0, 1.0, 1.0]) }, { "t": t.tempo(29), "v": caixa([0.25, 0.25, 0.25, 0.5]) }]);
    let mut silhueta = efeito_da_fixture("/principal/0/efeitos/9", "h1");
    silhueta["alvo"]["rastreio"] = json!("r1");
    let mut com_silencio = tr("c1", json!([]));
    com_silencio["audio"]["silencios"] = json!([{ "id": "s1", "de": 0.2, "ate": 0.4, "modo": "tom", "motivo": "nome" }]);
    let mut volume_animado = tr("c1", json!([]));
    volume_animado["audio"]["volume"] = json!([{ "t": 0.0, "v": 1.0 }, { "t": 0.5, "v": 0.5 }]);
    let narracao = |abaixa: bool| json!({ "audios": [{ "id": "a1", "nome": "Narração", "papel": "narracao", "volume": 1.0, "mudo": false, "abaixa_original": abaixa,
        "clipes": [{ "id": "ca1", "nome": "", "midia": "n1", "pos": { "ref": "programa", "t": 0.0 }, "de_s": 0.0, "ate_s": null, "volume": 1.0, "entrada_s": 0.0, "saida_s": 0.0 }] }] });
    let mixagem = |normalizar: Value, limitador: bool| json!({ "mixagem": { "volume_original": 1.0, "abaixar": { "db": -12.0, "ataque_s": 0.3, "soltura_s": 0.5 }, "normalizar": normalizar, "limitador": limitador } });
    let fim = alvo_final("cpu");
    let whatsapp: Alvo = serde_json::from_value(json!({ "tipo": "final", "exportacao": { "predefinicao": "whatsapp", "codificador": "cpu", "legendas": "nenhuma", "pacote": null, "pdf": "a3", "revisado_por": "", "personalizada": null } })).unwrap();
    let r = |arquivo, motor, codigo, (clipe, efeito, midia): (Option<&'static str>, Option<&'static str>, Option<&'static str>), p: ProjetoEdicao, alvo: &Alvo| Recusa { arquivo, motor, codigo, clipe, efeito, midia, p, alvo: alvo.clone() };
    use crate::edicao::mensagens::codigo::{ITEM_DESCONHECIDO as DESC, NAO_SUPORTADO as NS, PIXEL_NAO_SUPORTADO as PIX};
    let mut v = Vec::new();
    // efeitos de origem, lupa, régua e velocidade: `clipe::recusar_de_outra_fase` (um sítio de despacho para todos,
    // pela regra do pacote dono); cada caso só enquanto o pacote recusa
    let origem = [("estabilizar", "/principal/0/efeitos/0"), ("lente", "/principal/0/efeitos/1"), ("desentrelacar", "/principal/0/efeitos/2"), ("media_temporal", "/principal/0/efeitos/3"), ("ajustes", "/principal/0/efeitos/4"), ("nitidez", "/principal/0/efeitos/5")];
    for (tipo, ponteiro) in origem {
        let e = efeito_da_fixture(ponteiro, "e1");
        if ainda_recusa(&e, true) {
            v.push(r("motor/clipe.rs", tipo, NS, (Some("c1"), Some("e1"), None), base(vec![tr("c1", json!([e]))], json!({})), &fim));
        }
    }
    // a mesma recusa tem um sítio também no rasterizador (`camada/lupa.rs`, `camada/medidas.rs`)
    for (arquivo, tipo, ponteiro) in [("camada/lupa.rs", "lupa", "/principal/0/efeitos/12"), ("camada/medidas.rs", "regua", "/principal/0/efeitos/13"), ("camada/medidas.rs", "velocidade_exibida", "/principal/0/efeitos/14")] {
        let e = efeito_da_fixture(ponteiro, "e1");
        if ainda_recusa(&e, true) {
            v.push(r(arquivo, tipo, NS, (Some("c1"), Some("e1"), None), base(vec![tr("c1", json!([e]))], json!({})), &fim));
        }
    }
    v.push(r("camada/formas.rs", "silhueta", NS, (Some("c1"), Some("h1"), None), base(vec![tr("c1", json!([silhueta]))], json!({})), &fim));
    v.push(r("motor/clipe.rs", "em parte do congelado", NS, (Some("c2"), Some("o1"), None), base(vec![congelado(json!([ocultar("o1", "r1", "solido", json!({ "ativo": { "de": 0.0, "ate": 0.5 } }))]))], json!({})), &fim));
    v.push(r("motor/clipe.rs", "pixel_nao_suportado", PIX, (None, None, Some("pc")), base(vec![trecho("c1", "pc", t, 0, 30, 1.0, json!([]))], json!({})), &fim));
    v.push(r("motor/clipe.rs", "pixel_nao_suportado", PIX, (None, None, Some("pc")), base(vec![json!({ "tipo": "congelar", "id": "c2", "rotulo": "", "midia": "pc", "instante_s": t.tempo(10), "instante_pts": t.pts[10], "duracao_s": 1.0 })], json!({})), &fim));
    v.push(r("motor/clipe.rs", "em clipe sem fonte", NS, (Some("c3"), Some("o3"), None), base(vec![json!({ "tipo": "titulo", "id": "c3", "duracao_s": 1.0, "titulo": { "modelo": "livre", "fundo": { "tipo": "transparente" }, "blocos": [] }, "efeitos": [ocultar("o3", "r1", "solido", json!({}))] })], json!({})), &fim));
    v.push(r("motor/clipe.rs", "clipe tela c5", NS, (Some("c5"), None, None), base(vec![tela.clone()], json!({})), &fim));
    v.push(r("motor/clipe.rs", "item_desconhecido: efeito_do_futuro e99", DESC, (Some("c1"), Some("e99"), None), base(vec![tr("c1", json!([da_fixture("/principal/0/efeitos/20")]))], json!({})), &fim));
    // `fundir` (do `xfade`) entra no catálogo do P1.1 junto com a junção por pedaços
    let sem_xfade = !crate::edicao::motor::juncao::TRANSICOES.contains(&"fundir");
    if sem_xfade {
        v.push(r("motor/juncao.rs", "transicao com sobreposicao no clipe c2", NS, (Some("c2"), None, None), base(vec![tr("c1", json!([])), c2_fundir.clone()], json!({})), &fim));
    }
    let com_legenda = json!({ "legendas": { "estilo": da_fixture("/legendas/estilo"), "lugar": { "x": 0.5, "y": 0.9, "h": "centro", "v": "base" }, "linhas_max": 2, "origem": null,
        "itens": [{ "id": "l1", "texto": "Para!", "locutor": null, "origem": "perito", "conferida": true, "locutor_auto": false, "ref": { "tipo": "programa", "de": 0.1, "ate": 0.5 } }] } });
    let com_legendas: Alvo = serde_json::from_value(json!({ "tipo": "final", "exportacao": { "predefinicao": "juri", "codificador": "cpu", "legendas": "queimar", "pacote": null, "pdf": "a3", "revisado_por": "", "personalizada": null } })).unwrap();
    let p_legenda = base(vec![tr("c1", json!([]))], com_legenda);
    if matches!(&com_legendas, Alvo::Final { exportacao } if crate::edicao::motor::legendas::recusa(&p_legenda, exportacao).is_some()) {
        v.push(r("motor/legendas.rs", "nao_suportado: legendas", NS, (None, None, None), p_legenda, &com_legendas));
    }
    v.push(r("motor/mod.rs", "item_desconhecido: clipe_do_futuro c9", DESC, (Some("c9"), None, None), base(vec![tr("c1", json!([])), da_fixture("/principal/5")], json!({})), &fim));
    // tipo fora de qualquer catálogo: a recusa fica
    v.push(r("motor/juncao.rs", "transicao transicao_do_futuro", NS, (Some("c1"), None, None), base(vec![tr("c1", json!([]))], json!({ "fecho": { "tipo": "transicao_do_futuro", "duracao_s": 0.5, "audio": "acompanhar" } })), &fim));
    if matches!(&whatsapp, Alvo::Final { exportacao } if crate::edicao::motor::saida::recusa(exportacao).is_some()) {
        v.push(r("motor/saida.rs", "predefinicao whatsapp", NS, (None, None, None), base(vec![tr("c1", json!([]))], json!({})), &whatsapp));
    }
    v.push(r("motor/sobrepor.rs", "sobreposto video", NS, (Some("l1"), None, None), base(vec![tr("c1", json!([]))], video_sob(Value::Null)), &fim));
    v.push(r("motor/sobrepor.rs", "em sobreposto x1", NS, (Some("x1"), Some("e18"), None), base(vec![tr("c1", json!([]))], titulo_sob(json!([efeito_da_fixture("/principal/0/efeitos/16", "e18")]))), &fim));
    v.push(r("motor/sobrepor.rs", "item_desconhecido: sobreposto_do_futuro l9", DESC, (Some("l9"), None, None), base(vec![tr("c1", json!([]))], json!({ "sobreposicoes": [{ "id": "t1", "nome": "", "oculta": false, "clipes": [da_fixture("/sobreposicoes/0/clipes/3")] }] })), &fim));
    v.push(r("motor/sobrepor.rs", "item_desconhecido: efeito_do_futuro x9", DESC, (Some("x1"), Some("x9"), None), base(vec![tr("c1", json!([]))], titulo_sob(json!([{ "tipo": "efeito_do_futuro", "id": "x9", "ligado": true }]))), &fim));
    v.push(r("motor/efeitos/enquadramento.rs", "destino variável", NS, (Some("c1"), Some("q1"), None), base(vec![tr("c1", json!([com_id(enquadramento(zoom, "conter"), "q1")]))], json!({})), &fim));
    // som: `audio::montar` (tela: Fase 3; a rampa com som entrou no P3.4; o resto sai quando o P1.5 entrega, pela
    // lista `audio::recusa`)
    let a = crate::edicao::motor::audio::recusa;
    if a("volume_animado") {
        v.push(r("motor/audio.rs", "volume animado", NS, (Some("c1"), None, None), com_som(base(vec![volume_animado], json!({}))), &fim));
    }
    if a("normalizar") {
        v.push(r("motor/audio.rs", "normalizar", NS, (None, None, None), com_som(base(vec![tr("c1", json!([]))], mixagem(json!({ "lufs": -16.0, "pico_db": -1.5 }), false))), &fim));
    }
    if a("limitador") {
        v.push(r("motor/audio.rs", "limitador", NS, (None, None, None), com_som(base(vec![tr("c1", json!([]))], mixagem(Value::Null, true))), &fim));
    }
    if a("transicao") {
        v.push(r("motor/audio.rs", "transicao", NS, (Some("c2"), None, None), com_som(base(vec![tr("c1", json!([])), c2_fundir], json!({}))), &fim));
        v.push(r("motor/audio.rs", "transicao", NS, (Some("c1"), None, None), com_som(base(vec![tr("c1", json!([]))], json!({ "fecho": fundir }))), &fim));
    }
    v.push(r("motor/audio.rs", "tela", NS, (Some("c5"), None, None), com_som(base(vec![tela], json!({}))), &fim));
    if a("silenciar") {
        v.push(r("motor/audio.rs", "silenciar", NS, (Some("c1"), None, None), com_som(base(vec![com_silencio], json!({}))), &fim));
    }
    if a("abaixar") {
        v.push(r("motor/audio.rs", "abaixar", NS, (None, None, None), com_som(base(vec![tr("c1", json!([]))], narracao(true))), &fim));
    }
    v.push(r("motor/audio.rs", "video sobreposto", NS, (Some("l1"), None, None), com_som(base(vec![tr("c1", json!([]))], video_sob(json!({ "volume": 1.0, "mudo": false, "manter_tom": true, "silencios": [] })))), &fim));
    v
}

/// Recusas que repetem um sítio já contado em `recusas`, por outro caminho da validação: `ocultar` em FOTO, destino
/// variável em congelado, foto e título (tempo local), efeito em IMAGEM sobreposta, e alvos de janela (prévia e
/// quadro final em que o vídeo da janela nem passa pelo item recusado).
fn recusas_a_mais(mundo: &Mundo, midias: &[Value]) -> Vec<Recusa> {
    let t = &mundo.fontes["m1"].quadros;
    let tr = |id: &str, efeitos: Value| trecho(id, "m1", t, 0, 30, 1.0, efeitos);
    let r1 = rastreio("r1", Some("m1"), "fonte", vec![ancora(0.0, Some(0), [0.1, 0.1, 0.2, 0.2])]);
    let rl = rastreio("rl", None, "local", vec![ancora(0.0, None, [0.1, 0.1, 0.2, 0.2])]);
    let base = |principal: Vec<Value>, mut extra: Value, som: bool| {
        extra["rastreios"] = json!([r1.clone(), rl.clone()]);
        let mut p = projeto(320, 180, (30, 1), midias.to_vec(), principal, extra);
        p.saida.audio = som;
        p
    };
    let zoom = || com_id(enquadramento(json!([{ "t": 0.0, "v": caixa([0.0, 0.0, 1.0, 1.0]) }, { "t": 0.9, "v": caixa([0.25, 0.25, 0.25, 0.5]) }]), "conter"), "q1");
    let congelado = |efeitos: Value| json!({ "tipo": "congelar", "id": "c2", "rotulo": "", "midia": "m1", "instante_s": t.tempo(10), "instante_pts": t.pts[10], "duracao_s": 1.0, "efeitos": efeitos });
    let foto = |efeitos: Value| json!({ "tipo": "imagem", "id": "c3", "rotulo": "", "midia": "foto", "duracao_s": 1.0, "efeitos": efeitos });
    let titulo = |efeitos: Value| json!({ "tipo": "titulo", "id": "c4", "rotulo": "", "duracao_s": 1.0, "titulo": { "modelo": "livre", "fundo": { "tipo": "transparente" }, "blocos": [] }, "efeitos": efeitos });
    let imagem_sob = json!({ "sobreposicoes": [{ "id": "t1", "nome": "", "oculta": false, "clipes": [{ "tipo": "imagem", "id": "x2", "rotulo": "", "midia": "foto",
        "pos": { "ref": "programa", "t": 0.0 }, "duracao_s": 0.5, "transformacao": { "x": 0.5, "y": 0.5, "escala": 1.0, "rotacao": 0.0, "opacidade": 1.0 },
        "entrada_s": 0.0, "saida_s": 0.0, "efeitos": [efeito_da_fixture("/principal/0/efeitos/16", "e18")] }] }] });
    let fim = alvo_final("cpu");
    use crate::edicao::mensagens::codigo::NAO_SUPORTADO as NS;
    let r = |arquivo, motor, (clipe, efeito): (Option<&'static str>, Option<&'static str>), p: ProjetoEdicao, alvo: &Alvo| Recusa { arquivo, motor, codigo: NS, clipe, efeito, midia: None, p, alvo: alvo.clone() };
    let nitidez = efeito_da_fixture("/principal/0/efeitos/5", "e1");
    let mut v = vec![
        r("motor/clipe.rs", "em clipe sem fonte", (Some("c3"), Some("o3")), base(vec![foto(json!([ocultar("o3", "rl", "solido", json!({}))]))], json!({}), false), &fim),
        r("motor/efeitos/enquadramento.rs", "destino variável", (Some("c2"), Some("q1")), base(vec![congelado(json!([zoom()]))], json!({}), false), &fim),
        r("motor/efeitos/enquadramento.rs", "destino variável", (Some("c3"), Some("q1")), base(vec![foto(json!([zoom()]))], json!({}), false), &fim),
        r("motor/efeitos/enquadramento.rs", "destino variável", (Some("c4"), Some("q1")), base(vec![titulo(json!([zoom()]))], json!({}), false), &fim),
        // quadro final reduzido: o motor mede o destino no canvas do alvo, a validação no da saída
        r("motor/efeitos/enquadramento.rs", "destino variável", (Some("c4"), Some("q1")), base(vec![titulo(json!([zoom()]))], json!({}), false), &Alvo::Quadro { q: 20, largura: 160 }),
        r("motor/sobrepor.rs", "em sobreposto x2", (Some("x2"), Some("e18")), base(vec![tr("c1", json!([]))], imagem_sob, false), &fim),
        r("motor/clipe.rs", "em parte do congelado", (Some("c2"), Some("o1")), base(vec![tr("c1", json!([])), congelado(json!([ocultar("o1", "r1", "solido", json!({ "ativo": { "de": 0.0, "ate": 0.5 } }))]))], json!({}), false), &Alvo::Previa { altura: 90, de_q: 0, ate_q: 10 }),
    ];
    if ainda_recusa(&nitidez, true) {
        v.push(r("motor/clipe.rs", "nitidez e1", (Some("c2"), Some("e1")), base(vec![congelado(json!([nitidez]))], json!({}), false), &fim));
    }
    // prévia de uma janela que não pega a transição nem o congelado: só o som e a proveniência recusam
    if crate::edicao::motor::audio::recusa("transicao") {
        v.push(r("motor/audio.rs", "transicao", (Some("c1"), None), base(vec![tr("c1", json!([]))], json!({ "fecho": { "tipo": "fundir", "duracao_s": 0.2, "audio": "acompanhar" } }), true), &Alvo::Previa { altura: 90, de_q: 5, ate_q: 10 }));
    }
    v
}

/// Sítios de recusa no código (fora dos testes e dos comentários): linhas com `nao_suportado` ou com
/// `erro_do_desconhecido(`, menos as definições.
fn sitios_no_codigo(fonte: &str) -> usize {
    let codigo = fonte.split("#[cfg(test)]").next().unwrap_or("");
    codigo
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("//"))
        .filter(|l| (l.to_lowercase().contains("nao_suportado") || l.contains("erro_do_desconhecido(")) && !l.contains("fn nao_suportado") && !l.contains("\"nao_suportado: {o_que}\"") && !l.contains("pub const NAO_SUPORTADO"))
        .count()
}

/// Tudo o que o motor recusa a validação acusa ANTES, no item, com a frase do catálogo; `compilar` devolve o
/// erro da validação e não chega ao motor. E nenhum sítio de recusa do motor fica sem caso aqui.
#[test]
fn validacao_acusa_antes_tudo_o_que_o_motor_recusa() {
    let (mundo, midias) = mundo_das_recusas();
    let casos = recusas(&mundo, &midias);
    let a_mais = recusas_a_mais(&mundo, &midias);
    let ctx = Contexto { fontes: &mundo.fontes, cap: &mundo.cap, rastreios: &SemRastreios, pasta_projeto: Path::new("/nao/existe"), midias: &[] };
    for c in casos.iter().chain(&a_mais) {
        let rotulo = format!("{} «{}»", c.arquivo, c.motor);
        let linha = tempo::resolver(&c.p, &ctx);
        // o motor sozinho recusa (vídeo ou som)
        let do_motor = if c.arquivo == "motor/audio.rs" {
            crate::edicao::motor::audio::montar(&c.p, &linha, &ctx, &c.alvo).map(|_| ())
        } else {
            compilar_video(&c.p, &linha, &ctx, &c.alvo).map(|_| ())
        }
        .expect_err(&rotulo)
        .to_string();
        assert!(do_motor.contains(c.motor), "{rotulo}: o motor deu {do_motor}");
        // a validação acusa, no item, com a frase do catálogo
        let problemas = crate::edicao::validar::validar(&c.p, &linha, &ctx, Some(&c.alvo));
        let achado = problemas.iter().find(|p| {
            p.nivel == crate::edicao::modelo::Nivel::Erro
                && p.codigo == c.codigo
                && c.clipe.is_none_or(|x| p.clipe.as_deref() == Some(x))
                && c.efeito.is_none_or(|x| p.efeito.as_deref() == Some(x))
                && c.midia.is_none_or(|x| p.midia.as_deref() == Some(x))
        });
        let achado = achado.unwrap_or_else(|| panic!("{rotulo}: a validação não acusa {} no item: {problemas:#?}", c.codigo));
        let frase = crate::edicao::mensagens::frase(achado.codigo, achado.n, None);
        assert!(achado.mensagem == frase || achado.mensagem.starts_with(&format!("{frase}: ")), "{rotulo}: fora do catálogo: {}", achado.mensagem);
        if achado.codigo == crate::edicao::mensagens::codigo::NAO_SUPORTADO {
            assert!(achado.mensagem.starts_with("Ainda não suportado: "), "{rotulo}: sem rótulo no catálogo: {}", achado.mensagem);
        }
        // compilar para na validação: o erro é o primeiro da validação, nunca o do motor
        let primeiro = problemas.iter().find(|p| p.nivel == crate::edicao::modelo::Nivel::Erro).unwrap();
        let e = crate::edicao::motor::compilar(&c.p, &ctx, &c.alvo).expect_err(&rotulo).to_string();
        assert_eq!(e, crate::edicao::motor::erro_da_validacao(primeiro).to_string(), "{rotulo}");
        assert!(!e.contains(&do_motor), "{rotulo}: chegou ao motor");
    }
    eprintln!("validação = motor: {} recusas do motor acusadas antes pela validação", casos.len() + a_mais.len());
    // trilha de sobreposição oculta não entra no render: vídeo sobreposto com som nela não é recusado por ninguém
    let mut oculta = casos.iter().find(|c| c.motor == "video sobreposto").expect("caso do vídeo sobreposto com som").p.clone();
    assert!(crate::edicao::motor::compilar(&oculta, &ctx, &alvo_final("cpu")).is_err());
    oculta.sobreposicoes[0].oculta = true;
    let plano = crate::edicao::motor::compilar(&oculta, &ctx, &alvo_final("cpu")).expect("trilha oculta compila");
    assert!(plano.audio.is_some());
    // cada sítio de recusa do código tem caso
    let fontes: [(&str, &str); 12] = [
        ("motor/audio.rs", include_str!("../motor/audio.rs")),
        ("motor/clipe.rs", include_str!("../motor/clipe.rs")),
        ("motor/mod.rs", include_str!("../motor/mod.rs")),
        ("motor/juncao.rs", include_str!("../motor/juncao.rs")),
        ("motor/legendas.rs", include_str!("../motor/legendas.rs")),
        ("motor/saida.rs", include_str!("../motor/saida.rs")),
        ("motor/sobrepor.rs", include_str!("../motor/sobrepor.rs")),
        ("motor/efeitos/enquadramento.rs", include_str!("../motor/efeitos/enquadramento.rs")),
        ("motor/efeitos/regiao.rs", include_str!("../motor/efeitos/regiao.rs")),
        ("camada/formas.rs", include_str!("../camada/formas.rs")),
        ("camada/lupa.rs", include_str!("../camada/lupa.rs")),
        ("camada/medidas.rs", include_str!("../camada/medidas.rs")),
    ];
    // o despacho por tipo do `clipe.rs` (`recusar_de_outra_fase`) serve aos efeitos de origem, à lupa, à régua e à
    // velocidade: um sítio enquanto algum deles ainda for recusado (os de lupa, régua e velocidade levam o nome
    // do sítio do rasterizador)
    let despachados = casos.iter().filter(|c| c.arquivo == "motor/clipe.rs" && origem_do_clipe().contains(&c.motor)).count()
        + casos.iter().filter(|c| c.arquivo == "camada/lupa.rs" || c.arquivo == "camada/medidas.rs").count();
    for (arquivo, fonte) in fontes {
        let mut no_codigo = sitios_no_codigo(fonte);
        let com_caso = casos.iter().filter(|c| c.arquivo == arquivo).count();
        let com_caso = if arquivo == "motor/clipe.rs" {
            let origem = casos.iter().filter(|c| c.arquivo == arquivo && origem_do_clipe().contains(&c.motor)).count();
            if despachados == 0 {
                // nada mais a despachar: o sítio fica para fases seguintes sem pesar aqui
                no_codigo -= sitios_de_despacho(fonte);
            }
            com_caso - origem + usize::from(despachados > 0)
        } else {
            com_caso
        };
        assert_eq!(no_codigo, com_caso, "{arquivo}: {no_codigo} sítios de recusa no código, {com_caso} com caso aqui");
    }
}

/// `ativo` por uma regra só (contratos (h) 3): a validação conta os quadros da ocultação como o motor
/// (`efeitos::base_ativa`, intervalo fechado no instante do quadro), e o `ocultar_interpolado` bate com
/// `feito.ocultacoes[].quadros.interpolado`. `ativo` começando no meio de um quadro: esse quadro não conta.
#[test]
fn ocultar_interpolado_bate_com_a_proveniencia() {
    let (mundo, mut midias) = mundo_das_recusas();
    let t = &mundo.fontes["m1"].quadros;
    // âncoras nos quadros 30 e 60: 31..59 por interpolação; depois do 60 o manual segura
    let r = rastreio("r1", Some("m1"), "fonte", vec![ancora(t.tempo(30), Some(t.pts[30]), [0.1, 0.1, 0.2, 0.2]), ancora(t.tempo(60), Some(t.pts[60]), [0.5, 0.1, 0.2, 0.2])]);
    let meio = 0.5 / 30.0;
    midias[0]["ocultacoes"] = json!([ocultar("om", "r1", "solido", json!({ "ativo": { "de": t.tempo(40) + meio, "ate": t.tempo(70) } }))]);
    let p = projeto(
        320,
        180,
        (30, 1),
        midias,
        vec![
            trecho("c1", "m1", t, 30, 90, 1.0, json!([ocultar("oc", "r1", "solido", json!({ "ativo": { "de": t.tempo(50) + 0.2 / 30.0, "ate": null } }))])),
            trecho("c2", "m1", t, 20, 80, 0.5, json!([])),
            json!({ "tipo": "congelar", "id": "c3", "rotulo": "", "midia": "m1", "instante_s": t.tempo(45), "instante_pts": t.pts[45], "duracao_s": 1.0 }),
        ],
        json!({ "rastreios": [r] }),
    );
    let ctx = Contexto { fontes: &mundo.fontes, cap: &mundo.cap, rastreios: &SemRastreios, pasta_projeto: Path::new("/nao/existe"), midias: &[] };
    let plano = mundo.compilar(&p, &alvo_final("cpu")).expect("compila");
    let prov: u64 = plano.proveniencia.ocultacoes.iter().map(|o| o.quadros.interpolado as u64).sum();
    let linha = tempo::resolver(&p, &ctx);
    let problemas = crate::edicao::validar::validar(&p, &linha, &ctx, Some(&alvo_final("cpu")));
    let val: u64 = problemas.iter().filter(|x| x.codigo == crate::edicao::mensagens::codigo::OCULTAR_INTERPOLADO).map(|x| x.n.unwrap_or(0)).sum();
    // om: quadros 41..59 em c1 e em c2 (19 + 19) e o 45 congelado (1); oc: 51..59 (9)
    assert_eq!((val, prov), (48, 48), "{problemas:#?}");
    assert!(!crate::edicao::validar::tem_erro(&problemas), "{problemas:#?}");
    // `compilar` leva os periciais à proveniência, um por código, com a soma
    let completo = crate::edicao::motor::compilar(&p, &ctx, &alvo_final("cpu")).expect("compila");
    assert_eq!(
        completo.proveniencia.avisos,
        vec![crate::edicao::motor::plano::ProvAviso { codigo: crate::edicao::mensagens::codigo::OCULTAR_INTERPOLADO.into(), n: 48 }]
    );
}

/// A proveniência é sempre a do programa inteiro (contratos (h) 6): a prévia e o quadro final dão a mesma da
/// exportação com a exportação do projeto (tamanho do projeto, todos os slots, predefinição, `com_perdas`).
#[test]
fn proveniencia_do_programa_inteiro_em_todo_alvo() {
    let (mundo, midias) = mundo_das_recusas();
    let t = &mundo.fontes["m1"].quadros;
    let mut p = projeto(320, 180, (30, 1), midias, vec![trecho("c1", "m1", t, 0, 60, 1.0, json!([])), trecho("c2", "m1", t, 100, 160, 0.5, json!([]))], json!({}));
    p.saida.audio = true;
    let ctx = Contexto { fontes: &mundo.fontes, cap: &mundo.cap, rastreios: &SemRastreios, pasta_projeto: Path::new("/nao/existe"), midias: &[] };
    let final_ = crate::edicao::motor::compilar(&p, &ctx, &Alvo::Final { exportacao: p.saida.exportacao.clone() }).expect("final");
    assert_eq!((final_.proveniencia.saida.largura, final_.proveniencia.saida.n_quadros), (320, 180));
    for alvo in [Alvo::Previa { altura: 90, de_q: 50, ate_q: 70 }, Alvo::Quadro { q: 130, largura: 160 }] {
        let x = crate::edicao::motor::compilar(&p, &ctx, &alvo).expect("compila");
        assert!(x.canvas.w < 320 && x.total_q < 180, "{alvo:?}: o render é o da janela");
        assert_eq!(x.proveniencia, final_.proveniencia, "{alvo:?}");
    }
    // a tolerância do `atempo` é a do programa (o trecho a 0,5× está fora da janela da prévia)
    assert!(final_.proveniencia.audio.tolerancia_atempo_ms > 0.0);
    assert_eq!(final_.audio.as_ref().unwrap().tolerancia_atempo_ms, final_.proveniencia.audio.tolerancia_atempo_ms);
}

/// A proveniência do programa inteiro leva os mesmos avisos periciais em todo alvo (contratos (h) 6).
#[test]
fn proveniencia_com_periciais_em_todo_alvo() {
    use crate::edicao::modelo::{EstadoMidia, SituacaoMidia};
    use crate::edicao::quadros::{Busca, OrigemTabela};
    let (mundo, mut midias) = mundo_das_recusas();
    let t = &mundo.fontes["m1"].quadros;
    let r = rastreio("r1", Some("m1"), "fonte", vec![ancora(t.tempo(30), Some(t.pts[30]), [0.1, 0.1, 0.2, 0.2]), ancora(t.tempo(60), Some(t.pts[60]), [0.5, 0.1, 0.2, 0.2])]);
    midias[0]["ocultacoes"] = json!([ocultar("om", "r1", "desfoque", json!({ "conteudo": "placa", "intensidade": 0.3 }))]);
    let mut p = projeto(320, 180, (30, 1), midias, vec![trecho("c1", "m1", t, 30, 90, 1.0, json!([])), trecho("c2", "m1", t, 100, 160, 0.5, json!([]))], json!({ "rastreios": [r] }));
    p.saida.audio = true;
    let estados = vec![EstadoMidia { id: "m1".into(), estado: SituacaoMidia::Ok, tabela: Some(OrigemTabela::Decodificador), busca: Some(Busca::Confiavel), erros: Some(3) }];
    let ctx = Contexto { fontes: &mundo.fontes, cap: &mundo.cap, rastreios: &SemRastreios, pasta_projeto: Path::new("/nao/existe"), midias: &estados };
    let final_ = crate::edicao::motor::compilar(&p, &ctx, &Alvo::Final { exportacao: p.saida.exportacao.clone() }).expect("final");
    let codigos: Vec<(&str, u64)> = final_.proveniencia.avisos.iter().map(|a| (a.codigo.as_str(), a.n)).collect();
    assert_eq!(codigos, vec![("ocultacao_reversivel", 1), ("ocultacao_fraca", 1), ("ocultar_interpolado", 29), ("fonte_danificada", 3)]);
    for alvo in [Alvo::Previa { altura: 90, de_q: 150, ate_q: 170 }, Alvo::Quadro { q: 130, largura: 160 }, Alvo::Quadro { q: 5, largura: 160 }] {
        let x = crate::edicao::motor::compilar(&p, &ctx, &alvo).expect("compila");
        assert_eq!(x.proveniencia, final_.proveniencia, "{alvo:?}");
    }
}

/// Os campos que a abertura da Fase 1 pôs no `feito` (contratos-fase1 (b) B22) saem do projeto, no programa inteiro:
/// `fecho`, som de cada trecho (nenhum na foto), mixagem e trilhas que tocam (só com áudio na saída), ausências aceitas
/// do rastreio e margem, borda, cor e rastreio da ocultação da mídia.
#[test]
fn proveniencia_leva_os_campos_da_abertura_da_fase_1() {
    use crate::edicao::modelo::{Animavel, MotivoAusencia, PapelAudio};
    let (mundo, mut midias) = mundo_das_recusas();
    let t = &mundo.fontes["m1"].quadros;
    let mut r = rastreio("r1", Some("m1"), "fonte", vec![ancora(t.tempo(30), Some(t.pts[30]), [0.1, 0.1, 0.2, 0.2]), ancora(t.tempo(60), Some(t.pts[60]), [0.5, 0.1, 0.2, 0.2])]);
    r["ausencias"] = json!([{ "de_s": 1.2, "ate_s": 1.5, "motivo": "fora_de_cena", "aceita_em": "2026-11-14T10:00:00Z", "por": "Perito" }]);
    midias[0]["ocultacoes"] = json!([ocultar("om", "r1", "solido", json!({ "cor": "#112233ff",
        "alvo": { "rastreio": "r1", "forma": "elipse", "vertices": null, "margem": 0.15, "borda_suave": 0.2, "inverter": false } }))]);
    let mut c1 = trecho("c1", "m1", t, 30, 90, 1.0, json!([]));
    c1["audio"] = json!({ "volume": 0.5, "mudo": false, "manter_tom": true, "silencios": [] });
    let foto = json!({ "tipo": "imagem", "id": "c2", "rotulo": "", "midia": "foto", "duracao_s": 1.0, "efeitos": [] });
    let fecho = json!({ "tipo": "escurecer", "duracao_s": 0.3, "audio": "acompanhar" });
    let narracao = json!([{ "id": "a1", "nome": "Narração do perito", "papel": "narracao", "volume": 0.9, "mudo": false, "abaixa_original": false,
        "clipes": [{ "id": "ca1", "nome": "fala", "midia": "n1", "pos": { "ref": "programa", "t": 0.5 }, "de_s": 0.25, "ate_s": 1.0, "volume": 0.7, "entrada_s": 0.1, "saida_s": 0.2 }] }]);
    let mut p = projeto(320, 180, (30, 1), midias, vec![c1, foto], json!({ "rastreios": [r], "fecho": fecho, "audios": narracao }));
    p.saida.audio = true;
    let pv = mundo.compilar(&p, &alvo_final("cpu")).expect("compila").proveniencia;
    assert_eq!(pv.fecho.as_ref().map(|f| (f.tipo.as_str(), f.duracao_s)), Some(("escurecer", 0.3)));
    assert_eq!(pv.roteiro[0].audio.as_ref().map(|a| (a.volume.clone(), a.mudo)), Some((Animavel::Fixo(0.5), false)));
    assert_eq!(pv.roteiro[1].audio, None, "foto não tem som de fonte");
    assert_eq!(pv.audio.mixagem.as_ref(), Some(&p.mixagem));
    let tr = &pv.audio.trilhas;
    assert_eq!((tr.len(), tr[0].id.as_str(), &tr[0].papel, tr[0].volume, tr[0].clipes.len()), (1, "a1", &PapelAudio::Narracao, 0.9, 1));
    let ca = &tr[0].clipes[0];
    // 0,5 s a 30 qps = slot 15; 0,75 s de som = 23 slots (o resolvedor arredonda)
    assert_eq!((ca.id.as_str(), ca.midia.as_str(), ca.de_q, ca.de_s, ca.ate_s, ca.entrada_s, ca.saida_s), ("ca1", "n1", 15, 0.25, Some(1.0), 0.1, 0.2));
    assert!(ca.n_q > 0);
    assert_eq!(ca.volume, Animavel::Fixo(0.7));
    let feito = serde_json::to_string(&pv).unwrap();
    assert!(!feito.contains("Narração do perito") && !feito.contains("\"fala\""), "nome de trilha e de clipe de áudio é texto livre: fora do feito");
    assert_eq!(pv.rastreios.len(), 1);
    assert_eq!(pv.rastreios[0].ausencias.iter().map(|a| (a.de_s, a.ate_s, a.motivo)).collect::<Vec<_>>(), vec![(1.2, 1.5, MotivoAusencia::ForaDeCena)]);
    let o = &pv.ocultacoes[0];
    assert_eq!((o.margem, o.borda_suave, o.cor.as_str(), o.rastreio.as_str()), (0.15, 0.2, "#112233ff", "r1"));
    // sem áudio na saída: nem mixagem nem trilhas (o som do trecho segue no roteiro, como os silêncios)
    p.saida.audio = false;
    let sem = mundo.compilar(&p, &alvo_final("cpu")).expect("compila").proveniencia;
    assert_eq!((sem.audio.mixagem.is_none(), sem.audio.trilhas.len()), (true, 0));
}

/// Sobrepostos e legendas no `feito`: cada clipe sobreposto que entra no vídeo, um por um (os de trilha oculta e os
/// fora do programa não); as legendas com o modo que saiu, contando só as que saem. Resolução e tamanho da mídia.
#[test]
fn proveniencia_lista_sobrepostos_e_legendas_que_saem() {
    use crate::edicao::modelo::{ModeloTitulo, ModoLegendas};
    let (mut mundo, midias) = mundo_das_recusas();
    mundo.fontes.get_mut("m1").unwrap().tamanho = 5_129_295;
    let t = &mundo.fontes["m1"].quadros;
    let estilo = json!({ "fonte": "sans", "negrito": true, "italico": false, "tamanho": 0.05, "cor": "#ffffff", "alinhamento": "centro", "caixa": null, "contorno": null });
    let lugar = json!({ "x": 0.5, "y": 0.9, "h": "centro", "v": "base" });
    let tr = json!({ "x": 0.5, "y": 0.5, "escala": 1.0, "rotacao": 0.0, "opacidade": 1.0 });
    let bloco = |id: &str, texto: &str| json!({ "id": id, "papel": "titulo", "conteudo": texto, "estilo": estilo, "lugar": lugar, "largura_max": 0.9 });
    let sob_titulo = |id: &str, pos: Value, dur: Value, modelo: &str, textos: &[&str]| {
        json!({ "tipo": "titulo", "id": id, "rotulo": format!("rótulo {id}"), "pos": pos, "duracao_s": dur, "transformacao": tr, "entrada_s": 0.0, "saida_s": 0.0, "efeitos": [],
                "titulo": { "modelo": modelo, "fundo": { "tipo": "transparente" }, "blocos": textos.iter().enumerate().map(|(i, x)| bloco(&format!("{id}b{i}"), x)).collect::<Vec<_>>() } })
    };
    let sob_imagem = |id: &str, pos: Value, dur: Value| json!({ "tipo": "imagem", "id": id, "rotulo": "Brasão", "pos": pos, "duracao_s": dur, "transformacao": tr, "entrada_s": 0.0, "saida_s": 0.0, "efeitos": [], "midia": "foto" });
    let sobreposicoes = json!([
        { "id": "s1", "nome": "S1", "oculta": false, "clipes": [
            // terço inferior preso ao c2 até o fim dele; marca no programa inteiro; imagem que começa depois do fim
            sob_titulo("t1", json!({ "ref": "clipe", "clipe": "c2", "desloc": 0.0 }), json!(null), "terco_inferior", &["Rua das Flores", "", "Portão 2"]),
            sob_imagem("i1", json!({ "ref": "programa", "t": 0.0 }), json!(null)),
            sob_imagem("i2", json!({ "ref": "programa", "t": 50.0 }), json!(1.0)),
        ] },
        { "id": "s2", "nome": "S2", "oculta": true, "clipes": [sob_titulo("t9", json!({ "ref": "programa", "t": 0.0 }), json!(1.0), "livre", &["oculto"])] },
    ]);
    let legenda = |id: &str, origem: &str, conferida: bool, r: Value| json!({ "id": id, "texto": format!("fala {id}"), "locutor": null, "origem": origem, "conferida": conferida, "locutor_auto": false, "ref": r });
    let legendas = json!({ "estilo": estilo, "lugar": lugar, "linhas_max": 2, "origem": { "audio_sha256": "c".repeat(64), "importado_em": "2026-11-14T03:00:00Z", "motor": "whisper", "modelo_sha256": null },
        "itens": [
            legenda("g1", "ia", true, json!({ "tipo": "programa", "de": 0.5, "ate": 1.5 })),
            legenda("g2", "perito", false, json!({ "tipo": "fonte", "midia": "m1", "de": t.tempo(40), "ate": t.tempo(50) })),
            // fora do programa (3 s) e de trecho que não está na linha: não saem
            legenda("g3", "ia", true, json!({ "tipo": "programa", "de": 40.0, "ate": 41.0 })),
            legenda("g4", "perito", true, json!({ "tipo": "fonte", "midia": "m1", "de": t.tempo(200), "ate": t.tempo(210) })),
        ] });
    // estabilização com o `.trf` já gravado: vai para `feito.estabilizacoes`
    let estab = efeito("estabilizar", "est1", json!({ "suavizar": 10, "borda": "preta", "zoom_pct": 0.0, "trf_sha256": "a".repeat(64),
        "analise": { "de_s": t.tempo(30), "ate_s": t.tempo(59), "de_pts": t.pts[30], "ate_pts": t.pts[59] } }));
    let principal = vec![trecho("c1", "m1", t, 30, 60, 1.0, json!([estab])), trecho("c2", "m1", t, 100, 160, 1.0, json!([]))];
    let p = projeto(320, 180, (30, 1), midias, principal, json!({ "sobreposicoes": sobreposicoes, "legendas": legendas }));
    let com = |modo: &str| {
        let mut e = exportacao("cpu");
        e["legendas"] = json!(modo);
        mundo.compilar(&p, &serde_json::from_value(json!({ "tipo": "final", "exportacao": e })).unwrap()).unwrap_or_else(|e| panic!("{modo}: {e}")).proveniencia
    };
    let pv = com("queimar");
    // um por clipe que entra, na ordem das trilhas; nem o da trilha oculta nem o que cai fora do programa
    let s = &pv.sobrepostos;
    assert_eq!(s.iter().map(|x| (x.clipe.as_str(), x.tipo.as_str(), x.trilha.as_str())).collect::<Vec<_>>(), vec![("t1", "titulo", "s1"), ("i1", "imagem", "s1")]);
    assert_eq!((s[0].na_saida.de_q, s[0].na_saida.n_q, s[0].na_saida.de_s, s[0].na_saida.ate_s), (30, 60, 1.0, 3.0), "preso ao c2 até o fim dele");
    assert_eq!((s[0].preso_a.as_deref(), s[0].modelo, s[0].midia.as_deref(), s[0].nome.as_str()), (Some("c2"), Some(ModeloTitulo::TercoInferior), None, "rótulo t1"));
    assert_eq!(s[0].textos, vec!["Rua das Flores".to_string(), "Portão 2".to_string()], "os blocos com texto, na ordem");
    assert_eq!((s[1].na_saida.de_q, s[1].na_saida.n_q, s[1].preso_a.as_deref(), s[1].midia.as_deref(), s[1].modelo), (0, 90, None, Some("foto"), None), "marca no programa inteiro");
    assert!(s.iter().all(|x| x.selos.is_empty()));
    // a mesma imagem tratada por IA leva o selo fixo no `feito` (o que a camada global escreve nos slots dela)
    let mut p_ia = p.clone();
    p_ia.midias.iter_mut().find(|m| m.id == "foto").unwrap().derivacao =
        Some(serde_json::from_value(json!({ "tipo": "superres", "de_sha256": "a", "raiz_sha256": "a", "comando": "", "com_perdas": true })).unwrap());
    let ia = mundo.compilar(&p_ia, &alvo_final("cpu")).unwrap().proveniencia.sobrepostos;
    assert_eq!(ia.iter().map(|x| (x.clipe.as_str(), x.selos.clone())).collect::<Vec<_>>(), vec![("t1", vec![]), ("i1", vec!["Imagem ampliada por IA".to_string()])]);
    // a mídia do sobreposto segue citada, com a resolução; a do trecho leva o tamanho do arquivo
    let m = |id: &str| pv.midias.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("mídia {id} fora do feito"));
    assert_eq!((m("foto").largura, m("foto").altura), (400, 300));
    assert_eq!((m("m1").largura, m("m1").altura, m("m1").tamanho), (320, 180, 5_129_295));
    // legendas: o modo que saiu e só as que aparecem no programa (g1 de programa, g2 de fonte dentro do c1)
    let l = &pv.legendas;
    assert_eq!((l.modo, l.n, l.conferidas, l.no_projeto, l.ia, l.ia_conferidas), (ModoLegendas::Queimar, 2, 1, 4, 1, 1));
    for (modo, esperado) in [("arquivo", ModoLegendas::Arquivo), ("ambos", ModoLegendas::Ambos)] {
        let l = com(modo).legendas;
        assert_eq!((l.modo, l.n), (esperado, 2), "{modo}");
    }
    // sem legendas na exportação: nenhuma saiu, e o `feito` diz quantas ficaram no projeto
    let l = com("nenhuma").legendas;
    assert_eq!((l.modo, l.n, l.conferidas, l.no_projeto, l.ia), (ModoLegendas::Nenhuma, 0, 0, 4, 0));
    assert_eq!(pv.estabilizacoes.iter().map(|e| (e.efeito.as_str(), e.trf_sha256.clone())).collect::<Vec<_>>(), vec![("est1", Some("a".repeat(64)))]);
    // o `feito` do relatório técnico leva os dois
    let v = serde_json::to_value(&pv).unwrap();
    assert_eq!((v["sobrepostos"].as_array().map(Vec::len), v["legendas"]["modo"].as_str()), (Some(2), Some("queimar")));
    // `feito` gravado antes dos campos novos continua lendo
    let mut antigo = v.clone();
    antigo.as_object_mut().unwrap().remove("sobrepostos");
    for k in ["modo", "no_projeto", "ia", "ia_conferidas"] {
        antigo["legendas"].as_object_mut().unwrap().remove(k);
    }
    let lido: crate::edicao::motor::plano::Proveniencia = serde_json::from_value(antigo).unwrap();
    assert_eq!((lido.sobrepostos.len(), lido.legendas.modo), (0, ModoLegendas::Nenhuma));
}

/// Congelado da Fase 0 depois da abertura (a leitura vem de `realce::leitura` antes de ler, contratos-fase1 (b) B5):
/// o mesmo corte de um quadro só (com pts repetido, o primeiro) e a mesma conferência (1 lido, o pts dele).
#[test]
fn congelado_le_um_quadro_so_como_na_fase_0() {
    let (mundo, midias) = mundo_das_recusas();
    let t = &mundo.fontes["m1"].quadros;
    let congelado = json!({ "tipo": "congelar", "id": "c2", "rotulo": "", "midia": "m1", "instante_s": t.tempo(40), "instante_pts": t.pts[40], "duracao_s": 1.0, "efeitos": [] });
    let p = projeto(320, 180, (30, 1), midias, vec![trecho("c1", "m1", t, 0, 30, 1.0, json!([])), congelado], json!({}));
    let plano = mundo.compilar(&p, &alvo_final("cpu")).expect("compila");
    let (pj, b) = (t.pts[40], &plano.blocos[0]);
    assert!(b.grafo.contains(&format!("trim=start_pts={pj}:end_pts={},trim=end_frame=1,showinfo@c001=", pj + 1)), "{}", b.grafo);
    let c2 = b.conferir.iter().find(|c| c.clipe == "c2").expect("conferência do congelado");
    assert_eq!((c2.lidos, c2.pts.clone()), (1, vec![pj]));
    // a prévia não confere pts
    let previa = mundo.compilar(&p, &Alvo::Previa { altura: 90, de_q: 30, ate_q: 40 }).expect("prévia");
    assert_eq!(previa.blocos[0].conferir.iter().find(|c| c.clipe == "c2").map(|c| (c.lidos, c.pts.len())), Some((1, 0)));
}

/// `processamentos[].ordem` é a da cadeia que o grafo aplica (doc de `ProvProcessamento`), não a da lista do clipe:
/// ocultar → giro → enquadramento → holofote → anotações. O `ocultar` do congelado que não pega nenhum slot não
/// entra na cadeia e não sai como processamento.
#[test]
fn processamentos_na_ordem_do_grafo() {
    let (mundo, midias) = mundo_das_recusas();
    let t = &mundo.fontes["m1"].quadros;
    let r = rastreio("r1", Some("m1"), "fonte", vec![ancora(t.tempo(30), Some(t.pts[30]), [0.3, 0.3, 0.2, 0.2])]);
    let destaque = efeito("destaque", "d1", json!({ "forma": "retangulo", "rastreio": "r1", "traco": { "cor": "#00ff00", "espessura": 0.006, "contorno": true }, "tamanho": 0.1, "angulo_graus": 0.0, "margem": 0.0, "rotulo": null }));
    let holofote = efeito("holofote", "h1", json!({ "alvo": { "rastreio": "r1", "forma": "elipse", "vertices": null, "margem": 0.0, "borda_suave": 0.0, "inverter": false }, "escurecer": 0.5, "dessaturar": 0.5 }));
    let girar = efeito("girar", "g1", json!({ "graus": 90, "espelhar_h": false, "espelhar_v": false }));
    let enq = enquadramento(json!({ "x": 0.1, "y": 0.1, "w": 0.6, "h": 0.6 }), "cobrir");
    let carimbo = efeito("carimbo", "k1", json!({ "mostra": "tempo_fonte", "formato": "hms_ms", "relogio": null, "lugar": { "x": 0.05, "y": 0.95, "h": "esq", "v": "base" },
        "estilo": { "fonte": "mono", "negrito": false, "italico": false, "tamanho": 0.04, "cor": "#ffffff", "alinhamento": "esq", "caixa": null, "contorno": null } }));
    // a lista do clipe na ordem em que o front acrescenta (sempre no fim)
    let lista = json!([destaque, carimbo, holofote, enq, girar, ocultar("o1", "r1", "solido", json!({}))]);
    let congelado = json!({ "tipo": "congelar", "id": "c2", "rotulo": "", "midia": "m1", "instante_s": t.tempo(40), "instante_pts": t.pts[40], "duracao_s": 1.0,
        "efeitos": [destaque_com_id("d2"), ocultar("o2", "r1", "solido", json!({ "ativo": { "de": 3.0, "ate": 4.0 } }))] });
    let p = projeto(320, 180, (30, 1), midias, vec![trecho("c1", "m1", t, 30, 90, 1.0, lista), congelado], json!({ "rastreios": [r] }));
    let plano = mundo.compilar(&p, &alvo_final("cpu")).expect("compila");
    let nomes = |n: usize| plano.proveniencia.roteiro[n].processamentos.iter().map(|x| (x.ordem, x.nome.clone())).collect::<Vec<_>>();
    let esperado: Vec<(u32, String)> = ["ocultar", "girar", "enquadramento", "holofote", "destaque", "carimbo"].iter().enumerate().map(|(i, n)| (i as u32 + 1, n.to_string())).collect();
    assert_eq!(nomes(0), esperado);
    // a mesma ordem no grafo do clipe c1 (só a parte dele: até a saída `c000v`)
    let g = &plano.blocos[0].grafo;
    let c1 = &g[..g.find("[c000v]").expect("saída do c1")];
    // ocultar (alphamerge) → giro → recorte do enquadramento → holofote (lutyuv do escurecer) → anotação (overlay `pass`)
    let mut desde = 0;
    for marca in ["alphamerge", "transpose=dir=clock", "crop=", "lutyuv=y=clip", "eof_action=pass"] {
        let i = c1[desde..].find(marca).unwrap_or_else(|| panic!("{marca} fora de ordem no grafo do c1:\n{c1}"));
        desde += i + marca.len();
    }
    // congelado: o ocultar fora do `ativo` local não foi aplicado
    assert_eq!(nomes(1), vec![(1, "destaque".to_string())]);
    assert!(plano.proveniencia.ocultacoes.iter().all(|o| o.clipes != vec!["c2".to_string()]));
}

/// Ocultação invertida (contratos (g) 6: cobre o que está FORA da forma) não sai no `tecnico.json` como a comum:
/// `feito.ocultacoes[].inverter` diz qual das duas o grafo aplicou.
#[test]
fn ocultacao_invertida_dita_no_feito() {
    let (mundo, mut midias) = mundo_das_recusas();
    let t = &mundo.fontes["m1"].quadros;
    let r = rastreio("r1", Some("m1"), "fonte", vec![ancora(t.tempo(30), Some(t.pts[30]), [0.3, 0.3, 0.2, 0.2])]);
    let alvo = |inverter: bool| json!({ "rastreio": "r1", "forma": "elipse", "vertices": null, "margem": 0.0, "borda_suave": 0.0, "inverter": inverter });
    midias[0]["ocultacoes"] = json!([ocultar("om", "r1", "desfoque", json!({ "alvo": alvo(true), "conteudo": "rosto" }))]);
    let c2 = trecho("c2", "m1", t, 100, 130, 1.0, json!([ocultar("oc", "r1", "solido", json!({ "alvo": alvo(false) }))]));
    let p = projeto(320, 180, (30, 1), midias, vec![trecho("c1", "m1", t, 30, 90, 1.0, json!([])), c2], json!({ "rastreios": [r] }));
    let plano = mundo.compilar(&p, &alvo_final("cpu")).expect("compila");
    let feito: Vec<(String, Value)> = plano.proveniencia.ocultacoes.iter().map(|o| (o.clipes.join(","), serde_json::to_value(o).unwrap()["inverter"].clone())).collect();
    assert_eq!(feito, vec![("c1,c2".to_string(), json!(true)), ("c2".to_string(), json!(false))]);
    // a máscara do grafo é a invertida: a forma desenhada é a da ocultação, com `inverter`
    let invertida = plano.camadas.iter().filter(|c| c.tipo == TipoCamada::MascaraFonte).any(|c| {
        c.itens.iter().any(|i| matches!(&i.desenho, crate::edicao::motor::plano::Desenho::Efeito { efeito: crate::edicao::modelo::Efeito::Ocultar(o) } if o.alvo.inverter))
    });
    assert!(invertida);
}

fn destaque_com_id(id: &str) -> Value {
    efeito("destaque", id, json!({ "forma": "retangulo", "rastreio": "r1", "traco": { "cor": "#00ff00", "espessura": 0.006, "contorno": true }, "tamanho": 0.1, "angulo_graus": 0.0, "margem": 0.0, "rotulo": null }))
}

/// Casos de efeito de origem (todos no mesmo sítio de `clipe.rs`).
/// Os efeitos de origem do despacho do `clipe.rs`.
fn origem_do_clipe() -> [&'static str; 6] {
    ["estabilizar", "lente", "desentrelacar", "media_temporal", "ajustes", "nitidez"]
}

/// Linhas do despacho por tipo (`recusar_de_outra_fase`, marcada com `// despacho`).
fn sitios_de_despacho(fonte: &str) -> usize {
    fonte.split("#[cfg(test)]").next().unwrap_or("").lines().filter(|l| l.contains("nao_suportado") && l.contains("// despacho")).count()
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
pub(crate) fn no_canvas(efeitos: &[crate::edicao::modelo::Tolerante<crate::edicao::modelo::Efeito>], tau: f64, c: [f64; 4], origem: (u32, u32), canvas: (u32, u32)) -> ([f64; 4], bool) {
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

pub(crate) struct Integrado {
    pub(crate) p: ProjetoEdicao,
    pub(crate) mundo: Mundo,
    mp4: Arc<Medida>,
    mkv: Arc<Medida>,
}

/// Projeto do ensaio integrado, a 30000/1001: trecho a 0,5× com camada que muda a cada quadro e carimbo; título
/// (ou foto) de 45 slots entrando do preto; trecho de outra mídia com `ocultar`; quadro congelado; faixa cheia com
/// desfoque e holofote; zoom animado; 9 trechos curtos (15 clipes: 2 blocos); imagem sobreposta; fecho escurecendo.
pub(crate) fn integrado(cap: &Arc<Capacidades>, com_titulo: bool) -> Integrado {
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
    if com_titulo {
        // o texto do título no meio do canvas e o fundo de cor nos cantos, depois do escurecimento
        let x = &q[de1 + 30];
        let branco = (40..140u32).flat_map(|y| (40..280u32).map(move |xx| (xx, y))).filter(|&(xx, y)| pixel(x, xx, y).iter().all(|&v| v > 200)).count();
        assert!(branco > 300, "título sem texto: {branco} px brancos ({})", cap.versao);
        for (bx, by) in [(5, 5), (314, 174)] {
            let c = pixel(x, bx, by);
            assert!(c.iter().zip([0x10u8, 0x20, 0x40]).all(|(a, b)| a.abs_diff(b) <= 6), "fundo do título em ({bx}, {by}): {c:?}");
        }
    }
    // imagem sobreposta (amarela, canto de cima à direita) só nos slots dela, de 0,5 s a 2,5 s
    let amarelo = |x: &QuadroRgb| x.px.chunks_exact(3).filter(|p| p[0] > 180 && p[1] > 180 && p[2] < 90).count();
    assert!(amarelo(&q[5]) == 0 && amarelo(&q[40]) > 800 && amarelo(&q[80]) == 0, "sobreposta: {} {} {}", amarelo(&q[5]), amarelo(&q[40]), amarelo(&q[80]));
    eprintln!("A17 pelo compilador ({}, {}): {total} quadros em 2 blocos, {conferidos} slots com o quadro previsto, camada fora do slot 0", cap.versao, if com_titulo { "com título" } else { "com foto" });
}

#[test]
fn a17_ensaio_integrado_pelo_compilador() {
    let Some(cap) = fontes::cap() else { return };
    // com o título de verdade (Typst); a foto no lugar dele fica no ensaio do quadro final
    conferir_integrado(&cap, true);
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
    let Some(outro) = fontes::outro_ffmpeg(&cap) else { return };
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

/// 60 trechos de `n` quadros da mesma mídia, todos (menos o primeiro) com `fundir` de `d_s`: o plano dividido e o de
/// bloco único (a referência).
fn sessenta_com_fundir(cap: &Arc<Capacidades>, info: &FonteInfo, (w, h): (u32, u32), n: usize, d_s: f64) -> (Plano, Plano) {
    let tab = &info.quadros;
    let clipes: Vec<Value> = (0..60)
        .map(|i| {
            let mut c = trecho(&format!("c{i}"), "m1", tab, n * i, n * (i + 1), 1.0, json!([]));
            if i > 0 {
                c["transicao_entrada"] = json!({ "tipo": "fundir", "duracao_s": d_s, "audio": "acompanhar" });
            }
            c
        })
        .collect();
    let p = projeto(w, h, (30, 1), vec![midia("m1", "video", json!([]))], clipes, json!({}));
    let mundo = Mundo::novo(cap).com("m1", info);
    let dividido = mundo.compilar(&p, &alvo_final("cpu")).unwrap();
    crate::edicao::motor::blocos::UM_BLOCO.with(|c| c.set(true));
    let unico = mundo.compilar(&p, &alvo_final("cpu"));
    crate::edicao::motor::blocos::UM_BLOCO.with(|c| c.set(false));
    (dividido, unico.unwrap())
}

/// Entradas de mídia de um bloco.
fn midias_do_bloco(b: &crate::edicao::motor::plano::Bloco) -> usize {
    b.entradas.iter().filter(|e| matches!(e.papel, Papel::Midia { .. })).count()
}

/// Transição que sobrepõe em todo clipe: o bloco fecha em 12 entradas no primeiro slot da transição, e cada slot sai
/// igual ao do bloco único (hash por slot), inclusive os da transição cortada pela fronteira.
#[test]
fn sessenta_trechos_com_fundir_dao_os_quadros_do_bloco_unico() {
    let Some(cap) = fontes::cap() else { return };
    let m = apoio::medida(&cap, Fonte::Mp4);
    // 60 trechos de 7 quadros com 3 de sobreposição: 420 − 59 × 3
    let (dividido, unico) = sessenta_com_fundir(&cap, &m.info, (320, 180), 7, 0.1);
    assert_eq!((dividido.total_q, unico.total_q, unico.blocos.len()), (243, 243, 1));
    assert_eq!(midias_do_bloco(&unico.blocos[0]), 60);
    assert_eq!(dividido.blocos.iter().map(midias_do_bloco).collect::<Vec<_>>(), vec![8, 8, 8, 8, 8, 8, 8, 8, 4], "{}", cap.versao);
    let d = executor::rodar(&cap, &dividido, &pasta("fundir-dividido"), &sem_perdas()).unwrap_or_else(|e| panic!("{}: {e}", cap.versao));
    let u = executor::rodar(&cap, &unico, &pasta("fundir-unico"), &sem_perdas()).unwrap_or_else(|e| panic!("{}: {e}", cap.versao));
    assert_eq!(d.compostos.len(), 243);
    let diferentes: Vec<usize> = (0..243).filter(|&q| d.compostos[q] != u.compostos[q]).collect();
    assert!(diferentes.is_empty(), "{}: slots com outro quadro: {diferentes:?}", cap.versao);
    // e os quadros não são todos iguais entre si (a comparação mede alguma coisa)
    assert!(d.compostos.iter().collect::<std::collections::HashSet<_>>().len() > 200);
}

#[test]
#[ignore = "completo: 60 trechos de 1080p com fundir, memória medida (o bloco único de referência passa de 5 GiB no 7.0.2)"]
fn sessenta_trechos_com_fundir_com_pico_abaixo_de_1_5_gib() {
    let Some(cap) = fontes::cap() else { return };
    let dir = pasta("60-fundir");
    let video = gerar(&cap, &dir, "f1080.mp4", "testsrc2=s=1920x1080:r=30:d=45", &["-c:v", "libx264", "-preset", "ultrafast", "-g", "60", "-pix_fmt", "yuv420p"]);
    let info = fonte_de(&cap, &video);
    // 60 trechos de 15 quadros com 6 de sobreposição (0,2 s): 900 − 59 × 6
    let (dividido, unico) = sessenta_com_fundir(&cap, &info, (1920, 1080), 15, 0.2);
    assert_eq!((dividido.total_q, unico.blocos.len()), (546, 1));
    assert!(dividido.blocos.iter().all(|b| midias_do_bloco(b) <= crate::edicao::motor::blocos::MAX_ENTRADAS));
    assert_eq!(dividido.blocos.iter().map(midias_do_bloco).collect::<Vec<_>>(), vec![8, 8, 8, 8, 8, 8, 8, 8, 4]);
    let d = executor::rodar(&cap, &dividido, &dir.join("d"), &Opcoes { juntar: true, ..Opcoes::default() }).unwrap();
    let gib = d.pico_kb as f64 / (1024.0 * 1024.0);
    assert!(d.pico_kb > 0, "pico de memória não medido");
    let u = executor::rodar(&cap, &unico, &dir.join("u"), &Opcoes::default()).unwrap();
    let gib_unico = u.pico_kb as f64 / (1024.0 * 1024.0);
    eprintln!("60 trechos de 1080p com fundir ({}): {} blocos, pico de {gib:.2} GiB num ffmpeg; bloco único {gib_unico:.2} GiB", cap.versao, dividido.blocos.len());
    assert!(gib < PICO_GIB, "{gib:.2} GiB");
    assert_eq!(medir::pts(&cap, d.junto.as_ref().unwrap()).unwrap().len(), 546);
    let diferentes: Vec<usize> = (0..546).filter(|&q| d.compostos[q] != u.compostos[q]).collect();
    assert!(diferentes.is_empty(), "slots com outro quadro: {diferentes:?}");
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

// ------------------------------------------------------------------ ponta a ponta: compilar → exportar → relatório

/// O `escurecer` do vídeo e o do som têm a mesma duração `d` (`tempo::slots(duracao_s, fps)` limitado ao `n_q`
/// de cada clipe, o que entra e o que sai), inclusive quando o clipe é mais curto que a transição.
#[test]
fn escurecer_do_video_e_do_som_com_o_mesmo_d() {
    let (mundo, midias) = mundo_das_recusas();
    let t = &mundo.fontes["m1"].quadros;
    let escurecer = |d: f64| json!({ "tipo": "escurecer", "duracao_s": d, "audio": "acompanhar" });
    // 29,97 qps: 0,5 s = 15 slots (16 683 amostras ≠ 15 × 1602); o c2 tem 8 slots e limita a saída do c1 e a entrada dele
    for (fps, d) in [((30, 1), 0.5), ((30000, 1001), 0.5), ((25, 1), 0.7)] {
        let mut c2 = trecho("c2", "m1", t, 100, 108, 1.0, json!([]));
        c2["transicao_entrada"] = escurecer(d);
        let mut c3 = trecho("c3", "m1", t, 200, 260, 1.0, json!([]));
        c3["transicao_entrada"] = escurecer(d);
        let mut p = projeto(320, 180, fps, midias.clone(), vec![trecho("c1", "m1", t, 0, 60, 1.0, json!([])), c2, c3], json!({ "fecho": escurecer(d) }));
        p.saida.audio = true;
        let ctx = Contexto { fontes: &mundo.fontes, cap: &mundo.cap, rastreios: &SemRastreios, pasta_projeto: Path::new("/nao/existe"), midias: &[] };
        // o c2 curto a validação recusa (`transicao`); o motor tem de limitar igual nos dois lados mesmo assim
        let linha = tempo::resolver(&p, &ctx);
        let mut plano = compilar_video(&p, &linha, &ctx, &alvo_final("cpu")).expect("compila o vídeo");
        plano.audio = crate::edicao::motor::audio::montar(&p, &linha, &ctx, &alvo_final("cpu")).expect("monta o som").0;
        let r = Racional { num: fps.0, den: fps.1 };
        let a = |q: u64| tempo::amostra_do_slot(q, r);
        // fades do vídeo: (in|out, início em s no clipe, duração em s) → em slots
        let video: Vec<(bool, u64, u64)> = plano.blocos[0]
            .grafo
            .split("fade=")
            .skip(1)
            .map(|x| {
                let campo = |k: &str| x.split(&format!("{k}=")).nth(1).unwrap().split([':', ',', '[']).next().unwrap().parse::<f64>().unwrap();
                let slots = |s: f64| (s * fps.0 as f64 / fps.1 as f64).round() as u64;
                (x.starts_with("t=in"), slots(campo("st")), slots(campo("d")))
            })
            .collect();
        // fades do som: (in|out, amostra de início no pedaço, amostras)
        let grafo = &plano.audio.as_ref().unwrap().grafo;
        let som: Vec<(bool, u64, u64)> = grafo
            .split("afade=")
            .skip(1)
            .map(|x| {
                let campo = |k: &str| x.split(&format!("{k}=")).nth(1).unwrap().split([':', ',', '[']).next().unwrap().parse::<u64>().unwrap();
                (x.starts_with("t=in"), campo("ss"), campo("ns"))
            })
            .collect();
        assert_eq!(video.len(), som.len(), "{fps:?}: {video:?} × {som:?}");
        let clipes = &plano.proveniencia.roteiro;
        // a ordem é a dos clipes: [c1 sai], [c2 entra, c2 sai], [c3 entra, c3 sai (fecho)]
        let donos = [(0, false), (1, true), (1, false), (2, true), (2, false)];
        for ((v, s), (k, entra)) in video.iter().zip(&som).zip(donos) {
            let (de_q, n_q) = (clipes[k].na_saida.de_q, clipes[k].na_saida.n_q as u64);
            assert_eq!((v.0, s.0), (entra, entra), "{fps:?}");
            // o mesmo d em slots, nas mesmas fronteiras de amostra
            let (q0, q1) = if entra { (de_q, de_q + v.2) } else { (de_q + v.1, de_q + n_q) };
            assert_eq!(s.2, a(q1) - a(q0), "{fps:?} clipe {k}: vídeo {v:?} × som {s:?}");
            assert_eq!(v.2, (d * fps.0 as f64 / fps.1 as f64).round().min(n_q as f64) as u64, "{fps:?} clipe {k}");
        }
    }
}

/// Fonte 320×180 a 30 qps com som de nível constante (`nivel` de 0 a 1, PCM): cada amostra do programa diz de que
/// clipe veio e com que ganho.
fn fonte_com_nivel(cap: &Capacidades, pasta: &Path, nome: &str, cena: &str, nivel: f64) -> FonteInfo {
    std::fs::create_dir_all(pasta).unwrap();
    let (v, a) = (format!("{cena}=s=320x180:r=30:d=6"), format!("aevalsrc={nivel}|{nivel}:s=48000:d=6"));
    let args: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-f", "lavfi", "-i", &v, "-f", "lavfi", "-i", &a, "-c:v", "libx264", "-preset", "ultrafast", "-g", "30", "-bf", "0",
        "-pix_fmt", "yuv420p", "-c:a", "pcm_s16le", nome]
        .map(String::from)
        .into();
    let cmd = crate::edicao::ffmpeg::exec::Comando { nome: "gerar".into(), programa: cap.binario.clone(), args, pasta: pasta.to_path_buf(), ambiente: Vec::new(), sem_ambiente: Vec::new(), stderr: pasta.join(format!("{nome}.stderr")) };
    let fim = crate::edicao::ffmpeg::exec::rodar(&cmd, &std::sync::atomic::AtomicBool::new(false)).expect("ffmpeg");
    crate::edicao::ffmpeg::exec::exigir_sucesso(&cmd, &fim).expect("gerar fonte com som");
    let f = fonte_de(cap, &pasta.join(nome));
    assert!(f.audio.is_some() && f.quadros.pts.len() == 180, "fonte com som");
    f
}

/// Rampa de `len` amostras que começa em `ini`: 0 antes, 1 depois (a do `afade`, linear).
fn rampa(s: i64, ini: i64, len: i64) -> f64 {
    ((s - ini) as f64 / len.max(1) as f64).clamp(0.0, 1.0)
}

/// Amostras do corte seco do som ("Cortar": 10 ms de cada lado).
const CORTE: i64 = 480;
/// Amostras do som de um trecho de 60 quadros da fonte de 30 qps.
const SOM_DO_TRECHO: i64 = 96_000;
/// Erro aceito por amostra (LSB de 16 bits): arredondamento da rampa e da soma.
const TOLERANCIA_PCM: f64 = 1.0;

/// O som das transições (P1.5) e a imagem (P1.1) juntos, pelo compilador inteiro (validação, vídeo e som) e pela
/// exportação inteira (blocos, passo de som, junção, conferência), para cada tipo do catálogo, a 30 e a 29,97 qps:
/// os quadros são os do mesmo projeto sem som (hash por slot; a imagem é medida pixel a pixel em `motor::juncao`),
/// cada amostra é a soma dos clipes com a rampa da transição, "Acompanhar" cruza os dois sons na sobreposição
/// inteira e "Cortar" troca o som no MEIO dela.
#[test]
fn transicoes_com_som_quadros_e_amostras_pela_exportacao() {
    use crate::edicao::modelo::Nivel;
    use crate::edicao::motor::{blocos, compilar, juncao};
    use crate::edicao::{tempo as tp, validar};
    let Some(cap) = fontes::cap() else { return };
    let dir = pasta("transicoes-com-som");
    let (a, b) = (fonte_com_nivel(&cap, &dir, "a.mov", "testsrc2", 0.5), fonte_com_nivel(&cap, &dir, "b.mov", "testsrc", 0.25));
    let mundo = Mundo::novo(&cap).com("a", &a).com("b", &b);
    let ctx = Contexto { fontes: &mundo.fontes, cap: &cap, rastreios: &SemRastreios, pasta_projeto: Path::new("/nao/existe"), midias: &[] };
    let (na, nb) = (0.5 * 32768.0, 0.25 * 32768.0);
    let mut conferidas = 0usize;
    for fps in [(30, 1), (30000, 1001)] {
        let fr = Racional { num: fps.0, den: fps.1 };
        for &tipo in juncao::TRANSICOES {
            let tr = |d: f64, som: &str| json!({ "tipo": tipo, "duracao_s": d, "audio": som });
            let mut c0 = trecho("c0", "a", &a.quadros, 30, 90, 1.0, json!([]));
            c0["transicao_entrada"] = tr(0.3, "acompanhar");
            let mut c1 = trecho("c1", "b", &b.quadros, 30, 90, 1.0, json!([]));
            c1["transicao_entrada"] = tr(0.5, "acompanhar");
            let mut c2 = trecho("c2", "a", &a.quadros, 90, 150, 1.0, json!([]));
            c2["transicao_entrada"] = tr(0.4, "cortar");
            let midias = vec![midia("a", "video", json!([])), midia("b", "video", json!([]))];
            let mut p = projeto(320, 180, fps, midias, vec![c0, c1, c2], json!({ "fecho": tr(0.2, "cortar") }));
            p.saida.audio = true;
            let linha = tp::resolver(&p, &ctx);
            let erros: Vec<_> = validar::validar(&p, &linha, &ctx, Some(&alvo_final("cpu"))).into_iter().filter(|x| x.nivel == Nivel::Erro).collect();
            assert!(erros.is_empty(), "{tipo} a {fps:?}: {erros:?}");
            // o compilador inteiro: valida, monta o vídeo e o som
            let plano = compilar(&p, &ctx, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{tipo} a {fps:?} ({}): {e}", cap.versao));
            let passo = plano.audio.clone().expect("passo de som");
            let nome = format!("{tipo}-{}", fps.1);
            let (r, res) = executor::exportar(&cap, &plano, &dir.join(&nome), "2026-11-15T00:00:00Z").unwrap_or_else(|e| panic!("{tipo} a {fps:?} ({}): {e}", cap.versao));
            let sobrepoe = tp::transicao_sobrepoe(tipo);
            let total_q = plano.total_q;
            assert_eq!(linha.clipes.iter().map(|c| c.sobrepoe_q).collect::<Vec<_>>(), if sobrepoe { vec![0, tp::slots(0.5, fr) as u32, tp::slots(0.4, fr) as u32] } else { vec![0, 0, 0] }, "{tipo} a {fps:?}");
            // ---- imagem: os mesmos quadros do projeto sem som, e o arquivo entregue com som e a contagem certa
            let mut mudo = p.clone();
            mudo.saida.audio = false;
            let so_video = executor::rodar(&cap, &mundo.compilar(&mudo, &alvo_final("cpu")).unwrap(), &dir.join(format!("{nome}-mudo")), &Opcoes::default()).unwrap();
            assert_eq!((r.compostos.len() as u64, res.conferencia.as_ref().map(|c| c.quadros)), (total_q, Some(total_q)), "{tipo} a {fps:?}");
            assert_eq!(r.compostos, so_video.compostos, "{tipo} a {fps:?}: o som mudou a imagem");
            assert!(r.compostos.iter().collect::<std::collections::HashSet<_>>().len() as u64 > total_q / 2, "quadros todos iguais não medem nada");
            // ---- som: cada amostra do PCM do passo
            let pcm = pcm_do_wav(&std::fs::read(dir.join(&nome).join(&passo.saida)).unwrap());
            let am = |q: u64| tp::amostra_do_slot(q, fr) as i64;
            assert_eq!((pcm.len() as i64 / 2, passo.amostras as i64), (am(total_q), am(total_q)), "{tipo} a {fps:?}: amostras");
            assert!(res.audio_sha256.is_some());
            let faixa = |k: usize| (am(linha.clipes[k].inicio_q), am(linha.clipes[k].inicio_q + linha.clipes[k].n_q as u64));
            // rampas de entrada e de saída de cada clipe: (início, amostras) no tempo do programa; None = sem rampa
            let mut entra: [Option<(i64, i64)>; 3] = [None; 3];
            let mut sai: [Option<(i64, i64)>; 3] = [None; 3];
            let dur = [0.3, 0.5, 0.4];
            let cortar = [false, false, true];
            for i in 0..3 {
                let (ini, fim) = faixa(i);
                if i > 0 && sobrepoe {
                    // os dois clipes tocam juntos de `ini` ao fim do anterior
                    let s = faixa(i - 1).1 - ini;
                    assert_eq!(s, am(linha.clipes[i].inicio_q + linha.clipes[i].sobrepoe_q as u64) - ini, "a sobreposição do som é a da imagem");
                    if cortar[i] {
                        let meio = s / 2;
                        let k = CORTE.min(meio).min(s - meio).max(1);
                        entra[i] = Some((ini + meio, k));
                        sai[i - 1] = Some((ini + meio - k, k));
                    } else {
                        entra[i] = Some((ini, s));
                        sai[i - 1] = Some((ini, s));
                    }
                    continue;
                }
                // sem sobreposição (escurecer, ou a entrada do primeiro clipe): rampa na ponta, dentro do clipe
                let d = tp::slots(dur[i], fr).min(linha.clipes[i].n_q as u64);
                let n = am(linha.clipes[i].inicio_q + d) - ini;
                entra[i] = Some((ini, if cortar[i] { CORTE.min(n) } else { n }));
                if i > 0 {
                    let (_, fim_ant) = faixa(i - 1);
                    let ant = &linha.clipes[i - 1];
                    let d = tp::slots(dur[i], fr).min(ant.n_q as u64);
                    let n = fim_ant - am(ant.inicio_q + ant.n_q as u64 - d);
                    let k = if cortar[i] { CORTE.min(n) } else { n };
                    sai[i - 1] = Some((fim_ant - k, k));
                }
                let _ = fim;
            }
            // fecho: "cortar" de 0,2 s na ponta do último
            let (_, fim2) = faixa(2);
            let (_, d_fecho) = blocos::escurecimentos(&p, &linha, 2);
            let n = fim2 - am(linha.clipes[2].inicio_q + linha.clipes[2].n_q as u64 - d_fecho as u64);
            sai[2] = Some((fim2 - CORTE.min(n), CORTE.min(n)));
            let niveis = [na, nb, na];
            let esperado = |s: i64| -> f64 {
                (0..3)
                    .map(|i| {
                        let (ini, fim) = faixa(i);
                        // o som do trecho dura o que a fonte dura (60 quadros a 30 qps = 2 s): a 29,97 o clipe ocupa
                        // 96 amostras a mais, de silêncio
                        if s < ini || s >= fim || s - ini >= SOM_DO_TRECHO {
                            return 0.0;
                        }
                        let sobe = entra[i].map_or(1.0, |(a, n)| rampa(s, a, n));
                        let desce = sai[i].map_or(1.0, |(a, n)| 1.0 - rampa(s, a, n));
                        niveis[i] * sobe * desce
                    })
                    .sum()
            };
            let mut pior = 0.0f64;
            for (s, par) in pcm.chunks_exact(2).enumerate() {
                assert_eq!(par[0], par[1], "{tipo} a {fps:?}: canais diferentes na amostra {s}");
                let erro = (par[0] as f64 - esperado(s as i64)).abs();
                assert!(erro <= TOLERANCIA_PCM, "{tipo} a {fps:?} ({}): amostra {s}: {} × {:.1} previsto", cap.versao, par[0], esperado(s as i64));
                pior = pior.max(erro);
            }
            // "Cortar" troca no meio da sobreposição: antes da rampa só o que sai (b), depois só o que entra (a)
            let v = |s: i64| pcm[2 * s as usize] as f64;
            if sobrepoe {
                let (ini, fim_ant) = (faixa(2).0, faixa(1).1);
                let meio = ini + (fim_ant - ini) / 2;
                assert!(fim_ant - ini > 4 * CORTE, "sobreposição curta demais para medir o corte");
                assert!((v(meio - CORTE - 1) - nb).abs() <= TOLERANCIA_PCM && (v(meio + CORTE) - na).abs() <= TOLERANCIA_PCM, "{tipo} a {fps:?}: {} e {}", v(meio - CORTE - 1), v(meio + CORTE));
                assert!((v(ini + 10) - nb).abs() <= TOLERANCIA_PCM && (v(fim_ant - 10) - na).abs() <= TOLERANCIA_PCM, "{tipo} a {fps:?}: fora do meio só toca um");
                // "Acompanhar" cruza os dois na sobreposição inteira: no meio dela, metade de cada
                let (ini, fim_ant) = (faixa(1).0, faixa(0).1);
                let m = ini + (fim_ant - ini) / 2;
                assert!((v(m) - (na + nb) / 2.0).abs() <= TOLERANCIA_PCM + 1.0, "{tipo} a {fps:?}: {} no meio do cruzamento", v(m));
            }
            conferidas += 1;
            eprintln!("{tipo} a {fps:?} ({}): {total_q} quadros iguais aos do projeto sem som, {} amostras a até {pior:.1} LSB", cap.versao, pcm.len() / 2);
        }
    }
    assert_eq!(conferidas, 2 * juncao::TRANSICOES.len());
}

/// PCM s16 estéreo intercalado do `data` de um WAV.
fn pcm_do_wav(b: &[u8]) -> Vec<i16> {
    let mut i = 12;
    while i + 8 <= b.len() {
        let n = u32::from_le_bytes(b[i + 4..i + 8].try_into().unwrap()) as usize;
        if &b[i..i + 4] == b"data" {
            return b[i + 8..(i + 8 + n).min(b.len())].as_chunks::<2>().0.iter().map(|c| i16::from_le_bytes(*c)).collect();
        }
        i += 8 + n + (n & 1);
    }
    panic!("WAV sem data")
}

/// Amostras em que começam os cliques (canal esquerdo acima de `limiar` depois de 0,1 s abaixo dele).
fn cliques(pcm: &[i16], limiar: i16) -> Vec<usize> {
    let (mut v, mut ultimo) = (Vec::new(), None::<usize>);
    for (k, x) in pcm.iter().step_by(2).enumerate() {
        if x.unsigned_abs() > limiar as u16 {
            if ultimo.is_none_or(|u| k - u > 4800) {
                v.push(k);
            }
            ultimo = Some(k);
        }
    }
    v
}

/// Cliques da matriz de fontes: 1 ms em x,5 s do conteúdo; `som_s` = onde o conteúdo começa na linha do tempo da
/// fonte (a MKV feita pelo 7.0.2 põe o vídeo em 21 ms e o som com a pré-rolagem: os dois no mesmo lugar).
fn cliques_previstos(de_s: f64, ate_s: f64, som_s: f64, inicio_amostra: u64) -> Vec<i64> {
    let mut v = Vec::new();
    let mut c = som_s + (de_s - som_s).floor() + 0.5;
    while c < ate_s {
        if c >= de_s {
            v.push(inicio_amostra as i64 + ((c - de_s) * 48_000.0).round() as i64);
        }
        c += 1.0;
    }
    v
}

/// Saída de um ffmpeg (stdout) nos argumentos dados.
fn saida_do_ffmpeg(cap: &Capacidades, pasta: &Path, a: &[&str]) -> Vec<u8> {
    let cmd = crate::edicao::ffmpeg::exec::Comando { nome: "sonda".into(), programa: cap.binario.clone(), args: a.iter().map(|s| s.to_string()).collect(), pasta: pasta.to_path_buf(), ambiente: Vec::new(), sem_ambiente: Vec::new(), stderr: pasta.join("sonda.stderr") };
    let (fim, out) = crate::edicao::ffmpeg::exec::rodar_com_saida(&cmd, &std::sync::atomic::AtomicBool::new(false)).expect("ffmpeg");
    crate::edicao::ffmpeg::exec::exigir_sucesso(&cmd, &fim).expect("ffmpeg");
    out
}

/// Tolerância dos cliques (amostras): a MKV da matriz tem pts em ms (até +8 amostras medidas pelo P0.6b).
const CLIQUE_AMOSTRAS: i64 = 10;
/// Marca e desenho no zoom animado (px), como no aceite do P0.6a.
const ZOOM_PX: f64 = 1.0;
const DESENHO_PX: f64 = 1.5;

/// Ponta a ponta pelo compilador de verdade (`compilar`: resolver → validar → vídeo → som → proveniência), num
/// projeto com título (Typst), ocultação da mídia, destaque, zoom animado, quadro congelado e narração, exportado
/// duas vezes e uma sem perdas: quadro de origem por slot, pixel (ocultação, destaque, zoom, título) e amostras
/// (cliques e narração no lugar, contagem exata); `tecnico.json` e `mapa.csv` do plano compilado e executado,
/// `feito` igual nos dois renders e `mapa.csv` igual aos pts lidos e ao que o vídeo mostra.
#[test]
fn compilar_de_ponta_a_ponta_com_relatorio() {
    use crate::edicao::executar::{Ambiente, Autor, Cpu, Identidade, RetratoCaso, Sistema, VersaoSicro};
    use crate::edicao::modelo::{EstadoMidia, SituacaoMidia};
    use crate::edicao::quadros::{Busca, OrigemTabela};
    use crate::edicao::relatorio::{mapa, tecnico};
    let Some(cap) = fontes::cap() else { return };
    let dir = pasta("e2e");
    let (m4, mk) = (apoio::medida(&cap, Fonte::Mp4), apoio::medida(&cap, Fonte::Mkv30));
    let (t4, tk) = (&m4.info.quadros, &mk.info.quadros);
    assert!(m4.info.audio.is_some() && mk.info.audio.is_some(), "a matriz traz som");
    // narração: um clique em 0,25 s de um WAV mono
    let wav = gerar(&cap, &dir, "narracao.wav", "aevalsrc=exprs=0.9*lt(mod(t+0.75\\,1)\\,0.001):s=48000:c=mono:d=1", &["-c:a", "pcm_s16le"]);
    let tab = TabelaQuadros::de_pacotes(&cap, &wav).expect("tabela do som");
    let narr = crate::edicao::quadros::fonte_info(&cap, &wav, &"0".repeat(64), Arc::new(tab), None).expect("fonte do som");
    assert!(narr.audio.is_some() && narr.quadros.pts.is_empty());
    let nome = |mut r: Value, n: &str| {
        r["nome"] = json!(n);
        r
    };
    let pula: Vec<Value> = (60..120).map(|j| ancora(t4.tempo(j), Some(t4.pts[j]), [x_que_pula(j), 0.3, 0.1, 0.2])).collect();
    let titulo = json!({ "tipo": "titulo", "id": "c0", "rotulo": "", "duracao_s": 1.0,
        "titulo": { "modelo": "cartela", "fundo": { "tipo": "cor", "cor": "#102040" }, "blocos": [{ "id": "b1", "papel": "titulo", "conteudo": "Câmera 1", "largura_max": 0.8,
            "lugar": { "x": 0.5, "y": 0.5, "h": "centro", "v": "meio" },
            "estilo": { "fonte": "sans", "negrito": true, "italico": false, "tamanho": 0.12, "cor": "#ffffff", "alinhamento": "centro", "caixa": null, "contorno": null } }] } });
    let mut c1 = trecho("c1", "m4", t4, 60, 120, 1.0, json!([destaque("d1", "pula")]));
    c1["transicao_entrada"] = json!({ "tipo": "escurecer", "duracao_s": 0.5, "audio": "acompanhar" });
    let congelado = json!({ "tipo": "congelar", "id": "c3", "rotulo": "", "midia": "mk", "instante_s": tk.tempo(70), "instante_pts": tk.pts[70], "duracao_s": 0.5 });
    let zoom = json!([{ "t": t4.tempo(200), "v": caixa([0.0, 0.0, 1.0, 1.0]), "interp": "suave" }, { "t": t4.tempo(259), "v": caixa([0.3, 0.3, 0.5, 0.5]) }]);
    let c4 = trecho("c4", "m4", t4, 200, 260, 1.0, json!([enquadramento(zoom, "conter"), destaque("d4", "rm")]));
    let mut midias = vec![midia("m4", "video", json!([])), midia("mk", "video", json!([ocultar("o1", "rk", "solido", json!({}))])), midia("narr", "audio", json!([]))];
    midias[2]["nome"] = json!("narracao.wav");
    let mut p = projeto(
        320,
        180,
        (30, 1),
        midias,
        vec![titulo, c1, trecho("c2", "mk", tk, 40, 100, 1.0, json!([])), congelado, c4],
        json!({
            "rastreios": [
                nome(rastreio("pula", Some("m4"), "fonte", pula), "Suspeito de casaco"),
                nome(rastreio("rk", Some("mk"), "fonte", ancoras_da_marca(Fonte::Mkv30, &mk, 40, 100)), "Placa do carro"),
                nome(rastreio("rm", Some("m4"), "fonte", ancoras_da_marca(Fonte::Mp4, &m4, 200, 260)), "Marca vermelha"),
            ],
            "audios": [{ "id": "a1", "nome": "Narração", "papel": "narracao", "volume": 1.0, "mudo": false, "abaixa_original": false,
                "clipes": [{ "id": "ca1", "nome": "", "midia": "narr", "pos": { "ref": "programa", "t": 5.6 }, "de_s": 0.0, "ate_s": null, "volume": 1.0, "entrada_s": 0.0, "saida_s": 0.0 }] }]
        }),
    );
    p.saida.audio = true;
    let mundo = Mundo::novo(&cap).com("m4", &m4.info).com("mk", &mk.info).com("narr", &narr);
    let estados: Vec<EstadoMidia> = ["m4", "mk", "narr"]
        .iter()
        .map(|id| EstadoMidia { id: id.to_string(), estado: SituacaoMidia::Ok, tabela: Some(OrigemTabela::Decodificador), busca: Some(Busca::Confiavel), erros: Some(0) })
        .collect();
    let ctx = Contexto { fontes: &mundo.fontes, cap: &cap, rastreios: &SemRastreios, pasta_projeto: Path::new("/nao/existe/projeto"), midias: &estados };
    let alvo = alvo_final("cpu");
    let plano = crate::edicao::motor::compilar(&p, &ctx, &alvo).unwrap_or_else(|e| panic!("compilar ({}): {e}", cap.versao));
    assert_eq!(crate::edicao::motor::compilar(&p, &ctx, &alvo).unwrap(), plano, "compilar é determinística");
    let total = plano.total_q as usize;
    assert_eq!(total, 30 + 60 + 60 + 15 + 60);
    let passo = plano.audio.as_ref().expect("passo de áudio");
    assert!(passo.entradas.len() >= 3 && passo.globais[..2] == ["-cpuflags".to_string(), "0".to_string()]);
    assert!(matches!(&plano.juncao, Juncao::Concat { args, .. } if args.windows(2).any(|w| w == ["-map", "1:a"])), "junção com o som");
    assert!(plano.camadas.iter().any(|c| c.itens.iter().any(|i| matches!(i.desenho, crate::edicao::motor::plano::Desenho::Titulo { .. }))), "camada do título");
    assert!(plano.proveniencia.avisos.is_empty(), "{:?}", plano.proveniencia.avisos);
    let item = |n: usize| (plano.proveniencia.roteiro[n].na_saida.de_q as usize, plano.proveniencia.roteiro[n].na_saida.n_q as usize);

    // duas exportações do mesmo plano, em pastas diferentes
    let (r1, res1) = executor::exportar(&cap, &plano, &dir.join("t1"), "2026-11-14T10:00:00Z").unwrap_or_else(|e| panic!("exportar ({}): {e}", cap.versao));
    let (_, res2) = executor::exportar(&cap, &plano, &dir.join("t2"), "2026-11-14T11:00:00Z").unwrap_or_else(|e| panic!("exportar de novo ({}): {e}", cap.versao));
    let junto = r1.junto.as_ref().unwrap();
    assert_eq!((res1.composto_sha256.as_ref(), res1.audio_sha256.as_ref()), (res2.composto_sha256.as_ref(), res2.audio_sha256.as_ref()), "composto e PCM iguais nos dois renders");
    assert_eq!(res1.sha256, res2.sha256, "mesmo arquivo na mesma máquina, com `cpu`");
    assert_eq!(res1.conferencia.as_ref().map(|c| (c.quadros, c.erros)), Some((plano.total_q, 0)));

    // vídeo entregue: contagem, passo constante, quadro de origem por slot, ocultação, destaque, título, escurecer
    let pts = medir::pts(&cap, junto).unwrap();
    assert_eq!((pts.len(), pts[0]), (total, 0));
    assert!(pts.windows(2).all(|p| p[1] - p[0] == 512), "passos do arquivo");
    let framehash = String::from_utf8(saida_do_ffmpeg(&cap, &dir, &["-hide_banner", "-nostdin", "-loglevel", "error", "-i", &junto.to_string_lossy(), "-map", "0:v:0", "-fps_mode", "passthrough", "-f", "framehash", "-hash", "sha256", "-"])).unwrap();
    assert_eq!(crate::edicao::conferir::passo_irregular(&framehash), None, "Conferindo ({})", cap.versao);
    let numeros = medir::quadros_de_origem(&cap, junto).unwrap();
    let q = quadros_709(&cap, junto, 320, 180);
    assert_eq!(q.len(), total);
    let (mut conferidos, mut errados) = (0, 0);
    for (n, m, pula) in [(1, &m4, 15), (2, &mk, 0)] {
        let (de, n_q) = item(n);
        let previsto = previstos(&plano, n, m);
        for s in pula..n_q {
            conferidos += 1;
            errados += usize::from(numeros[de + s] != previsto[s]);
        }
    }
    let (de3, n3) = item(3);
    assert!(numeros[de3..de3 + n3].iter().all(|&x| x == mk.numeros[70]), "congelado: {:?}", &numeros[de3..de3 + n3]);
    let (de4, _) = item(4);
    assert_eq!(numeros[de4], previstos(&plano, 4, &m4)[0], "zoom começa no quadro inteiro");
    assert_eq!(errados, 0, "quadro de origem errado em {errados} de {conferidos} slots ({})", cap.versao);
    let (de2, n2) = item(2);
    assert_eq!(q[de2..de2 + n2].iter().chain(&q[de3..de3 + n3]).map(vermelho).max(), Some(0), "ocultação descoberta");
    let (de1, n1) = item(1);
    let mapa1 = &plano.proveniencia.roteiro[1].origem.as_ref().unwrap().quadro_do_slot;
    for s in 15..n1 {
        let esperado = (x_que_pula(mapa1[s] as usize) + 0.05) * 320.0;
        let (cx, _, _) = centro_da_caixa(&q[de1 + s], eh_verde).unwrap_or_else(|| panic!("slot {s}: sem destaque"));
        assert!((cx - esperado).abs() <= 2.5, "destaque no slot {s}: {cx} × {esperado}");
    }
    let branco = (40..140u32).flat_map(|y| (40..280u32).map(move |x| (x, y))).filter(|&(x, y)| pixel(&q[15], x, y).iter().all(|&v| v > 200)).count();
    assert!(branco > 300, "título sem texto: {branco} px");
    assert!(pixel(&q[15], 5, 5).iter().zip([0x10u8, 0x20, 0x40]).all(|(a, b)| a.abs_diff(b) <= 6), "fundo do título: {:?}", pixel(&q[15], 5, 5));
    let luz = |x: &QuadroRgb| x.px.iter().map(|&v| v as u64).sum::<u64>() / x.px.len() as u64;
    assert!(luz(&q[de1]) < 12 && luz(&q[de1]) < luz(&q[de1 + 5]) && luz(&q[de1 + 5]) < luz(&q[de1 + 10]) && luz(&q[de1 + 10]) < luz(&q[de1 + 14]), "escurecer de entrada");

    // zoom animado e desenho por cima, sem perdas: a marca vermelha onde `A(τ)` a põe
    let sem = executor::rodar(&cap, &plano, &dir.join("sem-perdas"), &sem_perdas()).unwrap();
    let mut todos = Vec::new();
    for b in &sem.blocos {
        todos.extend(medir::quadros(&cap, b).unwrap());
    }
    assert_eq!(todos.len(), total);
    let ef = efeitos_do_clipe(&p, 4);
    let mapa4 = &plano.proveniencia.roteiro[4].origem.as_ref().unwrap().quadro_do_slot;
    let (mut pior_zoom, mut pior_desenho, mut medidos) = (0.0f64, 0.0f64, 0);
    for s in 0..60 {
        let j = mapa4[s] as usize;
        let ([px, py, pw, ph], cabe) = no_canvas(ef, t4.tempo(j), fontes::marca_esperada(Fonte::Mp4, m4.numeros[j]), m4.info.exibido, (320, 180));
        if !cabe {
            continue;
        }
        let x = &todos[de4 + s];
        let r = centro_de(x, eh_vermelho).unwrap_or_else(|| panic!("zoom slot {s}: marca fora"));
        let v = centro_da_caixa(x, eh_verde).unwrap_or_else(|| panic!("zoom slot {s}: destaque fora"));
        pior_zoom = pior_zoom.max((r.0 - px - pw / 2.0).abs()).max((r.1 - py - ph / 2.0).abs());
        pior_desenho = pior_desenho.max((v.0 - r.0).abs()).max((v.1 - r.1).abs());
        medidos += 1;
    }
    assert!(medidos >= 20 && pior_zoom <= ZOOM_PX && pior_desenho <= DESENHO_PX, "zoom: {medidos} slots, marca a {pior_zoom:.2} px, destaque a {pior_desenho:.2} px ({})", cap.versao);
    // o mesmo conteúdo da exportação (o composto vem antes do codificador)
    assert_eq!(sem.compostos.len(), total);

    // som: contagem exata e cada clique no lugar (dos trechos e da narração); título e congelado sem som
    let pcm = pcm_do_wav(&std::fs::read(dir.join("t1").join("audio.wav")).unwrap());
    let a = |q: u64| tempo::amostra_do_slot(q, plano.canvas.fps);
    assert_eq!(pcm.len() as u64 / 2, a(plano.total_q), "amostras");
    let mut previstos_som: Vec<i64> = Vec::new();
    for n in [1, 2, 4] {
        let o = plano.proveniencia.roteiro[n].origem.as_ref().unwrap();
        // o som tem de cair junto do quadro que mostra o mesmo instante do conteúdo (número desenhado / 30)
        let m = if o.midia == "m4" { &m4 } else { &mk };
        let som_s = m.info.quadros.tempo(0) - m.numeros[0] as f64 / 30.0;
        previstos_som.extend(cliques_previstos(o.de_s, o.ate_s, som_s, a(plano.proveniencia.roteiro[n].na_saida.de_q)));
    }
    previstos_som.push((5.85 * 48_000.0f64).round() as i64);
    previstos_som.sort_unstable();
    let achados = cliques(&pcm, 14_000);
    assert_eq!(achados.len(), previstos_som.len(), "cliques {achados:?} × {previstos_som:?} ({})", cap.versao);
    let pior = achados.iter().zip(&previstos_som).map(|(&x, &y)| (x as i64 - y).abs()).max().unwrap();
    assert!(pior <= CLIQUE_AMOSTRAS, "clique a {pior} amostras do previsto: {achados:?} × {previstos_som:?}");
    let silencio = |q0: u64, q1: u64| pcm[2 * a(q0) as usize..2 * a(q1) as usize].iter().all(|&x| x == 0);
    assert!(silencio(0, 30) && silencio(de3 as u64, (de3 + n3) as u64), "título e congelado sem som");

    // relatório do plano compilado e executado: `feito` igual nos dois renders; nenhum caminho nem nome de rastreio
    let caso = dir.join("caso ç");
    std::fs::create_dir_all(&caso).unwrap();
    let ambiente = |tarefa: &str| Ambiente {
        cap: cap.clone(),
        trabalho: dir.join(tarefa),
        cache: dir.join("cache"),
        caso: caso.clone(),
        pasta_projeto: caso.join("Sistema/videos/edicao/p-teste"),
        paralelos: 1,
        identidade: Identidade {
            autor: Autor { nome: "Perito Teste".into(), matricula: String::new(), cargo: String::new() },
            caso: RetratoCaso { numero: "0001/2026".into(), referencia: String::new(), orgao: String::new(), requisitante: String::new() },
            sicro: VersaoSicro { versao: "6.0.0".into(), commit: "x".into(), alvo: "x86_64-unknown-linux-gnu".into() },
            sistema: Sistema { so: "Linux".into(), cpu: Cpu { modelo: "CPU".into(), instrucoes: vec![] }, gpu: None, driver: None },
        },
        ciencias: vec![],
    };
    let fecho = tecnico::Fecho { projeto_sha256: format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(serde_json::to_vec(&p).unwrap())), arquivo_rel: "Exportações/Vídeos/Edições/Teste/Teste.mp4".into(), ocultacoes: None };
    let t1 = tecnico::montar(&plano, &res1, &ambiente("t1"), &fecho).unwrap();
    let t2 = tecnico::montar(&plano, &res2, &ambiente("t2"), &fecho).unwrap();
    assert_eq!(String::from_utf8(tecnico::bytes(&t1["feito"])).unwrap(), String::from_utf8(tecnico::bytes(&t2["feito"])).unwrap(), "feito igual nos dois renders");
    assert_ne!(tecnico::bytes(&t1["execucao"]), tecnico::bytes(&t2["execucao"]));
    let sha = tecnico::gravar(&caso, "Teste", &t1).unwrap();
    let texto = std::fs::read_to_string(caso.join("Teste.tecnico.json")).unwrap();
    assert_eq!(sha, format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(texto.as_bytes())));
    let raizes = vec![dir.to_string_lossy().to_string(), std::env::temp_dir().to_string_lossy().to_string()];
    let achados_abs = crate::edicao::relatorio::apoio::absolutos_no_texto(&texto, &raizes, &["Suspeito de casaco", "Placa do carro", "Marca vermelha"]);
    assert!(achados_abs.is_empty(), "{achados_abs:?}");
    let t: Value = serde_json::from_str(&texto).unwrap();
    assert_eq!((t["feito"]["saida"]["n_quadros"].as_u64(), t["feito"]["roteiro"].as_array().map(Vec::len)), (Some(plano.total_q), Some(5)));
    assert_eq!(t["feito"]["audio"]["tolerancia_buraco_ms"], json!(100.0));
    // toda fonte do vídeo entregue está no relatório, a narração inclusive
    assert_eq!(t["feito"]["midias"].as_array().unwrap().iter().map(|m| m["id"].as_str().unwrap()).collect::<Vec<_>>(), vec!["m4", "mk", "narr"]);
    assert_eq!(t["feito"]["ocultacoes"][0]["clipes"], json!(["c2", "c3"]));
    assert_eq!(t["execucao"]["conteudo"]["composto_sha256"], json!(res1.composto_sha256));
    assert_eq!(t["execucao"]["conferencia"]["ocultacoes"], json!("pendente"));
    assert_eq!(t["execucao"]["blocos"].as_array().unwrap().len(), plano.blocos.len());

    // mapa.csv: pts de cada linha = os da tabela no índice; o conjunto por clipe = os pts lidos pelo `showinfo`;
    // e o número que o vídeo entregue mostra no slot = o do quadro do índice
    let m1 = mapa::mapa_csv(&plano, Some(&res1)).unwrap();
    assert_eq!(m1, mapa::mapa_csv(&plano, Some(&res2)).unwrap());
    let (mut por_clipe, mut slots) = (std::collections::BTreeMap::<String, Vec<i64>>::new(), 0usize);
    for l in m1.lines().skip(1) {
        let c: Vec<&str> = l.split(',').collect();
        if c[3].is_empty() {
            continue;
        }
        let (midia, pts_l, j): (&str, i64, usize) = (c[3], c[4].parse().unwrap(), c[5].parse().unwrap());
        let m = if midia == "m4" { &m4 } else { &mk };
        assert_eq!(pts_l, m.info.quadros.pts[j], "{l}");
        if c[7] != "repetido" {
            por_clipe.entry(c[2].to_string()).or_default().push(pts_l);
        }
        if !c[0].is_empty() {
            let q: usize = c[0].parse().unwrap();
            let (de1, _) = item(1);
            // o número não se lê no escurecer de entrada do c1 nem sob o zoom
            if !(de1..de1 + 15).contains(&q) && q < de4 {
                assert_eq!(numeros[q], m.numeros[j], "mapa.csv slot {q}: {l}");
                slots += 1;
            }
        }
    }
    for (clipe, mut v) in por_clipe {
        v.sort_unstable();
        let mut lidos: Vec<i64> = res1.lidos.iter().filter(|x| x.clipe == clipe).flat_map(|x| x.pts.clone()).collect();
        lidos.sort_unstable();
        assert_eq!(v, lidos, "mapa.csv × pts lidos do clipe {clipe}");
    }
    // pts lido trocado: o mapa recusa
    let mut torto = res1.clone();
    torto.lidos[0].pts[3] += 1;
    assert!(mapa::mapa_csv(&plano, Some(&torto)).unwrap_err().to_string().contains("leitura_divergente"));
    eprintln!(
        "ponta a ponta ({}): {total} slots, {conferidos} com o quadro previsto, mapa.csv conferido em {slots} slots; zoom a {pior_zoom:.2} px, destaque a {pior_desenho:.2} px; {} cliques a até {pior} amostras; feito com {} bytes igual nos dois renders",
        cap.versao,
        achados.len(),
        tecnico::bytes(&t1["feito"]).len()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `eof_action=repeat` no `overlay` da ocultação (contratos (h) 5) não muda o composto em leitura normal: os mesmos
/// hashes de quadro do `pass`, com sólido, desfoque em elipse de borda suave, mosaico, 2×, 0,5×, congelado e fim da mídia.
#[test]
fn eof_repeat_da_ocultacao_nao_muda_o_composto() {
    let Some(cap) = fontes::cap() else { return };
    let dir = pasta("eof-repeat");
    let mk = apoio::medida(&cap, Fonte::Mkv30);
    let tk = &mk.info.quadros;
    let n = tk.pts.len();
    let congelado = json!({ "tipo": "congelar", "id": "c3", "rotulo": "", "midia": "mk", "instante_s": tk.tempo(70), "instante_pts": tk.pts[70], "duracao_s": 0.5 });
    let elipse = json!({ "alvo": { "rastreio": "rk", "forma": "elipse", "vertices": null, "margem": 0.3, "borda_suave": 0.2, "inverter": false } });
    let p = projeto(
        320,
        180,
        (30, 1),
        vec![midia("mk", "video", json!([ocultar("o1", "rk", "solido", json!({})), ocultar("o2", "rk", "desfoque", elipse)]))],
        vec![
            trecho("c1", "mk", tk, 40, 100, 1.0, json!([])),
            trecho("c2", "mk", tk, 100, 160, 2.0, json!([])),
            congelado,
            trecho("c4", "mk", tk, 20, 50, 0.5, json!([ocultar("o3", "rk", "mosaico", json!({}))])),
            trecho("c5", "mk", tk, n - 20, n, 1.0, json!([])),
        ],
        json!({ "rastreios": [rastreio("rk", Some("mk"), "fonte", ancoras_da_marca(Fonte::Mkv30, &mk, 0, n))] }),
    );
    let plano = Mundo::novo(&cap).com("mk", &mk.info).compilar(&p, &alvo_final("cpu")).unwrap();
    let overlays: usize = plano.blocos.iter().map(|b| b.grafo.matches("eof_action=repeat").count()).sum();
    assert_eq!(overlays, 11, "um `overlay` por ocultação de cada clipe");
    let novo = executor::rodar(&cap, &plano, &dir.join("repeat"), &Opcoes::default()).unwrap();
    let mut antigo = plano.clone();
    for b in &mut antigo.blocos {
        b.grafo = b.grafo.replace("eof_action=repeat", "eof_action=pass");
    }
    let velho = executor::rodar(&cap, &antigo, &dir.join("pass"), &Opcoes::default()).unwrap();
    assert_eq!(novo.compostos.len(), plano.total_q as usize);
    let diferentes = novo.compostos.iter().zip(&velho.compostos).filter(|(a, b)| a != b).count();
    assert_eq!(diferentes, 0, "{diferentes} de {} quadros compostos mudaram ({})", novo.compostos.len(), cap.versao);
    let _ = std::fs::remove_dir_all(&dir);
}
