//! Efeitos de origem (§5.3, §5.4): `bwdif` (desentrelaçar), `vidstabtransform` (estabilizar), `lenscorrection`
//! (lente), `eq` (ajustes), `unsharp` (nitidez), `tmix` (média de quadros), entre o `ocultar` e o mapa de quadros.
//! O `.trf` sai por `Previa::Vidstab` (só com `trf_sha256` None) e o intermediário de origem por `edicao/origem.rs`.
//! Dono: P1.4.
//!
//! A cadeia escolhe os quadros de saída pelo ÍNDICE e impõe o pts da tabela do clipe por árvore sobre `N`: não
//! depende do pts que o `bwdif` e o `tmix` dão.

use std::borrow::Cow;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::edicao::cor;
use crate::edicao::modelo::{
    BordaEstab, ClipePrincipal, EfAjustes, EfDesentrelacar, EfEstabilizar, EfLente, EfMediaTemporal, EfNitidez, Efeito, ModoDesentrelace, OrdemCampos, ProjetoEdicao, Tolerante,
};
use crate::edicao::motor::clipe::{erro, Montador};
use crate::edicao::motor::efeitos::{self, Familia};
use crate::edicao::motor::grafo::filtro;
use crate::edicao::motor::plano::{Previa, ProvEstabilizacao};
use crate::edicao::motor::{expr, FonteInfo};
use crate::edicao::quadros::TabelaQuadros;
use crate::edicao::ffmpeg::args;
use crate::error::Result;
use crate::image_processing::lens_correction::{para_ffmpeg, LensCoefficients};

/// Quadros que o `bwdif` olha antes e depois de cada um (§5.3, "Janela").
const VIZINHOS_ANTES: usize = 2;
const VIZINHOS_DEPOIS: usize = 1;
/// `media_temporal.quadros` aceito (o mesmo da validação).
const MEDIA_QUADROS: std::ops::RangeInclusive<u32> = 2..=15;
/// Teto do `smoothing` do vidstab (quadros de cada lado).
const SUAVIZAR_MAX: u32 = 1000;
/// Detecção do vidstab: os padrões, escritos (mudança de padrão do ffmpeg não muda o `.trf`).
const DETECCAO: &str = "shakiness=5:accuracy=15:stepsize=6:mincontrast=0.3";

/// Detalhe do `nao_suportado` de um efeito de origem que o motor ainda não aplica; None = aplica (ou não é de
/// origem). `com_fonte` = clipe que lê quadros da fonte (trecho, congelado); foto e título passam `false`.
/// Uma regra só: `validar` e `clipe::cadeia` perguntam aqui (contratos (h) 1).
pub fn recusa(e: &Efeito, com_fonte: bool) -> Option<&'static str> {
    // foto e título não passam pela cadeia de origem: recusado, nunca ignorado
    (efeitos::familia(e) == Familia::Origem && !com_fonte).then(|| efeitos::tipo(e))
}

/// Detalhe da recusa que depende da tabela da fonte: desentrelace por campo em trecho de fonte cujo passo entre
/// quadros não tem meio (um tique por quadro: AVI, fluxo cru). O motor recusa em `leitura`.
// CONTRATO: acréscimo. A validação (P1.R) pode acusar antes com esta mesma regra.
pub fn recusa_na_fonte(e: &Efeito, fonte: &TabelaQuadros) -> Option<&'static str> {
    matches!(e, Efeito::Desentrelacar(d) if d.modo == ModoDesentrelace::Campo && !campo_cabe(fonte)).then_some("desentrelacar_campo")
}

/// Os efeitos de origem ligados, um por tipo, na ordem da cadeia.
#[derive(Default)]
struct Origem<'a> {
    des: Option<&'a EfDesentrelacar>,
    est: Option<&'a EfEstabilizar>,
    lente: Option<&'a EfLente>,
    ajustes: Option<&'a EfAjustes>,
    nitidez: Option<&'a EfNitidez>,
    media: Option<&'a EfMediaTemporal>,
}

/// Efeito de origem vale no clipe inteiro (`ativo` parcial não entra na cadeia) e um só por tipo.
fn origem(efeitos_do_clipe: &[Tolerante<Efeito>]) -> Result<Origem<'_>> {
    let mut o = Origem::default();
    for e in efeitos::ligados(efeitos_do_clipe, Familia::Origem) {
        let (tipo, id) = (efeitos::tipo(e), &efeitos::base(e).id);
        if efeitos::base(e).ativo.is_some() {
            return Err(erro("item_invalido", format!("efeito {tipo} {id}: ativo")));
        }
        let repetido = match e {
            Efeito::Desentrelacar(x) => o.des.replace(x).is_some(),
            Efeito::Estabilizar(x) => o.est.replace(x).is_some(),
            Efeito::Lente(x) => o.lente.replace(x).is_some(),
            Efeito::Ajustes(x) => o.ajustes.replace(x).is_some(),
            Efeito::Nitidez(x) => o.nitidez.replace(x).is_some(),
            Efeito::MediaTemporal(x) => o.media.replace(x).is_some(),
            _ => false,
        };
        if repetido {
            return Err(erro("item_invalido", format!("efeito {tipo} {id}: repetido")));
        }
    }
    if let Some(m) = o.media {
        if !MEDIA_QUADROS.contains(&m.quadros) {
            return Err(erro("item_invalido", format!("efeito media_temporal {}: quadros", m.base.id)));
        }
    }
    Ok(o)
}

/// `trf_sha256` com forma de SHA-256 em minúsculas (o nome vira caminho e entra no grafo).
fn sha_do_trf(sha: &str) -> bool {
    sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// Campo do efeito de origem fora do que os filtros aceitam (`eq`, `unsharp`, `lenscorrection`, `vidstabtransform`,
/// `tmix`); None = ok (ou não é de origem). `exibido` = tamanho da fonte (a lente converte os coeficientes por ele).
fn campo_fora(e: &Efeito, exibido: (u32, u32)) -> Option<&'static str> {
    match e {
        Efeito::MediaTemporal(m) => (!MEDIA_QUADROS.contains(&m.quadros)).then_some("quadros"),
        Efeito::Estabilizar(s) => {
            if !finito(s.zoom_pct, -100.0..=100.0) {
                Some("zoom_pct")
            } else if s.trf_sha256.as_deref().is_some_and(|x| !sha_do_trf(x)) {
                // o nome vira arquivo e texto do grafo
                Some("trf_sha256")
            } else {
                None
            }
        }
        Efeito::Lente(l) => {
            if ![l.k1, l.k2].iter().all(|v| v.is_finite()) {
                return Some("k1");
            }
            if !finito(l.cx, 0.0..=1.0) {
                return Some("cx");
            }
            if !finito(l.cy, 0.0..=1.0) {
                return Some("cy");
            }
            let c = LensCoefficients { k1: l.k1 as f32, k2: l.k2 as f32, k3: 0.0 };
            (!c.is_identity() && para_ffmpeg(c, exibido.0, exibido.1).is_none()).then_some("k1")
        }
        Efeito::Ajustes(a) => {
            if !finito(a.brilho, -1.0..=1.0) {
                Some("brilho")
            } else if !finito(a.contraste, -1000.0..=1000.0) {
                Some("contraste")
            } else if !finito(a.gama, 0.1..=10.0) {
                Some("gama")
            } else if !finito(a.saturacao, 0.0..=3.0) {
                Some("saturacao")
            } else {
                None
            }
        }
        Efeito::Nitidez(n) => (!finito(n.quantidade, -2.0..=5.0)).then_some("quantidade"),
        _ => None,
    }
}

// CONTRATO: acréscimo (fecho da Fase 1): tudo o que a cadeia de origem recusa com `item_invalido`, para a validação
// acusar antes com a MESMA regra (contratos (h) 1).
/// Efeitos de origem ligados que a cadeia recusa: (id, campo). `ativo` = intervalo parcial (o efeito de origem vale
/// no clipe inteiro); campo None = segundo efeito ligado do mesmo tipo; os demais, o campo fora da faixa do filtro.
pub fn invalidos(efeitos_do_clipe: &[Tolerante<Efeito>], exibido: (u32, u32)) -> Vec<(String, Option<&'static str>)> {
    let mut v = Vec::new();
    let mut vistos: Vec<&'static str> = Vec::new();
    for e in efeitos::ligados(efeitos_do_clipe, Familia::Origem) {
        let (tipo, id) = (efeitos::tipo(e), efeitos::base(e).id.clone());
        if efeitos::base(e).ativo.is_some() {
            v.push((id, Some("ativo")));
        } else if vistos.contains(&tipo) {
            v.push((id, None));
        } else if let Some(c) = campo_fora(e, exibido) {
            v.push((id, Some(c)));
        }
        vistos.push(tipo);
    }
    v
}

/// Primeiro `desentrelacar` ligado.
fn desentrelace(efeitos_do_clipe: &[Tolerante<Efeito>]) -> Option<&EfDesentrelacar> {
    efeitos_do_clipe.iter().find_map(|e| match e {
        Tolerante::Conhecido(Efeito::Desentrelacar(d)) if d.base.ligado => Some(d),
        _ => None,
    })
}

/// O segundo campo cabe entre dois quadros na base da fonte: todo passo (e o mediano, que fecha o último) ≥ 2 tiques.
fn campo_cabe(fonte: &TabelaQuadros) -> bool {
    !fonte.pts.is_empty() && fonte.passo_mediano >= 2 && fonte.pts.windows(2).all(|p| p[1].saturating_sub(p[0]) >= 2)
}

/// O grafo dobra os quadros (`bwdif=mode=send_field`)?
fn dobra(d: Option<&EfDesentrelacar>, fonte: &TabelaQuadros) -> bool {
    d.is_some_and(|d| d.modo == ModoDesentrelace::Campo) && campo_cabe(fonte)
}

/// Metade do passo, arredondada para cima: onde cai o segundo campo.
fn meio(passo: i64) -> i64 {
    (passo + 1) / 2
}

/// Tabela que o mapa de quadros do clipe usa: a da fonte; com `desentrelacar` por campo, uma entrada por campo.
/// Quem chama: `tempo::resolver` (via `medir`) e `clipe::cadeia`.
// CONTRATO: a tabela de campos fica na BASE DA FONTE (o §5.4 diz "na base dobrada"): o primeiro campo guarda o pts
// do quadro (a identidade do D4 segue valendo para corte e âncora) e o segundo cai no ponto médio até o seguinte,
// arredondado para o tique (meio tique de erro quando o passo é ímpar). Passo de 1 tique não tem meio: a tabela
// fica a da fonte e `leitura` recusa o trecho.
pub fn tabela<'a>(efeitos: &[Tolerante<Efeito>], fonte: &'a TabelaQuadros) -> Cow<'a, TabelaQuadros> {
    if !dobra(desentrelace(efeitos), fonte) {
        return Cow::Borrowed(fonte);
    }
    let n = fonte.pts.len();
    let (mut pts, mut chave) = (Vec::with_capacity(2 * n), Vec::with_capacity(2 * n));
    let (mut menor, mut maior) = (i64::MAX, i64::MIN);
    for j in 0..n {
        let passo = if j + 1 < n { fonte.pts[j + 1] - fonte.pts[j] } else { fonte.passo_mediano };
        let m = meio(passo);
        pts.extend([fonte.pts[j], fonte.pts[j] + m]);
        chave.extend([fonte.chave.get(j).copied().unwrap_or(false), false]);
        (menor, maior) = (menor.min(m), maior.max(m));
        if j + 1 < n {
            (menor, maior) = (menor.min(passo - m), maior.max(passo - m));
        }
    }
    Cow::Owned(TabelaQuadros {
        pts,
        chave,
        // o fim do último campo é o fim do último quadro da fonte
        passo_mediano: fonte.passo_mediano - meio(fonte.passo_mediano),
        passo: (menor, maior),
        ..fonte.clone()
    })
}

/// O que a cadeia lê da fonte para entregar os quadros `[ja, jb)` da tabela do clipe.
// CONTRATO: `faixa`, `campos` e `centrada` acrescentados: `aplicar` não recebe a tabela nem a faixa pedida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Leitura {
    /// Quadros da FONTE lidos (corte por pts, `showinfo`, máscara do `ocultar`): [de, ate). Média lê N−1 antes,
    /// desentrelace 2, estabilizar desde `analise.de_pts` (§5.3, "Janela").
    pub ler: (usize, usize),
    /// Quadros da fonte do clipe inteiro (recorte e raio da ocultação, que não podem variar com a janela).
    pub clipe: (usize, usize),
    /// Quadros que chegam ao `fps@` (a conferência espera `lidos + 1`).
    pub lidos: u32,
    /// A faixa pedida, nos índices da tabela do clipe.
    pub faixa: (usize, usize),
    /// A tabela do clipe é a de campos.
    pub campos: bool,
    /// Média centrada no quadro (clipe de um quadro só: congelado).
    pub centrada: bool,
}

/// Quadros `[d0, d1]` da fonte cobertos pelo `.trf` (a mesma conta da validação).
fn analise(e: &EfEstabilizar, fonte: &TabelaQuadros) -> (usize, usize) {
    (fonte.do_par(e.analise.de_s, Some(e.analise.de_pts)), fonte.do_par(e.analise.ate_s, Some(e.analise.ate_pts)))
}

/// Quadros antes e depois que a média junta a cada quadro de saída.
fn margem_da_media(m: Option<&EfMediaTemporal>, centrada: bool) -> (usize, usize) {
    match m.map(|m| m.quadros as usize) {
        None => (0, 0),
        Some(n) if centrada => ((n - 1) / 2, n / 2),
        Some(n) => (n - 1, 0),
    }
}

/// Faixa `[a, b)` da tabela do clipe nos índices da saída do `bwdif` (campos quando o grafo dobra).
fn na_saida(dobra: bool, campos: bool, (a, b): (usize, usize)) -> (usize, usize) {
    if dobra && !campos {
        // clipe de um quadro com a tabela da fonte (congelado): o primeiro campo dele
        (2 * a, 2 * a + (b - a))
    } else {
        (a, b)
    }
}

/// Quadros da fonte lidos para entregar `[oa, ob)` da saída do `bwdif`.
fn ler_de(o: &Origem, fonte: &TabelaQuadros, dobra: bool, centrada: bool, (oa, ob): (usize, usize)) -> (usize, usize) {
    let n = fonte.pts.len();
    let (antes, depois) = margem_da_media(o.media, centrada);
    let (mut sa, mut sb) = (oa.saturating_sub(antes), ob + depois);
    if dobra {
        (sa, sb) = (sa / 2, sb.div_ceil(2));
    }
    if o.des.is_some() {
        (sa, sb) = (sa.saturating_sub(VIZINHOS_ANTES), sb + VIZINHOS_DEPOIS);
    }
    sb = sb.min(n);
    if let Some(e) = o.est {
        // o `.trf` é indexado desde o primeiro quadro analisado; nada além do último
        let (d0, d1) = analise(e, fonte);
        (sa, sb) = (d0, sb.min(d1 + 1));
    }
    (sa, sb)
}

/// Leitura dos quadros `faixa` = [ja, jb) da tabela do clipe; `clipe` = [j_in, j_out) do mapa.
/// Sem efeito de origem: a própria faixa.
pub fn leitura(efeitos: &[Tolerante<Efeito>], fonte: &TabelaQuadros, tabela: &TabelaQuadros, faixa: (usize, usize), clipe: (usize, usize)) -> Result<Leitura> {
    let lidos = (faixa.1 - faixa.0) as u32;
    let o = origem(efeitos)?;
    let campos = tabela.pts.len() != fonte.pts.len();
    let centrada = o.media.is_some() && clipe.1 - clipe.0 == 1;
    let tempo = o.des.is_some() || o.est.is_some() || o.media.is_some();
    if !tempo {
        return Ok(Leitura { ler: faixa, clipe, lidos, faixa, campos, centrada });
    }
    let dobra = dobra(o.des, fonte);
    if let Some(d) = o.des.filter(|d| d.modo == ModoDesentrelace::Campo && !campos) {
        // por campo com a tabela da fonte só serve a um quadro (congelado: o primeiro campo dele)
        if clipe.1 - clipe.0 != 1 || faixa.1 - faixa.0 != 1 {
            return Err(erro("nao_suportado", format!("efeito desentrelacar {} por campo nesta base de tempo", d.base.id)));
        }
    }
    let inteiro = ler_de(&o, fonte, dobra, centrada, na_saida(dobra, campos, clipe));
    let ler = ler_de(&o, fonte, dobra, centrada, na_saida(dobra, campos, faixa));
    if let Some(e) = o.est {
        let (d0, d1) = analise(e, fonte);
        let (ca, cb) = if campos { (clipe.0 / 2, clipe.1.div_ceil(2)) } else { clipe };
        if d1 < d0 || ca < d0 || cb > d1 + 1 {
            return Err(erro("estabilizar_fora_da_analise", &e.base.id));
        }
    }
    Ok(Leitura { ler, clipe: inteiro, lidos, faixa, campos, centrada })
}

/// Há efeito de origem ligado que o motor aplica?
pub fn ativos(efeitos: &[Tolerante<Efeito>], com_fonte: bool) -> bool {
    efeitos::ligados(efeitos, Familia::Origem).into_iter().any(|e| recusa(e, com_fonte).is_none())
}

fn finito(v: f64, faixa: std::ops::RangeInclusive<f64>) -> bool {
    v.is_finite() && faixa.contains(&v)
}

/// Cadeia de cor da fonte para este clipe (`cor::cadeia_fonte`). Com `desentrelacar` ligado o `scale` trata cada
/// campo em separado (`interl=1`): a conversão comum mistura o croma dos dois campos antes do `bwdif` (26 % do croma
/// do outro campo nas bordas em movimento, igual nos dois ffmpeg).
// CONTRATO: acréscimo. O intermediário, a análise do movimento, `clipe::trecho` e `clipe::congelar` usam esta.
pub fn cor_da_fonte(efeitos: &[Tolerante<Efeito>], fonte: &FonteInfo) -> Option<String> {
    let c = cor::cadeia_fonte(fonte.matriz, fonte.faixa, &fonte.pix_fmt, fonte.exibido)?;
    if desentrelace(efeitos).is_none() {
        return Some(c);
    }
    let (w, h) = fonte.exibido;
    Some(c.replacen(&format!("scale={w}:{h}:"), &format!("scale={w}:{h}:interl=1:"), 1))
}

