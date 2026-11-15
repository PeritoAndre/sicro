//! Selos de edição na imagem (§2.1 `Selos`, §10.2 `feito.roteiro[].selos`): velocidade, ampliação e realce, mais os
//! selos fixos da média de quadros e da imagem tratada por IA (não desligam). Entram na camada global
//! (`motor/sobrepor.rs`) como um texto por clipe, no canto de cima à direita. Dono: P1.R.

use std::collections::BTreeMap;

use super::formas::QuadroCamada;
use crate::edicao::geometria::Afim;
use crate::edicao::modelo::{
    Alinhamento, AlinhamentoV, Animavel, CaixaTexto, ClipePrincipal, EfBase, EfTexto, Efeito, EstiloTexto, Fonte, Fundo, LinhaResolvida, Lugar,
    ProjetoEdicao, TamanhoPx, TipoDerivacao, Tolerante, Velocidade,
};
use crate::edicao::motor::plano::{Desenho, ItemCamada, QuadroItem};
use crate::edicao::tempo;
use crate::error::Result;

/// Os selos saem na imagem e no `feito`. Ligado no fecho da Fase 1: todo bloco que tem clipe marcado ganha a camada
/// global. Os testes de pixel que medem a cadeia de um efeito (e não o selo) desligam na própria thread
/// (`desligar_no_teste`); o selo é medido em `camada::selos::testes` e na ponta a ponta.
pub const NA_IMAGEM: bool = true;

