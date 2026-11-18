//! Marcações e processamentos em texto legível: o item do roteiro diz o que foi posto ("3 setas: PP1 (vermelha)…") e o
//! anexo C traz uma linha por processamento, com os valores para refazer. O detalhe inteiro fica no `tecnico.json`.
//! Os nomes dos valores do modelo vêm do catálogo (`frases`); os termos técnicos do anexo C ficam aqui, como os do
//! anexo E em `juri.rs`.

use serde::Serialize;
use serde_json::{Map, Value};

use super::frases::{self, modelo, CORES, FORMAS_DESTAQUE};
use super::juri::{dec, num, rot, rot_ou, tempo};

/// Teto da linha de um processamento que o relatório não conhece (o inteiro está no `tecnico.json` anexado).
const MAX_GENERICO: usize = 800;
/// Chaves animadas listadas no anexo C; as outras só contadas.
const MAX_CHAVES: usize = 8;
/// Espaço que não quebra entre o número e a unidade ("0,3 %").
const NB: char = '\u{a0}';

/// Uma marcação no item do roteiro: rótulo e nome da cor; `cor` = "#rrggbb" para a amostra (None = sem cor legível).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct Marca {
    pub texto: String,
    pub cor: Option<String>,
}

/// "#rrggbb" ou "#rrggbbaa" (o alfa sai) em minúsculas; outra coisa = None.
pub(super) fn hex(s: &str) -> Option<String> {
    let h = s.trim().strip_prefix('#')?;
    ((h.len() == 6 || h.len() == 8) && h.chars().all(|c| c.is_ascii_hexdigit())).then(|| format!("#{}", h[..6].to_ascii_lowercase()))
}

fn rgb(hex: &str) -> Option<[u8; 3]> {
    let h = hex.strip_prefix('#')?;
    let b = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}