/// Só deixa passar o quadro de índice `i` cujo pts é `pts[i]` (na base do elo). A seleção por índice adiante tem
/// folga (vizinho lido depois, clone do `ocultar`): leitura que abre depois do previsto, ou que perde um quadro,
/// entregaria o quadro vizinho com a contagem fechando. Com a guarda não chega quadro e a conferência por execução
/// acusa. O que vem além da lista (o clone do fim) passa.
pub(crate) fn guarda(pts: &[i64]) -> String {
    // o `select` conta em `n` (do zero, medido nos dois ffmpeg); a árvore do `setpts` conta em `N`
    let previsto = expr::arvore_pts(pts).replace('N', "n");
    filtro("select", None, &[("expr", format!("gte(n,{})+eq(pts,{previsto})", pts.len()))])
}

/// A guarda da análise do movimento, cuja cadeia vai na linha de comando (a árvore de um trecho longo de passo
/// irregular não cabe nela): só passa quadro se o PRIMEIRO lido for o de pts `p0`. Leitura que perde quadro no meio
/// não é vista aqui: quem roda a análise confere a contagem (`quadros_da_analise`).
fn guarda_do_inicio(p0: i64) -> String {
    filtro("select", None, &[("expr", format!("eq(start_pts,{p0})"))])
}

/// `bwdif` com a paridade ESCRITA: a que o perito impôs, senão a do fluxo (`FonteInfo.ordem_campos`). A marca por
/// quadro, que o `parity=auto` leria, não sobrevive à cadeia de cor da faixa cheia (`mergeplanes`): com `auto` o campo
/// de baixo primeiro saía trocado. Só fica `auto` quando a fonte não tem marca nenhuma.
fn bwdif(d: &EfDesentrelacar, dobra: bool, fonte: &FonteInfo) -> String {
    let modo = if dobra { "send_field" } else { "send_frame" };
    let ordem = match (d.ordem, fonte.ordem_campos) {
        (OrdemCampos::Tff, _) | (OrdemCampos::Auto, OrdemCampos::Tff) => "tff",
        (OrdemCampos::Bff, _) | (OrdemCampos::Auto, OrdemCampos::Bff) => "bff",
        (OrdemCampos::Auto, OrdemCampos::Auto) => "auto",
    };
    format!("bwdif=mode={modo}:parity={ordem}:deint=all")
}

/// Nome do `.trf` na pasta de trabalho: o guardado no projeto, ou o que a prévia vai gerar.
pub(crate) fn nome_do_trf(efeitos: &[Tolerante<Efeito>], e: &EfEstabilizar, fonte: &FonteInfo) -> String {
    match &e.trf_sha256 {
        Some(sha) => format!("{sha}.trf"),
        None => {
            let tab = &fonte.quadros;
            let d = desentrelace(efeitos).map(|d| bwdif(d, dobra(Some(d), tab), fonte)).unwrap_or_default();
            let texto = format!("{}|{}|{}|{:?}|{:?}|{}|{d}", fonte.sha256, e.analise.de_pts, e.analise.ate_pts, fonte.matriz, fonte.faixa, fonte.pix_fmt);
            format!("vs{}.trf", &format!("{:x}", Sha256::digest(texto.as_bytes()))[..16])
        }
    }
}

/// Linha do ffmpeg (depois do binário) que analisa o movimento dos quadros `analise` do efeito, na cor interna e
/// depois do desentrelace do clipe, e grava `nome` na pasta de trabalho. Roda com `OMP_NUM_THREADS=1` (§5.3).
/// `com_busca` falso = lê desde o começo do arquivo (a leitura com busca não conferiu).
pub(crate) fn deteccao(efeitos: &[Tolerante<Efeito>], e: &EfEstabilizar, fonte: &FonteInfo, nome: &str, com_busca: bool) -> Result<Vec<String>> {
    let tab = &fonte.quadros;
    let (d0, d1) = analise(e, tab);
    if d1 < d0 || d1 >= tab.pts.len() {
        return Err(erro("estabilizar_fora_da_analise", &e.base.id));
    }
    let mut f = vec![match tab.pts.get(d1 + 1) {
        Some(fim) => format!("trim=start_pts={}:end_pts={fim}", tab.pts[d0]),
        None => format!("trim=start_pts={}", tab.pts[d0]),
    }];
    // busca que abre adiante daria um `.trf` mais curto, indexado do quadro errado, com código 0
    f.push(guarda_do_inicio(tab.pts[d0]));
    f.push(cor_da_fonte(efeitos, fonte).ok_or_else(|| erro("pixel_nao_suportado", &fonte.sha256))?);
    if let Some(d) = desentrelace(efeitos) {
        f.push(bwdif(d, dobra(Some(d), tab), fonte));
    }
    f.extend(["format=yuv420p".to_string(), format!("vidstabdetect=result={nome}:{DETECCAO}")]);
    let mut a = args::globais();
    if let Some(alvo) = tab.busca_em(fonte.inicio_s, d0).filter(|_| com_busca) {
        let fim = if d1 + 1 < tab.pts.len() { tab.tempo(d1 + 1) } else { tab.fim_s() } - fonte.inicio_s;
        a.extend(["-ss".to_string(), format!("{alvo:.6}"), "-t".to_string(), format!("{:.6}", fim - alvo + 1.0)]);
    }
    a.extend(["-i".to_string(), fonte.caminho.to_string_lossy().into_owned(), "-map".into(), "0:v:0".into(), "-vf".into(), f.join(","), "-an".into(), "-f".into(), "null".into(), "-".into()]);
    Ok(a)
}

/// Quadros que a análise do movimento do efeito tem de entregar (campos, quando o grafo dobra).
pub(crate) fn quadros_da_analise(efeitos: &[Tolerante<Efeito>], e: &EfEstabilizar, fonte: &TabelaQuadros) -> u64 {
    let (d0, d1) = analise(e, fonte);
    let n = (d1 + 1).saturating_sub(d0) as u64;
    if dobra(desentrelace(efeitos), fonte) { 2 * n } else { n }
}

/// `feito.estabilizacoes` (§10.2): os `estabilizar` ligados dos clipes ligados da principal, na ordem.
// CONTRATO: acréscimo. É o que `motor::proveniencia` grava.
pub fn estabilizacoes(p: &ProjetoEdicao) -> Vec<ProvEstabilizacao> {
    let mut v = Vec::new();
    for c in &p.principal {
        let base = match c {
            Tolerante::Conhecido(ClipePrincipal::Trecho(x)) => &x.base,
            Tolerante::Conhecido(ClipePrincipal::Congelar(x)) => &x.base,
            _ => continue,
        };
        if base.desligado {
            continue;
        }
        for e in efeitos::ligados(&base.efeitos, Familia::Origem) {
            if let Efeito::Estabilizar(e) = e {
                v.push(ProvEstabilizacao { efeito: e.base.id.clone(), trf_sha256: e.trf_sha256.clone(), analise: e.analise, zoom_pct: e.zoom_pct });
            }
        }
    }
    v
}

/// Põe na pasta de trabalho os `.trf` guardados no projeto (`vidstab/<sha256>.trf`) com o nome que o grafo cita
/// (`<sha256>.trf`), conferindo o conteúdo. Ausente: `arquivo_ausente`; com outro conteúdo: `leitura_divergente`.
// CONTRATO: acréscimo. O executor (miolo) ainda não chama: sem isso o bloco de um clipe com `trf_sha256` gravado
// não acha o arquivo.
pub fn trazer_trf(estabilizacoes: &[ProvEstabilizacao], pasta_projeto: &Path, trabalho: &Path) -> Result<()> {
    for e in estabilizacoes {
        let Some(sha) = &e.trf_sha256 else { continue };
        // o nome vira caminho: só SHA-256 em minúsculas
        if !sha_do_trf(sha) {
            return Err(erro("item_invalido", format!("efeito estabilizar {}: trf_sha256", e.efeito)));
        }
        let destino = trabalho.join(format!("{sha}.trf"));
        if destino.is_file() {
            continue;
        }
        let bytes = std::fs::read(pasta_projeto.join("vidstab").join(format!("{sha}.trf"))).map_err(|_| erro("arquivo_ausente", format!("vidstab/{sha}.trf")))?;
        if format!("{:x}", Sha256::digest(&bytes)) != *sha {
            return Err(erro("leitura_divergente", format!("vidstab/{sha}.trf")));
        }
        // inteiro ou nada: outro bloco pode estar abrindo o mesmo nome
        let tmp = trabalho.join(format!("{sha}.trf.{}.tmp", std::process::id()));
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &destino)?;
    }
    Ok(())
}

/// `ampliar` ainda sem zoom gravado: sem `.trf`, ou com `zoom_pct` zerado (borda trocada depois da análise).
pub(crate) fn zoom_a_calcular(e: &EfEstabilizar) -> bool {
    e.borda == BordaEstab::Ampliar && (e.trf_sha256.is_none() || e.zoom_pct <= 0.0)
}

/// `vidstabtransform` do efeito. Zoom: o gravado (`ampliar`), o calculado de novo (`zoom_a_calcular`: o mesmo em
/// todo bloco, sai do `.trf` inteiro) ou nenhum.
pub(crate) fn transformacao(e: &EfEstabilizar, nome: &str) -> Result<String> {
    // faixa do `zoom` do filtro e forma do nome do `.trf` (a mesma regra que a validação lê em `invalidos`)
    if let Some(campo) = campo_fora(&Efeito::Estabilizar(e.clone()), (0, 0)) {
        return Err(erro("item_invalido", format!("efeito estabilizar {}: {campo}", e.base.id)));
    }
    let zoom = match e.borda {
        BordaEstab::Ampliar if zoom_a_calcular(e) => "optzoom=1".to_string(),
        BordaEstab::Ampliar => format!("optzoom=0:zoom={}", e.zoom_pct),
        BordaEstab::Preta => "optzoom=0:zoom=0".to_string(),
    };
    Ok(format!("vidstabtransform=input={nome}:{zoom}:crop=black:smoothing={}:interpol=bilinear", e.suavizar.min(SUAVIZAR_MAX)))
}

/// Filtros que mexem só no quadro (lente, ajustes, nitidez), na ordem da cadeia. Valor neutro não vira filtro.
fn por_quadro(o: &Origem, fonte: &FonteInfo) -> Result<Vec<String>> {
    let mut f = Vec::new();
    if let Some(l) = o.lente {
        let ruim = || erro("item_invalido", format!("efeito lente {}", l.base.id));
        if campo_fora(&Efeito::Lente(l.clone()), fonte.exibido).is_some() {
            return Err(ruim());
        }
        let c = LensCoefficients { k1: l.k1 as f32, k2: l.k2 as f32, k3: 0.0 };
        if !c.is_identity() {
            let (k1, k2) = para_ffmpeg(c, fonte.exibido.0, fonte.exibido.1).ok_or_else(ruim)?;
            f.push(format!("lenscorrection=cx={}:cy={}:k1={k1}:k2={k2}:i=bilinear", l.cx, l.cy));
        }
    }
    if let Some(a) = o.ajustes {
        // faixas do `eq`
        if campo_fora(&Efeito::Ajustes(a.clone()), fonte.exibido).is_some() {
            return Err(erro("item_invalido", format!("efeito ajustes {}", a.base.id)));
        }
        if (a.brilho, a.contraste, a.gama, a.saturacao) != (0.0, 1.0, 1.0, 1.0) {
            f.push(format!("eq=brightness={}:contrast={}:gamma={}:saturation={}", a.brilho, a.contraste, a.gama, a.saturacao));
        }
    }
    if let Some(n) = o.nitidez {
        // faixa do `la` do `unsharp`
        if campo_fora(&Efeito::Nitidez(n.clone()), fonte.exibido).is_some() {
            return Err(erro("item_invalido", format!("efeito nitidez {}", n.base.id)));
        }
        if n.quantidade != 0.0 {
            f.push(format!("unsharp=lx=5:ly=5:la={}:cx=5:cy=5:ca=0", n.quantidade));
        }
    }
    Ok(f)
}

/// Cadeia de origem sobre `base` (quadros `leitura.ler`, já na cor interna e ocultados). Devolve o rótulo com só os
/// quadros que o mapa usa (`leitura.lidos`), com os pts da tabela do clipe. Pode pedir `Previa::Vidstab` e arquivos
/// auxiliares ao montador. Só é chamada quando `ativos` é verdadeiro.
pub fn aplicar(m: &mut dyn Montador, base: &str, efeitos: &[Tolerante<Efeito>], fonte: &FonteInfo, leitura: &Leitura) -> Result<String> {
    let o = origem(efeitos)?;
    let tab = &fonte.quadros;
    let dobra = dobra(o.des, tab);
    let tempo = o.des.is_some() || o.est.is_some() || o.media.is_some();
    let mut f = Vec::new();
    if tempo {
        let lidos = tab.pts.get(leitura.ler.0..leitura.ler.1).ok_or_else(|| erro("intervalo", "leitura de origem"))?;
        f.extend([format!("settb={}/{}", tab.tb.num, tab.tb.den), guarda(lidos)]);
    }
    if let Some(d) = o.des {
        f.push(bwdif(d, dobra, fonte));
    }
    if let Some(e) = o.est {
        let nome = nome_do_trf(efeitos, e, fonte);
        if e.trf_sha256.is_none() {
            m.previa(Previa::Vidstab { efeito: e.base.id.clone(), args: deteccao(efeitos, e, fonte, &nome, true)?, trf: nome.clone() });
        }
        f.extend(["format=yuv420p".to_string(), transformacao(e, &nome)?]);
    }
    f.extend(por_quadro(&o, fonte)?);
    if tempo {
        let escala = if dobra { 2 } else { 1 };
        let (la, lb) = (leitura.ler.0 * escala, leitura.ler.1 * escala);
        let (oa, ob) = na_saida(dobra, leitura.campos, leitura.faixa);
        let (_, depois) = margem_da_media(o.media, leitura.centrada);
        if let Some(media) = o.media {
            // média centrada perto do fim da leitura: o último quadro repete, como o `tmix` repete o primeiro
            let falta = (ob + depois).saturating_sub(lb);
            if falta > 0 {
                f.push(format!("tpad=stop_mode=clone:stop={falta}"));
            }
            f.push(format!("tmix=frames={}", media.quadros));
        }
        // o quadro `oa` da saída é o de índice `oa − la`; na média centrada, o que fecha a janela dele
        let k0 = oa.checked_sub(la).ok_or_else(|| erro("intervalo", "leitura de origem"))? + depois;
        let n = leitura.lidos as usize;
        let tab_clipe = tabela(efeitos, tab);
        let da_tabela: &TabelaQuadros = if leitura.campos { &tab_clipe } else { tab };
        let mut pts = da_tabela.pts.get(leitura.faixa.0..leitura.faixa.1).filter(|p| p.len() == n && n > 0).ok_or_else(|| erro("intervalo", "leitura de origem"))?.to_vec();
        // o `setpts` avalia a árvore também no fim do fluxo (N = n) e é desse pts que sai o clone do `tpad` que o mapa
        // espera DEPOIS do último quadro: o pts do quadro seguinte (fim da mídia: mais um passo), como na cadeia sem
        // efeito. Com o pts do último repetido ali, o `fps` do 7.0.2 perde o último quadro do clipe
        let depois_do_ultimo = da_tabela.pts.get(leitura.faixa.1).copied().unwrap_or(pts[n - 1].saturating_add(da_tabela.passo_mediano.max(1)));
        pts.push(depois_do_ultimo);
        f.extend([
            format!("trim=start_frame={k0}:end_frame={}", k0 + n),
            format!("settb={}/{}", tab.tb.num, tab.tb.den),
            filtro("setpts", None, &[("expr", expr::arvore_pts(&pts))]),
        ]);
    }
    if f.is_empty() {
        return Ok(base.to_string());
    }
    let saida = m.grafo().rotulo("o");
    m.grafo().cadeia(&[base], &f, &[&saida]);
    Ok(saida)
}

#[cfg(test)]
pub(crate) mod testes {
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    use serde_json::{json, Value};

    use super::*;
    use crate::edicao::curva::fixtures;
    use crate::edicao::ffmpeg::exec::{self, Comando};
    use crate::edicao::ffmpeg::sonda::Capacidades;
    use crate::edicao::motor::plano::Plano;
    use crate::edicao::motor::Alvo;
    use crate::edicao::quadros::apoio::{self, Medida};
    use crate::edicao::testes::aceite::{alvo_final, ancoras_da_marca, efeito, midia, ocultar, projeto, rastreio, trecho, Mundo};
    use crate::edicao::testes::executor::{self, Opcoes, Rodado};
    use crate::edicao::testes::fontes::{self, Fonte};
    use crate::edicao::testes::medir::{self, QuadroRgb};

    // ------------------------------------------------------------------ apoio (também do `origem.rs`)

    pub(crate) fn efs(v: Value) -> Vec<Tolerante<Efeito>> {
        serde_json::from_value(v).expect("efeitos")
    }

    pub(crate) fn desentrelacar(modo: &str) -> Value {
        efeito("desentrelacar", "d1", json!({ "modo": modo, "ordem": "auto" }))
    }

    pub(crate) fn media(quadros: u32) -> Value {
        efeito("media_temporal", "m1", json!({ "quadros": quadros }))
    }

    pub(crate) fn ajustes(brilho: f64, contraste: f64) -> Value {
        efeito("ajustes", "a1", json!({ "brilho": brilho, "contraste": contraste, "gama": 1.0, "saturacao": 1.0 }))
    }

    /// `estabilizar` com a análise nos quadros `[d0, d1]` da tabela.
    pub(crate) fn estabilizar(tab: &TabelaQuadros, d0: usize, d1: usize, borda: &str, zoom_pct: f64, trf: Option<&str>) -> Value {
        let analise = json!({ "de_s": tab.tempo(d0), "ate_s": tab.tempo(d1), "de_pts": tab.pts[d0], "ate_pts": tab.pts[d1] });
        efeito("estabilizar", "s1", json!({ "suavizar": 15, "borda": borda, "analise": analise, "zoom_pct": zoom_pct, "trf_sha256": trf }))
    }

