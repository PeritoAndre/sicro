//! Aceite do P4.R (arquitetura §11, Fase 4): as contagens do PDF conferem com o projeto, num projeto com tudo da
//! Fase 4 que o motor já compila (rampa, zoom com chaves, texto livre com sombra, faixa e entrada animada, sugestões
//! de detecção confirmadas, marcações do perito e do "Simplificar", cópia de gravador com relógio e taxa suposta).
//! Cada número esperado sai do PROJETO (e do denso gravado à mão), não do `feito`: o motor, o `tecnico.json`, os
//! dados e o PDF ficam no meio. O zoom que segue entra quando o pacote dele for juntado (teste `#[ignore]`).

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::apoio::{self, Cena};
use super::frases::{self, modelo};
use super::juri::{dados_com, entrada, Complementos};
use super::tecnico::{montar, Fecho};
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::{menor_necessario, ModoPdf, ProjetoEdicao, Velocidade};
use crate::edicao::motor::efeitos::regiao::testes::denso::{gravado, Duble};
use crate::edicao::motor::plano::Plano;
use crate::edicao::motor::{Contexto, FonteInfo};
use crate::edicao::quadros::apoio as q;
use crate::edicao::quadros::TabelaQuadros;
use crate::edicao::rastreio::{ArquivoDenso, ESTADO_INCERTO, ESTADO_OK, TIPO_ESTENDIDO, TIPO_INTERPOLADO, TIPO_MEDIDO};
use crate::edicao::testes::aceite::{alvo_final, ancora, efeito, midia, ocultar, projeto, rastreio, trecho};
use crate::relatorio_pdf;

/// 300 quadros a 30 qps, base 1/15360 (a da cena de apoio).
fn tabela() -> TabelaQuadros {
    let tb = (1, 15360);
    q::tabela(tb, q::pts_regulares(300, (1, 30), tb, 0))
}

/// Trecho do item 1 (quadros 30..120) e o denso da sugestão de detecção nele: detectado, interpolado entre
/// detecções (60..70) e estendido no fim (116..119); dois incertos.
const TRECHO: (usize, usize) = (30, 120);

fn denso_da_deteccao(tab: &TabelaQuadros, corrida: &Value) -> ArquivoDenso {
    let (a, b) = TRECHO;
    ArquivoDenso {
        esquema: 2,
        rastreio: "rd".into(),
        midia_sha256: "0".repeat(64),
        espaco: "original".into(),
        pixel: crate::edicao::modelo::TamanhoPx { w: 320, h: 180 },
        pts: tab.pts[a..b].to_vec(),
        x: vec![0.4; b - a],
        y: vec![0.3; b - a],
        w: vec![0.1; b - a],
        h: vec![0.2; b - a],
        conf: vec![0.8; b - a],
        estado: (a..b).map(|j| if j == 40 || j == 41 { ESTADO_INCERTO } else { ESTADO_OK }).collect(),
        tipo: (a..b)
            .map(|j| match j {
                60..=70 => TIPO_INTERPOLADO,
                116.. => TIPO_ESTENDIDO,
                _ => TIPO_MEDIDO,
            })
            .collect(),
        corrida: vec![0; b - a],
        corridas: vec![corrida.clone()],
    }
}

/// Corrida da detecção como `rastreio::detectar` grava (motor `deteccao`, arquivos do modelo).
fn corrida_da_deteccao(tab: &TabelaQuadros) -> Value {
    let (a, b) = TRECHO;
    json!({ "motor": "deteccao", "modelo": { "nome": "YOLOX-S cabeça", "origem": "https://github.com/x/modelos-redacao", "commit": "1",
                "arquivos": [{ "nome": "cabeca_yolox_s.onnx", "sha256": "7".repeat(64) }], "desvio": "" },
            "runtime": "onnxruntime 1.31.1", "provedor": "cpu", "sentido": "frente", "ancora_pts": tab.pts[a], "de_pts": tab.pts[a], "ate_pts": tab.pts[b - 1],
            "limiares": { "ok": 0.5, "perdido": 0.25, "paciencia": 30 }, "parou": "fim", "feito_em": "2026-11-18T01:00:00Z" })
}

