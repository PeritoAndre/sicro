//! Defeito de 17/11 (caso PINHO): depois do "Simplificar" o relatório dizia "sem registro do programa usado" e o anexo
//! H ficava sem as corridas. Com `feito.rastreios[].corridas_do_programa` o relatório diz do programa o mesmo que
//! diria com o denso; o `feito` de antes dele segue "sem registro".

use serde_json::{json, Value};

use super::apoio::{self, Cena};
use super::frases::{self, condicao, modelo};
use super::juri::dados;
use super::tecnico::{montar, Fecho};
use crate::edicao::modelo::OrigemRastreio;
use crate::edicao::motor::plano::ProvRastreio;
use crate::edicao::rastreio::{vittrack, ArquivoModelo, MotorRastreio, Sentido};

/// Corrida nos quadros `de..=ate` da cena de apoio (30 qps, base 1/15360).
fn corrida(motor: MotorRastreio, de: i64, ate: i64) -> Value {
    let mut c = crate::edicao::rastreio::arquivo::testes::corrida(Sentido::Frente, 512 * de, 512 * de, 512 * ate);
    c.motor.motor = motor;
    if motor == MotorRastreio::Vittrack {
        (c.motor.modelo.nome, c.motor.runtime) = ("VitTrack".into(), "onnxruntime 1.31.1".into());
        c.motor.modelo.arquivos = vec![ArquivoModelo { nome: vittrack::NOME_ARQUIVO.into(), sha256: vittrack::SHA256.into() }];
    }
    serde_json::to_value(c).unwrap()
}

#[test]
fn simplificado_diz_do_programa_o_mesmo_que_o_denso() {
    let c = Cena::nova("juri-simplificar");
    let (mut plano, res) = apoio::plano_e_resultado(&c);
    plano.proveniencia.ia.clear();
    (plano.proveniencia.rastreios[0].origem, plano.proveniencia.rastreios[0].confirmacao) = (OrigemRastreio::Assistido, None);
    plano.proveniencia.rastreios[0].corridas.clear();
    let fecho = Fecho { projeto_sha256: "9".repeat(64), arquivo_rel: "Exportações/Vídeos/Edições/Caso/Caso.mp4".into(), ocultacoes: None };
    let base = montar(&plano, &res, &c.ambiente("t1"), &fecho).unwrap();
    // r1 é usado pela ocultação 1, no clipe c2 (quadros 30..89): com o denso, medidos; simplificado, âncoras e
    // interpolados do programa
    let feito = |corridas: &[&Value], simplificado: Option<bool>| -> Value {
        let mut t = base.clone();
        let r = &mut t["feito"]["rastreios"][0];
        let lista = json!(corridas);
        match simplificado {
            Some(true) => (r["corridas"], r["denso_sha256"], r["corridas_do_programa"]) = (json!([]), Value::Null, lista),
            // `feito` de antes do registro: o campo não existe
            Some(false) => (r["corridas"], r["denso_sha256"]) = (lista, Value::Null),
            None => r["corridas"] = lista,
        }
        t["feito"]["ocultacoes"][0]["quadros"] = match simplificado {
            None => json!({ "ancora": 0, "medido": 60, "interpolado": 0, "incerto": 0 }),
            Some(_) => json!({ "ancora": 0, "ancora_do_programa": 4, "medido": 0, "interpolado": 56, "interpolado_do_programa": 56, "incerto": 0 }),
        };
        // `feito.ia` como `ia::uso` grava, das corridas que o `feito` traz
        let rs: Vec<ProvRastreio> = serde_json::from_value(t["feito"]["rastreios"].clone()).unwrap();
        t["feito"]["ia"] = serde_json::to_value(crate::ia::uso::da_proveniencia(&rs)).unwrap();
        dados(&t, &[], &[]).unwrap()
    };
    let resumo = |d: &Value| d["ia_resumo"].as_array().unwrap().iter().map(|l| l.as_str().unwrap().to_string()).collect::<Vec<_>>();
    let anexo_h = |d: &Value| d["rastreios"][0]["corridas"].clone();
    let sem_registro = "Alvos acompanhados por programa a partir de marcação do perito, sem registro do programa usado: 1.".to_string();
    let nenhuma = frases::da_condicao(condicao::IA_NENHUMA).unwrap().to_string();
    let (vit, mancha) = (corrida(MotorRastreio::Vittrack, 30, 60), corrida(MotorRastreio::Mancha, 61, 89));
    for cs in [vec![&vit], vec![&mancha], vec![&vit, &mancha]] {
        let (d, s) = (feito(&cs, None), feito(&cs, Some(true)));
        assert_eq!((resumo(&s), anexo_h(&s), &s["modelos_ia"]), (resumo(&d), anexo_h(&d), &d["modelos_ia"]), "{cs:?}");
        assert!(!resumo(&s).contains(&sem_registro), "{:?}", resumo(&s));
        assert_eq!(anexo_h(&s).as_array().unwrap().len(), cs.len());
    }
    assert_eq!(resumo(&feito(&[&mancha], Some(true))), [nenhuma]);
    let com_vit = feito(&[&vit], Some(true));
    assert_eq!(resumo(&com_vit), ["Alvos acompanhados por programa de inteligência artificial a partir de marcação do perito: 1.".to_string(), modelo::IA_MODELOS.to_string()]);
    assert!(com_vit["modelos_ia"].to_string().contains(vittrack::SHA256), "{}", com_vit["modelos_ia"]);
    // âncora do programa sem registro (`null`): "sem registro", e o anexo H diz que há corrida ilegível
    let nulo = Value::Null;
    for cs in [vec![&nulo], vec![&mancha, &nulo]] {
        let s = feito(&cs, Some(true));
        assert_eq!(resumo(&s), [sem_registro.clone()], "{cs:?}");
        assert!(anexo_h(&s).to_string().contains("corridas sem registro legível: 1"), "{}", anexo_h(&s));
    }
    // `feito` de antes do registro, com a correlação do denso por baixo: segue "sem registro"
    assert_eq!(resumo(&feito(&[&mancha], Some(false))), [sem_registro]);
}