    pub(crate) fn sha256_do_arquivo(p: &Path) -> String {
        format!("{:x}", Sha256::digest(std::fs::read(p).expect("arquivo")))
    }

    pub(crate) fn comando(cap: &Capacidades, pasta: &Path, nome: &str, a: Vec<String>) -> Comando {
        Comando { nome: nome.into(), programa: cap.binario.clone(), args: a, pasta: pasta.to_path_buf(), ambiente: Vec::new(), sem_ambiente: Vec::new(), stderr: pasta.join(format!("{nome}.stderr")) }
    }

    /// Vídeo de teste de uma linha `lavfi`.
    pub(crate) fn gerar(cap: &Capacidades, pasta: &Path, nome: &str, lavfi: &str, codificar: &[&str]) -> PathBuf {
        let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-f", "lavfi", "-i", lavfi].map(String::from).into();
        a.extend(codificar.iter().map(|s| s.to_string()));
        a.push(nome.to_string());
        let cmd = comando(cap, pasta, "gerar", a);
        let fim = exec::rodar(&cmd, &AtomicBool::new(false)).expect("ffmpeg");
        exec::exigir_sucesso(&cmd, &fim).expect("fonte de teste");
        pasta.join(nome)
    }

    /// `FonteInfo` de um arquivo, com a tabela do decodificador.
    pub(crate) fn fonte_de(cap: &Capacidades, video: &Path) -> FonteInfo {
        let tab = TabelaQuadros::do_decodificador(cap, video, &AtomicBool::new(false), &mut |_| {}).expect("tabela do decodificador");
        let mut f = crate::edicao::quadros::fonte_info(cap, video, &"0".repeat(64), Arc::new(tab), None).expect("fonte_info");
        f.sha256 = sha256_do_arquivo(video);
        f
    }

    /// Lado e posição (px) do quadrado magenta da cena tremida, na cena parada.
    pub(crate) const ALVO_TREMIDO: [u32; 4] = [300, 150, 40, 40];
    pub(crate) const TREMIDO: (u32, u32, usize) = (640, 360, 120);

    /// Cena parada (textura cinza + quadrado magenta) vista por uma câmera que treme até 11 px: 640×360, 120 quadros, 30 qps.
    pub(crate) fn tremido(cap: &Capacidades, pasta: &Path) -> FonteInfo {
        let [x, y, w, h] = ALVO_TREMIDO;
        // a cena tem 800×520 e o recorte de 640×360 anda em volta de (80, 80)
        let lavfi = format!(
            "testsrc2=s=800x520:r=30,trim=end_frame=1,hue=s=0,drawbox=x={}:y={}:w={w}:h={h}:color=0xff00ff:t=fill,loop=loop=-1:size=1,trim=end_frame={},setpts=N/30/TB,\
             crop=640:360:x='80+7*sin(n*1.3)+4*sin(n*2.9)':y='80+6*cos(n*1.7)+3*sin(n*3.7)'",
            x + 80,
            y + 80,
            TREMIDO.2
        );
        let v = gerar(cap, pasta, "tremido.mp4", &lavfi, &["-c:v", "libx264", "-preset", "veryfast", "-crf", "12", "-pix_fmt", "yuv420p", "-bf", "0", "-g", "30", "-video_track_timescale", "15360"]);
        fonte_de(cap, &v)
    }

    /// Roda as prévias `Vidstab` do plano na pasta (como o executor: `OMP_NUM_THREADS=1`) e devolve (efeito, sha256 do `.trf`, arquivo).
    pub(crate) fn rodar_previas(cap: &Capacidades, plano: &Plano, pasta: &Path) -> Vec<(String, String, PathBuf)> {
        std::fs::create_dir_all(pasta).unwrap();
        let mut v = Vec::new();
        for (i, p) in plano.previas.iter().enumerate() {
            let Previa::Vidstab { efeito, args, trf } = p else { continue };
            let mut cmd = comando(cap, pasta, &format!("previa{i}"), args.clone());
            cmd.ambiente.push(("OMP_NUM_THREADS".into(), "1".into()));
            let fim = exec::rodar(&cmd, &AtomicBool::new(false)).expect("ffmpeg");
            exec::exigir_sucesso(&cmd, &fim).unwrap_or_else(|e| panic!("prévia do vidstab ({}): {e}", cap.versao));
            v.push((efeito.clone(), sha256_do_arquivo(&pasta.join(trf)), pasta.join(trf)));
        }
        v
    }

    /// Prévias, blocos sem perdas e os quadros de todos os blocos. `trfs` = `.trf` guardados que o grafo cita pelo
    /// nome `<sha256>.trf` (quem os põe na pasta de trabalho na exportação é o executor).
    pub(crate) fn renderizar(cap: &Arc<Capacidades>, plano: &Plano, nome: &str, trfs: &[&Path]) -> (Rodado, Vec<QuadroRgb>) {
        let pasta = fontes::pasta_temporaria(nome);
        por_na_pasta(&pasta, trfs);
        rodar_previas(cap, plano, &pasta);
        let r = executor::rodar(cap, plano, &pasta, &Opcoes { sem_perdas: true, ..Opcoes::default() }).unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
        let mut q = Vec::new();
        for b in &r.blocos {
            q.extend(medir::quadros(cap, b).unwrap_or_else(|e| panic!("{nome}: {e}")));
        }
        assert_eq!(q.len() as u64, plano.total_q, "{nome} ({}): quadros × slots", cap.versao);
        (r, q)
    }

    /// Os `.trf` como estariam guardados no projeto, levados à pasta de trabalho por `trazer_trf`.
    fn por_na_pasta(pasta: &Path, trfs: &[&Path]) {
        let projeto = fontes::pasta_temporaria("projeto-trf");
        std::fs::create_dir_all(projeto.join("vidstab")).unwrap();
        let mut lista = Vec::new();
        for t in trfs {
            let sha = sha256_do_arquivo(t);
            std::fs::copy(t, projeto.join("vidstab").join(format!("{sha}.trf"))).unwrap();
            let analise = serde_json::from_value(json!({ "de_s": 0.0, "ate_s": 0.0, "de_pts": 0, "ate_pts": 0 })).unwrap();
            lista.push(ProvEstabilizacao { efeito: String::new(), trf_sha256: Some(sha), analise, zoom_pct: 0.0 });
        }
        trazer_trf(&lista, &projeto, pasta).expect("`.trf` guardado");
    }

    /// Pixels com cor (vermelho do alvo), mesmo diluído pela média de até 8 quadros: os canais da cena são cinza.
    pub(crate) fn sobra(q: &QuadroRgb) -> u64 {
        q.px.chunks_exact(3).filter(|p| p[0] as i32 - p[1] as i32 > 24 && p[0] as i32 - p[2] as i32 > 24).count() as u64
    }

    /// Caixa [x0, y0, x1, y1] (px, exclusivo no fim) e centro dos pixels que `cor` aceita; None = nenhum.
    pub(crate) fn caixa_da_cor(q: &QuadroRgb, cor: impl Fn(&[u8]) -> bool) -> Option<([u32; 4], (f64, f64))> {
        let (mut x0, mut y0, mut x1, mut y1, mut sx, mut sy, mut n) = (u32::MAX, u32::MAX, 0, 0, 0.0, 0.0, 0u64);
        for (i, p) in q.px.chunks_exact(3).enumerate() {
            if cor(p) {
                let (x, y) = (i as u32 % q.w, i as u32 / q.w);
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
                (sx, sy, n) = (sx + x as f64 + 0.5, sy + y as f64 + 0.5, n + 1);
            }
        }
        (n > 0).then(|| ([x0, y0, x1, y1], (sx / n as f64, sy / n as f64)))
    }

    pub(crate) fn vermelho(p: &[u8]) -> bool {
        p[0] as i32 - p[1] as i32 > 90 && p[0] as i32 - p[2] as i32 > 90
    }

    pub(crate) fn magenta(p: &[u8]) -> bool {
        p[0] > 170 && p[2] > 170 && p[1] < 90
    }

    pub(crate) fn verde(p: &[u8]) -> bool {
        p[1] > 170 && p[0] < 90 && p[2] < 90
    }

    /// Cinza médio (0..255) do miolo da faixa `s` (0..6) da fonte da matriz, longe do alvo.
    fn cinza_da_faixa(q: &QuadroRgb, s: u32) -> f64 {
        let (a, b) = (s * q.w / 6 + q.w / 24, (s + 1) * q.w / 6 - q.w / 24);
        let c = medir::cor_media(q, [a, q.h / 20, b - a, q.h * 7 / 20]);
        (c[0] + c[1] + c[2]) / 3.0
    }

    /// Luma (16..235) que a fonte da matriz desenha na faixa `s` do quadro de número `j`.
    fn luma_da_faixa(j: u32, s: u32) -> f64 {
        let digitos = [0, 9, j / 1000 % 10, j / 100 % 10, j / 10 % 10, j % 10];
        fontes::nivel_do_digito(digitos[s as usize], false) as f64
    }

    /// Cinza (0..255) que o medidor dá a uma luma, pela reta entre os dois dígitos medidos em volta dela
    /// (`sem_efeito` = `cinza_por_digito`: a conversão para RGB do medidor não é a nominal).
    fn cinza(sem_efeito: &[f64; 10], luma: f64) -> f64 {
        let y = |d: usize| fontes::nivel_do_digito(d as u32, false) as f64;
        let d = (1..9).find(|&d| luma < y(d)).unwrap_or(9);
        (sem_efeito[d - 1] + (luma - y(d - 1)) * (sem_efeito[d] - sem_efeito[d - 1]) / (y(d) - y(d - 1))).clamp(0.0, 255.0)
    }

    /// Quadro (índice da tabela do clipe) de cada slot do clipe `n`, pela proveniência.
    fn quadro_do_slot(plano: &Plano, n: usize) -> Vec<usize> {
        plano.proveniencia.roteiro[n].origem.as_ref().expect("clipe com fonte").quadro_do_slot.iter().map(|&j| j as usize).collect()
    }

    fn janela(altura: u32, de: u64, ate: u64) -> Alvo {
        Alvo::Previa { altura, de_q: de, ate_q: ate }
    }

    // ------------------------------------------------------------------ sem ffmpeg

    fn tab(tb: (i64, i64), pts: Vec<i64>) -> TabelaQuadros {
        apoio::tabela(tb, pts)
    }

    #[test]
    fn recusa_so_em_clipe_sem_fonte() {
        for e in efs(json!([desentrelacar("campo"), media(4), ajustes(0.1, 1.0), efeito("nitidez", "n1", json!({ "quantidade": 0.5 })),
            efeito("lente", "l1", json!({ "k1": -0.2, "k2": 0.0, "cx": 0.5, "cy": 0.5, "perfil": "" }))]))
        {
            let Tolerante::Conhecido(e) = e else { panic!() };
            assert_eq!(recusa(&e, true), None);
            assert_eq!(recusa(&e, false), Some(efeitos::tipo(&e)));
        }
        // efeito de outra família não é com esta regra
        let girar: Efeito = serde_json::from_value(efeito("girar", "g", json!({ "graus": 90, "espelhar_h": false, "espelhar_v": false }))).unwrap();
        assert_eq!(recusa(&girar, false), None);
        assert!(ativos(&efs(json!([media(4)])), true) && !ativos(&efs(json!([media(4)])), false) && !ativos(&[], true));
    }

    /// Tabela de campos: o pts do quadro e o meio até o seguinte, na base da fonte; fim igual ao da fonte.
    #[test]
    fn tabela_de_campos() {
        let campo = efs(json!([desentrelacar("campo")]));
        // NTSC em 1/30000: passo 1001 (ímpar)
        let f = tab((1, 30000), apoio::pts_regulares(5, (1001, 30000), (1, 30000), 900));
        let t = tabela(&campo, &f);
        assert_eq!(t.pts, vec![900, 1401, 1901, 2402, 2902, 3403, 3903, 4404, 4904, 5405]);
        assert_eq!((t.tb, t.passo_mediano, t.passo), (f.tb, 500, (500, 501)));
        assert_eq!(t.fim_s(), f.fim_s());
        assert_eq!(t.chave, vec![true, false, false, false, false, false, false, false, false, false]);
        let mut g = f.clone();
        g.chave = vec![true, false, true, false, true];
        assert_eq!(tabela(&campo, &g).chave, vec![true, false, false, false, true, false, false, false, true, false], "chave só no primeiro campo do quadro-chave");
        // o pts do quadro acha o primeiro campo dele; o tempo do quadro também
        for j in 0..5 {
            assert_eq!(t.indice_do_pts(f.pts[j]), Some(2 * j));
            assert_eq!(t.do_par(f.tempo(j), None), 2 * j);
        }
        assert_eq!(t.fim_do_par(f.fim_s(), None), 10);
        // passo variável (MKV de 1 ms: 33, 34, 33) e negativo
        let f = tab((1, 1000), vec![-40, -7, 27, 60]);
        assert_eq!(tabela(&campo, &f).pts, vec![-40, -23, -7, 10, 27, 44, 60, 77]);
        // quadro, desligado ou sem efeito: a da fonte, emprestada
        for e in [json!([desentrelacar("quadro")]), json!([]), json!([{ "tipo": "desentrelacar", "id": "d", "ligado": false, "nome": "", "ativo": null, "modo": "campo", "ordem": "auto" }])] {
            assert!(matches!(tabela(&efs(e), &f), Cow::Borrowed(_)));
        }
        // um tique por quadro (AVI 25): não há meio; a tabela fica a da fonte e a regra acusa
        let avi = tab((1, 25), (0..10).collect());
        assert!(matches!(tabela(&campo, &avi), Cow::Borrowed(_)));
        let Tolerante::Conhecido(e) = &campo[0] else { panic!() };
        assert_eq!((recusa_na_fonte(e, &avi), recusa_na_fonte(e, &f)), (Some("desentrelacar_campo"), None));
        // tabela vazia
        assert!(tabela(&campo, &tab((1, 1000), vec![])).pts.is_empty());
    }

    /// Espelho `modelo/realce.ts`: a tabela compacta do clipe para as mesmas entradas.
    #[test]
    fn fixture_realce_tabela() {
        // quadros-chave no meio: no espelho o índice de cada um dobra
        let mut com_chaves = tab((1, 15360), apoio::pts_regulares(8, (1, 30), (1, 15360), 1024));
        com_chaves.chave = vec![true, false, false, true, false, false, true, false];
        let tabelas = [
            ("ntsc 1/30000", tab((1, 30000), apoio::pts_regulares(7, (1001, 30000), (1, 30000), 0))),
            ("pal 1/90000 com início", tab((1, 90000), apoio::pts_regulares(6, (1, 25), (1, 90000), 126000))),
            ("mkv 1 ms", tab((1, 1000), vec![0, 33, 67, 100, 133, 167])),
            ("negativo", tab((1, 1000), vec![-40, -7, 27, 60])),
            ("um quadro", tab((1, 15360), vec![512])),
            ("avi um tique por quadro", tab((1, 25), (0..6).collect())),
            ("pts repetido", tab((1, 1000), vec![0, 40, 40, 80])),
            ("quadros-chave no meio", com_chaves),
        ];
        let listas = [
            ("campo", json!([desentrelacar("campo")])),
            ("quadro", json!([desentrelacar("quadro")])),
            ("sem efeito", json!([])),
            ("campo desligado", json!([{ "tipo": "desentrelacar", "id": "d", "ligado": false, "nome": "", "ativo": null, "modo": "campo", "ordem": "auto" }])),
            ("desconhecido e campo", json!([{ "tipo": "do_futuro", "id": "x" }, media(4), desentrelacar("campo")])),
        ];
        let mut casos = Vec::new();
        for (nt, t) in &tabelas {
            for (nl, l) in &listas {
                let saida = tabela(&efs(l.clone()), t).compacta().expect("compacta");
                casos.push(json!({ "nome": format!("{nt}, {nl}"), "entrada": { "tab": t.compacta().unwrap(), "efeitos": l }, "saida": saida }));
            }
        }
        fixtures::conferir("realce-tabela.json", Value::Array(casos));
    }

    fn ler(efeitos: Value, fonte: &TabelaQuadros, faixa: (usize, usize), clipe: (usize, usize)) -> Result<Leitura> {
        let e = efs(efeitos);
        leitura(&e, fonte, &tabela(&e, fonte), faixa, clipe)
    }

    /// `feito.estabilizacoes`: só o `estabilizar` ligado de clipe ligado, com o que o efeito guarda.
    #[test]
    fn estabilizacoes_do_projeto() {
        let t = tab((1, 15360), apoio::pts_regulares(120, (1, 30), (1, 15360), 0));
        let sha = "c".repeat(64);
        let mut desligado = estabilizar(&t, 5, 60, "preta", 0.0, None);
        desligado["ligado"] = json!(false);
        let mut em_clipe_desligado = trecho("c3", "m1", &t, 0, 30, 1.0, json!([estabilizar(&t, 0, 29, "preta", 0.0, None)]));
        em_clipe_desligado["desligado"] = json!(true);
        let p = projeto(
            640,
            360,
            (30, 1),
            vec![midia("m1", "video", json!([]))],
            vec![
                trecho("c1", "m1", &t, 20, 80, 1.0, json!([ajustes(0.1, 1.0), estabilizar(&t, 10, 109, "ampliar", 4.5, Some(&sha))])),
                trecho("c2", "m1", &t, 20, 50, 1.0, json!([desligado])),
                em_clipe_desligado,
                trecho("c4", "m1", &t, 30, 40, 1.0, json!([estabilizar(&t, 30, 39, "preta", 0.0, None)])),
            ],
            json!({}),
        );
        let e = estabilizacoes(&p);
        assert_eq!(e.len(), 2);
        assert_eq!((e[0].efeito.as_str(), e[0].trf_sha256.as_deref(), e[0].zoom_pct, e[0].analise.de_pts, e[0].analise.ate_pts), ("s1", Some(sha.as_str()), 4.5, t.pts[10], t.pts[109]));
        assert_eq!((e[1].trf_sha256.as_deref(), e[1].zoom_pct, e[1].analise.de_pts, e[1].analise.ate_pts), (None, 0.0, t.pts[30], t.pts[39]));
    }

