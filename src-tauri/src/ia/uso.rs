//! Registro de uso de modelos (§6.6): `feito.ia` da exportação junta `{ modelo, arquivos, runtime, provedor, uso }`
//! das corridas dos rastreios usados (e do motor de transcrição das legendas). Dono: P2.1.

use crate::edicao::motor::plano::{ProvIa, ProvRastreio};
use crate::edicao::rastreio::{Corrida, MotorRastreio};

/// Uma entrada de `feito.ia` por modelo e provedor, a partir de `feito.rastreios[].corridas` e
/// `corridas_do_programa` (o compilador é puro: lê só o que já está na proveniência). `uso` = "rastreio". Ordem
/// estável (modelo, provedor, resto), sem repetição; corrida que não lê como `Corrida` (Fases 0 e 1) e a da mancha
/// (sem modelo) não entram.
pub fn da_proveniencia(rastreios: &[ProvRastreio]) -> Vec<ProvIa> {
    let mut v: Vec<ProvIa> = rastreios
        .iter()
        .flat_map(|r| r.corridas.iter().chain(&r.corridas_do_programa))
        .filter_map(|c| serde_json::from_value::<Corrida>(c.clone()).ok())
        .filter(|c| c.motor.motor != MotorRastreio::Mancha)
        .map(|c| da_corrida(&c, "rastreio"))
        .collect();
    let chave = |p: &ProvIa| (p.modelo.clone(), p.provedor.clone(), serde_json::to_string(p).unwrap_or_default());
    v.sort_by_key(chave);
    v.dedup();
    v
}

/// A entrada de uma corrida.
pub fn da_corrida(c: &Corrida, uso: &str) -> ProvIa {
    let m = &c.motor;
    ProvIa {
        modelo: m.modelo.nome.clone(),
        arquivos: m.modelo.arquivos.iter().map(|a| crate::edicao::motor::plano::ProvArquivo { nome: a.nome.clone(), sha256: a.sha256.clone() }).collect(),
        origem: m.modelo.origem.clone(),
        commit: m.modelo.commit.clone(),
        desvio: m.modelo.desvio.clone(),
        runtime: m.runtime.clone(),
        provedor: m.provedor.clone(),
        uso: uso.to_string(),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::modelo::OrigemRastreio;
    use serde_json::{json, Value};

    fn corrida(motor: &str, nome: &str, provedor: &str, sha: &str, de: i64) -> Value {
        json!({
            "motor": motor,
            "modelo": { "nome": nome, "origem": "o", "commit": "c", "arquivos": [{ "nome": "a.onnx", "sha256": sha }], "desvio": "" },
            "runtime": "onnxruntime 1.31.1", "provedor": provedor,
            "sentido": "frente", "ancora_pts": de, "de_pts": de, "ate_pts": de + 100,
            "limiares": { "ok": 0.45, "perdido": 0.3, "paciencia": 60 }, "parou": "fim", "feito_em": "2026-11-15T10:00:00Z"
        })
    }

    fn rastreio(id: &str, corridas: Vec<Value>) -> ProvRastreio {
        ProvRastreio { id: id.into(), origem: OrigemRastreio::Assistido, ancoras: 1, denso_sha256: None, silhuetas_sha256: None, corridas, corridas_do_programa: vec![], confirmacao: None, ausencias: vec![] }
    }

    /// Agrupa por modelo e provedor (sem repetir), na mesma ordem em qualquer ordem de entrada; mancha e corrida
    /// antiga (sem o formato de `Corrida`, como a fixture do relatório) ficam de fora sem erro.
    #[test]
    fn junta_por_modelo_e_provedor_em_ordem_estavel() {
        let ed_cuda = corrida("edgetam", "EdgeTAM vídeo", "cuda", &"e".repeat(64), 0);
        let ed_cuda2 = corrida("edgetam", "EdgeTAM vídeo", "cuda", &"e".repeat(64), 500);
        let ed_cpu = corrida("edgetam", "EdgeTAM vídeo", "cpu", &"e".repeat(64), 900);
        let vit = corrida("vittrack", "VitTrack", "cpu", &"v".repeat(64), 0);
        let mancha = corrida("mancha", "mancha", "", &"m".repeat(64), 0);
        let antiga = json!({ "motor": "edgetam", "modelo": { "nome": "EdgeTAM vídeo", "arquivo": "/x/modelos/x.onnx" } });
        let a = vec![rastreio("r1", vec![vit.clone(), ed_cuda.clone(), antiga.clone()]), rastreio("r2", vec![ed_cpu.clone(), ed_cuda2.clone(), mancha.clone()])];
        let b = vec![rastreio("r2", vec![mancha, ed_cuda2, ed_cpu]), rastreio("r1", vec![antiga, ed_cuda, vit])];
        let ia = da_proveniencia(&a);
        assert_eq!(ia, da_proveniencia(&b));
        let resumo: Vec<(&str, &str)> = ia.iter().map(|p| (p.modelo.as_str(), p.provedor.as_str())).collect();
        assert_eq!(resumo, [("EdgeTAM vídeo", "cpu"), ("EdgeTAM vídeo", "cuda"), ("VitTrack", "cpu")]);
        assert!(ia.iter().all(|p| p.uso == "rastreio" && p.runtime == "onnxruntime 1.31.1" && p.arquivos.len() == 1));
        assert_eq!(serde_json::to_string(&da_proveniencia(&a)).unwrap(), serde_json::to_string(&da_proveniencia(&a)).unwrap());
        assert!(da_proveniencia(&[]).is_empty());
    }
}