#[cfg(test)]
thread_local! {
    /// Só nos testes: liga (ou desliga) os selos na thread do teste, sem mexer no que os outros testes veem.
    static FORCADO: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Só nos testes: com esta variável a suíte inteira roda com os selos na imagem, sem mexer na constante (mostra ao
/// integrador o que muda antes de ligar a chave).
#[cfg(test)]
const VARIAVEL_DE_TESTE: &str = "SICRO_SELOS_NA_IMAGEM";

/// A chave como os testes a veem: a constante, ou a variável de teste.
#[cfg(test)]
fn chave() -> bool {
    NA_IMAGEM || std::env::var_os(VARIAVEL_DE_TESTE).is_some_and(|v| v == "1")
}

#[cfg(not(test))]
fn chave() -> bool {
    NA_IMAGEM
}

fn na_imagem() -> bool {
    #[cfg(test)]
    if let Some(v) = FORCADO.with(|f| f.get()) {
        return v;
    }
    chave()
}

#[cfg(test)]
struct Volta(Option<bool>);

#[cfg(test)]
impl Drop for Volta {
    fn drop(&mut self) {
        FORCADO.with(|f| f.set(self.0));
    }
}

/// Liga os selos na thread do teste enquanto o valor devolvido viver.
#[cfg(test)]
pub(crate) fn ligar_no_teste() -> impl Drop {
    Volta(FORCADO.with(|f| f.replace(Some(true))))
}

/// Tira os selos da thread do teste enquanto o valor devolvido viver: para o teste que mede os pixels da cadeia de
/// um efeito na faixa de cima da imagem, onde o selo cairia.
#[cfg(test)]
pub(crate) fn desligar_no_teste() -> impl Drop {
    Volta(FORCADO.with(|f| f.replace(Some(false))))
}

/// Textos escritos na imagem (os mesmos que vão para `feito.roteiro[].selos`).
pub const IMAGEM_PARADA: &str = "Imagem parada";
pub const VELOCIDADE_VARIAVEL: &str = "Velocidade variável";
pub const IMAGEM_AMPLIADA: &str = "Imagem ampliada";
pub const IMAGEM_TRATADA: &str = "Imagem tratada";
pub const IMAGEM_IA: &str = "Imagem ampliada por IA";
/// Entre os selos do mesmo clipe.
const SEPARADOR: &str = " · ";
/// Id do efeito de texto do selo (um por item; não entra no sprite).
const ID: &str = "selo";
/// Lugar do selo (0..1 do canvas): o canto de cima à direita. No de cima à esquerda nasce o carimbo de tempo
/// (`inspetor/novos.ts`, até 40 % da largura) e embaixo ficam texto e legendas: ali um cobriria o outro. A imagem
/// sobreposta também nasce em cima à direita: quem a usar tem de tirá-la dali.
/// Depois: altura da letra, margem e raio da caixa (fração da altura) e largura máxima (o selo longo quebra a linha
/// antes de chegar ao carimbo).
const LUGAR: Lugar = Lugar { x: 0.98, y: 0.03, h: Alinhamento::Dir, v: AlinhamentoV::Topo };
const TAMANHO: f64 = 0.03;
const MARGEM: f64 = 0.008;
const RAIO: f64 = 0.004;
const LARGURA_MAX: f64 = 0.5;
const COR: &str = "#ffffff";
const COR_DA_CAIXA: &str = "#000000b3";
/// Região que mostra menos que isto da imagem (em algum lado) é ampliação.
const FOLGA: f64 = 1e-6;

/// "Velocidade 0,5×".
fn texto_da_velocidade(fator: f64) -> String {
    let s = format!("{fator:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("Velocidade {}×", s.replace('.', ","))
}

/// "Média de 8 imagens".
fn texto_da_media(quadros: u32) -> String {
    format!("Média de {quadros} imagens")
}

fn clipe<'a>(p: &'a ProjetoEdicao, id: &str) -> Option<&'a ClipePrincipal> {
    p.principal.iter().find_map(|c| match c {
        Tolerante::Conhecido(c) if base(c).id == id => Some(c),
        _ => None,
    })
}

fn base(c: &ClipePrincipal) -> &crate::edicao::modelo::ClipeBase {
    match c {
        ClipePrincipal::Trecho(x) => &x.base,
        ClipePrincipal::Congelar(x) => &x.base,
        ClipePrincipal::Imagem(x) => &x.base,
        ClipePrincipal::Titulo(x) => &x.base,
        ClipePrincipal::Tela(x) => &x.base,
    }
}

fn ligados(efeitos: &[Tolerante<Efeito>]) -> impl Iterator<Item = &Efeito> {
    efeitos.iter().filter_map(|e| match e {
        Tolerante::Conhecido(e) if crate::edicao::motor::efeitos::base(e).ligado => Some(e),
        _ => None,
    })
}

/// O enquadramento que vale (o primeiro ligado) mostra só parte da imagem em algum instante, ou há lupa ligada.
fn ampliado(efeitos: &[Tolerante<Efeito>]) -> bool {
    let parte = |c: &crate::edicao::modelo::Caixa| c.w < 1.0 - FOLGA || c.h < 1.0 - FOLGA;
    let enquadre = ligados(efeitos).find_map(|e| match e {
        Efeito::Enquadramento(q) => Some(match &q.regiao {
            Animavel::Fixo(c) => parte(c),
            Animavel::Chaves(ks) => ks.iter().any(|k| parte(&k.v)),
        }),
        _ => None,
    });
    enquadre.unwrap_or(false) || ligados(efeitos).any(|e| matches!(e, Efeito::Lupa(_)))
}

fn realcado(efeitos: &[Tolerante<Efeito>]) -> bool {
    ligados(efeitos).any(|e| matches!(e, Efeito::Estabilizar(_) | Efeito::Lente(_) | Efeito::Desentrelacar(_) | Efeito::Ajustes(_) | Efeito::Nitidez(_)))
}

/// Mídias que o clipe mostra (a do trecho ou do quadro, a foto, o fundo do título, as células da tela).
fn midias_do_clipe(c: &ClipePrincipal) -> Vec<&str> {
    match c {
        ClipePrincipal::Trecho(x) => vec![x.midia.as_str()],
        ClipePrincipal::Congelar(x) => vec![x.midia.as_str()],
        ClipePrincipal::Imagem(x) => vec![x.midia.as_str()],
        ClipePrincipal::Titulo(x) => match &x.titulo.fundo {
            Fundo::Imagem { midia } => vec![midia.as_str()],
            _ => Vec::new(),
        },
        ClipePrincipal::Tela(x) => x.celulas.iter().map(|k| k.midia.as_str()).collect(),
    }
}

/// Textos dos selos que o clipe `id` leva na imagem (os de `feito.roteiro[].selos`), na ordem: velocidade, ampliação,
/// realce (os três desligáveis em `saida.selos`), média de quadros e imagem por IA (fixos).
pub fn do_clipe(p: &ProjetoEdicao, linha: &LinhaResolvida, id: &str) -> Vec<String> {
    let _ = linha;
    if !na_imagem() {
        return Vec::new();
    }
    let Some(c) = clipe(p, id) else { return Vec::new() };
    let efeitos = &base(c).efeitos;
    let s = &p.saida.selos;
    let mut v = Vec::new();
    if s.velocidade {
        match c {
            ClipePrincipal::Congelar(_) => v.push(IMAGEM_PARADA.to_string()),
            ClipePrincipal::Trecho(x) => v.extend(da_velocidade(&x.velocidade)),
            ClipePrincipal::Tela(x) => v.extend(da_velocidade(&x.velocidade)),
            ClipePrincipal::Imagem(_) | ClipePrincipal::Titulo(_) => {}
        }
    }
    if s.ampliacao && ampliado(efeitos) {
        v.push(IMAGEM_AMPLIADA.to_string());
    }
    if s.realce && realcado(efeitos) {
        v.push(IMAGEM_TRATADA.to_string());
    }
    // fixos: o perito não desliga
    if let Some(n) = ligados(efeitos).find_map(|e| match e {
        Efeito::MediaTemporal(m) => Some(m.quadros),
        _ => None,
    }) {
        v.push(texto_da_media(n));
    }
    let por_ia = |midia: &str| p.midias.iter().any(|m| m.id == midia && m.derivacao.as_ref().is_some_and(|d| d.tipo == TipoDerivacao::Superres));
    let sobreposta = ligados(efeitos).any(|e| matches!(e, Efeito::Imagem(i) if por_ia(&i.midia)));
    if sobreposta || midias_do_clipe(c).into_iter().any(por_ia) {
        v.push(IMAGEM_IA.to_string());
    }
    v
}

fn da_velocidade(v: &Velocidade) -> Option<String> {
    match v {
        Velocidade::Constante { fator } if (*fator - 1.0).abs() > 1e-9 => Some(texto_da_velocidade(*fator)),
        Velocidade::Constante { .. } => None,
        Velocidade::Rampa { .. } => Some(VELOCIDADE_VARIAVEL.to_string()),
    }
}

/// O texto do selo como efeito da camada: branco, em caixa escura, no canto de cima à direita.
fn efeito(conteudo: String) -> Efeito {
    Efeito::Texto(EfTexto {
        base: EfBase { id: ID.into(), ligado: true, nome: String::new(), ativo: None },
        conteudo,
        estilo: EstiloTexto {
            fonte: Fonte::Sans,
            negrito: false,
            italico: false,
            tamanho: TAMANHO,
            cor: COR.into(),
            alinhamento: Alinhamento::Dir,
            caixa: Some(CaixaTexto { cor: COR_DA_CAIXA.into(), margem: MARGEM, raio: RAIO }),
            contorno: None,
        },
        lugar: LUGAR,
        largura_max: LARGURA_MAX,
    })
}

/// Itens dos selos nos slots [de_q, de_q + n_q) do bloco, um por clipe que pede selo, em px do canvas `canvas`
/// (`ItemCamada.de` relativo a `de_q`). Na sobreposição de uma transição vale o selo do clipe que entra.
pub fn itens(p: &ProjetoEdicao, linha: &LinhaResolvida, de_q: u64, n_q: u32, canvas: TamanhoPx) -> Result<Vec<ItemCamada>> {
    let ate_q = de_q + n_q as u64;
    let mut v = Vec::new();
    for (i, c) in linha.clipes.iter().enumerate() {
        let textos = do_clipe(p, linha, &c.id);
        if textos.is_empty() {
            continue;
        }
        let fim = linha.clipes.get(i + 1).map_or(c.inicio_q + c.n_q as u64, |prox| prox.inicio_q.min(c.inicio_q + c.n_q as u64));
        let (a, z) = (c.inicio_q.max(de_q), fim.min(ate_q));
        if a >= z {
            continue;
        }
        let quadros = (a..z)
            .map(|q| QuadroItem {
                quadro: QuadroCamada {
                    q,
                    t: tempo::instante_do_slot(q - c.inicio_q, linha.fps),
                    pts: None,
                    a: Afim::IDENTIDADE,
                    canvas,
                    rastreios: BTreeMap::new(),
                    indice: None,
                    programa_s: tempo::instante_do_slot(q, linha.fps),
                },
                transformacao: None,
            })
            .collect();
        v.push(ItemCamada { desenho: Desenho::Efeito { efeito: efeito(textos.join(SEPARADOR)) }, de: (a - de_q) as u32, quadros });
    }
    Ok(v)
}

#[cfg(test)]
mod testes {
    use std::collections::HashMap;
    use std::rc::Rc;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    use serde_json::{json, Value};

    use super::*;
    use crate::edicao::camada::{self, Bitmap};
    use crate::edicao::ffmpeg::sonda::Capacidades;
    use crate::edicao::modelo::{Racional, Selos};
    use crate::edicao::motor::plano::{PedidoCamada, TipoCamada};
    use crate::edicao::quadros::TabelaQuadros;
    use crate::edicao::testes::aceite::{alvo_final, midia, projeto, trecho, Mundo};
    use crate::edicao::testes::fontes::{self, Fonte as FonteDeTeste};
    use crate::edicao::testes::{executor, medir};

    const CANVAS: TamanhoPx = TamanhoPx { w: 1280, h: 720 };

    fn base_do_efeito(tipo: &str, id: &str, campos: Value) -> Value {
        let mut e = json!({ "tipo": tipo, "id": id, "ligado": true, "nome": "", "ativo": null });
        for (k, v) in campos.as_object().unwrap() {
            e[k] = v.clone();
        }
        e
    }

    fn enquadramento(w: f64) -> Value {
        base_do_efeito("enquadramento", "q1", json!({ "regiao": { "x": 0.1, "y": 0.1, "w": w, "h": w }, "ajuste": "cobrir", "interpolacao": "bilinear" }))
    }

    /// Projeto 1280×720 a 30 qps com os clipes dados; `selos` = (velocidade, ampliação, realce).
    fn com(principal: Vec<Value>, selos: (bool, bool, bool), midias: Vec<Value>) -> ProjetoEdicao {
        let mut p = projeto(CANVAS.w, CANVAS.h, (30, 1), midias, principal, json!({}));
        p.saida.selos = Selos { velocidade: selos.0, ampliacao: selos.1, realce: selos.2 };
        p
    }

    fn trecho_json(id: &str, fator: f64, efeitos: Value) -> Value {
        json!({ "tipo": "trecho", "id": id, "rotulo": "", "desligado": false, "transicao_entrada": null, "efeitos": efeitos, "midia": "m1",
                "inicio_s": 1.0, "fim_s": 2.0, "inicio_pts": null, "fim_pts": null, "velocidade": { "tipo": "constante", "fator": fator },
                "audio": { "volume": 1.0, "mudo": false, "manter_tom": true, "silencios": [] } })
    }

    fn congelar_json(id: &str) -> Value {
        json!({ "tipo": "congelar", "id": id, "rotulo": "", "desligado": false, "transicao_entrada": null, "efeitos": [], "midia": "m1",
                "instante_s": 1.0, "instante_pts": null, "duracao_s": 1.0 })
    }

    /// Linha do tempo de `n` clipes de 30 slots, um depois do outro (os ids são os do projeto).
    fn linha(p: &ProjetoEdicao) -> LinhaResolvida {
        let clipes = p
            .principal
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let Tolerante::Conhecido(c) = c else { panic!("clipe desconhecido") };
                crate::edicao::modelo::ClipeResolvido { id: base(c).id.clone(), inicio_q: 30 * i as u64, n_q: 30, sobrepoe_q: 0, fonte: None }
            })
            .collect::<Vec<_>>();
        LinhaResolvida { fps: Racional { num: 30, den: 1 }, total_q: 30 * p.principal.len() as u64, clipes, livres: Vec::new() }
    }

    fn texto_do_item(it: &ItemCamada) -> &str {
        match &it.desenho {
            Desenho::Efeito { efeito: Efeito::Texto(t) } => &t.conteudo,
            outro => panic!("selo que não é texto: {outro:?}"),
        }
    }

    /// Rasteriza a camada dos selos dos slots [de_q, de_q + n) e devolve o bitmap RGBA de cada slot (os iguais
    /// dividem a memória). `SICRO_GUARDAR_QUADROS=<pasta>` copia os PNG para lá.
    fn rasterizar(p: &ProjetoEdicao, l: &LinhaResolvida, de_q: u64, n: u32) -> Vec<Rc<Bitmap>> {
        let itens = itens(p, l, de_q, n, CANVAS).unwrap();
        let mut pedido = PedidoCamada { id: "g".into(), tipo: TipoCamada::Anotacao, caixa_px: [0, 0, CANVAS.w, CANVAS.h], n, hash: String::new(), itens, imagens: Vec::new() };
        pedido.hash = camada::hash_do_pedido(&pedido);
        let tmp = tempfile::Builder::new().prefix("p1r-selos-").tempdir().unwrap();
        let pronta = camada::rasterizar(&pedido, tmp.path(), &AtomicBool::new(false)).unwrap();
        let mut lidos: HashMap<String, Rc<Bitmap>> = HashMap::new();
        let mut png = |nome: &str| {
            lidos
                .entry(nome.to_string())
                .or_insert_with(|| {
                    if let Ok(d) = std::env::var("SICRO_GUARDAR_QUADROS") {
                        let _ = std::fs::copy(pronta.pasta.join(nome), std::path::Path::new(&d).join(format!("selos-{de_q}-{nome}")));
                    }
                    let img = image::open(pronta.pasta.join(nome)).unwrap().to_rgba8();
                    Rc::new(Bitmap { w: img.width(), h: img.height(), canais: 4, px: img.into_raw() })
                })
                .clone()
        };
        match &pronta.lista {
            None => vec![png(&pronta.arquivos[0].arquivo); n as usize],
            Some(lista) => {
                let texto = std::fs::read_to_string(pronta.pasta.join(lista)).unwrap();
                let nomes: Vec<&str> = texto.lines().filter_map(|l| l.strip_prefix("file ")).map(|l| l.trim_matches('\'')).collect();
                // a lista repete o último arquivo no fim (regra do concat): valem os `n` primeiros
                assert!(nomes.len() >= n as usize, "{texto}");
                nomes[..n as usize].iter().map(|n| png(n)).collect()
            }
        }
    }

    /// Caixa [x0, y0, x1, y1) dos pixels não transparentes; None = camada vazia.
    fn caixa_pintada(b: &Bitmap) -> Option<[u32; 4]> {
        let mut c: Option<[u32; 4]> = None;
        for y in 0..b.h {
            for x in 0..b.w {
                if b.px[((y * b.w + x) * 4 + 3) as usize] != 0 {
                    c = Some(match c {
                        None => [x, y, x + 1, y + 1],
                        Some([a, t, z, f]) => [a.min(x), t.min(y), z.max(x + 1), f.max(y + 1)],
                    });
                }
            }
        }
        c
    }

    fn pixel(b: &Bitmap, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * b.w + x) * 4) as usize;
        [b.px[i], b.px[i + 1], b.px[i + 2], b.px[i + 3]]
    }

    /// Aceite do §11: selos ligados por padrão. Com os três ligados, cada clipe leva o texto do que foi mudado nele,
    /// desenhado no canto de cima à direita só nos slots dele; desligar um tira só o dele.
    #[test]
    fn ligados_por_padrao() {
        let _selos = ligar_no_teste();
        // o projeto que a tela cria (fixture gravada pela store) nasce com os três ligados
        let novo: Value = serde_json::from_str(include_str!("../../../tests/fixtures/edicao/projeto-do-store.json")).unwrap();
        assert_eq!(novo["saida"]["selos"], json!({ "velocidade": true, "ampliacao": true, "realce": true }), "selos do projeto novo");
        let principal = || {
            vec![
                trecho_json("c1", 1.0, json!([])),
                trecho_json("c2", 0.5, json!([enquadramento(0.5), base_do_efeito("nitidez", "n1", json!({ "quantidade": 0.5 }))])),
                congelar_json("c3"),
                trecho_json("c4", 4.0, json!([enquadramento(1.0)])),
            ]
        };
        let p = com(principal(), (true, true, true), vec![midia("m1", "video", json!([]))]);
        let l = linha(&p);
        assert_eq!(do_clipe(&p, &l, "c1"), Vec::<String>::new(), "clipe sem mudança não leva selo");
        assert_eq!(do_clipe(&p, &l, "c2"), ["Velocidade 0,5×", "Imagem ampliada", "Imagem tratada"]);
        assert_eq!(do_clipe(&p, &l, "c3"), ["Imagem parada"]);
        assert_eq!(do_clipe(&p, &l, "c4"), ["Velocidade 4×"], "região inteira não é ampliação");
        assert_eq!(do_clipe(&p, &l, "nao-existe"), Vec::<String>::new());
        // um item por clipe com selo, nos slots dele, relativo ao bloco
        let its = itens(&p, &l, 0, 120, CANVAS).unwrap();
        assert_eq!(its.iter().map(|i| (texto_do_item(i), i.de, i.quadros.len())).collect::<Vec<_>>(), [
            ("Velocidade 0,5× · Imagem ampliada · Imagem tratada", 30, 30),
            ("Imagem parada", 60, 30),
            ("Velocidade 4×", 90, 30),
        ]);
        assert_eq!((its[0].quadros[0].quadro.q, its[0].quadros[29].quadro.q, its[0].quadros[0].quadro.canvas), (30, 59, CANVAS));
        // bloco que corta o clipe no meio: só a parte dele
        let its = itens(&p, &l, 45, 30, CANVAS).unwrap();
        assert_eq!(its.iter().map(|i| (i.de, i.quadros.len(), i.quadros[0].quadro.q)).collect::<Vec<_>>(), [(0, 15, 45), (15, 15, 60)]);
        assert!(itens(&p, &l, 0, 30, CANVAS).unwrap().is_empty());
        // desenho: nada no clipe sem selo; no canto de cima à direita nos outros, texto branco sobre caixa escura
        let q = rasterizar(&p, &l, 0, 120);
        assert_eq!(caixa_pintada(&q[0]), None);
        assert_eq!(caixa_pintada(&q[29]), None);
        let [x0, y0, x1, y1] = caixa_pintada(&q[30]).expect("selo do c2");
        let (ex, ey) = ((LUGAR.x * CANVAS.w as f64).round() as u32, (LUGAR.y * CANVAS.h as f64).round() as u32);
        assert!(x1.abs_diff(ex) <= 1 && y0.abs_diff(ey) <= 1, "canto do selo em ({x1}, {y0}), esperado ({ex}, {ey})");
        let larga = (LARGURA_MAX * CANVAS.w as f64 + 2.0 * MARGEM * CANVAS.h as f64).ceil() as u32 + 2;
        assert!(y1 - y0 <= (0.075 * CANVAS.h as f64) as u32 && x1 - x0 > 400 && x1 - x0 <= larga, "caixa do selo {x0},{y0} a {x1},{y1}");
        let meio = pixel(&q[30], x0 + 2, (y0 + y1) / 2);
        assert!(meio[0] < 8 && meio[1] < 8 && meio[2] < 8 && meio[3].abs_diff(0xb3) <= 2, "caixa escura translúcida: {meio:?}");
        let brancos = (x0..x1).flat_map(|x| (y0..y1).map(move |y| (x, y))).filter(|&(x, y)| pixel(&q[30], x, y)[..3].iter().all(|c| *c > 240)).count();
        assert!(brancos > 400, "texto branco: {brancos} px");
        assert_eq!(q[30], q[59], "o selo não muda dentro do clipe");
        // o texto muda a largura: o selo curto do c3 é mais estreito que o do c2
        let c3 = caixa_pintada(&q[60]).expect("selo do c3");
        assert!(c3[2] == x1 && c3[1] == y0 && c3[0] > x0, "{c3:?}");
        // desligar um selo tira só o texto dele; desligar os três tira tudo
        let p = com(principal(), (false, true, true), vec![midia("m1", "video", json!([]))]);
        assert_eq!(do_clipe(&p, &l, "c2"), ["Imagem ampliada", "Imagem tratada"]);
        assert_eq!((do_clipe(&p, &l, "c3"), do_clipe(&p, &l, "c4")), (vec![], vec![]));
        let p = com(principal(), (true, false, true), vec![midia("m1", "video", json!([]))]);
        assert_eq!(do_clipe(&p, &l, "c2"), ["Velocidade 0,5×", "Imagem tratada"]);
        let p = com(principal(), (true, true, false), vec![midia("m1", "video", json!([]))]);
        assert_eq!(do_clipe(&p, &l, "c2"), ["Velocidade 0,5×", "Imagem ampliada"]);
        let p = com(principal(), (false, false, false), vec![midia("m1", "video", json!([]))]);
        assert!(itens(&p, &l, 0, 120, CANVAS).unwrap().is_empty());
        // efeito desligado não pede selo; lupa ligada pede o de ampliação; rampa = velocidade variável
        let mut desligado = enquadramento(0.5);
        desligado["ligado"] = json!(false);
        let lupa = base_do_efeito("lupa", "l1", json!({ "rastreio": "r1", "janela": 0.2, "fator": 2.0, "forma": "circulo", "interpolacao": "bilinear",
            "onde": { "modo": "fixa", "x": 0.8, "y": 0.2 }, "moldura": { "cor": "#ffffff", "espessura": 0.004, "contorno": true }, "ligacao": true }));
        let mut rampa = trecho_json("c3", 1.0, json!([]));
        rampa["velocidade"] = json!({ "tipo": "rampa", "pontos": [{ "t": 1.0, "v": 1.0 }, { "t": 2.0, "v": 0.5 }] });
        let p = com(vec![trecho_json("c1", 1.0, json!([desligado])), trecho_json("c2", 1.0, json!([lupa])), rampa], (true, true, true), vec![midia("m1", "video", json!([]))]);
        let l = linha(&p);
        assert_eq!((do_clipe(&p, &l, "c1"), do_clipe(&p, &l, "c2"), do_clipe(&p, &l, "c3")), (vec![], vec![IMAGEM_AMPLIADA.to_string()], vec![VELOCIDADE_VARIAVEL.to_string()]));
    }

    /// Aceite do §11: selo fixo da média. A média de quadros (e a imagem ampliada por IA) leva selo mesmo com os três
    /// selos desligados, com a contagem, e o selo chega aos pixels.
    #[test]
    fn selo_fixo_da_media() {
        let _selos = ligar_no_teste();
        let media = |n: u32| base_do_efeito("media_temporal", "m9", json!({ "quadros": n }));
        let mut ia = midia("m2", "imagem", json!([]));
        ia["derivacao"] = json!({ "tipo": "superres", "de_sha256": "a", "raiz_sha256": "a", "comando": "", "com_perdas": true });
        let foto = json!({ "tipo": "imagem", "id": "c3", "rotulo": "", "desligado": false, "transicao_entrada": null, "efeitos": [], "midia": "m2", "duracao_s": 1.0 });
        let principal = || vec![trecho_json("c1", 1.0, json!([media(8)])), trecho_json("c2", 1.0, json!([])), foto.clone()];
        let midias = || vec![midia("m1", "video", json!([])), ia.clone()];
        let p = com(principal(), (false, false, false), midias());
        let l = linha(&p);
        assert_eq!(do_clipe(&p, &l, "c1"), ["Média de 8 imagens"], "fixo: não depende de `saida.selos`");
        assert_eq!(do_clipe(&p, &l, "c2"), Vec::<String>::new());
        assert_eq!(do_clipe(&p, &l, "c3"), [IMAGEM_IA]);
        let q = rasterizar(&p, &l, 0, 90);
        let caixa = caixa_pintada(&q[0]).expect("selo da média com os selos desligados");
        assert!(caixa[2].abs_diff((LUGAR.x * CANVAS.w as f64).round() as u32) <= 1 && caixa[1] <= 23 && caixa[2] - caixa[0] > 200, "{caixa:?}");
        assert_eq!(caixa_pintada(&q[30]), None, "clipe sem média");
        assert!(caixa_pintada(&q[60]).is_some(), "foto ampliada por IA");
        // com o selo de realce ligado, a média não vira "Imagem tratada": o texto dela já diz o que foi feito
        let p = com(principal(), (true, true, true), midias());
        assert_eq!(do_clipe(&p, &l, "c1"), ["Média de 8 imagens"]);
        // média desligada não leva selo; a contagem é a do efeito
        let mut desligada = media(15);
        desligada["ligado"] = json!(false);
        let p = com(vec![trecho_json("c1", 1.0, json!([desligada])), trecho_json("c2", 0.25, json!([media(15), base_do_efeito("ajustes", "a1", json!({ "brilho": 0.1, "contraste": 1.0, "saturacao": 1.0, "gama": 1.0 }))]))], (true, true, true), midias());
        let l = linha(&p);
        assert_eq!(do_clipe(&p, &l, "c1"), Vec::<String>::new());
        assert_eq!(do_clipe(&p, &l, "c2"), ["Velocidade 0,25×", "Imagem tratada", "Média de 15 imagens"]);
    }

    /// O selo não cai sobre o carimbo de tempo no lugar em que ele nasce (`novoCarimbo`: 0,02 × 0,03, à esquerda, no
    /// formato mais largo). Com o selo mais longo (todos os textos de um clipe), as caixas pintadas não se tocam.
    #[test]
    fn selo_nao_cobre_o_carimbo_no_lugar_padrao() {
        let _selos = ligar_no_teste();
        let media = base_do_efeito("media_temporal", "m9", json!({ "quadros": 15 }));
        let nitidez = base_do_efeito("nitidez", "n1", json!({ "quantidade": 0.5 }));
        let mut ia = midia("m1", "video", json!([]));
        ia["derivacao"] = json!({ "tipo": "superres", "de_sha256": "a", "raiz_sha256": "a", "comando": "", "com_perdas": true });
        let p = com(vec![trecho_json("c1", 0.25, json!([enquadramento(0.5), nitidez, media]))], (true, true, true), vec![ia]);
        let l = linha(&p);
        assert_eq!(do_clipe(&p, &l, "c1"), ["Velocidade 0,25×", "Imagem ampliada", "Imagem tratada", "Média de 15 imagens", "Imagem ampliada por IA"]);
        let selo = caixa_pintada(&rasterizar(&p, &l, 0, 1)[0]).expect("selo");
        // o carimbo como a tela o cria (`inspetor/novos.ts`)
        let carimbo: Efeito = serde_json::from_value(base_do_efeito("carimbo", "k1", json!({ "mostra": "tempo_fonte", "formato": "hms_ms", "relogio": null,
            "estilo": { "fonte": "sans", "negrito": false, "italico": false, "tamanho": 0.033, "cor": "#ffffff", "alinhamento": "centro",
                        "caixa": { "cor": "#0000008c", "margem": 0.008, "raio": 0.0 }, "contorno": null },
            "lugar": { "x": 0.02, "y": 0.03, "h": "esq", "v": "topo" } }))).unwrap();
        let quadro = QuadroCamada { q: 0, t: 86399.888, pts: None, a: Afim::IDENTIDADE, canvas: CANVAS, rastreios: BTreeMap::new(), indice: Some(0), programa_s: 0.0 };
        let item = ItemCamada { desenho: Desenho::Efeito { efeito: carimbo }, de: 0, quadros: vec![QuadroItem { quadro, transformacao: None }] };
        let mut pedido = PedidoCamada { id: "k".into(), tipo: TipoCamada::Anotacao, caixa_px: [0, 0, CANVAS.w, CANVAS.h], n: 1, hash: String::new(), itens: vec![item], imagens: Vec::new() };
        pedido.hash = camada::hash_do_pedido(&pedido);
        let tmp = tempfile::Builder::new().prefix("p1r-selos-k-").tempdir().unwrap();
        let pronta = camada::rasterizar(&pedido, tmp.path(), &AtomicBool::new(false)).unwrap();
        let img = image::open(pronta.pasta.join(&pronta.arquivos[0].arquivo)).unwrap().to_rgba8();
        let carimbo = caixa_pintada(&Bitmap { w: img.width(), h: img.height(), canais: 4, px: img.into_raw() }).expect("carimbo");
        assert!(carimbo[0] <= 27 && carimbo[2] > 200, "carimbo no canto de cima à esquerda: {carimbo:?}");
        let cruza = |a: [u32; 4], b: [u32; 4]| a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
        assert!(!cruza(selo, carimbo), "selo {selo:?} sobre o carimbo {carimbo:?}");
        // o selo longo quebra a linha em vez de passar da largura máxima
        let larga = (LARGURA_MAX * CANVAS.w as f64 + 2.0 * MARGEM * CANVAS.h as f64).ceil() as u32 + 2;
        assert!(selo[2] - selo[0] <= larga && selo[3] - selo[1] > (0.075 * CANVAS.h as f64) as u32 && selo[0] > carimbo[2], "{selo:?}");
    }

    /// Na sobreposição de uma transição os dois clipes dividem slots: vale um selo só, o do clipe que entra.
    #[test]
    fn na_transicao_vale_o_selo_de_quem_entra() {
        let _selos = ligar_no_teste();
        let p = com(vec![trecho_json("c1", 0.5, json!([])), congelar_json("c2")], (true, true, true), vec![midia("m1", "video", json!([]))]);
        let mut l = linha(&p);
        // c2 entra 10 slots antes do fim do c1
        l.clipes[1].inicio_q = 20;
        l.clipes[1].sobrepoe_q = 10;
        let its = itens(&p, &l, 0, 50, CANVAS).unwrap();
        assert_eq!(its.iter().map(|i| (texto_do_item(i), i.de, i.quadros.len())).collect::<Vec<_>>(), [("Velocidade 0,5×", 0, 20), ("Imagem parada", 20, 30)]);
    }

    /// A chave do integrador: com `NA_IMAGEM` desligado nada é desenhado nem listado no `feito` (os dois juntos: o
    /// relatório nunca diz que há selo numa imagem que não o tem); ligado, o clipe a 0,5× leva o dele.
    #[test]
    fn selos_seguem_a_chave_do_integrador() {
        let p = com(vec![trecho_json("c1", 0.5, json!([base_do_efeito("media_temporal", "m9", json!({ "quadros": 4 }))]))], (true, true, true), vec![midia("m1", "video", json!([]))]);
        let l = linha(&p);
        let esperado: Vec<String> = if chave() { vec!["Velocidade 0,5×".into(), "Média de 4 imagens".into()] } else { vec![] };
        assert_eq!(do_clipe(&p, &l, "c1"), esperado);
        assert_eq!(itens(&p, &l, 0, 30, CANVAS).unwrap().len(), usize::from(chave()));
        {
            let _selos = ligar_no_teste();
            assert_eq!(do_clipe(&p, &l, "c1"), ["Velocidade 0,5×", "Média de 4 imagens"]);
            assert_eq!(itens(&p, &l, 0, 30, CANVAS).unwrap().len(), 1);
        }
        // ao sair do teste volta ao que a chave manda
        assert_eq!(do_clipe(&p, &l, "c1"), esperado);
    }

    fn cap_do_teste() -> Option<Arc<Capacidades>> {
        let cap = fontes::cap();
        if cap.is_none() && std::env::var("SICRO_EXIGE_FFMPEG").is_ok_and(|v| v == "1") {
            panic!("SICRO_EXIGE_FFMPEG=1 e nenhum ffmpeg achado");
        }
        cap
    }

    /// O selo chega ao vídeo: trecho a 0,5× renderizado pelo compilador. Nos slots do clipe com selo, o canto de cima
    /// à direita traz a caixa escura com texto claro; no clipe a 1× o mesmo canto é o da fonte, e o resto do quadro
    /// não muda com o selo (comparado com o mesmo projeto de selo desligado).
    #[test]
    fn selo_chega_ao_video_e_nao_mexe_no_resto() {
        let Some(cap) = cap_do_teste() else { return };
        let _selos = ligar_no_teste();
        let video = fontes::gerar(FonteDeTeste::Mp4).unwrap();
        let tab = TabelaQuadros::do_decodificador(&cap, &video, &AtomicBool::new(false), &mut |_| {}).unwrap();
        let info = crate::edicao::quadros::fonte_info(&cap, &video, &"a".repeat(64), Arc::new(tab), None).unwrap();
        let tab = info.quadros.clone();
        let mundo = Mundo::novo(&cap).com("m1", &info);
        let principal = || vec![trecho("c1", "m1", &tab, 30, 40, 1.0, json!([])), trecho("c2", "m1", &tab, 40, 50, 0.5, json!([]))];
        let render = |selo: bool| {
            // canvas com o dobro da fonte (320×180): letra de 11 px
            let mut p = projeto(2 * info.exibido.0, 2 * info.exibido.1, (30, 1), vec![midia("m1", "video", json!([]))], principal(), json!({}));
            p.saida.selos.velocidade = selo;
            let plano = mundo.compilar(&p, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{}: {e}", cap.versao));
            assert_eq!(plano.proveniencia.roteiro[1].selos, if selo { vec!["Velocidade 0,5×".to_string()] } else { vec![] });
            assert_eq!(plano.proveniencia.roteiro[0].selos, Vec::<String>::new());
            let pasta = fontes::pasta_temporaria(&format!("p1r-selo-{selo}"));
            let op = executor::Opcoes { sem_perdas: true, ..Default::default() };
            let r = executor::rodar(&cap, &plano, &pasta, &op).unwrap_or_else(|e| panic!("{}: {e}", cap.versao));
            let (mut q, mut origem) = (Vec::new(), Vec::new());
            for b in &r.blocos {
                q.extend(medir::quadros(&cap, b).unwrap());
                origem.extend(medir::quadros_de_origem(&cap, b).unwrap());
            }
            assert_eq!(q.len() as u64, plano.total_q);
            let _ = std::fs::remove_dir_all(&pasta);
            (q, origem)
        };
        let ((com_selo, origem_com), (sem_selo, origem_sem)) = (render(true), render(false));
        assert_eq!(com_selo.len(), 30, "10 slots a 1× e 20 a 0,5×");
        let (w, h) = (com_selo[0].w, com_selo[0].h);
        // região do selo: do canto para a esquerda, na largura máxima dele (com a margem da caixa e 2 px do croma)
        let (x1, y0) = ((LUGAR.x * w as f64).round() as u32, (LUGAR.y * h as f64).round() as u32);
        let alto = ((TAMANHO * 1.6 + 2.0 * MARGEM) * h as f64).ceil() as u32;
        let larga = (LARGURA_MAX * w as f64 + 2.0 * MARGEM * h as f64).ceil() as u32 + 2;
        let dentro = |x: u32, y: u32| x + larga >= x1 && x < x1 + 2 && y + 2 >= y0 && y < y0 + alto + 2;
        let fora_diferente = |a: &medir::QuadroRgb, b: &medir::QuadroRgb| {
            (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| !dentro(x, y)).filter(|&(x, y)| {
                let i = ((y * w + x) * 3) as usize;
                (0..3).any(|c| a.px[i + c].abs_diff(b.px[i + c]) > 2)
            }).count()
        };
        let na_regiao = |a: &medir::QuadroRgb, f: &dyn Fn(&[u8]) -> bool| {
            (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| dentro(x, y)).filter(|&(x, y)| f(&a.px[((y * w + x) * 3) as usize..][..3])).count()
        };
        for q in 0..30usize {
            assert_eq!(fora_diferente(&com_selo[q], &sem_selo[q]), 0, "slot {q}: o selo mexeu fora da região dele ({})", cap.versao);
            let mudou = na_regiao(&com_selo[q], &|_| true) - {
                let (a, b) = (&com_selo[q], &sem_selo[q]);
                (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| dentro(x, y)).filter(|&(x, y)| {
                    let i = ((y * w + x) * 3) as usize;
                    (0..3).all(|c| a.px[i + c].abs_diff(b.px[i + c]) <= 2)
                }).count()
            };
            if q < 10 {
                assert_eq!(mudou, 0, "slot {q} (1×): não leva selo ({})", cap.versao);
            } else {
                assert!(mudou > 200, "slot {q} (0,5×): selo ausente, {mudou} px mudaram ({})", cap.versao);
                // no retângulo do que mudou: a caixa escura e, sobre ela, o texto claro (seja qual for o fundo da fonte)
                let (a, b) = (&com_selo[q], &sem_selo[q]);
                let mudados: Vec<(u32, u32)> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| dentro(x, y)).filter(|&(x, y)| {
                    let i = ((y * w + x) * 3) as usize;
                    (0..3).any(|c| a.px[i + c].abs_diff(b.px[i + c]) > 2)
                }).collect();
                let (xa, xb) = (mudados.iter().map(|p| p.0).min().unwrap(), mudados.iter().map(|p| p.0).max().unwrap());
                let (ya, yb) = (mudados.iter().map(|p| p.1).min().unwrap(), mudados.iter().map(|p| p.1).max().unwrap());
                let conta = |f: &dyn Fn(&[u8]) -> bool| (ya..=yb).flat_map(|y| (xa..=xb).map(move |x| (x, y))).filter(|&(x, y)| f(&a.px[((y * w + x) * 3) as usize..][..3])).count();
                let (claros, escuros) = (conta(&|p| p.iter().all(|c| *c > 180)), conta(&|p| p.iter().all(|c| *c < 100)));
                assert!(claros > 60 && escuros > 200, "slot {q}: texto claro sobre caixa escura ({claros} claros, {escuros} escuros) ({})", cap.versao);
                // encostado no canto (sobre fundo escuro só o texto muda: fica a margem da caixa de folga)
                let folga = (MARGEM * h as f64).ceil() as u32 + 3;
                assert!((xb + 1).abs_diff(x1) <= folga && ya.abs_diff(y0) <= folga + (TAMANHO * h as f64 / 2.0) as u32, "slot {q}: selo de {xa},{ya} a {xb},{yb}, canto em {x1},{y0} ({})", cap.versao);
            }
        }
        // a identidade do quadro continua legível com o selo por cima: 30..39 a 1× e 40..49 duas vezes cada
        let esperado: Vec<u32> = (30..40).chain((40..50).flat_map(|j| [j, j])).collect();
        assert_eq!(origem_com, esperado, "{}", cap.versao);
        assert_eq!(origem_sem, esperado, "{}", cap.versao);
    }
}