    /// O `.trf` guardado chega à pasta de trabalho com o nome do grafo, conferido pelo conteúdo.
    #[test]
    fn trf_guardado_vai_para_a_pasta_de_trabalho() {
        let raiz = fontes::pasta_temporaria("trazer-trf");
        let (projeto, trabalho) = (raiz.join("projeto"), raiz.join("trabalho"));
        std::fs::create_dir_all(projeto.join("vidstab")).unwrap();
        std::fs::create_dir_all(&trabalho).unwrap();
        let conteudo = b"VID.STAB 1\n#  accuracy = 15\nFrame 1 (List 0 [])\n";
        let sha = format!("{:x}", Sha256::digest(conteudo));
        std::fs::write(projeto.join("vidstab").join(format!("{sha}.trf")), conteudo).unwrap();
        let est = |trf: Option<&str>| ProvEstabilizacao { efeito: "s1".into(), trf_sha256: trf.map(String::from), analise: serde_json::from_value(json!({ "de_s": 0.0, "ate_s": 1.0, "de_pts": 0, "ate_pts": 30 })).unwrap(), zoom_pct: 0.0 };
        // sem `.trf` gravado não há o que trazer
        trazer_trf(&[est(None)], &projeto, &trabalho).unwrap();
        assert_eq!(std::fs::read_dir(&trabalho).unwrap().count(), 0);
        trazer_trf(&[est(Some(&sha)), est(Some(&sha))], &projeto, &trabalho).unwrap();
        assert_eq!(std::fs::read(trabalho.join(format!("{sha}.trf"))).unwrap(), conteudo);
        assert_eq!(std::fs::read_dir(&trabalho).unwrap().count(), 1, "nenhum temporário sobra");
        // citado e ausente; presente com outro conteúdo; nome que não é SHA-256
        let outro = "d".repeat(64);
        let e = trazer_trf(&[est(Some(&outro))], &projeto, &trabalho).unwrap_err().to_string();
        assert!(e.contains(&format!("arquivo_ausente: vidstab/{outro}.trf")), "{e}");
        std::fs::write(projeto.join("vidstab").join(format!("{outro}.trf")), conteudo).unwrap();
        let e = trazer_trf(&[est(Some(&outro))], &projeto, &trabalho).unwrap_err().to_string();
        assert!(e.contains("leitura_divergente") && !trabalho.join(format!("{outro}.trf")).exists(), "{e}");
        let maiusculas = "E".repeat(64);
        for ruim in ["../fora", "ABCDEF", maiusculas.as_str()] {
            let e = trazer_trf(&[est(Some(ruim))], &projeto, &trabalho).unwrap_err().to_string();
            assert!(e.contains("item_invalido"), "{ruim}: {e}");
        }
    }

    fn est_de(v: Value) -> EfEstabilizar {
        match serde_json::from_value(v).expect("efeito") {
            Efeito::Estabilizar(e) => e,
            _ => panic!("estabilizar"),
        }
    }

    /// A guarda confere o pts de cada quadro lido pelo índice; o zoom fora da faixa do filtro é recusado.
    #[test]
    fn guarda_e_faixa_do_zoom() {
        let g = guarda(&[100, 612, 1124, 1636]);
        assert!(g.starts_with("select=expr=") && g.contains("gte(n") && g.contains("100+n*512") && !g.contains('N'), "{g}");
        // passo irregular: árvore sobre `n`, um quadro por folha
        let g = guarda(&[0, 33, 67, 100, 133, 167]);
        assert!(g.contains("if(lt(n") && !g.contains('N'), "{g}");
        // a análise vai na linha de comando: guarda curta, do tamanho que for o trecho
        let irregular = tab((1, 1000), (0..20_000).map(|k| k * 100 / 3).collect());
        let longa = deteccao(&[], &est_de(estabilizar(&irregular, 5, 19_990, "preta", 0.0, None)), &apoio::fonte(irregular.clone(), (640, 360)), "x.trf", true).unwrap();
        let vf = &longa[longa.iter().position(|a| a == "-vf").unwrap() + 1];
        assert!(vf.contains(&format!("select=expr=eq(start_pts\\,{})", irregular.pts[5])) && vf.len() < 600, "{} caracteres: {vf}", vf.len());
        let t = tab((1, 15360), apoio::pts_regulares(20, (1, 30), (1, 15360), 0));
        let sha = "a".repeat(64);
        let filtro_de = |borda: &str, zoom: f64| transformacao(&est_de(estabilizar(&t, 0, 9, borda, zoom, Some(&sha))), "x.trf");
        assert!(filtro_de("ampliar", 12.5).unwrap().contains(":optzoom=0:zoom=12.5:"));
        for zoom in [100.5, -100.5] {
            let e = filtro_de("ampliar", zoom).unwrap_err().to_string();
            assert!(e.contains("item_invalido") && e.contains("zoom_pct"), "{zoom}: {e}");
        }
        // o nome do `.trf` gravado vai para o grafo: só SHA-256 em minúsculas, senão nem compila
        let (maiusculo, curto) = ("A".repeat(64), "a".repeat(63));
        for ruim in ["../x", maiusculo.as_str(), "x.trf:optzoom=1", curto.as_str(), ""] {
            let e = transformacao(&est_de(estabilizar(&t, 0, 9, "preta", 0.0, Some(ruim))), "x.trf").unwrap_err().to_string();
            assert!(e.contains("item_invalido") && e.contains("trf_sha256"), "{ruim}: {e}");
        }
        assert_eq!(quadros_da_analise(&[], &est_de(estabilizar(&t, 3, 12, "preta", 0.0, None)), &t), 10);
        assert_eq!(quadros_da_analise(&efs(json!([desentrelacar("campo")])), &est_de(estabilizar(&t, 3, 12, "preta", 0.0, None)), &t), 20);
    }

    /// O que cada efeito lê a mais (§5.3, "Janela"), e o que não pode variar com a janela.
    #[test]
    fn leitura_por_efeito() {
        let f = tab((1, 30000), apoio::pts_regulares(100, (1001, 30000), (1, 30000), 0));
        // sem efeito de origem e com efeito só de quadro: a faixa pedida, como na Fase 0
        for e in [json!([]), json!([ajustes(0.1, 1.2)])] {
            let l = ler(e, &f, (40, 50), (30, 90)).unwrap();
            assert_eq!((l.ler, l.clipe, l.lidos, l.faixa, l.campos, l.centrada), ((40, 50), (30, 90), 10, (40, 50), false, false));
        }
        // média de 4: 3 antes; o clipe inteiro lê desde 3 antes do primeiro (a máscara cobre todos)
        let l = ler(json!([media(4)]), &f, (40, 50), (30, 90)).unwrap();
        assert_eq!((l.ler, l.clipe, l.lidos, l.centrada), ((37, 50), (27, 90), 10, false));
        assert_eq!(ler(json!([media(4)]), &f, (1, 5), (1, 90)).unwrap().ler, (0, 5));
        // congelado: centrada (4: 1 antes e 2 depois; 5: 2 e 2), aparada nas pontas da mídia
        let l = ler(json!([media(4)]), &f, (40, 41), (40, 41)).unwrap();
        assert_eq!((l.ler, l.clipe, l.lidos, l.centrada), ((39, 43), (39, 43), 1, true));
        assert_eq!(ler(json!([media(5)]), &f, (40, 41), (40, 41)).unwrap().ler, (38, 43));
        assert_eq!(ler(json!([media(5)]), &f, (99, 100), (99, 100)).unwrap().ler, (97, 100));
        assert_eq!(ler(json!([media(5)]), &f, (0, 1), (0, 1)).unwrap().ler, (0, 3));
        // desentrelace por quadro: 2 antes e 1 depois
        let l = ler(json!([desentrelacar("quadro")]), &f, (40, 50), (30, 90)).unwrap();
        assert_eq!((l.ler, l.clipe, l.lidos, l.campos), ((38, 51), (28, 91), 10, false));
        assert_eq!(ler(json!([desentrelacar("quadro")]), &f, (0, 100), (0, 100)).unwrap().ler, (0, 100));
        // por campo: a faixa vem em campos; os quadros que os contêm, com os vizinhos
        let l = ler(json!([desentrelacar("campo")]), &f, (81, 100), (60, 180)).unwrap();
        assert_eq!((l.ler, l.clipe, l.lidos, l.campos), ((38, 51), (28, 91), 19, true));
        // por campo com média de 4 campos: 3 campos antes do 81 = quadro 39, menos 2
        assert_eq!(ler(json!([desentrelacar("campo"), media(4)]), &f, (81, 100), (60, 180)).unwrap().ler, (37, 51));
        // congelado por campo: o primeiro campo do quadro
        // trecho de um campo só: o quadro que o contém, com os vizinhos
        let l = ler(json!([desentrelacar("campo")]), &f, (41, 42), (41, 42)).unwrap();
        assert_eq!((l.ler, l.lidos, l.campos), ((18, 22), 1, true));
        let e = efs(json!([desentrelacar("campo")]));
        let l = leitura(&e, &f, &f, (40, 41), (40, 41)).unwrap();
        assert_eq!((l.ler, l.lidos, l.campos), ((38, 42), 1, false));
        // trecho por campo em base de um tique por quadro: recusado
        let avi = tab((1, 25), (0..50).collect());
        let erro = ler(json!([desentrelacar("campo")]), &avi, (10, 20), (10, 20)).unwrap_err().to_string();
        assert!(erro.contains("desentrelacar d1 por campo"), "{erro}");
        assert_eq!(ler(json!([desentrelacar("campo")]), &avi, (10, 11), (10, 11)).unwrap().ler, (8, 12));
        // estabilizar: sempre desde o primeiro quadro analisado, nunca além do último; fora da análise é erro
        let est = |d0, d1| estabilizar(&f, d0, d1, "preta", 0.0, None);
        let l = ler(json!([est(30, 89)]), &f, (60, 70), (40, 80)).unwrap();
        assert_eq!((l.ler, l.clipe, l.lidos), ((30, 70), (30, 80), 10));
        assert_eq!(ler(json!([est(30, 89), media(4), desentrelacar("quadro")]), &f, (88, 90), (30, 90)).unwrap().ler, (30, 90));
        for clipe in [(29, 80), (40, 91)] {
            let erro = ler(json!([est(30, 89)]), &f, (60, 70), clipe).unwrap_err().to_string();
            assert!(erro.starts_with("Validation error: estabilizar_fora_da_analise") || erro.contains("estabilizar_fora_da_analise: s1"), "{erro}");
        }
        // efeito repetido, `ativo` parcial e média fora de 2..15: nunca ignorados
        for (e, pedaco) in [
            (json!([media(4), media(4)]), "repetido"),
            (json!([{ "tipo": "ajustes", "id": "a", "ligado": true, "nome": "", "ativo": { "de": 1.0, "ate": 2.0 }, "brilho": 0.0, "contraste": 1.0, "gama": 1.0, "saturacao": 1.0 }]), "ativo"),
            (json!([media(16)]), "quadros"),
        ] {
            let erro = ler(e, &f, (40, 50), (30, 90)).unwrap_err().to_string();
            assert!(erro.contains("item_invalido") && erro.contains(pedaco), "{erro}");
        }
    }

    // ------------------------------------------------------------------ com ffmpeg