/// O projeto com tudo da Fase 4 que o motor compila.
fn projeto_da_fase4(tab: &TabelaQuadros, sha_denso: &str) -> ProjetoEdicao {
    let (a, b) = TRECHO;
    let t = |j: usize| tab.tempo(j);
    // sugestão de detecção confirmada, com a ocultação da mídia apontando para ela (`Sugestoes.aplicarSugestoes`)
    let mut rd = rastreio("rd", Some("m1"), "fonte", vec![]);
    rd["origem"] = json!({ "tipo": "deteccao", "classe": "cabeca" });
    rd["denso"] = json!({ "sha256": sha_denso, "n": b - a, "de_s": t(a), "ate_s": t(b - 1), "ok": b - a - 2, "incertos": 2, "ausentes": 0 });
    rd["confirmacao"] = json!({ "por": "Perito Teste", "em": "2026-11-18T01:10:00Z", "denso_sha256": sha_denso, "de_s": t(a), "ate_s": t(b - 1) });
    let o1 = ocultar("o1", "rd", "solido", json!({ "conteudo": "rosto", "ativo": { "de": t(a), "ate": t(b - 1) } }));
    // marcação à mão com âncoras do "Simplificar" (40 e 60) que o perito não mexeu
    let caixa = [0.5, 0.5, 0.1, 0.1];
    let mut rm = rastreio("rm", Some("m1"), "fonte", [30usize, 40, 50, 60, 90].iter().map(|&j| ancora(t(j), Some(tab.pts[j]), caixa)).collect());
    rm["ancoras_do_programa"] = json!([t(40), t(60)]);
    // cópia de gravador: relógio, taxa suposta, nome do original e versão do ffmpeg
    let mut m1 = midia("m1", "video", json!([o1]));
    m1["nome"] = json!("cam 1.mkv");
    m1["derivacao"] = json!({ "tipo": "copia_dvr", "de_sha256": "b".repeat(64), "raiz_sha256": "b".repeat(64),
        "comando": "/usr/bin/ffmpeg -fflags +genpts -i /caso/Mídias/Vídeos/cam 1.dav -c copy -copyinkf /caso/Sistema/videos/copias-dvr/cam 1.mkv",
        "com_perdas": false, "inicio_epoca_s": 1505762699.25, "taxa_suposta": { "num": 25, "den": 1 }, "de_nome": "cam 1.dav", "ffmpeg_versao": "n9.0.1" });
    let destaque = efeito("destaque", "e1", json!({ "forma": "retangulo", "rastreio": "rm", "traco": { "cor": "#ffcc00", "espessura": 0.006, "contorno": true },
        "tamanho": 0.12, "angulo_graus": 0.0, "margem": 0.02, "rotulo": null }));
    // zoom que aproxima, segura e afasta sem dividir (chaves na base do clipe)
    let k = |j: usize, w: f64| json!({ "t": t(j), "pts": tab.pts[j], "v": { "x": (1.0 - w) / 2.0, "y": (1.0 - w) / 2.0, "w": w, "h": w }, "interp": "suave" });
    let zoom = efeito("enquadramento", "e2", json!({ "regiao": [k(30, 1.0), k(50, 0.5), k(80, 0.5), k(110, 1.0)], "ajuste": "cobrir", "interpolacao": "bilinear" }));
    let mut c1 = trecho("c1", "m1", tab, a, b, 1.0, json!([destaque, zoom]));
    c1["velocidade"] = json!({ "tipo": "rampa", "pontos": [{ "t": t(30), "v": 1.0 }, { "t": t(60), "v": 0.25 }, { "t": t(90), "v": 1.0 }] });
    // texto livre: faixa, sombra, entra deslizando de baixo e sai em corte
    let estilo = json!({ "fonte": "sans", "negrito": false, "italico": false, "tamanho": 0.052, "cor": "#ffffff", "alinhamento": "centro",
        "caixa": { "cor": "#000000a0", "margem": 0.012, "raio": 0.0, "faixa": true }, "contorno": null,
        "sombra": { "cor": "#000000b3", "dx": 0.0, "dy": 0.003, "desfoque": 0.004 } });
    let livre = |id: &str, texto: &str, desloc: f64, animacao: Value| {
        let mut v = json!({ "tipo": "titulo", "id": id, "rotulo": "", "pos": { "ref": "clipe", "clipe": "c1", "desloc": desloc }, "duracao_s": 1.0,
            "transformacao": { "x": 0.5, "y": 0.4, "escala": 1.0, "rotacao": 0.0, "opacidade": 1.0 }, "entrada_s": 0.3, "saida_s": 0.3, "efeitos": [],
            "titulo": { "modelo": "livre", "fundo": { "tipo": "transparente" }, "blocos": [{ "id": format!("b{id}"), "papel": "corpo", "conteudo": texto, "estilo": estilo,
                "lugar": { "x": 0.5, "y": 0.5, "h": "centro", "v": "meio" }, "largura_max": 0.6 }] } });
        if !animacao.is_null() {
            v["animacao"] = animacao;
        }
        v
    };
    let sobreposicoes = json!([{ "id": "t1", "nome": "", "oculta": false, "clipes": [
        livre("s1", "Portão da garagem", 0.2, json!({ "entrada": "deslizar", "saida": "corte", "lado": "baixo" })),
        livre("s2", "Vítima", 1.5, Value::Null),
    ] }]);
    projeto(320, 180, (30, 1), vec![m1], vec![c1], json!({ "rastreios": [rd, rm], "sobreposicoes": sobreposicoes }))
}