/// sRGB (D65) → CIE Lab.
fn lab(c: [u8; 3]) -> [f64; 3] {
    let lin = |u: u8| {
        let c = u as f64 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (lin(c[0]), lin(c[1]), lin(c[2]));
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
    let f = |t: f64| if t > 0.008856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// Nome da cor do catálogo mais próxima (masculino, feminino); None = não é cor "#rrggbb".
pub(super) fn nome_da_cor(cor: &str) -> Option<(&'static str, &'static str)> {
    let alvo = lab(rgb(&hex(cor)?)?);
    let dist = |c: [u8; 3]| lab(c).iter().zip(alvo).map(|(a, b)| (a - b).powi(2)).sum::<f64>();
    CORES.iter().min_by(|a, b| dist(a.2).total_cmp(&dist(b.2))).map(|c| (c.0, c.1))
}

/// "amarela (#ffd84a)"; fora do formato, como veio.
fn cor_txt(cor: &str, feminino: bool) -> String {
    match (nome_da_cor(cor), hex(cor)) {
        (Some((m, f)), Some(h)) => format!("{} ({h})", if feminino { f } else { m }),
        _ => cor.trim().to_string(),
    }
}

/// (singular, plural, feminino) da forma da marcação; fora do catálogo, "marcação".
fn forma(f: &str) -> (&'static str, &'static str, bool) {
    FORMAS_DESTAQUE.iter().find(|x| x.0 == f).map(|x| (x.1, x.2, x.3)).unwrap_or(("marcação", "marcações", true))
}

/// Plural da forma, para a linha da seção 5 ("Marcação (setas e retângulos)"); None = fora do catálogo.
pub(super) fn plural_da_forma(f: &str) -> Option<&'static str> {
    FORMAS_DESTAQUE.iter().find(|x| x.0 == f).map(|x| x.2)
}

/// "a", "a e b", "a, b e c".
pub(super) fn lista_e(v: &[String]) -> String {
    match v {
        [] => String::new(),
        [a] => a.clone(),
        [resto @ .., ultimo] => format!("{} e {ultimo}", resto.join(", ")),
    }
}

fn maiuscula(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|p| p.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// Texto do perito em uma linha.
fn uma_linha(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Destaques de um item, um grupo por forma na ordem em que aparecem: ("3 setas", marcas). Marca igual repetida
/// (mesmo rótulo e mesmo nome de cor, ainda que em tons diferentes) sai uma vez, com a conta; o tom exato fica no
/// anexo C.
pub(super) fn destaques(ps: &[&Value]) -> Vec<(String, Vec<Marca>)> {
    let mut grupos: Vec<(&str, usize, Vec<(Marca, usize)>)> = Vec::new();
    for p in ps {
        let f = p["forma"].as_str().unwrap_or("");
        let (_, _, feminino) = forma(f);
        let rotulo = p["rotulo"]["texto"].as_str().map(uma_linha).filter(|r| !r.is_empty());
        let cor = p["traco"]["cor"].as_str().and_then(hex);
        let nome = cor.as_deref().and_then(nome_da_cor).map(|(m, fe)| if feminino { fe } else { m });
        let rotulo = rotulo.unwrap_or_else(|| rot("marca.sem_rotulo").to_string());
        let texto = match nome {
            Some(n) => frases::preencher(rot("marca.item"), &[("rotulo", &rotulo), ("cor", n)]),
            None => rotulo,
        };
        let marca = Marca { texto, cor };
        let i = match grupos.iter().position(|g| g.0 == f) {
            Some(i) => i,
            None => {
                grupos.push((f, 0, Vec::new()));
                grupos.len() - 1
            }
        };
        let g = &mut grupos[i];
        g.1 += 1;
        match g.2.iter_mut().find(|(m, _)| m.texto == marca.texto) {
            Some((_, n)) => *n += 1,
            None => g.2.push((marca, 1)),
        }
    }
    grupos
        .into_iter()
        .map(|(f, n, marcas)| {
            let (um, varios, _) = forma(f);
            let grupo = frases::preencher(rot("marca.grupo"), &[("n", &n.to_string()), ("forma", if n == 1 { um } else { varios })]);
            let marcas = marcas
                .into_iter()
                .map(|(m, k)| if k == 1 { m } else { Marca { texto: frases::preencher(rot("marca.repetida"), &[("marca", &m.texto), ("n", &k.to_string())]), ..m } })
                .collect();
            (grupo, marcas)
        })
        .collect()
}

/// "Lupa (aumento de 6×)" · "2 lupas (aumento de 6× e 4×)".
pub(super) fn lupas(ps: &[&Value]) -> String {
    let mut fatores: Vec<String> = Vec::new();
    for p in ps {
        if let Some(f) = p["fator"].as_f64().map(|f| format!("{}×", num(f, 2))) {
            if !fatores.contains(&f) {
                fatores.push(f);
            }
        }
    }
    let base = frases::rotulo_do_efeito("lupa").unwrap_or("Lupa").to_string();
    match (ps.len(), fatores.is_empty()) {
        (1, false) => frases::preencher(rot("lupa.uma"), &[("f", fatores[0].trim_end_matches('×'))]),
        (1, true) => base,
        (n, false) => frases::preencher(rot("lupa.varias"), &[("n", &n.to_string()), ("f", &lista_e(&fatores))]),
        (n, true) => format!("{n} {}", base.to_lowercase()),
    }
}

/// "Holofote" · "2 holofotes".
pub(super) fn holofotes(n: usize) -> String {
    if n == 1 {
        frases::rotulo_do_efeito("holofote").unwrap_or("Holofote").to_string()
    } else {
        frases::preencher(rot("holofote.varios"), &[("n", &n.to_string())])
    }
}

// ---------------------------------------------------------------------------
// Anexo C

/// 0,004 → "0,4 %".
fn pct(v: f64) -> String {
    format!("{}{NB}%", num(v * 100.0, 2))
}

fn f(p: &Value, k: &str) -> Option<f64> {
    p[k].as_f64()
}

fn s<'a>(p: &'a Value, k: &str) -> &'a str {
    p[k].as_str().unwrap_or("")
}

/// Rótulo do valor do modelo; valor ausente = nada (o campo some da linha).
fn rv(grupo: &str, valor: &str) -> String {
    if valor.trim().is_empty() {
        String::new()
    } else {
        rot_ou(grupo, valor)
    }
}

/// "prefixo valor", só com valor.
fn com(prefixo: &str, valor: String) -> Option<String> {
    (!valor.trim().is_empty()).then(|| format!("{prefixo}{valor}"))
}

/// As partes que existem, separadas por `sep`.
fn juntar(partes: Vec<String>, sep: &str) -> String {
    partes.into_iter().filter(|x| !x.trim().is_empty()).collect::<Vec<_>>().join(sep)
}

/// "x 20 %, y 50 %" (posição no quadro, 0..1).
fn ponto(p: &Value) -> Option<String> {
    Some(format!("x {}, y {}", pct(p["x"].as_f64()?), pct(p["y"].as_f64()?)))
}

/// Traço: "amarela (#ffd84a), espessura 0,4 %, com contorno".
fn traco(t: &Value, feminino: bool) -> String {
    let mut v = vec![cor_txt(s(t, "cor"), feminino)];
    v.extend(f(t, "espessura").map(|e| format!("espessura {}", pct(e))));
    if let Some(c) = t["contorno"].as_bool() {
        v.push(if c { "com contorno" } else { "sem contorno" }.to_string());
    }
    juntar(v, ", ")
}

/// Estilo de texto: "letra 3,3 %, amarela (#ffd84a), negrito, contorno preto (#000000) 0,3 %, caixa …".
fn estilo(e: &Value) -> String {
    let mut v = Vec::new();
    v.extend(f(e, "tamanho").map(|t| format!("letra {}", pct(t))));
    if !s(e, "cor").is_empty() {
        v.push(cor_txt(s(e, "cor"), true));
    }
    if !s(e, "fonte").is_empty() && s(e, "fonte") != "sans" {
        v.push(rot_ou("fonte", s(e, "fonte")));
    }
    if e["negrito"] == true {
        v.push("negrito".into());
    }
    if e["italico"] == true {
        v.push("itálico".into());
    }
    if e["contorno"].is_object() {
        let c = &e["contorno"];
        v.push(format!("contorno {}{}", cor_txt(s(c, "cor"), false), f(c, "largura").map(|l| format!(" {}", pct(l))).unwrap_or_default()));
    }
    if e["caixa"].is_object() {
        // Fase 4 (texto livre): a faixa vai de uma borda à outra do bloco
        let nome = if e["caixa"]["faixa"] == true { "faixa" } else { "caixa" };
        v.push(format!("{nome} {}", cor_txt(s(&e["caixa"], "cor"), true)));
    }
    if e["sombra"].is_object() {
        let sb = &e["sombra"];
        v.push(format!("sombra {}{}", cor_txt(s(sb, "cor"), true), f(sb, "desfoque").filter(|d| *d > 0.0).map(|d| format!(" desfocada {}", pct(d))).unwrap_or_default()));
    }
    v.join(", ")
}

/// Região de ocultação ou holofote (`AlvoRegiao`): "elipse, invertida, margem 5 %, borda suave 2 %".
fn regiao(a: &Value) -> String {
    let mut v = vec![rv("forma", s(a, "forma"))];
    if a["inverter"] == true {
        v.push(rot("forma.invertida").to_string());
    }
    if let Some(m) = f(a, "margem").filter(|m| *m > 0.0) {
        v.push(format!("margem {}", pct(m)));
    }
    if let Some(b) = f(a, "borda_suave").filter(|b| *b > 0.0) {
        v.push(format!("borda suave {}", pct(b)));
    }
    if let Some(n) = a["vertices"].as_array() {
        v.push(format!("{} vértices", n.len()));
    }
    juntar(v, ", ")
}

/// Caixa da região do enquadramento: "x 10 %, y 0 %, largura 50 %, altura 50 %".
fn caixa(c: &Value) -> Option<String> {
    Some(format!("x {}, y {}, largura {}, altura {}", pct(f(c, "x")?), pct(f(c, "y")?), pct(f(c, "w")?), pct(f(c, "h")?)))
}

/// "largura 50 %, altura 50 %" (região cujo centro vem de outro lugar).
fn tamanho(c: &Value) -> Option<String> {
    Some(format!("largura {}, altura {}", pct(f(c, "w")?), pct(f(c, "h")?)))
}

/// "segue o rastreio R4", pelo `ref_rastreio` de quem monta os dados.
fn segue(id: &str, ref_rastreio: &dyn Fn(&str) -> String) -> Option<String> {
    (!id.trim().is_empty()).then(|| format!("segue o rastreio {}", ref_rastreio(id)))
}

/// `ativo` (base de tempo do clipe): "ativo até 9:01,845 da gravação"; ausente = o clipe inteiro (nada a dizer).
fn ativo(p: &Value, base: &str) -> Option<String> {
    let a = p["ativo"].as_object()?;
    let (de, ate) = (a.get("de").and_then(Value::as_f64), a.get("ate").and_then(Value::as_f64));
    match (de, ate) {
        (None, None) => None,
        (Some(d), None) => Some(format!("ativo a partir de {} {base}", tempo(d))),
        (None, Some(t)) => Some(format!("ativo até {} {base}", tempo(t))),
        (Some(d), Some(t)) => Some(format!("ativo de {} a {} {base}", tempo(d), tempo(t))),
    }
}

/// Uma linha por processamento do anexo C: os valores que refazem o efeito. `ref_rastreio` diz como citar um
/// rastreio ("R4"); `base` = de que tempo é o `ativo` ("da gravação", "do clipe").
pub(super) fn parametros(nome: &str, p: &Value, ref_rastreio: &dyn Fn(&str) -> String, base: &str) -> String {
    let mut v: Vec<String> = Vec::new();
    match nome {
        "destaque" => {
            let (um, _, feminino) = forma(s(p, "forma"));
            v.push(juntar(vec![um.to_string(), traco(&p["traco"], feminino)], " "));
            let fm = s(p, "forma");
            if matches!(fm, "seta" | "circulo") {
                v.extend(f(p, "tamanho").map(|t| format!("tamanho {}", pct(t))));
            }
            if fm == "seta" {
                v.extend(f(p, "angulo_graus").map(|a| format!("direção {}°", num(a, 1))));
            }
            if !matches!(fm, "seta" | "circulo") || f(p, "margem").is_some_and(|m| m > 0.0) {
                v.extend(f(p, "margem").map(|m| format!("margem {}", pct(m))));
            }
            let r = &p["rotulo"];
            if r.is_object() {
                let texto = uma_linha(s(r, "texto"));
                v.push(juntar(vec![juntar(vec![format!("rótulo «{texto}»"), rv("lado", s(r, "lado"))], " "), estilo(&r["estilo"])], ", "));
            } else {
                v.push("sem rótulo".into());
            }
            v.extend(segue(s(p, "rastreio"), ref_rastreio));
        }
        "lupa" => {
            v.push(format!("lupa {}", if s(p, "forma") == "circulo" { "circular" } else { "retangular" }));
            v.extend(f(p, "fator").map(|x| format!("aumento {}×", num(x, 2))));
            v.extend(f(p, "janela").map(|j| format!("janela {} da altura da gravação", pct(j))));
            v.extend(com("interpolação ", rv("interpolacao", s(p, "interpolacao"))));
            let o = &p["onde"];
            match s(o, "modo") {
                "fixa" => v.extend(ponto(o).map(|x| format!("posição fixa ({x})"))),
                "segue" => v.push(format!("acompanha o alvo, deslocada x {}, y {}", pct(f(o, "dx").unwrap_or(0.0)), pct(f(o, "dy").unwrap_or(0.0)))),
                _ => {}
            }
            v.extend(com("moldura ", traco(&p["moldura"], true)));
            v.push(if p["ligacao"] == true { "com linha de ligação" } else { "sem linha de ligação" }.into());
            v.extend(segue(s(p, "rastreio"), ref_rastreio));
        }
        "enquadramento" => {
            v.push(rv("ajuste", s(p, "ajuste")));
            v.extend(com("interpolação ", rv("interpolacao", s(p, "interpolacao"))));
            // Fase 4 (zoom que segue): o centro vem do rastreio; da região valem só largura e altura
            let segue_id = p["segue"]["rastreio"].as_str().filter(|x| !x.trim().is_empty());
            let regiao = |c: &Value| if segue_id.is_some() { tamanho(c) } else { caixa(c) };
            match &p["regiao"] {
                Value::Array(chaves) => {
                    let mut lista: Vec<String> = chaves
                        .iter()
                        .take(MAX_CHAVES)
                        .map(|k| format!("{}: {}", k["t"].as_f64().map(tempo).unwrap_or_else(|| "—".into()), regiao(&k["v"]).unwrap_or_else(|| "—".into())))
                        .collect();
                    if chaves.len() > MAX_CHAVES {
                        lista.push(format!("e mais {}", chaves.len() - MAX_CHAVES));
                    }
                    v.push(format!("região animada em {} chaves (tempos {base}: {})", chaves.len(), lista.join("; ")));
                }
                fixa if segue_id.is_some() => v.extend(tamanho(fixa).map(|c| format!("região de {c}"))),
                fixa => v.extend(caixa(fixa).map(|c| format!("região fixa: {c}"))),
            }
            if let Some(id) = segue_id {
                let r = ref_rastreio(id);
                v.push(match f(&p["segue"], "suavizar_s").filter(|x| *x > 0.0) {
                    Some(x) => frases::preencher(modelo::ACOMPANHANDO, &[("r", &r), ("s", &num(x, 2))]),
                    None => frases::preencher(modelo::ACOMPANHANDO_SEM_MEDIA, &[("r", &r)]),
                });
            }
        }
        "ocultar" => {
            v.push(rv("modo", s(p, "modo")));
            v.push(rv("conteudo", s(p, "conteudo")));
            if !s(p, "motivo").trim().is_empty() {
                v.push(format!("motivo «{}»", uma_linha(s(p, "motivo"))));
            }
            v.push(regiao(&p["alvo"]));
            if s(p, "modo") == "solido" {
                v.extend(com("cor ", s(p, "cor").to_string()));
            } else {
                v.extend(f(p, "intensidade").map(|i| format!("intensidade {}", num(i, 2))));
            }
            v.extend(segue(s(&p["alvo"], "rastreio"), ref_rastreio));
        }
        "holofote" => {
            v.extend(f(p, "escurecer").map(|x| format!("escurece {}", pct(x))));
            v.extend(f(p, "dessaturar").map(|x| format!("tira {} da cor", pct(x))));
            v.extend(com("fora da região: ", regiao(&p["alvo"])));
            v.extend(segue(s(&p["alvo"], "rastreio"), ref_rastreio));
        }
        "media_temporal" => v.extend(p["quadros"].as_u64().map(|n| format!("média de {n} imagens seguidas"))),
        "ajustes" => {
            for (k, r) in [("brilho", "brilho"), ("contraste", "contraste"), ("gama", "gama"), ("saturacao", "saturação")] {
                v.extend(f(p, k).map(|x| format!("{r} {}", num(x, 3))));
            }
        }
        "nitidez" => v.extend(f(p, "quantidade").map(|x| format!("quantidade {}", num(x, 3)))),
        "girar" => {
            v.extend(p["graus"].as_u64().map(|g| format!("giro de {g}°")));
            if p["espelhar_h"] == true {
                v.push("espelhada na horizontal".into());
            }
            if p["espelhar_v"] == true {
                v.push("espelhada na vertical".into());
            }
        }
        "estabilizar" => {
            v.extend(p["suavizar"].as_u64().map(|n| format!("suavização de {n} quadros")));
            v.push(rv("borda", s(p, "borda")));
            v.extend(f(p, "zoom_pct").filter(|z| *z > 0.0).map(|z| format!("ampliação de {}{NB}%", num(z, 2))));
            let a = &p["analise"];
            if let (Some(de), Some(ate)) = (f(a, "de_s"), f(a, "ate_s")) {
                v.push(format!("análise de {} a {} da gravação (pts {} a {})", tempo(de), tempo(ate), a["de_pts"], a["ate_pts"]));
            }
            v.extend(p["trf_sha256"].as_str().map(|h| format!("análise SHA-256 {h}")));
        }
        "lente" => {
            v.extend(f(p, "k1").map(|x| format!("k1 {}", num(x, 4))));
            v.extend(f(p, "k2").map(|x| format!("k2 {}", num(x, 4))));
            if let (Some(x), Some(y)) = (f(p, "cx"), f(p, "cy")) {
                v.push(format!("centro x {}, y {}", pct(x), pct(y)));
            }
            if !s(p, "perfil").trim().is_empty() {
                v.push(format!("perfil «{}»", uma_linha(s(p, "perfil"))));
            }
        }
        "desentrelacar" => {
            v.push(rv("desentrelace", s(p, "modo")));
            v.push(rv("campos", s(p, "ordem")));
        }
        "regua" => {
            v.extend(f(p, "comprimento_m").map(|m| format!("comprimento {}{NB}m", dec(m, 2))));
            v.extend(p["divisoes"].as_u64().map(|n| format!("{n} divisões")));
            v.extend(f(p, "direcao_graus").map(|a| format!("direção {}°", num(a, 1))));
            v.extend(ponto(&p["ancora"]).map(|x| format!("começa em {x}")));
            v.extend(com("traço ", traco(&p["traco"], false)));
            v.push(if p["rotulo"] == true { "com rótulo" } else { "sem rótulo" }.into());
            let c = &p["calibracao"];
            let mut cal = vec![rv("calibracao", s(c, "metodo"))];
            cal.extend(f(c, "residuo_m").map(|r| format!("erro médio {}{NB}m", dec(r, 3))));
            cal.extend(c["pontos_sha256"].as_str().map(|h| format!("pontos SHA-256 {h}")));
            v.extend(com("calibração: ", juntar(cal, ", ")));
        }
        "velocidade_exibida" => {
            let c = &p["calculo"];
            v.extend(f(c, "kmh").map(|k| format!("{}{NB}km/h", num(k, 1))));
            if let (Some(a), Some(b)) = (c["faixa"][0].as_f64(), c["faixa"][1].as_f64()) {
                v.push(format!("faixa de {} a {}{NB}km/h", num(a, 1), num(b, 1)));
            }
            if let (Some(de), Some(ate)) = (f(c, "de_s"), f(c, "ate_s")) {
                v.push(format!("medida de {} a {} da gravação", tempo(de), tempo(ate)));
            }
            if !s(c, "metodo").is_empty() {
                v.push(format!("método {}", s(c, "metodo")));
            }
            let o = &p["onde"];
            match s(o, "modo") {
                "fixa" => v.extend(ponto(&o["lugar"]).map(|x| format!("posição fixa ({x})"))),
                "segue" => v.extend(segue(s(o, "rastreio"), ref_rastreio)),
                _ => {}
            }
            v.push(estilo(&p["estilo"]));
        }
        "texto" => {
            v.push(format!("«{}»", uma_linha(s(p, "conteudo"))));
            v.push(estilo(&p["estilo"]));
            v.extend(ponto(&p["lugar"]).map(|x| format!("posição {x}")));
            v.extend(f(p, "largura_max").map(|l| format!("largura máxima {} da largura", pct(l))));
        }
        "carimbo" => {
            v.extend(com("mostra o ", rv("mostra", s(p, "mostra"))));
            v.extend(com("formato ", rv("formato_carimbo", s(p, "formato"))));
            v.push(estilo(&p["estilo"]));
            v.extend(ponto(&p["lugar"]).map(|x| format!("posição {x}")));
        }
        "imagem" => {
            if !s(p, "midia").is_empty() {
                v.push(format!("imagem {}", s(p, "midia").chars().take(8).collect::<String>()));
            }
            v.extend(f(p, "largura").map(|l| format!("largura {} da largura", pct(l))));
            v.extend(f(p, "opacidade").map(|o| format!("opacidade {}", pct(o))));
            v.extend(ponto(&p["lugar"]).map(|x| format!("posição {x}")));
        }
        _ => return generico(p),
    }
    v.extend(ativo(p, base));
    let v: Vec<String> = v.into_iter().filter(|x| !x.trim().is_empty()).collect();
    if v.is_empty() {
        return "—".into();
    }
    maiuscula(&v.join(" · "))
}

/// Processamento que o relatório não conhece (versão mais nova): `chave valor · …`, aninhados entre parênteses,
/// com espaço depois de cada vírgula (a linha quebra), cortado no teto.
pub(super) fn generico(p: &Value) -> String {
    fn valor(v: &Value) -> String {
        match v {
            Value::Null => "—".into(),
            Value::Bool(b) => if *b { "sim" } else { "não" }.into(),
            Value::Number(n) => n.as_f64().map(|x| num(x, 4)).unwrap_or_else(|| n.to_string()),
            Value::String(s) => uma_linha(s),
            Value::Array(a) => format!("[{}]", a.iter().map(valor).collect::<Vec<_>>().join(", ")),
            Value::Object(o) => format!("({})", pares(o, ", ")),
        }
    }
    fn pares(o: &Map<String, Value>, sep: &str) -> String {
        o.iter().filter(|(k, _)| !matches!(k.as_str(), "tipo" | "id" | "ligado" | "nome")).map(|(k, v)| format!("{k} {}", valor(v))).collect::<Vec<_>>().join(sep)
    }
    let Some(o) = p.as_object() else { return "—".into() };
    let t = pares(o, " · ");
    if t.is_empty() {
        return "—".into();
    }
    if t.chars().count() <= MAX_GENERICO {
        return t;
    }
    let mut c: String = t.chars().take(MAX_GENERICO).collect();
    c.push('…');
    c
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    #[test]
    fn nome_da_cor_mais_proxima() {
        let casos = [
            ("#ffd84a", "amarela"),
            ("#dc2626", "vermelha"),
            ("#ed333b", "vermelha"),
            ("#33d17a", "verde"),
            ("#1a5fb4", "azul"),
            ("#0000ff", "azul"),
            ("#613583", "roxa"),
            ("#f6f5f4", "branca"),
            ("#FFFFFFFF", "branca"),
            ("#000000", "preta"),
            ("#ff7800", "laranja"),
            ("#99c1f1", "azul-clara"),
            ("#00ffff", "azul-clara"),
            ("#ff00ff", "rosa"),
            ("#865e3c", "marrom"),
            ("#9a9996", "cinza"),
        ];
        for (h, nome) in casos {
            assert_eq!(nome_da_cor(h).map(|x| x.1), Some(nome), "{h}");
        }
        assert_eq!((nome_da_cor("vermelho"), nome_da_cor("#12345"), hex("#ABCDEF80")), (None, None, Some("#abcdef".into())));
        assert_eq!(nome_da_cor("#ffd84a").map(|x| x.0), Some("amarelo"));
    }

    #[test]
    fn destaques_por_forma_com_rotulo_e_cor() {
        let seta = |rotulo: Option<&str>, cor: &str| {
            let mut v = json!({ "forma": "seta", "traco": { "cor": cor, "espessura": 0.002, "contorno": true }, "tamanho": 0.02, "angulo_graus": 90.0, "rastreio": "r" });
            if let Some(r) = rotulo {
                v["rotulo"] = json!({ "texto": r, "lado": "cima", "estilo": {} });
            }
            v
        };
        let ret = json!({ "forma": "retangulo", "traco": { "cor": "#ffd84a" }, "rastreio": "r" });
        // o segundo PP1 é outro tom de vermelho: para quem lê, a mesma marca
        let ps = [seta(Some("PP1"), "#ed333b"), seta(Some(" PP2\n"), "#33d17a"), ret.clone(), seta(None, "#1a5fb4"), seta(Some("PP1"), "#dc2626"), json!({ "forma": "seta" })];
        let refs: Vec<&Value> = ps.iter().collect();
        let g = destaques(&refs);
        let m = |t: &str, c: Option<&str>| Marca { texto: t.into(), cor: c.map(str::to_string) };
        assert_eq!(g, vec![
            ("5 setas".to_string(), vec![m("PP1 (vermelha) ×2", Some("#ed333b")), m("PP2 (verde)", Some("#33d17a")), m("sem rótulo (azul)", Some("#1a5fb4")), m("sem rótulo", None)]),
            ("1 retângulo".to_string(), vec![m("sem rótulo (amarelo)", Some("#ffd84a"))]),
        ]);
        let l = [json!({ "fator": 6.0 }), json!({ "fator": 2.5 })];
        assert_eq!((lupas(&[&l[0]]), lupas(&[&l[0], &l[1]]), lupas(&[&json!({})])), ("Lupa (aumento de 6×)".into(), "2 lupas (aumento de 6× e 2,5×)".into(), "Lupa".into()));
        assert_eq!((holofotes(1), holofotes(3)), ("Holofote".into(), "3 holofotes".into()));
        assert_eq!(lista_e(&["a".into(), "b".into(), "c".into()]), "a, b e c");
    }

    /// A linha do anexo C: legível, com os valores que refazem o efeito, sem JSON.
    #[test]
    fn linha_do_anexo_c() {
        // o número e a unidade ficam juntos na quebra de linha
        let linha = |nome: &str, p: &Value, base: &str| {
            let r = |id: &str| if id == "r4" { "R4".to_string() } else { format!("{id} (marcação do perito)") };
            let t = parametros(nome, p, &r, base);
            assert!(!t.contains(" %"), "{t}");
            t.replace(NB, " ")
        };
        let r = |id: &str| if id == "r4" { "R4".to_string() } else { format!("{id} (marcação do perito)") };
        let destaque = json!({ "tipo": "destaque", "id": "e", "ligado": true, "nome": "", "ativo": null, "forma": "seta", "rastreio": "r4", "tamanho": 0.02, "angulo_graus": 90.0, "margem": 0.0,
            "traco": { "contorno": true, "cor": "#ffd84a", "espessura": 0.002 },
            "rotulo": { "texto": "PP3", "lado": "cima", "estilo": { "alinhamento": "centro", "caixa": null, "contorno": { "cor": "#000000", "largura": 0.003 }, "cor": "#ffd84a", "fonte": "sans", "italico": false, "negrito": false, "tamanho": 0.033 } } });
        assert_eq!(
            linha("destaque", &destaque, "da gravação"),
            "Seta amarela (#ffd84a), espessura 0,2 %, com contorno · tamanho 2 % · direção 90° · rótulo «PP3» acima, letra 3,3 %, amarela (#ffd84a), contorno preto (#000000) 0,3 % · segue o rastreio R4"
        );
        let lupa = json!({ "ativo": { "ate": 541.8453333333333, "de": null }, "fator": 6.0, "forma": "retangulo", "interpolacao": "bicubica", "janela": 0.17, "ligacao": true,
            "moldura": { "contorno": false, "cor": "#ffd84a", "espessura": 0.001 }, "onde": { "modo": "fixa", "x": 0.2, "y": 0.5 }, "rastreio": "x1" });
        assert_eq!(
            linha("lupa", &lupa, "da gravação"),
            "Lupa retangular · aumento 6× · janela 17 % da altura da gravação · interpolação bicúbica · posição fixa (x 20 %, y 50 %) · moldura amarela (#ffd84a), espessura 0,1 %, sem contorno · com linha de ligação · segue o rastreio x1 (marcação do perito) · ativo até 9:01,845 da gravação"
        );
        let q = json!({ "ajuste": "cobrir", "interpolacao": "bicubica", "regiao": [{ "interp": "linear", "t": 719.8356, "v": { "h": 1.0, "w": 0.998, "x": 0.001, "y": 0.0 } }, { "t": 729.37, "v": { "h": 0.186, "w": 0.188, "x": 0.178, "y": 0.0 } }] });
        assert_eq!(
            linha("enquadramento", &q, "da gravação"),
            "Preenche a tela · interpolação bicúbica · região animada em 2 chaves (tempos da gravação: 11:59,836: x 0,1 %, y 0 %, largura 99,8 %, altura 100 %; 12:09,370: x 17,8 %, y 0 %, largura 18,8 %, altura 18,6 %)"
        );
        assert_eq!(linha("media_temporal", &json!({ "quadros": 8 }), ""), "Média de 8 imagens seguidas");
        // desconhecido: legível, sem aspas nem chaves de JSON
        let g = parametros("futuro", &json!({ "tipo": "futuro", "id": "e1", "ligado": true, "nome": "n", "b": 1.5, "a": "t /x", "c": null, "d": { "k": [1, 2] } }), &r, "");
        assert_eq!(g, "a t /x · b 1,5 · c — · d (k [1, 2])");
        assert_eq!(generico(&json!({ "regiao": "x".repeat(1000) })).chars().count(), MAX_GENERICO + 1);
    }
}