    /// [A2] E1 completo, com `eq`: trecho com `ocultar` sólido por quadro lido e ajustes, a `v`× na taxa `fps`.
    fn caso_e1(cap: &Arc<Capacidades>, fonte: Fonte, v: f64, fps: (i64, i64), jan: Option<(u64, u64)>, sem_ajustes: &[f64; 10]) -> std::result::Result<u64, String> {
        let m = apoio::medida(cap, fonte);
        let (j_in, j_out, brilho, contraste) = (40, 130, 0.05, 1.03);
        let rotulo = format!("{fonte:?} {v}× em {}/{} janela {jan:?} ({})", fps.0, fps.1, cap.versao);
        let p = projeto(
            320,
            180,
            fps,
            vec![midia("m1", "video", json!([ocultar("o1", "r1", "solido", json!({}))]))],
            vec![trecho("c1", "m1", &m.info.quadros, j_in, j_out, v, json!([ajustes(brilho, contraste)]))],
            json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras_da_marca(fonte, &m, j_in, j_out))] }),
        );
        let alvo = jan.map(|(de, ate)| janela(180, de, ate)).unwrap_or_else(|| alvo_final("cpu"));
        let plano = Mundo::novo(cap).com("m1", &m.info).compilar(&p, &alvo).map_err(|e| format!("{rotulo}: {e}"))?;
        if !plano.blocos[0].grafo.contains(&format!("eq=brightness={brilho}:contrast={contraste}:gamma=1:saturation=1")) {
            return Err(format!("{rotulo}: sem o eq no grafo"));
        }
        let (r, quadros) = renderizar(cap, &plano, "e1eq", &[]);
        let todos = quadro_do_slot(&plano, 0);
        let slots = jan.map(|(de, ate)| todos[de as usize..ate as usize].to_vec()).unwrap_or(todos);
        let esperado: Vec<u32> = slots.iter().map(|&j| m.numeros[j]).collect();
        let numeros = medir::quadros_de_origem(cap, &r.blocos[0]).map_err(|e| format!("{rotulo}: {e}"))?;
        if numeros != esperado {
            let q = numeros.iter().zip(&esperado).position(|(a, b)| a != b).unwrap_or(numeros.len().min(esperado.len()));
            return Err(format!("{rotulo}: {} slots × {} previstos; primeiro diferente {q}: {:?} × {:?}", numeros.len(), esperado.len(), numeros.get(q), esperado.get(q)));
        }
        if let Some(s) = quadros.iter().map(crate::edicao::testes::aceite::vermelho).max().filter(|&s| s != 0) {
            return Err(format!("{rotulo}: {s} px vermelhos de sobra"));
        }
        // os valores do `eq` chegaram ao pixel: a conta inteira do filtro sobre a luma que o render sem ajustes dá
        let (c, b) = ((contraste * 4096.0) as i64, ((100.0 * brilho + 100.0) as i64 * 511) / 200 - 128 - (contraste * 4096.0) as i64 / 32);
        for (q, n) in quadros.iter().zip(&esperado).step_by(7) {
            for s in 3..6 {
                let luma = luma_da_faixa(*n, s);
                let (previsto, antes) = (cinza(sem_ajustes, (((luma as i64 * c) >> 12) + b) as f64), cinza(sem_ajustes, luma));
                let medido = cinza_da_faixa(q, s);
                // o dígito 9 já sai quase branco: lá só se confere o previsto
                if (medido - previsto).abs() > 2.0 || (antes < 240.0 && (medido - antes).abs() < 4.0) {
                    return Err(format!("{rotulo}: faixa {s} do quadro {n}: cinza {medido:.1} × {previsto:.1} previsto ({antes:.1} sem ajustes)"));
                }
            }
        }
        if jan.is_none() && r.compostos.len() != esperado.len() {
            return Err(format!("{rotulo}: {} hashes do composto", r.compostos.len()));
        }
        Ok(esperado.len() as u64)
    }

    /// Cinza de cada dígito (0..9) no render SEM efeito: a faixa das unidades dos quadros 40..50.
    pub(crate) fn cinza_por_digito(cap: &Arc<Capacidades>, fonte: Fonte) -> [f64; 10] {
        let m = apoio::medida(cap, fonte);
        let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", &m.info.quadros, 40, 50, 1.0, json!([]))], json!({}));
        let plano = Mundo::novo(cap).com("m1", &m.info).compilar(&p, &alvo_final("cpu")).expect("compila");
        let (_, quadros) = renderizar(cap, &plano, "e1ref", &[]);
        let mut v = [0.0; 10];
        for (q, j) in quadros.iter().zip(40..) {
            v[(m.numeros[j] % 10) as usize] = cinza_da_faixa(q, 5);
        }
        eprintln!("cinza por dígito sem ajustes ({fonte:?}): {v:?}");
        v
    }

    #[test]
    fn e1_completo_com_eq() {
        // mede os pixels da cadeia; o selo (camada global) cairia na faixa medida
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let casos: [(f64, (i64, i64), Option<(u64, u64)>); 6] =
            [(0.5, (30, 1), None), (1.0, (30, 1), None), (4.0, (30, 1), None), (0.5, (30000, 1001), None), (1.0, (25, 1), None), (0.5, (30, 1), Some((50, 80)))];
        let (mut slots, mut falhas) = (0, Vec::new());
        for f in [Fonte::Mp4, Fonte::Mkv30] {
            let sem_ajustes = cinza_por_digito(&cap, f);
            for (v, fps, jan) in casos {
                match caso_e1(&cap, f, v, fps, jan, &sem_ajustes) {
                    Ok(n) => slots += n,
                    Err(e) => falhas.push(e),
                }
            }
        }
        eprintln!("E1 com eq pelo compilador ({}): 12 casos, {slots} slots, {} falhas", cap.versao, falhas.len());
        assert!(falhas.is_empty(), "{}", falhas.join("\n"));
    }

    /// Cinza previsto da faixa `s` na média dos quadros `[j − antes, j + depois]` da fonte (pontas repetidas).
    fn cinza_da_media(sem_efeito: &[f64; 10], m: &Medida, j: usize, antes: usize, depois: usize, s: u32) -> f64 {
        let n = m.numeros.len() as i64;
        let soma: f64 = (j as i64 - antes as i64..=j as i64 + depois as i64).map(|k| luma_da_faixa(m.numeros[k.clamp(0, n - 1) as usize], s)).sum();
        cinza(sem_efeito, soma / (antes + depois + 1) as f64)
    }

    /// `ocultar` + `tmix`: a máscara entra em cada quadro lido ANTES da média, também nos lidos antes do trecho.
    #[test]
    fn ocultar_e_tmix_sem_sobra() {
        // mede os pixels da cadeia; o selo (camada global) cairia na faixa medida
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let (mut slots, mut controle) = (0, 0);
        for fonte in [Fonte::Mp4, Fonte::Mkv30] {
            let m = apoio::medida(&cap, fonte);
            let sem_efeito = cinza_por_digito(&cap, fonte);
            for (n, j_in, j_out, v, fps, jan) in [(4u32, 40usize, 100usize, 1.0, (30, 1), None), (2, 40, 100, 0.5, (30, 1), Some((50u64, 80u64))), (4, 2, 40, 1.0, (25, 1), None)] {
                let rotulo = format!("{fonte:?} média {n} {j_in}..{j_out} {v}× {fps:?} {jan:?} ({})", cap.versao);
                let montar = |com_ocultar: bool| {
                    let ocultacoes = if com_ocultar { json!([ocultar("o1", "r1", "solido", json!({}))]) } else { json!([]) };
                    let p = projeto(
                        320,
                        180,
                        fps,
                        vec![midia("m1", "video", ocultacoes)],
                        vec![trecho("c1", "m1", &m.info.quadros, j_in, j_out, v, json!([media(n)]))],
                        // âncoras em todos os quadros que a média lê, também antes do trecho
                        json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras_da_marca(fonte, &m, j_in.saturating_sub(8), j_out))] }),
                    );
                    let alvo = jan.map(|(de, ate)| janela(180, de, ate)).unwrap_or_else(|| alvo_final("cpu"));
                    Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &alvo).unwrap_or_else(|e| panic!("{rotulo}: {e}"))
                };
                let plano = montar(true);
                assert!(plano.blocos[0].grafo.contains(&format!("tmix=frames={n}")), "{rotulo}");
                let (_, quadros) = renderizar(&cap, &plano, "tmix", &[]);
                let todos = quadro_do_slot(&plano, 0);
                let do_slot = jan.map(|(de, ate)| todos[de as usize..ate as usize].to_vec()).unwrap_or(todos);
                for (q, (quadro, &j)) in quadros.iter().zip(&do_slot).enumerate() {
                    assert_eq!(sobra(quadro), 0, "{rotulo}: slot {q} com pixel do alvo de sobra");
                    // identidade: a média é a dos `n` quadros que terminam no previsto
                    for s in 4..6 {
                        let (medido, previsto) = (cinza_da_faixa(quadro, s), cinza_da_media(&sem_efeito, &m, j, n as usize - 1, 0, s));
                        assert!((medido - previsto).abs() <= 1.5, "{rotulo}: slot {q} (quadro {j}) faixa {s}: {medido:.1} × {previsto:.1}");
                    }
                    slots += 1;
                }
                // controle: sem a ocultação o alvo aparece, diluído, e o medidor o vê
                let (_, sem) = renderizar(&cap, &montar(false), "tmix-sem", &[]);
                let visto = sem.iter().map(sobra).min().unwrap();
                assert!(visto > 200, "{rotulo}: o medidor não vê o alvo diluído ({visto} px)");
                controle += 1;
            }
        }
        eprintln!("ocultar + tmix ({}): {slots} slots sem sobra e com a média prevista; {controle} controles", cap.versao);
    }

    /// Largura da marca vermelha, em px, e o centro dela.
    fn marca_de(q: &QuadroRgb) -> (u32, f64) {
        let (c, (cx, _)) = caixa_da_cor(q, vermelho).expect("marca vermelha");
        (c[2] - c[0], cx)
    }

    /// Centro x (px) e largura da marca no campo de cima ou de baixo do quadro `j` da fonte entrelaçada.
    fn marca_no_campo(j: u32, de_baixo: bool, w: u32) -> (f64, f64) {
        let c = fontes::marca_esperada_no_campo(Fonte::Entrelacado, j, de_baixo);
        ((c[0] + c[2] / 2.0) * w as f64, c[2] * w as f64)
    }

    fn plano_entrelacado(cap: &Arc<Capacidades>, m: &Medida, modo: Option<&str>, com_ocultar: bool, fps: (i64, i64), alvo: &Alvo) -> Plano {
        let (j_in, j_out) = FAIXA_ENTRELACADA;
        let efeitos = modo.map(|m| json!([desentrelacar(m)])).unwrap_or(json!([]));
        let ocultacoes = if com_ocultar { json!([ocultar("o1", "r1", "solido", json!({}))]) } else { json!([]) };
        let p = projeto(
            720,
            480,
            fps,
            vec![midia("m1", "video", ocultacoes)],
            vec![trecho("c1", "m1", &m.info.quadros, j_in, j_out, 1.0, efeitos)],
            json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras_da_marca(Fonte::Entrelacado, m, j_in.saturating_sub(4), j_out + 4))] }),
        );
        Mundo::novo(cap).com("m1", &m.info).compilar(&p, alvo).unwrap_or_else(|e| panic!("entrelaçado {modo:?} ({}): {e}", cap.versao))
    }

    /// Quadros 20..52 da fonte entrelaçada: a marca anda 4 px por campo (longe da volta, em 2 s).
    const FAIXA_ENTRELACADA: (usize, usize) = (20, 52);
    const NTSC: (i64, i64) = (30000, 1001);
    const NTSC_CAMPOS: (i64, i64) = (60000, 1001);

    /// `quadro` mantém a contagem; `campo` dobra junto com a tabela: cada campo vira um quadro, no instante dele.
    #[test]
    fn desentrelace_quadro_e_campo() {
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Entrelacado);
        let tab = &m.info.quadros;
        let (j_in, j_out) = FAIXA_ENTRELACADA;
        let n = j_out - j_in;
        // controle: sem o efeito o quadro traz os dois campos (a marca sai 4 px mais larga)
        let (r, cru) = renderizar(&cap, &plano_entrelacado(&cap, &m, None, false, NTSC, &alvo_final("cpu")), "cru", &[]);
        assert_eq!(medir::quadros_de_origem(&cap, &r.blocos[0]).unwrap(), m.numeros[j_in..j_out]);
        let lado = marca_no_campo(m.numeros[j_in], false, 720).1;
        assert!(cru.iter().all(|q| marca_de(q).0 as f64 >= lado + 3.0), "a fonte não está entrelaçada como o teste espera");

        // quadro: mesma contagem, mesmo quadro, só o campo de cima
        let plano = plano_entrelacado(&cap, &m, Some("quadro"), false, NTSC, &alvo_final("cpu"));
        // a ordem do fluxo (`tt`), escrita
        assert!(plano.blocos[0].grafo.contains("bwdif=mode=send_frame:parity=tff:deint=all"), "{}", cap.versao);
        let o = plano.proveniencia.roteiro[0].origem.as_ref().unwrap();
        assert_eq!((plano.total_q, o.lidos as usize, &o.pts[..]), (n as u64, n, &tab.pts[j_in..j_out]));
        let (r, quadros) = renderizar(&cap, &plano, "quadro", &[]);
        assert_eq!(medir::quadros_de_origem(&cap, &r.blocos[0]).unwrap(), m.numeros[j_in..j_out]);
        for (q, j) in quadros.iter().zip(j_in..) {
            let ((largura, cx), (previsto, lado)) = (marca_de(q), marca_no_campo(m.numeros[j], false, 720));
            assert!((largura as f64 - lado).abs() <= 1.0 && (cx - previsto).abs() <= 1.0, "quadro {j} ({}): marca {largura} px em {cx:.1} × {lado} px em {previsto:.1}", cap.versao);
        }

        // campo: o dobro de quadros, na taxa dos campos; par = campo de cima, ímpar = o de baixo (meio quadro depois)
        let plano = plano_entrelacado(&cap, &m, Some("campo"), false, NTSC_CAMPOS, &alvo_final("cpu"));
        assert!(plano.blocos[0].grafo.contains("bwdif=mode=send_field:parity=tff:deint=all"), "{}", cap.versao);
        let campos = tabela(&efs(json!([desentrelacar("campo")])), tab);
        let o = plano.proveniencia.roteiro[0].origem.as_ref().unwrap();
        assert_eq!((plano.total_q, o.lidos as usize, &o.pts[..]), (2 * n as u64, 2 * n, &campos.pts[2 * j_in..2 * j_out]));
        assert_eq!(quadro_do_slot(&plano, 0), (2 * j_in..2 * j_out).collect::<Vec<_>>(), "um campo por slot");
        let (r, quadros) = renderizar(&cap, &plano, "campo", &[]);
        let numeros = medir::quadros_de_origem(&cap, &r.blocos[0]).unwrap();
        for (k, q) in quadros.iter().enumerate() {
            let j = j_in + k / 2;
            assert_eq!(numeros[k], m.numeros[j], "campo {k}");
            let ((largura, cx), (previsto, lado)) = (marca_de(q), marca_no_campo(m.numeros[j], k % 2 == 1, 720));
            assert!((largura as f64 - lado).abs() <= 1.0 && (cx - previsto).abs() <= 1.0, "campo {k} do quadro {j} ({}): marca {largura} px em {cx:.1} × {lado} px em {previsto:.1}", cap.versao);
        }
        // os dois campos de um quadro não são o mesmo quadro repetido
        assert!((marca_de(&quadros[10]).1 - marca_de(&quadros[11]).1).abs() >= 3.0);

        // janela no meio (quadro final, prévia): os mesmos pixels da exportação
        let (de, ate) = (21u64, 34u64);
        let (_, parte) = renderizar(&cap, &plano_entrelacado(&cap, &m, Some("campo"), false, NTSC_CAMPOS, &janela(480, de, ate)), "campo-janela", &[]);
        for (k, q) in parte.iter().enumerate() {
            assert_eq!(medir::pixels_diferentes(q, &quadros[de as usize + k], 0), 0, "slot {} da janela ({})", de as usize + k, cap.versao);
        }
        let (_, inteiro) = renderizar(&cap, &plano_entrelacado(&cap, &m, Some("quadro"), false, NTSC, &alvo_final("cpu")), "quadro-inteiro", &[]);
        let (_, parte) = renderizar(&cap, &plano_entrelacado(&cap, &m, Some("quadro"), false, NTSC, &janela(480, 9, 15)), "quadro-janela", &[]);
        for (k, q) in parte.iter().enumerate() {
            assert_eq!(medir::pixels_diferentes(q, &inteiro[9 + k], 0), 0, "slot {} da janela ({})", 9 + k, cap.versao);
        }
        eprintln!("desentrelace ({}): quadro {n} de {n}; campo {} de {}; janelas iguais à exportação", cap.versao, 2 * n, 2 * n);
    }

    /// A leitura alargada (2 quadros antes, 1 depois, seleção por índice e guarda de pts) na matriz de fontes inteira,
    /// por quadro e por campo: o quadro de cada slot é o previsto, na exportação e na janela, do começo e até o fim
    /// da mídia. Base de um tique por quadro: por campo é recusado, nunca ignorado.
    #[test]
    fn leitura_alargada_na_matriz_de_fontes() {
        let Some(cap) = fontes::cap() else { return };
        let (mut falhas, mut slots, mut recusados) = (Vec::new(), 0, 0);
        for &f in fontes::MATRIZ {
            let m = apoio::medida(&cap, f);
            let tab = &m.info.quadros;
            let n = m.limite.min(tab.pts.len());
            for (modo, fps) in [("quadro", (30, 1)), ("campo", (60, 1))] {
                for (j_in, j_out, jan) in [(0, 24, None), (9, 33, Some((5u64, 12u64))), (n - 20, n, None)] {
                    let rotulo = format!("{f:?} por {modo} {j_in}..{j_out} {jan:?} ({})", cap.versao);
                    // canvas do tamanho da fonte exibida: o medidor lê as faixas do número no quadro inteiro
                    let (w, h) = m.info.exibido;
                    let p = projeto(w, h, fps, vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", tab, j_in, j_out, 1.0, json!([desentrelacar(modo)]))], json!({}));
                    let alvo = jan.map(|(de, ate)| janela(h, de, ate)).unwrap_or_else(|| alvo_final("cpu"));
                    let por_campo = modo == "campo";
                    let plano = match Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &alvo) {
                        Ok(p) => p,
                        Err(e) if por_campo && !campo_cabe(tab) && e.to_string().contains("nao_suportado") => {
                            recusados += 1;
                            continue;
                        }
                        Err(e) => {
                            falhas.push(format!("{rotulo}: {e}"));
                            continue;
                        }
                    };
                    if por_campo && !campo_cabe(tab) {
                        falhas.push(format!("{rotulo}: devia ser recusado"));
                        continue;
                    }
                    let todos = quadro_do_slot(&plano, 0);
                    let do_slot = jan.map(|(de, ate)| todos[de as usize..ate as usize].to_vec()).unwrap_or(todos);
                    // por campo o índice é de campo: dois por quadro da fonte
                    let esperado: Vec<u32> = do_slot.iter().map(|&j| m.numeros[if por_campo { j / 2 } else { j }]).collect();
                    let lidos = executor::rodar(&cap, &plano, &fontes::pasta_temporaria("alargada"), &Opcoes { sem_perdas: true, ..Opcoes::default() }).and_then(|r| medir::quadros_de_origem(&cap, &r.blocos[0]));
                    match lidos {
                        Ok(v) if v == esperado => slots += v.len(),
                        Ok(v) => {
                            let q = v.iter().zip(&esperado).position(|(a, b)| a != b).unwrap_or(v.len().min(esperado.len()));
                            falhas.push(format!("{rotulo}: {} slots × {} previstos; primeiro diferente {q}: {:?} × {:?}", v.len(), esperado.len(), v.get(q), esperado.get(q)));
                        }
                        Err(e) => falhas.push(format!("{rotulo}: {e}")),
                    }
                }
            }
        }
        eprintln!("leitura alargada na matriz ({}): {slots} slots, {recusados} recusados (um tique por quadro), {} falhas", cap.versao, falhas.len());
        assert!(falhas.is_empty(), "{}", falhas.join("\n"));
    }

    /// O9: `desentrelacar` (quadro e campo) + `ocultar`: 0 pixel de sobra, nos dois campos.
    #[test]
    fn desentrelacar_e_ocultar_sem_sobra() {
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Entrelacado);
        for (modo, fps) in [("quadro", NTSC), ("campo", NTSC_CAMPOS)] {
            let (_, quadros) = renderizar(&cap, &plano_entrelacado(&cap, &m, Some(modo), true, fps, &alvo_final("cpu")), "des-ocultar", &[]);
            for (q, quadro) in quadros.iter().enumerate() {
                assert_eq!(sobra(quadro), 0, "{modo} ({}): slot {q} com pixel do alvo de sobra", cap.versao);
                // a caixa preta está lá (o alvo foi coberto, não sumiu)
                let pretos = quadro.px.chunks_exact(3).filter(|p| p.iter().all(|&c| c < 12)).count();
                assert!(pretos > 5000, "{modo}: slot {q} sem a caixa ({pretos} px)");
            }
            let (_, sem) = renderizar(&cap, &plano_entrelacado(&cap, &m, Some(modo), false, fps, &alvo_final("cpu")), "des-sem-ocultar", &[]);
            assert!(sem.iter().all(|q| sobra(q) > 3000), "{modo}: o medidor não vê o alvo");
            eprintln!("{modo} + ocultar ({}): {} slots sem sobra", cap.versao, quadros.len());
        }
    }

    /// `ocultar` sólido da mídia com `ativo` = [de, ate] (tempo de fonte), como a tela grava.
    fn oculta_em(de: f64, ate: f64) -> Value {
        json!([ocultar("o1", "r1", "solido", json!({ "ativo": { "de": de, "ate": ate } }))])
    }

    /// Projeto 320×180 da fonte MP4 com o rastreio da marca em volta de [j_in, j_out), compilado para a exportação.
    fn plano_da_marca(cap: &Arc<Capacidades>, m: &Medida, ocultacoes: Value, clipe: Value, (j_in, j_out): (usize, usize)) -> Plano {
        let rastreios = json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras_da_marca(Fonte::Mp4, m, j_in - 8, j_out + 8))] });
        let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", ocultacoes)], vec![clipe], rastreios);
        Mundo::novo(cap).com("m1", &m.info).compilar(&p, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{e}"))
    }

    /// A ocultação criada pela tela vale só na faixa do clipe (`ativo` em tempo de fonte). A média lê vizinhos de
    /// antes dela, que nunca aparecem sozinhos: saem cobertos (lidos pelo próprio instante, entravam descobertos e
    /// o alvo vazava diluído nos primeiros quadros do clipe).
    #[test]
    fn media_com_ocultar_so_na_faixa_do_clipe_sem_sobra() {
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        let tab = &m.info.quadros;
        let (j_in, j_out) = (40usize, 64usize);
        let montar = |ocultacoes: Value| plano_da_marca(&cap, &m, ocultacoes, trecho("c1", "m1", tab, j_in, j_out, 1.0, json!([media(4)])), (j_in, j_out));
        let (_, quadros) = renderizar(&cap, &montar(oculta_em(tab.tempo(j_in), tab.tempo(j_out))), "viz-trecho", &[]);
        assert_eq!(quadros.len(), j_out - j_in);
        for (q, quadro) in quadros.iter().enumerate() {
            assert_eq!(sobra(quadro), 0, "trecho com média ({}): slot {q} com pixel do alvo de sobra", cap.versao);
        }
        let (_, sem) = renderizar(&cap, &montar(json!([])), "viz-trecho-sem", &[]);
        assert!(sobra(&sem[0]) > 200, "o medidor não vê o alvo diluído");
        // a ocultação que não vale na ponta do clipe não passa a cobrir o vizinho: o que o perito deixou à mostra fica
        let (_, quadros) = renderizar(&cap, &montar(oculta_em(tab.tempo(j_in + 6), tab.tempo(j_out))), "viz-meio", &[]);
        assert!(sobra(&quadros[0]) > 200, "ocultação inativa no começo do clipe cobriu o vizinho");
        assert_eq!(sobra(&quadros[quadros.len() - 1]), 0);
    }

    /// Quadro congelado com média centrada e a ocultação da mídia no instante do quadro (contratos (h) 23): os
    /// vizinhos lidos saem cobertos (antes, 4 dos 5 quadros da média entravam com o alvo à mostra).
    #[test]
    fn congelado_com_media_e_ocultar_no_instante_sem_sobra() {
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        let tab = &m.info.quadros;
        let j = 52;
        let montar = |ocultacoes: Value| plano_da_marca(&cap, &m, ocultacoes, congelado(&m, j, json!([media(5)])), (j, j + 1));
        let (_, quadros) = renderizar(&cap, &montar(oculta_em(tab.tempo(j), tab.tempo(j))), "viz-congelado", &[]);
        assert!(!quadros.is_empty());
        for quadro in &quadros {
            assert_eq!(sobra(quadro), 0, "congelado com média ({}): pixel do alvo de sobra", cap.versao);
        }
        let (_, sem) = renderizar(&cap, &montar(json!([])), "viz-congelado-sem", &[]);
        assert!(sobra(&sem[0]) > 200, "o medidor não vê o alvo no congelado");
    }

    fn plano_entrelacado_na_faixa(cap: &Arc<Capacidades>, m: &Medida, modo: &str, fps: (i64, i64)) -> Plano {
        let tab = &m.info.quadros;
        let (j_in, j_out) = FAIXA_ENTRELACADA;
        let p = projeto(
            720,
            480,
            fps,
            vec![midia("m1", "video", oculta_em(tab.tempo(j_in), tab.tempo(j_out)))],
            vec![trecho("c1", "m1", tab, j_in, j_out, 1.0, json!([desentrelacar(modo)]))],
            json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras_da_marca(Fonte::Entrelacado, m, j_in - 4, j_out + 4))] }),
        );
        Mundo::novo(cap).com("m1", &m.info).compilar(&p, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{modo}: {e}"))
    }

    /// Desentrelace (lê 2 quadros antes e 1 depois) com a ocultação só na faixa do clipe: 0 pixel de sobra.
    #[test]
    fn desentrelace_com_ocultar_so_na_faixa_do_clipe_sem_sobra() {
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Entrelacado);
        for (modo, fps) in [("quadro", NTSC), ("campo", NTSC_CAMPOS)] {
            let (_, quadros) = renderizar(&cap, &plano_entrelacado_na_faixa(&cap, &m, modo, fps), "viz-des", &[]);
            for (q, quadro) in quadros.iter().enumerate() {
                assert_eq!(sobra(quadro), 0, "desentrelace por {modo} ({}): slot {q} com pixel do alvo de sobra", cap.versao);
            }
        }
    }

    /// O `feito` conta os quadros da FONTE cobertos. Por campo o mapa conta campos (dois por quadro): contados na
    /// tabela de campos, metade saía como "interpolada" sem que nenhuma posição tivesse sido interpolada.
    #[test]
    fn feito_conta_os_quadros_ocultados_da_fonte_tambem_por_campo() {
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Entrelacado);
        let (j_in, j_out) = FAIXA_ENTRELACADA;
        for (modo, fps) in [("quadro", NTSC), ("campo", NTSC_CAMPOS)] {
            let plano = plano_entrelacado_na_faixa(&cap, &m, modo, fps);
            let n = &plano.proveniencia.ocultacoes[0].quadros;
            assert_eq!((n.ancora, n.interpolado, n.medido), ((j_out - j_in) as u32, 0, 0), "por {modo}: quadros ocultados no feito");
        }
    }

    fn congelado(m: &Medida, j: usize, efeitos: Value) -> Value {
        let t = &m.info.quadros;
        json!({ "tipo": "congelar", "id": "g1", "rotulo": "", "midia": "m1", "instante_s": t.tempo(j), "instante_pts": t.pts[j], "duracao_s": 0.2, "efeitos": efeitos })
    }

    /// Quadro congelado: média CENTRADA no instante (§5.3), com a ocultação em todos os quadros lidos; nas pontas da
    /// mídia a ponta repete.
    #[test]
    fn media_no_congelado_e_centrada() {
        // mede os pixels da cadeia; o selo (camada global) cairia na faixa medida
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        let sem_efeito = cinza_por_digito(&cap, Fonte::Mp4);
        let ultimo = m.numeros.len() - 1;
        for (n, j, antes, depois) in [(5u32, 60usize, 2usize, 2usize), (4, 60, 1, 2), (5, ultimo, 2, 2), (5, 1, 2, 2)] {
            let rotulo = format!("média {n} no quadro {j} ({})", cap.versao);
            let montar = |com_ocultar: bool| {
                let ocultacoes = if com_ocultar { json!([ocultar("o1", "r1", "solido", json!({}))]) } else { json!([]) };
                let ancoras = ancoras_da_marca(Fonte::Mp4, &m, j.saturating_sub(8), (j + 8).min(ultimo + 1));
                let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", ocultacoes)], vec![congelado(&m, j, json!([media(n)]))], json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras)] }));
                Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{rotulo}: {e}"))
            };
            let plano = montar(true);
            let (_, quadros) = renderizar(&cap, &plano, "congelado-media", &[]);
            assert_eq!(quadros.len(), 6, "{rotulo}");
            for q in &quadros {
                assert_eq!(sobra(q), 0, "{rotulo}: pixel do alvo de sobra");
                for s in 4..6 {
                    let (medido, previsto) = (cinza_da_faixa(q, s), cinza_da_media(&sem_efeito, &m, j, antes, depois, s));
                    assert!((medido - previsto).abs() <= 1.5, "{rotulo}: faixa {s}: {medido:.1} × {previsto:.1}");
                }
            }
            // a média à esquerda (a do trecho) daria outro cinza: o teste separa as duas
            if j == 60 {
                let esquerda = cinza_da_media(&sem_efeito, &m, j, n as usize - 1, 0, 5);
                assert!((cinza_da_faixa(&quadros[0], 5) - esquerda).abs() > 10.0, "{rotulo}: não distingue a centrada da à esquerda");
            }
            let (_, sem) = renderizar(&cap, &montar(false), "congelado-media-sem", &[]);
            assert!(sobra(&sem[0]) > 200, "{rotulo}: o medidor não vê o alvo");
        }
    }

    /// Congelado de fonte entrelaçada: o quadro desentrelaçado é o do trecho (por quadro, e o primeiro campo por campo).
    #[test]
    fn desentrelace_no_congelado() {
        // mede os pixels da cadeia; o selo (camada global) cairia na faixa medida
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Entrelacado);
        let j = 30;
        let (_, do_trecho) = renderizar(&cap, &plano_entrelacado(&cap, &m, Some("quadro"), false, NTSC, &alvo_final("cpu")), "des-trecho", &[]);
        for modo in ["quadro", "campo"] {
            let p = projeto(720, 480, NTSC, vec![midia("m1", "video", json!([]))], vec![congelado(&m, j, json!([desentrelacar(modo)]))], json!({}));
            let plano = Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{modo}: {e}"));
            let (_, quadros) = renderizar(&cap, &plano, "des-congelado", &[]);
            for q in &quadros {
                assert_eq!(medir::pixels_diferentes(q, &do_trecho[j - FAIXA_ENTRELACADA.0], 0), 0, "congelado por {modo} ({})", cap.versao);
            }
        }
    }

    /// Diferença média (níveis de 0 a 255) entre quadros vizinhos: o `tblend=difference` + `YAVG` do ensaio.
    pub(crate) fn dif_entre_vizinhos(quadros: &[QuadroRgb]) -> f64 {
        let luma = |p: &[u8]| (p[0] as i32 + 2 * p[1] as i32 + p[2] as i32) / 4;
        let mut soma = 0.0;
        for par in quadros.windows(2) {
            let d: u64 = par[0].px.chunks_exact(3).zip(par[1].px.chunks_exact(3)).map(|(a, b)| (luma(a) - luma(b)).unsigned_abs() as u64).sum();
            soma += d as f64 / (par[0].w * par[0].h) as f64;
        }
        soma / (quadros.len() - 1) as f64
    }

    /// Trecho `[j_in, j_out)` da cena tremida com os efeitos dados, no canvas do tamanho da fonte.
    pub(crate) fn plano_tremido(cap: &Arc<Capacidades>, f: &FonteInfo, j_in: usize, j_out: usize, efeitos: Value, extra: Value, alvo: &Alvo) -> Plano {
        let p = projeto(TREMIDO.0, TREMIDO.1, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", &f.quadros, j_in, j_out, 1.0, efeitos)], extra);
        Mundo::novo(cap).com("m1", f).compilar(&p, alvo).unwrap_or_else(|e| panic!("cena tremida ({}): {e}", cap.versao))
    }

    /// `Final zoom` que o `vidstabtransform` do bloco imprimiu.
    fn zoom_do_bloco(r: &Rodado) -> f64 {
        let log = std::fs::read(r.blocos[0].parent().unwrap().join("b000.stderr")).unwrap();
        let log = String::from_utf8_lossy(&log);
        log.lines().filter_map(|l| l.split("Final zoom:").nth(1)).filter_map(|v| v.trim().parse::<f64>().ok()).next_back().expect("Final zoom no registro do bloco")
    }

    /// Lado (px) do quadrado magenta da cena tremida.
    fn lado_do_alvo(q: &QuadroRgb) -> f64 {
        let (c, _) = caixa_da_cor(q, magenta).expect("alvo magenta");
        ((c[2] - c[0]) + (c[3] - c[1])) as f64 / 2.0
    }

    /// Faixa analisada dos testes da estabilização.
    const ANALISE: (usize, usize) = (10, 109);

    /// Estabilizar derruba a diferença entre vizinhos (ensaio: 11,76 → abaixo de 1), com a borda ampliada.
    #[test]
    fn estabilizar_derruba_vizinhos() {
        let Some(cap) = fontes::cap() else { return };
        let f = tremido(&cap, &fontes::pasta_temporaria("tremido"));
        let (d0, d1) = ANALISE;
        let (_, cru) = renderizar(&cap, &plano_tremido(&cap, &f, d0, d1 + 1, json!([]), json!({}), &alvo_final("cpu")), "tremido-cru", &[]);
        let plano = plano_tremido(&cap, &f, d0, d1 + 1, json!([estabilizar(&f.quadros, d0, d1, "ampliar", 0.0, None)]), json!({}), &alvo_final("cpu"));
        assert_eq!(plano.previas.len(), 1, "sem `.trf` gravado: uma prévia de detecção");
        let (_, firme) = renderizar(&cap, &plano, "tremido-firme", &[]);
        let (antes, depois) = (dif_entre_vizinhos(&cru), dif_entre_vizinhos(&firme));
        eprintln!("estabilizar ({}): diferença entre vizinhos {antes:.2} → {depois:.2}", cap.versao);
        assert!(antes > 8.0, "a cena de teste não treme: {antes:.2}");
        assert!(depois < 1.0, "estabilizado ainda treme: {antes:.2} → {depois:.2}");
        // o alvo quase para: o passo médio de um quadro ao seguinte cai de mais de 5 px para menos de 1
        let passo = |qs: &[QuadroRgb]| {
            let c: Vec<(f64, f64)> = qs[20..80].iter().map(|q| caixa_da_cor(q, magenta).expect("alvo").1).collect();
            c.windows(2).map(|p| (p[1].0 - p[0].0).hypot(p[1].1 - p[0].1)).sum::<f64>() / (c.len() - 1) as f64
        };
        let (solto, preso) = (passo(&cru), passo(&firme));
        eprintln!("estabilizar ({}): o alvo anda {solto:.2} px por quadro na fonte e {preso:.2} px estabilizado", cap.versao);
        assert!(solto > 5.0 && preso < 1.0, "o alvo anda {solto:.2} px por quadro na fonte e {preso:.2} px estabilizado");
    }

    /// `.trf` igual em 3 execuções (`OMP_NUM_THREADS=1`), com o nome que o grafo cita.
    #[test]
    fn trf_igual_em_3_execucoes() {
        let Some(cap) = fontes::cap() else { return };
        let f = tremido(&cap, &fontes::pasta_temporaria("tremido"));
        let (d0, d1) = ANALISE;
        let plano = plano_tremido(&cap, &f, d0 + 20, d1 - 20, json!([estabilizar(&f.quadros, d0, d1, "preta", 0.0, None)]), json!({}), &alvo_final("cpu"));
        let Previa::Vidstab { efeito, args, trf } = &plano.previas[0] else { panic!("prévia") };
        assert_eq!(efeito, "s1");
        assert!(plano.blocos[0].grafo.contains(&format!("vidstabtransform=input={trf}:optzoom=0:zoom=0:crop=black:smoothing=15")), "{}", plano.blocos[0].grafo);
        // a análise cobre a faixa `analise`, não a do clipe
        let vf = &args[args.iter().position(|a| a == "-vf").unwrap() + 1];
        assert!(vf.starts_with(&format!("trim=start_pts={}:end_pts={},", f.quadros.pts[d0], f.quadros.pts[d1 + 1])), "{vf}");
        let hashes: Vec<String> = (0..3).map(|i| rodar_previas(&cap, &plano, &fontes::pasta_temporaria(&format!("trf{i}")))[0].1.clone()).collect();
        assert!(hashes[0] == hashes[1] && hashes[1] == hashes[2], "({}) {hashes:?}", cap.versao);
        eprintln!("`.trf` ({}): {} nas 3 execuções", cap.versao, &hashes[0][..16]);
        // o mesmo clipe em dois blocos pede uma prévia só (mesmo nome, mesmos argumentos)
        let dois = plano_tremido(&cap, &f, d0, d1 + 1, json!([estabilizar(&f.quadros, d0, d1, "preta", 0.0, None)]), json!({}), &janela(360, 50, 60));
        assert_eq!(dois.previas, plano.previas);
    }

    /// Aparar o clipe (começo e fim) não muda os quadros que ficaram: 0 de 24, com borda preta e com o zoom gravado.
    /// Controle: análise refeita só no clipe aparado, com zoom recalculado, muda os 24.
    #[test]
    fn aparar_nao_muda_quadros() {
        let Some(cap) = fontes::cap() else { return };
        let f = tremido(&cap, &fontes::pasta_temporaria("tremido"));
        let (d0, d1) = ANALISE;
        let tab = &f.quadros;
        let pasta = fontes::pasta_temporaria("aparar-trf");
        let sem_trf = plano_tremido(&cap, &f, d0, d1 + 1, json!([estabilizar(tab, d0, d1, "ampliar", 0.0, None)]), json!({}), &alvo_final("cpu"));
        let (_, sha, trf) = rodar_previas(&cap, &sem_trf, &pasta).remove(0);
        let diferentes = |a: &[QuadroRgb], b: &[QuadroRgb]| a.iter().zip(b).filter(|(x, y)| medir::pixels_diferentes(x, y, 0) > 0).count();
        for (borda, zoom) in [("preta", 0.0), ("ampliar", 12.5)] {
            let ef = json!([estabilizar(tab, d0, d1, borda, zoom, Some(&sha))]);
            let (_, inteiro) = renderizar(&cap, &plano_tremido(&cap, &f, d0, d1 + 1, ef.clone(), json!({}), &alvo_final("cpu")), "aparar-a", &[&trf]);
            let (_, aparado) = renderizar(&cap, &plano_tremido(&cap, &f, 40, 80, ef.clone(), json!({}), &alvo_final("cpu")), "aparar-b", &[&trf]);
            // quadros 48..72 da fonte nos dois
            let n = diferentes(&inteiro[48 - d0..72 - d0], &aparado[48 - 40..72 - 40]);
            assert_eq!(n, 0, "{borda} ({}): {n} de 24 quadros mudaram ao aparar", cap.versao);
            // e a janela (quadro final) dá o quadro da exportação
            let (_, um) = renderizar(&cap, &plano_tremido(&cap, &f, 40, 80, ef, json!({}), &janela(360, 30, 31)), "aparar-q", &[&trf]);
            assert_eq!(medir::pixels_diferentes(&um[0], &aparado[30], 0), 0, "{borda}: quadro final ≠ exportação");
        }
        let (_, inteiro) = renderizar(&cap, &sem_trf, "aparar-c", &[]);
        let refeito = plano_tremido(&cap, &f, 40, 80, json!([estabilizar(tab, 40, 79, "ampliar", 0.0, None)]), json!({}), &alvo_final("cpu"));
        let (_, aparado) = renderizar(&cap, &refeito, "aparar-d", &[]);
        let n = diferentes(&inteiro[48 - d0..72 - d0], &aparado[48 - 40..72 - 40]);
        assert_eq!(n, 24, "controle ({}): análise e zoom refeitos no clipe aparado deviam mudar os 24", cap.versao);
        eprintln!("aparar ({}): 0 de 24 com o `.trf` e o zoom gravados; 24 de 24 refazendo", cap.versao);
    }

    /// `ampliar` usa o `zoom_pct` gravado: o do `.trf` dá os quadros do zoom calculado; outro valor amplia na medida dele.
    #[test]
    fn ampliar_usa_zoom_gravado() {
        let Some(cap) = fontes::cap() else { return };
        let f = tremido(&cap, &fontes::pasta_temporaria("tremido"));
        let (d0, d1) = ANALISE;
        let tab = &f.quadros;
        let render = |nome: &str, borda: &str, zoom: f64, trf: Option<(&str, &Path)>, j_in: usize, j_out: usize| {
            let plano = plano_tremido(&cap, &f, j_in, j_out, json!([estabilizar(tab, d0, d1, borda, zoom, trf.map(|t| t.0))]), json!({}), &alvo_final("cpu"));
            let arquivos: Vec<&Path> = trf.iter().map(|t| t.1).collect();
            let (r, q) = renderizar(&cap, &plano, nome, &arquivos);
            (plano, r, q)
        };
        // sem `.trf`: a prévia analisa e o bloco calcula o zoom do `.trf` inteiro
        let (plano, r, calculado) = render("zoom-calc", "ampliar", 0.0, None, 30, 60);
        assert!(plano.blocos[0].grafo.contains(":optzoom=1:crop=black"));
        let z = zoom_do_bloco(&r);
        assert!(z > 3.0, "zoom calculado {z}");
        let trf = r.blocos[0].parent().unwrap().join(match &plano.previas[0] {
            Previa::Vidstab { trf, .. } => trf.clone(),
            _ => panic!("prévia"),
        });
        let sha = sha256_do_arquivo(&trf);
        // gravado: o mesmo zoom dá os mesmos quadros, também em outro recorte do clipe
        let (plano, _, gravado) = render("zoom-gravado", "ampliar", z, Some((&sha, &trf)), 30, 60);
        assert!(plano.previas.is_empty() && plano.blocos[0].grafo.contains(&format!("vidstabtransform=input={sha}.trf:optzoom=0:zoom={z}:crop=black")), "{}", plano.blocos[0].grafo);
        let n = calculado.iter().zip(&gravado).filter(|(a, b)| medir::pixels_diferentes(a, b, 0) > 0).count();
        assert_eq!(n, 0, "zoom gravado {z} ({}): {n} de 30 quadros diferentes do calculado", cap.versao);
        // outro valor gravado vale: o alvo sai ampliado nessa medida (lado de 40 px na fonte)
        let (_, _, preta) = render("zoom-0", "preta", z, Some((&sha, &trf)), 30, 60);
        let (_, _, maior) = render("zoom-maior", "ampliar", z + 20.0, Some((&sha, &trf)), 30, 60);
        for (nome, q, zoom) in [("preta", &preta, 0.0), ("gravado", &gravado, z), ("maior", &maior, z + 20.0)] {
            // zoom do vidstab em %: a imagem cresce 1 / (1 − zoom/100)
            let (medido, previsto) = (lado_do_alvo(&q[15]), ALVO_TREMIDO[2] as f64 / (1.0 - zoom / 100.0));
            assert!((medido - previsto).abs() <= 1.5, "{nome} ({}): lado do alvo {medido:.1} px × {previsto:.1}", cap.versao);
        }
        eprintln!("ampliar ({}): zoom {z} gravado = calculado em 30 quadros; lados {:.1}, {:.1} e {:.1} px", cap.versao, lado_do_alvo(&preta[15]), lado_do_alvo(&gravado[15]), lado_do_alvo(&maior[15]));
    }

    /// O outro ffmpeg da máquina (o do sistema ou o 7.0.2 do AppImage); com `SICRO_EXIGE_FFMPEG=1` a falta dele é falha.
    fn outro_ffmpeg(cap: &Capacidades) -> Option<Arc<Capacidades>> {
        use crate::edicao::ffmpeg::sonda;
        let candidatos = [Path::new(env!("CARGO_MANIFEST_DIR")).join("ffmpeg-linux/ffmpeg"), PathBuf::from("/usr/bin/ffmpeg")];
        let mesmo = |p: &Path| std::fs::canonicalize(p).ok() == std::fs::canonicalize(&cap.binario).ok();
        let achado = candidatos.iter().find(|p| p.exists() && !mesmo(p)).and_then(|p| sonda::capacidades_de(p, sonda::Origem::Configurado, None, false).ok());
        let achado = achado.filter(|c| c.faltam.is_empty() && c.sha256 != cap.sha256);
        if achado.is_none() {
            assert!(!std::env::var("SICRO_EXIGE_FFMPEG").is_ok_and(|v| v == "1"), "SICRO_EXIGE_FFMPEG=1 e o segundo ffmpeg não foi achado");
            eprintln!("aviso: teste dispensado, sem o segundo ffmpeg");
        }
        achado
    }

    /// Hash do composto de cada slot, com opções globais a mais (`-cpuflags …`).
    fn compostos(cap: &Capacidades, plano: &Plano, nome: &str, trfs: &[&Path], globais: &[&str]) -> Vec<String> {
        let pasta = fontes::pasta_temporaria(nome);
        por_na_pasta(&pasta, trfs);
        let op = Opcoes { sem_perdas: true, globais: globais.iter().map(|s| s.to_string()).collect(), ..Opcoes::default() };
        let r = executor::rodar(cap, plano, &pasta, &op).unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
        assert_eq!(r.compostos.len() as u64, plano.total_q, "{nome} ({})", cap.versao);
        r.compostos
    }

    /// D11 com os efeitos de origem: o composto de cada slot é o mesmo sem instruções de CPU (`-cpuflags 0`) e no
    /// outro ffmpeg, efeito por efeito. O `.trf` é o guardado (o "refazer" nunca recalcula).
    #[test]
    fn composto_igual_entre_binarios_e_cpus() {
        let Some(cap) = fontes::cap() else { return };
        let outro = outro_ffmpeg(&cap);
        let f = tremido(&cap, &fontes::pasta_temporaria("tremido"));
        let (d0, d1) = ANALISE;
        let tab = &f.quadros;
        let sem_trf = plano_tremido(&cap, &f, d0, d1 + 1, json!([estabilizar(tab, d0, d1, "preta", 0.0, None)]), json!({}), &alvo_final("cpu"));
        let (_, sha, trf) = rodar_previas(&cap, &sem_trf, &fontes::pasta_temporaria("composto-trf")).remove(0);
        let lente = efeito("lente", "l1", json!({ "k1": -0.2, "k2": 0.03, "cx": 0.5, "cy": 0.5, "perfil": "" }));
        let nitidez = efeito("nitidez", "n1", json!({ "quantidade": 0.8 }));
        let m = apoio::medida(&cap, Fonte::Entrelacado);
        let casos: Vec<(&str, Plano, Vec<&Path>)> = vec![
            ("estabilizar preta", plano_tremido(&cap, &f, 30, 54, json!([estabilizar(tab, d0, d1, "preta", 0.0, Some(&sha))]), json!({}), &alvo_final("cpu")), vec![&trf]),
            ("estabilizar ampliar", plano_tremido(&cap, &f, 30, 54, json!([estabilizar(tab, d0, d1, "ampliar", 6.5, Some(&sha))]), json!({}), &alvo_final("cpu")), vec![&trf]),
            ("lente", plano_tremido(&cap, &f, 30, 54, json!([lente]), json!({}), &alvo_final("cpu")), vec![]),
            ("ajustes", plano_tremido(&cap, &f, 30, 54, json!([efeito("ajustes", "a1", json!({ "brilho": 0.06, "contraste": 1.15, "gama": 0.9, "saturacao": 1.3 }))]), json!({}), &alvo_final("cpu")), vec![]),
            ("nitidez", plano_tremido(&cap, &f, 30, 54, json!([nitidez]), json!({}), &alvo_final("cpu")), vec![]),
            ("média de 5", plano_tremido(&cap, &f, 30, 54, json!([media(5)]), json!({}), &alvo_final("cpu")), vec![]),
            ("desentrelace por quadro", plano_entrelacado(&cap, &m, Some("quadro"), false, NTSC, &alvo_final("cpu")), vec![]),
            ("desentrelace por campo", plano_entrelacado(&cap, &m, Some("campo"), false, NTSC_CAMPOS, &alvo_final("cpu")), vec![]),
        ];
        let mut falhas = Vec::new();
        for (nome, plano, trfs) in &casos {
            assert!(plano.previas.is_empty(), "{nome}: com o `.trf` guardado não há prévia");
            let base = compostos(&cap, plano, "composto", trfs, &[]);
            let mut comparar = |com: &str, outros: Vec<String>| {
                let n = base.iter().zip(&outros).filter(|(a, b)| a != b).count();
                if n != 0 || outros.len() != base.len() {
                    falhas.push(format!("{nome}: {n} de {} quadros com outro hash ({com} × {})", base.len(), cap.versao));
                }
            };
            comparar("-cpuflags 0", compostos(&cap, plano, "composto-c", trfs, &["-cpuflags", "0"]));
            if let Some(outro) = &outro {
                comparar(&outro.versao, compostos(outro, plano, "composto-o", trfs, &[]));
            }
        }
        eprintln!("composto dos efeitos de origem ({}): {} casos, {} falhas; outro ffmpeg: {:?}", cap.versao, casos.len(), falhas.len(), outro.as_ref().map(|o| &o.versao));
        assert!(falhas.is_empty(), "{}", falhas.join("\n"));
    }

    /// Fonte entrelaçada com o campo de BAIXO primeiro (DV), 720×480, 60 quadros: marca vermelha de 48 px que anda
    /// 4 px por campo (pelo tempo: o `n` do `overlay` muda entre versões), em x = 40 + 4 × campo.
    pub(crate) fn fonte_bff(cap: &Capacidades, pasta: &Path) -> FonteInfo {
        fonte_bff_de(cap, pasta, false)
    }

    /// `fonte_bff`; `cheia` = gravada em faixa cheia (`yuvj420p`), como sai de muito gravador.
    fn fonte_bff_de(cap: &Capacidades, pasta: &Path, cheia: bool) -> FonteInfo {
        let lavfi = "color=c=gray:s=720x480:r=60000/1001[b];color=c=red:s=48x48:r=60000/1001[m];[b][m]overlay=x='40+4*round(t*60000/1001)':y=200:shortest=1,trim=end_frame=120,interlace=scan=bff:lowpass=0";
        let mut codificar = vec!["-c:v", "libx264", "-preset", "veryfast", "-crf", "10", "-pix_fmt", if cheia { "yuvj420p" } else { "yuv420p" }, "-bf", "0", "-g", "30", "-flags", "+ildct+ilme", "-x264-params", "bff=1"];
        if cheia {
            codificar.extend(["-color_range", "pc"]);
        }
        let v = gerar(cap, pasta, if cheia { "bff-cheia.mp4" } else { "bff.mp4" }, lavfi, &codificar);
        let f = fonte_de(cap, &v);
        assert_eq!(f.quadros.pts.len(), 60, "fonte de campos de baixo primeiro");
        assert_eq!(matches!(f.faixa, crate::edicao::modelo::Faixa::Pc), cheia, "faixa da fonte de teste");
        f
    }

    /// Trecho `[10, 40)` da fonte BFF por campo: o slot k mostra o campo 20 + k.
    pub(crate) fn projeto_bff(f: &FonteInfo, ordem: &str) -> crate::edicao::modelo::ProjetoEdicao {
        let d = efeito("desentrelacar", "d1", json!({ "modo": "campo", "ordem": ordem }));
        projeto(720, 480, NTSC_CAMPOS, vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", &f.quadros, 10, 40, 1.0, json!([d]))], json!({}))
    }

    /// Croma (V médio; fundo 128, marca perto de 240) na faixa de 4 px onde só o OUTRO campo do quadro tem a marca,
    /// nos dois primeiros quadros que `vf` deixa passar (campos 20 e 21 da fonte BFF); devolve o pior dos dois.
    pub(crate) fn croma_do_outro_campo(cap: &Capacidades, video: &Path, vf: &str) -> f64 {
        let pasta = fontes::pasta_temporaria("croma");
        let entrada = video.to_string_lossy();
        let a = ["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-i", entrada.as_ref(), "-vf", vf, "-frames:v", "2", "-f", "rawvideo", "-pix_fmt", "yuv420p", "c.yuv"].map(String::from).into();
        let cmd = comando(cap, &pasta, "croma", a);
        let fim = exec::rodar(&cmd, &AtomicBool::new(false)).expect("ffmpeg");
        exec::exigir_sucesso(&cmd, &fim).unwrap_or_else(|e| panic!("croma ({}): {e}", cap.versao));
        let d = std::fs::read(pasta.join("c.yuv")).unwrap();
        let (w, h) = (720usize, 480usize);
        let quadro = w * h * 3 / 2;
        assert_eq!(d.len(), 2 * quadro, "dois quadros 720×480 em yuv420p");
        let mut pior = 0.0f64;
        for k in 0..2 {
            let v = &d[k * quadro + w * h + (w / 2) * (h / 2)..(k + 1) * quadro];
            let a = 40 + 4 * (20 + k);
            // o campo de antes vê a marca do seguinte 4 px à direita; o de depois, a do anterior 4 px à esquerda
            let colunas = if k == 0 { [(a + 48) / 2, (a + 50) / 2] } else { [(a - 4) / 2, (a - 2) / 2] };
            let dentro: Vec<u8> = (102..122).map(|cy| v[cy * (w / 2) + (a + 24) / 2]).collect();
            assert!(dentro.iter().all(|&x| x > 220), "campo {k}: a marca não está onde o teste espera ({dentro:?})");
            let soma: u32 = (102..122).flat_map(|cy| colunas.map(|c| v[cy * (w / 2) + c] as u32)).sum();
            pior = pior.max(soma as f64 / 40.0);
        }
        pior
    }

    /// A cor por campo (`cor_da_fonte` com `desentrelacar`) não mistura o croma dos dois campos antes do `bwdif`;
    /// a comum mistura (controle). E dá os mesmos quadros sem instruções de CPU e no outro ffmpeg.
    #[test]
    fn cor_por_campo_nao_mistura_o_croma() {
        let Some(cap) = fontes::cap() else { return };
        let f = fonte_bff(&cap, &fontes::pasta_temporaria("bff"));
        let campo = efs(json!([desentrelacar("campo")]));
        let (comum, por_campo) = (cor_da_fonte(&[], &f).unwrap(), cor_da_fonte(&campo, &f).unwrap());
        assert_eq!(comum, cor::cadeia_fonte(f.matriz, f.faixa, &f.pix_fmt, f.exibido).unwrap());
        assert_eq!(por_campo, comum.replacen("scale=720:480:", "scale=720:480:interl=1:", 1));
        assert_ne!(por_campo, comum);
        // faixa cheia: o mesmo `scale`, depois da tabela
        let cheia = FonteInfo { faixa: crate::edicao::modelo::Faixa::Pc, pix_fmt: "yuvj420p".into(), ..f.clone() };
        assert!(cor_da_fonte(&campo, &cheia).unwrap().contains(",scale=720:480:interl=1:flags="));
        // desligado ou por quadro
        assert_eq!(cor_da_fonte(&efs(json!([{ "tipo": "desentrelacar", "id": "d", "ligado": false, "nome": "", "ativo": null, "modo": "campo", "ordem": "auto" }])), &f).unwrap(), comum);
        assert_eq!(cor_da_fonte(&efs(json!([desentrelacar("quadro")])), &f).unwrap(), por_campo);
        let bw = "bwdif=mode=send_field:parity=auto:deint=all,trim=start_frame=20";
        let (sem, com) = (croma_do_outro_campo(&cap, &f.caminho, &format!("{comum},{bw}")), croma_do_outro_campo(&cap, &f.caminho, &format!("{por_campo},{bw}")));
        eprintln!("croma do outro campo ({}): V {sem:.1} com a cor comum, {com:.1} com a cor por campo (fundo 128, marca ~240)", cap.versao);
        assert!(sem > 145.0, "controle ({}): a cor comum devia misturar o croma dos campos: V {sem:.1}", cap.versao);
        assert!(com < 132.0, "({}) croma do outro campo com a cor por campo: V {com:.1}", cap.versao);
        // mesmos quadros em C puro e no outro binário
        let hashes = |c: &Capacidades, globais: &[&str]| {
            let pasta = fontes::pasta_temporaria("croma-hash");
            let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-loglevel", "error"].map(String::from).into();
            a.extend(globais.iter().map(|s| s.to_string()));
            let (entrada, vf) = (f.caminho.to_string_lossy(), format!("{por_campo},{bw}"));
            a.extend(["-i", entrada.as_ref(), "-vf", vf.as_str(), "-frames:v", "40", "-f", "framehash", "-hash", "sha256", "h.txt"].map(String::from));
            let cmd = comando(c, &pasta, "hash", a);
            let fim = exec::rodar(&cmd, &AtomicBool::new(false)).expect("ffmpeg");
            exec::exigir_sucesso(&cmd, &fim).unwrap_or_else(|e| panic!("hash ({}): {e}", c.versao));
            let h: Vec<String> = std::fs::read_to_string(pasta.join("h.txt")).unwrap().lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.rsplit(',').next().map(|x| x.trim().to_string())).collect();
            assert_eq!(h.len(), 40);
            h
        };
        let base = hashes(&cap, &[]);
        assert_eq!(hashes(&cap, &["-cpuflags", "0"]), base, "({}) -cpuflags 0", cap.versao);
        if let Some(outro) = outro_ffmpeg(&cap) {
            assert_eq!(hashes(&outro, &[]), base, "{} × {}", outro.versao, cap.versao);
        }
    }

    /// O render por campo sem o croma do outro campo: depende de o motor usar `cor_da_fonte`.
    #[test]
    fn render_por_campo_sem_croma_do_outro_campo() {
        let Some(cap) = fontes::cap() else { return };
        let f = fonte_bff(&cap, &fontes::pasta_temporaria("bff"));
        let plano = Mundo::novo(&cap).com("m1", &f).compilar(&projeto_bff(&f, "auto"), &alvo_final("cpu")).expect("compila");
        let (r, _) = renderizar(&cap, &plano, "bff-croma", &[]);
        let v = croma_do_outro_campo(&cap, &r.blocos[0], "null");
        assert!(v < 132.0, "({}) croma do outro campo no render: V {v:.1}", cap.versao);
    }

    /// Fonte com o campo de BAIXO primeiro (DV): com `ordem = auto` os campos saem na ordem do tempo (a marca só
    /// avança); com a ordem errada imposta ela vai e volta (controle).
    #[test]
    fn desentrelace_por_campo_segue_a_ordem_da_fonte() {
        let Some(cap) = fontes::cap() else { return };
        let f = fonte_bff(&cap, &fontes::pasta_temporaria("bff"));
        let centros = |ordem: &str| {
            let plano = Mundo::novo(&cap).com("m1", &f).compilar(&projeto_bff(&f, ordem), &alvo_final("cpu")).unwrap_or_else(|e| panic!("bff {ordem} ({}): {e}", cap.versao));
            // `auto` vira a ordem do fluxo, escrita
            let escrita = if ordem == "auto" { "bff" } else { ordem };
            assert!(plano.blocos[0].grafo.contains(&format!("bwdif=mode=send_field:parity={escrita}:deint=all")), "{ordem} ({})", cap.versao);
            let (_, quadros) = renderizar(&cap, &plano, "bff", &[]);
            assert_eq!(quadros.len(), 60);
            quadros.iter().map(|q| marca_de(q).1).collect::<Vec<f64>>()
        };
        let passos = |c: &[f64]| c.windows(2).map(|p| p[1] - p[0]).collect::<Vec<f64>>();
        let auto = centros("auto");
        // o campo k do trecho é o campo 20 + k da cena: marca de 48 px em 40 + 4 × (20 + k). O centro pela cor sai
        // meio px para o lado do outro campo (croma misturado antes do `bwdif`: ver `cor_da_fonte`)
        for (k, x) in auto.iter().enumerate() {
            let previsto = 40.0 + 4.0 * (20 + k) as f64 + 24.0;
            assert!((x - previsto).abs() <= 1.0, "campo {k} ({}): marca em {x:.1} × {previsto:.1}", cap.versao);
        }
        assert!(passos(&auto).iter().all(|d| (d - 4.0).abs() <= 1.5), "auto ({}): {:?}", cap.versao, passos(&auto));
        assert_eq!(centros("bff"), auto, "ordem imposta igual à da fonte");
        let errada = passos(&centros("tff"));
        assert!(errada.iter().filter(|d| **d < 0.0).count() >= 25, "controle ({}): com a ordem trocada a marca devia ir e voltar: {errada:?}", cap.versao);
        eprintln!("campo de baixo primeiro ({}): 60 campos na ordem do tempo com `auto`; com `tff` imposto, {} voltas", cap.versao, errada.iter().filter(|d| **d < 0.0).count());
    }

    /// Faixa cheia com o campo de baixo primeiro: a cadeia de cor da faixa cheia (`mergeplanes`) perde a marca de
    /// ordem dos campos do quadro e o `bwdif` com `parity=auto` assumiria o de cima (medido nos dois ffmpeg). A
    /// paridade vai escrita, pela ordem do fluxo (`FonteInfo.ordem_campos`); sem ela a marca vai e volta (controle).
    #[test]
    fn desentrelace_por_campo_em_faixa_cheia_segue_a_ordem_da_fonte() {
        let Some(cap) = fontes::cap() else { return };
        let f = fonte_bff_de(&cap, &fontes::pasta_temporaria("bff-cheia"), true);
        assert_eq!(f.ordem_campos, OrdemCampos::Bff, "({}) ordem lida do fluxo", cap.versao);
        let voltas = |f: &FonteInfo, nome: &str| {
            let plano = Mundo::novo(&cap).com("m1", f).compilar(&projeto_bff(f, "auto"), &alvo_final("cpu")).expect("compila");
            let (_, quadros) = renderizar(&cap, &plano, nome, &[]);
            assert_eq!(quadros.len(), 60);
            let centros: Vec<f64> = quadros.iter().map(|q| marca_de(q).1).collect();
            (plano.blocos[0].grafo.clone(), centros.windows(2).filter(|p| p[1] < p[0]).count())
        };
        let (grafo, n) = voltas(&f, "bff-cheia");
        assert!(grafo.contains("mergeplanes") && grafo.contains("bwdif=mode=send_field:parity=bff:deint=all"), "({})", cap.versao);
        assert_eq!(n, 0, "({}) a marca vai e volta {n} vezes em 60 campos", cap.versao);
        // controle: a mesma fonte sem a ordem (o `auto` de antes) sai com os campos trocados
        let (grafo, n) = voltas(&FonteInfo { ordem_campos: OrdemCampos::Auto, ..f.clone() }, "bff-cheia-auto");
        assert!(grafo.contains("parity=auto"));
        assert!(n >= 25, "controle ({}): sem a ordem a marca devia ir e voltar: {n} voltas", cap.versao);
    }

    /// Busca que abre UM quadro depois do previsto (injetada) em prévia de clipe com leitura alargada: a seleção por
    /// índice tem folga (o vizinho lido depois, o clone do `ocultar`) e a contagem fecharia com o quadro seguinte no
    /// lugar do previsto. Com a guarda o bloco falha na conferência; refeito sem busca, dá os quadros certos.
    #[test]
    fn busca_errada_com_leitura_alargada_falha_na_conferencia() {
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        let tab = &m.info.quadros;
        // o trecho começa um quadro depois de um quadro-chave: a leitura alargada começa um quadro antes dele
        let (chave, n) = (390usize, 30usize);
        assert!(tab.chave[chave] && !tab.chave[chave - 1] && !tab.chave[chave + 1]);
        let (j_in, j_out) = (chave + 1, chave + 1 + n);
        let sem_perdas = |sem_busca: bool| Opcoes { sem_perdas: true, sem_busca, ..Opcoes::default() };
        for (nome, efeitos, com_ocultar, numerado) in [("desentrelace", json!([desentrelacar("quadro")]), false, true), ("média + ocultar", json!([media(3)]), true, false)] {
            let rotulo = format!("{nome} ({})", cap.versao);
            let ocultacoes = if com_ocultar { json!([ocultar("o1", "r1", "solido", json!({}))]) } else { json!([]) };
            let p = projeto(
                320,
                180,
                (30, 1),
                vec![midia("m1", "video", ocultacoes)],
                vec![trecho("c1", "m1", tab, j_in, j_out, 1.0, efeitos)],
                json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras_da_marca(Fonte::Mp4, &m, j_in - 4, j_out + 2))] }),
            );
            let plano = Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &janela(180, 0, n as u64)).unwrap_or_else(|e| panic!("{rotulo}: {e}"));
            assert!(plano.blocos[0].grafo.contains("select=expr="), "{rotulo}: sem a guarda no grafo");
            let certo = executor::rodar(&cap, &plano, &fontes::pasta_temporaria("guarda-certa"), &sem_perdas(false)).unwrap_or_else(|e| panic!("{rotulo}: {e}"));
            let quadros_certos = medir::quadros(&cap, &certo.blocos[0]).unwrap();
            if numerado {
                assert_eq!(medir::quadros_de_origem(&cap, &certo.blocos[0]).unwrap(), m.numeros[j_in..j_out], "{rotulo}");
            }
            // a busca cai no quadro-chave: o decodificador entrega dele em diante, sem o quadro de antes
            let mut errado = plano.clone();
            let entrada = errado.blocos[0].entradas.iter_mut().find(|e| e.antes.first().is_some_and(|a| a == "-ss")).unwrap_or_else(|| panic!("{rotulo}: o caso tem de usar busca"));
            entrada.antes[1] = format!("{:.6}", tab.tempo(chave) - m.info.inicio_s);
            match executor::rodar(&cap, &errado, &fontes::pasta_temporaria("guarda-errada"), &sem_perdas(false)) {
                Err(e) => assert!(e.to_string().contains("leitura_divergente"), "{rotulo}: {e}"),
                Ok(r) => {
                    let quadros = medir::quadros(&cap, &r.blocos[0]).unwrap();
                    let trocados = quadros.iter().zip(&quadros_certos).filter(|(a, b)| medir::pixels_diferentes(a, b, 0) > 0).count();
                    panic!("{rotulo}: a leitura sem o primeiro quadro passou na conferência com {trocados} de {n} quadros trocados");
                }
            }
            let refeito = executor::rodar(&cap, &errado, &fontes::pasta_temporaria("guarda-refeita"), &sem_perdas(true)).unwrap_or_else(|e| panic!("{rotulo}: sem busca: {e}"));
            let quadros = medir::quadros(&cap, &refeito.blocos[0]).unwrap();
            let trocados = quadros.iter().zip(&quadros_certos).filter(|(a, b)| medir::pixels_diferentes(a, b, 0) > 0).count();
            assert_eq!((quadros.len(), trocados), (n, 0), "{rotulo}: refeito sem busca");
        }
    }

    /// Análise do movimento com a busca que abre adiante (injetada): sem a guarda sairia, com código 0, um `.trf`
    /// mais curto, que estabiliza cada quadro com o movimento de outro. Com ela a análise fica sem quadro nenhum e o
    /// bloco que a usa falha; sem busca, a análise é a mesma da busca certa.
    #[test]
    fn analise_com_busca_errada_nao_da_trf_deslocado() {
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        let tab = &m.info.quadros;
        let (d0, d1) = (385usize, 430usize);
        let efeitos = json!([estabilizar(tab, d0, d1, "preta", 0.0, None)]);
        let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", tab, 391, 421, 1.0, efeitos.clone())], json!({}));
        let plano = Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &janela(180, 0, 30)).expect("compila");
        let Previa::Vidstab { args, trf, .. } = &plano.previas[0] else { panic!("prévia") };
        let ss = args.iter().position(|a| a == "-ss").expect("a análise tem de usar busca");
        let com_args = |a: Vec<String>| {
            let mut outro = plano.clone();
            if let Previa::Vidstab { args, .. } = &mut outro.previas[0] {
                *args = a;
            }
            outro
        };
        let (_, certo, _) = rodar_previas(&cap, &plano, &fontes::pasta_temporaria("analise-certa")).remove(0);
        // sem busca: a mesma análise
        let sem_busca = deteccao(&efs(efeitos.clone()), &est_de(estabilizar(tab, d0, d1, "preta", 0.0, None)), &m.info, trf, false).unwrap();
        assert!(!sem_busca.iter().any(|a| a == "-ss"));
        let (_, do_inicio, _) = rodar_previas(&cap, &com_args(sem_busca), &fontes::pasta_temporaria("analise-sem-busca")).remove(0);
        assert_eq!(do_inicio, certo, "({}) análise sem busca × com a busca da regra", cap.versao);
        // busca no quadro-chave seguinte (390): faltam os 5 primeiros quadros da análise
        let mut atrasada = args.clone();
        atrasada[ss + 1] = format!("{:.6}", tab.tempo(390) - m.info.inicio_s);
        let errado = com_args(atrasada);
        let pasta = fontes::pasta_temporaria("analise-errada");
        let (_, sha, _) = rodar_previas(&cap, &errado, &pasta).remove(0);
        assert_ne!(sha, certo);
        let r = executor::rodar(&cap, &errado, &pasta, &Opcoes { sem_perdas: true, ..Opcoes::default() });
        assert!(r.is_err(), "({}) o bloco estabilizou com a análise de uma leitura que perdeu quadros", cap.versao);
    }

    /// Exportação inteira (blocos codificados, junção, conferência) com efeito que lê quadros a mais: os pts lidos
    /// conferem e o `mapa.csv` sai com o quadro de cada slot.
    fn mapa_da_exportacao(cap: &Arc<Capacidades>, nome: &str, plano: &Plano, trfs: &[&Path]) -> std::result::Result<String, String> {
        let pasta = fontes::pasta_temporaria(nome);
        por_na_pasta(&pasta, trfs);
        let (r, res) = executor::exportar(cap, plano, &pasta, "2026-11-14T00:00:00Z").map_err(|e| format!("{nome} ({}): {e}", cap.versao))?;
        let conferir = &plano.blocos[0].conferir[0];
        assert_eq!(r.pts[0].pts, conferir.pts, "{nome}: pts lidos (`showinfo`) = os previstos na conferência");
        assert_eq!(res.conferencia.as_ref().map(|c| c.quadros), Some(plano.total_q), "{nome}: quadros do arquivo entregue");
        crate::edicao::relatorio::mapa::mapa_csv(plano, Some(&res)).map_err(|e| format!("{nome} ({}): {e}", cap.versao))
    }

    /// Linhas de slot do `mapa.csv`: (q, pts, índice).
    fn slots_do_mapa(csv: &str) -> Vec<(u64, i64, usize)> {
        csv.lines().skip(1).filter_map(|l| {
            let c: Vec<&str> = l.split(',').collect();
            Some((c[0].parse().ok()?, c[4].parse().ok()?, c[5].parse().ok()?))
        }).collect()
    }

    #[test]
    fn exportacao_com_leitura_alargada_confere_e_da_o_mapa() {
        let Some(cap) = fontes::cap() else { return };
        // média de 4 em MKV: lê 3 quadros antes do trecho; o mapa fala só dos quadros do trecho
        let m = apoio::medida(&cap, Fonte::Mkv30);
        let tab = &m.info.quadros;
        let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", tab, 40, 70, 1.0, json!([media(4)]))], json!({}));
        let plano = Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &alvo_final("cpu")).expect("compila");
        assert_eq!(plano.blocos[0].conferir[0].pts, tab.pts[37..70], "lidos: 3 antes do trecho");
        let csv = mapa_da_exportacao(&cap, "mapa-media", &plano, &[]).unwrap();
        assert_eq!(slots_do_mapa(&csv), (0..30).map(|k| (k as u64, tab.pts[40 + k], 40 + k)).collect::<Vec<_>>());
        // desentrelace por quadro: 2 antes e 1 depois
        let e = apoio::medida(&cap, Fonte::Entrelacado);
        let (j_in, j_out) = FAIXA_ENTRELACADA;
        let plano = plano_entrelacado(&cap, &e, Some("quadro"), false, NTSC, &alvo_final("cpu"));
        assert_eq!(plano.blocos[0].conferir[0].pts, e.info.quadros.pts[j_in - 2..j_out + 1]);
        let csv = mapa_da_exportacao(&cap, "mapa-quadro", &plano, &[]).unwrap();
        assert_eq!(slots_do_mapa(&csv), (0..j_out - j_in).map(|k| (k as u64, e.info.quadros.pts[j_in + k], j_in + k)).collect::<Vec<_>>());
        // estabilizar com o `.trf` guardado: lê desde o primeiro quadro analisado
        let f = tremido(&cap, &fontes::pasta_temporaria("tremido"));
        let (d0, d1) = ANALISE;
        let sem_trf = plano_tremido(&cap, &f, d0, d1 + 1, json!([estabilizar(&f.quadros, d0, d1, "preta", 0.0, None)]), json!({}), &alvo_final("cpu"));
        let (_, sha, trf) = rodar_previas(&cap, &sem_trf, &fontes::pasta_temporaria("mapa-trf")).remove(0);
        let plano = plano_tremido(&cap, &f, 50, 70, json!([estabilizar(&f.quadros, d0, d1, "preta", 0.0, Some(&sha))]), json!({}), &alvo_final("cpu"));
        assert_eq!(plano.blocos[0].conferir[0].pts, f.quadros.pts[d0..70]);
        let csv = mapa_da_exportacao(&cap, "mapa-estab", &plano, &[&trf]).unwrap();
        assert_eq!(slots_do_mapa(&csv), (0..20).map(|k| (k as u64, f.quadros.pts[50 + k], 50 + k)).collect::<Vec<_>>());
    }

    /// Por campo, o `mapa.csv` traz um campo por slot (pts do quadro e o meio até o seguinte).
    #[test]
    fn exportacao_por_campo_da_o_mapa() {
        let Some(cap) = fontes::cap() else { return };
        let e = apoio::medida(&cap, Fonte::Entrelacado);
        let (j_in, j_out) = FAIXA_ENTRELACADA;
        let plano = plano_entrelacado(&cap, &e, Some("campo"), false, NTSC_CAMPOS, &alvo_final("cpu"));
        assert_eq!(plano.blocos[0].conferir[0].pts, e.info.quadros.pts[j_in - 2..j_out + 1]);
        let campos = tabela(&efs(json!([desentrelacar("campo")])), &e.info.quadros);
        let csv = mapa_da_exportacao(&cap, "mapa-campo", &plano, &[]).unwrap();
        assert_eq!(slots_do_mapa(&csv), (0..2 * (j_out - j_in)).map(|k| (k as u64, campos.pts[2 * j_in + k], 2 * j_in + k)).collect::<Vec<_>>());
    }

    /// Lente: o `lenscorrection` do motor dá a imagem da correção radial do Realce (`quadro_lente`: `corrigir_rgb`).
    #[test]
    fn lente_confere_com_quadro_lente() {
        use crate::image_processing::lens_correction::corrigir_rgb;
        let Some(cap) = fontes::cap() else { return };
        let f = tremido(&cap, &fontes::pasta_temporaria("tremido"));
        let (_, cru) = renderizar(&cap, &plano_tremido(&cap, &f, 10, 13, json!([]), json!({}), &alvo_final("cpu")), "lente-cru", &[]);
        for (k1, k2, cx, cy) in [(-0.22, 0.04, 0.5, 0.5), (0.12, 0.0, 0.45, 0.55)] {
            let lente = efeito("lente", "l1", json!({ "k1": k1, "k2": k2, "cx": cx, "cy": cy, "perfil": "" }));
            let plano = plano_tremido(&cap, &f, 10, 13, json!([lente]), json!({}), &alvo_final("cpu"));
            assert!(plano.blocos[0].grafo.contains("lenscorrection=cx="), "{}", plano.blocos[0].grafo);
            let (_, quadros) = renderizar(&cap, &plano, "lente", &[]);
            for (q, c) in quadros.iter().zip(&cru) {
                let coef = LensCoefficients { k1: k1 as f32, k2: k2 as f32, k3: 0.0 };
                let previsto = QuadroRgb { w: c.w, h: c.h, px: corrigir_rgb(&c.px, c.w, c.h, coef, cx as f32, cy as f32) };
                // a lente mexeu na imagem, e do jeito previsto: erro médio no miolo e posição do alvo
                // só onde a origem do pixel cai dentro do quadro: fora, o motor pinta de preto e o Realce repete a borda
                let rms = |a: &QuadroRgb, b: &QuadroRgb| {
                    let (mut s, mut n) = (0.0, 0u64);
                    for y in 24..a.h - 24 {
                        for x in 24..a.w - 24 {
                            let (xs, ys) = crate::image_processing::lens_correction::origem(coef, a.w, a.h, cx as f32, cy as f32, x as f32, y as f32);
                            if xs < 2.0 || ys < 2.0 || xs > (a.w - 3) as f32 || ys > (a.h - 3) as f32 {
                                continue;
                            }
                            let i = ((y * a.w + x) * 3) as usize;
                            for k in 0..3 {
                                s += (a.px[i + k] as f64 - b.px[i + k] as f64).powi(2);
                            }
                            n += 3;
                        }
                    }
                    assert!(n > (a.w * a.h) as u64, "miolo medido pequeno demais");
                    (s / n as f64).sqrt()
                };
                let (erro, sem_lente) = (rms(q, &previsto), rms(q, c));
                assert!(erro < 9.0 && sem_lente > 3.0 * erro, "lente {k1} {k2} {cx} {cy} ({}): rms {erro:.2} contra o Realce, {sem_lente:.2} contra o quadro sem lente", cap.versao);
                let ((_, a), (_, b)) = (caixa_da_cor(q, magenta).expect("alvo"), caixa_da_cor(&previsto, magenta).expect("alvo previsto"));
                // pela cor: o croma 4:2:0 do motor tem metade da resolução
                assert!((a.0 - b.0).abs() <= 1.5 && (a.1 - b.1).abs() <= 1.5, "lente {k1} {k2} ({}): alvo em {a:?} × {b:?}", cap.versao);
                eprintln!("lente {k1} {k2} {cx} {cy} ({}): rms {erro:.2} contra o Realce ({sem_lente:.2} sem lente); alvo em ({:.1}, {:.1}) × ({:.1}, {:.1})", cap.versao, a.0, a.1, b.0, b.1);
            }
        }
    }
}