/// O plano como a exportação compila (`motor::compilar`: validação, vídeo, áudio e os avisos periciais no `feito`).
fn compilar(fonte: &FonteInfo, p: &ProjetoEdicao, leitor: &Duble) -> Plano {
    let fontes: std::collections::HashMap<String, FonteInfo> = [("m1".to_string(), fonte.clone())].into();
    let mut cap = q::cap_falsa();
    // o zoom com chaves anda por `perspective`
    cap.filtros.insert("perspective".into());
    let ctx = Contexto { fontes: &fontes, cap: &cap, rastreios: leitor, pasta_projeto: std::path::Path::new("/nao/existe"), midias: &[] };
    crate::edicao::motor::compilar(p, &ctx, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{e}"))
}

/// Linha do anexo/dado como texto (o espaço que não quebra vira espaço).
fn texto(v: &Value) -> String {
    v.to_string().replace('\u{a0}', " ")
}

#[test]
fn contagens_do_pdf_conferem_com_o_projeto() {
    let tab = tabela();
    let corrida = corrida_da_deteccao(&tab);
    let d = denso_da_deteccao(&tab, &corrida);
    let (sha_denso, leitor) = gravado(&d, tab.tb);
    let p = projeto_da_fase4(&tab, &sha_denso);
    // o documento pede `esquema_menor` 1 (sombra, faixa, animação, taxa suposta)
    assert_eq!(menor_necessario(&p), 1);
    // o projeto como a 6.4 grava (`armazem::salvar` sobe o `esquema_menor` até `menor_necessario`), para conferir
    // por fora com o leitor da 6.3.1: tudo; só o que é informação do PDF (mais a corrida de detecção do "Simplificar",
    // em JSON não tipado); e isso com a taxa suposta
    if let Ok(pasta) = std::env::var("SICRO_PROJETO_FASE4") {
        let gravar = |nome: &str, v: &Value| {
            let mut v = v.clone();
            v["esquema_menor"] = json!(menor_necessario(&serde_json::from_value(v.clone()).unwrap()));
            std::fs::write(std::path::Path::new(&pasta).join(nome), serde_json::to_vec_pretty(&v).unwrap()).unwrap();
        };
        let tudo = serde_json::to_value(&p).unwrap();
        gravar("fase4-tudo.json", &tudo);
        let mut info = tudo.clone();
        info["sobreposicoes"] = json!([]);
        info["midias"][0]["derivacao"].as_object_mut().unwrap().remove("taxa_suposta");
        info["rastreios"][0]["corridas_do_programa"] = json!([corrida]);
        gravar("fase4-informacao.json", &info);
        info["midias"][0]["derivacao"]["taxa_suposta"] = json!({ "num": 25, "den": 1 });
        gravar("fase4-taxa-suposta.json", &info);
    }
    let projeto_bytes = serde_json::to_vec(&p).unwrap();
    for codec in ["h264", "ffv1"] {
        let mut fonte = q::fonte(tab.clone(), (320, 180));
        fonte.codec = codec.into();
        let plano = compilar(&fonte, &p, &leitor);
        let c = Cena::nova("fase4");
        let res = apoio::resultado(&c, &plano, "t1");
        let fecho = Fecho { projeto_sha256: format!("{:x}", Sha256::digest(&projeto_bytes)), arquivo_rel: "Exportações/Vídeos/Edições/Caso/Caso.mp4".into(), ocultacoes: None };
        let t = montar(&plano, &res, &c.ambiente("t1"), &fecho).unwrap();
        let dd = dados_com(&t, &[], &[], Complementos { projeto: Some(&projeto_bytes), mapa: None }).unwrap();
        let e = entrada(&t, dd.clone(), &[], &projeto_bytes, ModoPdf::A3).unwrap();
        // conferência por fora (páginas, leiaute): SICRO_GUARDAR_PDF=<pasta>
        if let Ok(pasta) = std::env::var("SICRO_GUARDAR_PDF") {
            std::fs::write(std::path::Path::new(&pasta).join(format!("fase4-{codec}.pdf")), relatorio_pdf::gerar(&e).unwrap().pdf).unwrap();
        }
        let pdf = relatorio_pdf::texto(&e).unwrap().split_whitespace().collect::<Vec<_>>().join(" ");
        let no_pdf = |s: &str| assert!(pdf.contains(s), "{codec}: falta no PDF: {s}");
        let ia: Vec<String> = serde_json::from_value(dd["ia_resumo"].clone()).unwrap();

        // ---- sugestões de detecção: propostas e confirmadas, e as imagens achadas e calculadas, contadas no PROJETO
        let deteccao: Vec<&crate::edicao::modelo::Rastreio> = p.rastreios.iter().filter(|r| matches!(r.origem, crate::edicao::modelo::OrigemRastreio::Deteccao { .. })).collect();
        let confirmadas = deteccao.iter().filter(|r| r.confirmacao.as_ref().is_some_and(|c| r.denso.as_ref().is_some_and(|d| c.denso_sha256.as_deref() == Some(d.sha256.as_str())))).count();
        let n_oc = p.midias.iter().flat_map(|m| &m.ocultacoes).filter(|o| deteccao.iter().any(|r| r.id == o.alvo.rastreio)).count();
        assert_eq!((n_oc, confirmadas), (1, 1));
        let linha = frases::preencher(modelo::IA_OCULTACOES, &[("n", &n_oc.to_string())]);
        // as linhas do denso nos quadros lidos do trecho: medidas = achadas; interpoladas e estendidas = calculadas
        let (achadas, calculadas) = (TRECHO.0..TRECHO.1).fold((0, 0), |(x, y), j| if d.tipo[j - TRECHO.0] == TIPO_MEDIDO { (x + 1, y) } else { (x, y + 1) });
        assert_eq!((achadas, calculadas), (75, 15));
        let quadros = frases::preencher(modelo::IA_OCULTACOES_QUADROS, &[("achadas", &achadas.to_string()), ("calculadas", &calculadas.to_string())]);
        assert_eq!(&ia[..2], [linha.clone(), quadros.clone()], "{codec}");
        no_pdf(&linha);
        no_pdf(&quadros);
        // o modelo da detecção no anexo H, ligado ao rastreio, com o uso do P4.5
        let m = dd["modelos_ia"].as_array().unwrap().iter().find(|m| m["modelo"] == "YOLOX-S cabeça").expect("modelo da detecção");
        assert_eq!(m["uso"], json!("proposta de ocultação por detecção automática (R1)"));
        no_pdf(&"7".repeat(64));

        // ---- marcações do perito: as âncoras fora das do "Simplificar" (a conta do `marcadasAMao` do painel)
        let rm = p.rastreios.iter().find(|r| r.id == "rm").unwrap();
        let a_mao = rm.ancoras.iter().filter(|k| !rm.ancoras_do_programa.iter().any(|x| (x - k.t).abs() <= 1e-6)).count();
        assert_eq!(a_mao, 3);
        let marcacoes = frases::preencher(modelo::MARCACOES, &[("n", &a_mao.to_string())]);
        let r2 = dd["rastreios"].as_array().unwrap().iter().find(|r| r["origem"].as_str().unwrap().contains("marcação do perito")).expect("o rm no anexo H");
        assert!(r2["origem"].as_str().unwrap().ends_with(&marcacoes), "{r2}");
        no_pdf(&marcacoes);

        // ---- rampa: cada ponto do projeto no item
        let Velocidade::Rampa { pontos } = (match &p.principal[0] {
            crate::edicao::modelo::Tolerante::Conhecido(crate::edicao::modelo::ClipePrincipal::Trecho(x)) => x.velocidade.clone(),
            _ => unreachable!(),
        }) else {
            panic!("rampa")
        };
        let vel = dd["roteiro"][0]["velocidade"].as_str().unwrap();
        for k in &pontos {
            let ponto = format!("{}× em {}", super::juri::num(k.v, 3), super::juri::tempo(k.t));
            assert!(vel.contains(&ponto), "{codec}: {vel} sem {ponto}");
            no_pdf(&ponto);
        }
        let mudancas = texto(&dd["mudancas"]);
        assert!(mudancas.contains(frases::rotulo_do_efeito(frases::tipo::CAMERA_LENTA).unwrap()), "{mudancas}");

        // ---- zoom com chaves: ampliação no item e as quatro chaves no anexo C
        let anexo_c = texto(&dd["processamentos"]);
        assert!(anexo_c.contains("região animada em 4 chaves"), "{anexo_c}");
        assert!(texto(&dd["roteiro"][0]["efeitos"]).contains("\"Ampliação\""));

        // ---- texto livre: uma linha por texto do projeto; entrada e saída só no que não é o padrão
        let rotulo = frases::rotulo_do_efeito(frases::tipo::TEXTO_SOBREPOSTO).unwrap();
        let textos: Vec<&Value> = dd["acrescimos"].as_array().unwrap().iter().filter(|l| l["o_que"].as_str().unwrap().starts_with(rotulo)).collect();
        assert_eq!(textos.len(), p.sobreposicoes.iter().map(|t| t.clipes.len()).sum::<usize>());
        assert!(textos[0]["explicacao"].as_str().unwrap().ends_with("Entra deslizando de baixo em 0,3 s e some de uma vez."), "{}", textos[0]);
        assert_eq!(textos[1]["explicacao"].as_str(), frases::do_efeito(frases::tipo::TEXTO_SOBREPOSTO));
        no_pdf("Entra deslizando de baixo em 0,3 s e some de uma vez.");

        // ---- cópia de gravador: nome do original, relógio, taxa suposta (no ritmo e nos limites), ffmpeg e comando
        let o = &dd["originais"][0];
        let como = if codec == "ffv1" { "regravada em formato sem perda: cada imagem é igual à do original" } else { "sem recompressão" };
        assert_eq!(o["derivacao"]["texto"], json!(format!("Este arquivo foi feito pela perícia a partir do arquivo «cam 1.dav», também do caso (cópia convertida do formato do gravador, {como}).")));
        assert_eq!(o["relogio"], json!("18/09/2017, 19:24:59,250 no início do arquivo"));
        assert_eq!(o["ritmo"], json!("25 imagens por segundo, supostas: o arquivo do gravador não informa o ritmo"));
        assert_eq!(o["tecnico"]["derivacao"], json!("cópia convertida do formato do gravador · FFmpeg n9.0.1 · taxa suposta 25/1 quadros/s · início no relógio do gravador 18/09/2017, 19:24:59,250 (1505762699,25 s desde 1970)"));
        assert_eq!(o["tecnico"]["comando"], json!("ffmpeg -fflags +genpts -i cam 1.dav -c copy -copyinkf cam 1.mkv"));
        let limite = frases::da_condicao(codigo::TAXA_SUPOSTA).unwrap();
        assert!(dd["limitacoes"].as_array().unwrap().iter().any(|l| l.as_str().unwrap().starts_with(limite)), "{}", dd["limitacoes"]);
        for s in ["«cam 1.dav»", "19:24:59,250 no início do arquivo", "FFmpeg n9.0.1", "supostas: o arquivo do gravador não informa o ritmo", como, "-copyinkf"] {
            no_pdf(s);
        }
        no_pdf("A perícia leu essa gravação supondo um ritmo");
        // a identidade da 6.3.1
        no_pdf("Gerado pelo SICRO 6.0.0");
        assert!(!pdf.contains("/caso/"), "{codec}: caminho da máquina no PDF");
    }
}

/// Zoom que segue pelo motor: o `feito` que a compilação grava chega ao PDF ("acompanhando o rastreio R<n>") e o
/// rastreio entra no anexo H com o uso do item. Depende da entrega do ZS (`segue::na_cauda`).
#[test]
#[ignore = "fecho do bloco 6.4: ZS"]
fn zoom_que_segue_pelo_motor() {
    let tab = tabela();
    let corrida = corrida_da_deteccao(&tab);
    let d = denso_da_deteccao(&tab, &corrida);
    let (sha_denso, leitor) = gravado(&d, tab.tb);
    let mut v = serde_json::to_value(projeto_da_fase4(&tab, &sha_denso)).unwrap();
    // o zoom acompanha a sugestão confirmada
    v["principal"][0]["efeitos"][1]["regiao"] = json!({ "x": 0.25, "y": 0.25, "w": 0.5, "h": 0.5 });
    v["principal"][0]["efeitos"][1]["segue"] = json!({ "rastreio": "rd", "suavizar_s": 1.0 });
    let p: ProjetoEdicao = serde_json::from_value(v).unwrap();
    let plano = compilar(&q::fonte(tab, (320, 180)), &p, &leitor);
    let c = Cena::nova("fase4-segue");
    let res = apoio::resultado(&c, &plano, "t1");
    let t = montar(&plano, &res, &c.ambiente("t1"), &Fecho { projeto_sha256: "9".repeat(64), arquivo_rel: "Caso.mp4".into(), ocultacoes: None }).unwrap();
    let dd = super::juri::dados(&t, &[], &[]).unwrap();
    assert!(texto(&dd["processamentos"]).contains("acompanhando o rastreio R1 (média de 1 s)"), "{}", dd["processamentos"]);
    assert!(dd["rastreios"][0]["uso"].as_str().unwrap().contains("item 1"), "{}", dd["rastreios"]);
    assert!(texto(&dd["roteiro"][0]["efeitos"]).contains(frases::rotulo_do_efeito(frases::tipo::AMPLIACAO_QUE_SEGUE).unwrap()));
}
