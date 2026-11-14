//! Divisão em blocos e junção final (§5.8). Dono: P0.6a.

use sha2::{Digest, Sha256};

use super::plano::{ArquivoTexto, Bloco, Canvas, Juncao, PassoAudio};
use super::saida::Codificacao;
use super::Alvo;
use crate::edicao::camada::lista;
use crate::edicao::ffmpeg::args;
use crate::edicao::modelo::{ClipePrincipal, LinhaResolvida, ProjetoEdicao, Tolerante, Transicao};
use crate::edicao::tempo;

/// Limites de um bloco: entradas de mídia e duração (s).
pub const MAX_ENTRADAS: usize = 12;
pub const MAX_DURACAO_S: f64 = 300.0;

/// Janela de slots de um bloco: [de_q, de_q + n_q).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JanelaBloco {
    pub de_q: u64,
    pub n_q: u32,
}

/// Transição de entrada do clipe da principal com esse id.
pub(crate) fn transicao_de<'a>(p: &'a ProjetoEdicao, id: &str) -> Option<&'a Transicao> {
    p.principal.iter().find_map(|c| match c {
        Tolerante::Conhecido(c) => {
            let b = match c {
                ClipePrincipal::Trecho(x) => &x.base,
                ClipePrincipal::Congelar(x) => &x.base,
                ClipePrincipal::Imagem(x) => &x.base,
                ClipePrincipal::Titulo(x) => &x.base,
                ClipePrincipal::Tela(x) => &x.base,
            };
            (b.id == id).then_some(b.transicao_entrada.as_ref())
        }
        _ => None,
    })?
}

/// Slots de uma transição dentro de um clipe de `n_q` slots.
pub(crate) fn slots_da_transicao(t: Option<&Transicao>, linha: &LinhaResolvida, n_q: u32) -> u32 {
    t.map(|t| tempo::slots(t.duracao_s, linha.fps).min(n_q as u64) as u32).unwrap_or(0)
}

/// Slots do escurecimento de entrada e de saída do clipe `i` da linha (a saída é a entrada do seguinte, ou o `fecho`).
pub(crate) fn escurecimentos(p: &ProjetoEdicao, linha: &LinhaResolvida, i: usize) -> (u32, u32) {
    let c = &linha.clipes[i];
    let entrada = slots_da_transicao(transicao_de(p, &c.id), linha, c.n_q);
    let seguinte = match linha.clipes.get(i + 1) {
        Some(s) => transicao_de(p, &s.id),
        None => p.fecho.as_ref(),
    };
    (entrada, slots_da_transicao(seguinte, linha, c.n_q))
}

/// Divide o programa em blocos: em corte seco quando houver, senão em fronteira de slot fora de transição.
/// `Alvo::Previa` e `Alvo::Quadro` dão uma janela só.
pub fn dividir(p: &ProjetoEdicao, linha: &LinhaResolvida, alvo: &Alvo) -> Vec<JanelaBloco> {
    let total = linha.total_q;
    let janela = |de: u64, ate: u64| {
        let (de, ate) = (de.min(total), ate.min(total));
        (ate > de).then(|| JanelaBloco { de_q: de, n_q: (ate - de).min(u32::MAX as u64) as u32 })
    };
    match alvo {
        Alvo::Previa { de_q, ate_q, .. } => return janela(*de_q, *ate_q).into_iter().collect(),
        Alvo::Quadro { q, .. } => return janela(*q, q.saturating_add(1)).into_iter().collect(),
        Alvo::Final { .. } => {}
    }
    let max_q = tempo::slots(MAX_DURACAO_S, linha.fps).max(1);
    let mut blocos = Vec::new();
    let (mut inicio, mut entradas) = (0u64, 0usize);
    for (i, c) in linha.clipes.iter().enumerate() {
        let fim = c.inicio_q + c.n_q as u64;
        // corte seco antes deste clipe: fecha se ele não cabe
        if c.sobrepoe_q == 0 && c.inicio_q > inicio && (entradas + 1 > MAX_ENTRADAS || fim - inicio > max_q) {
            blocos.extend(janela(inicio, c.inicio_q));
            (inicio, entradas) = (c.inicio_q, 0);
        }
        entradas += 1;
        // clipe maior que o limite: corta no meio dele, fora do escurecimento
        let (entra, sai) = escurecimentos(p, linha, i);
        while fim - inicio > max_q {
            let corte = (inicio + max_q).clamp(c.inicio_q + entra as u64, fim.saturating_sub(sai as u64).max(c.inicio_q + entra as u64));
            if corte <= inicio || corte >= fim {
                break;
            }
            blocos.extend(janela(inicio, corte));
            (inicio, entradas) = (corte, 1);
        }
    }
    blocos.extend(janela(inicio, total));
    blocos
}

/// SHA-256 do bloco: grafo, argumentos, SHA-256 das mídias, `hash` dos pedidos de camada e SHA-256 do ffmpeg.
pub fn chave(grafo: &str, args: &[String], midias: &[String], camadas: &[String], ffmpeg_sha256: &str) -> String {
    let mut h = Sha256::new();
    // cada parte com o tamanho na frente: nenhuma lista se confunde com a vizinha
    let mut parte = |nome: &str, itens: &[&str]| {
        h.update(format!("{nome} {}\n", itens.len()).as_bytes());
        for i in itens {
            h.update(format!("{}\n", i.len()).as_bytes());
            h.update(i.as_bytes());
            h.update(b"\n");
        }
    };
    parte("sicro.edicao.bloco/1 grafo", &[grafo]);
    parte("args", &args.iter().map(String::as_str).collect::<Vec<_>>());
    parte("midias", &midias.iter().map(String::as_str).collect::<Vec<_>>());
    parte("camadas", &camadas.iter().map(String::as_str).collect::<Vec<_>>());
    parte("ffmpeg", &[ffmpeg_sha256]);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Lista do concat e argumentos do mux final.
pub fn juncao(blocos: &[Bloco], canvas: &Canvas, cod: &Codificacao, audio: Option<&PassoAudio>, alvo: &Alvo) -> Juncao {
    if matches!(alvo, Alvo::Quadro { .. }) {
        return Juncao::Direta;
    }
    let nomes: Vec<String> = blocos.iter().map(|b| b.saida.clone()).collect();
    let mut a: Vec<String> = ["-map", "0:v", "-c:v", "copy"].iter().map(|s| s.to_string()).collect();
    if audio.is_some() {
        a.extend(["-map".to_string(), "1:a".to_string()]);
        a.extend(cod.audio.iter().cloned());
    }
    a.extend(args::bitexact());
    a.extend(["-movflags", "+faststart", "-video_track_timescale"].iter().map(|s| s.to_string()));
    a.push(cod.timescale.to_string());
    let _ = canvas;
    Juncao::Concat { lista: ArquivoTexto { nome: "blocos.ffconcat".into(), conteudo: lista::ffconcat(&nomes) }, args: a, extensao: cod.extensao.clone() }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::modelo::{ClipeResolvido, Racional};
    use serde_json::json;

    fn projeto(clipes: serde_json::Value, fecho: serde_json::Value) -> ProjetoEdicao {
        serde_json::from_value(json!({
            "esquema": 2, "esquema_menor": 0, "id": "p", "titulo": "t", "created_at": "", "updated_at": "",
            "saida": { "largura": 320, "altura": 180, "fps": { "num": 30, "den": 1 }, "auto": false, "cor_fundo": "#000000", "audio": false,
                       "selos": { "velocidade": true, "ampliacao": true, "realce": true },
                       "exportacao": { "predefinicao": "juri", "codificador": "auto", "legendas": "nenhuma", "pacote": null, "pdf": "a3", "revisado_por": "", "personalizada": null } },
            "principal": clipes, "fecho": fecho
        }))
        .unwrap()
    }
    fn titulo(id: &str, transicao: serde_json::Value) -> serde_json::Value {
        json!({ "tipo": "titulo", "id": id, "duracao_s": 1.0, "transicao_entrada": transicao,
                "titulo": { "modelo": "livre", "fundo": { "tipo": "transparente" }, "blocos": [] } })
    }
    fn linha(n: &[u32]) -> LinhaResolvida {
        let mut q = 0u64;
        let clipes = n
            .iter()
            .enumerate()
            .map(|(i, &n_q)| {
                let c = ClipeResolvido { id: format!("c{i}"), inicio_q: q, n_q, sobrepoe_q: 0, fonte: None };
                q += n_q as u64;
                c
            })
            .collect();
        LinhaResolvida { fps: Racional { num: 30, den: 1 }, total_q: q, clipes, livres: Vec::new() }
    }
    fn alvo_final() -> Alvo {
        serde_json::from_value(json!({ "tipo": "final", "exportacao": { "predefinicao": "juri", "codificador": "auto", "legendas": "nenhuma",
            "pacote": null, "pdf": "a3", "revisado_por": "", "personalizada": null } }))
        .unwrap()
    }

    #[test]
    fn projeto_pequeno_um_bloco_e_janelas_de_previa_e_quadro() {
        let clipes: Vec<_> = (0..3).map(|i| titulo(&format!("c{i}"), json!(null))).collect();
        let p = projeto(json!(clipes), json!(null));
        let l = linha(&[30, 30, 30]);
        assert_eq!(dividir(&p, &l, &alvo_final()), vec![JanelaBloco { de_q: 0, n_q: 90 }]);
        assert_eq!(dividir(&p, &l, &Alvo::Previa { altura: 90, de_q: 20, ate_q: 500 }), vec![JanelaBloco { de_q: 20, n_q: 70 }]);
        assert_eq!(dividir(&p, &l, &Alvo::Quadro { q: 89, largura: 320 }), vec![JanelaBloco { de_q: 89, n_q: 1 }]);
        assert!(dividir(&p, &l, &Alvo::Quadro { q: 90, largura: 320 }).is_empty());
    }

    #[test]
    fn fecha_em_12_entradas_em_5_minutos_e_no_meio_do_clipe_longo() {
        // 30 clipes de 1 s: 12 + 12 + 6
        let clipes: Vec<_> = (0..30).map(|i| titulo(&format!("c{i}"), json!(null))).collect();
        let p = projeto(json!(clipes), json!(null));
        let b = dividir(&p, &linha(&[30; 30]), &alvo_final());
        assert_eq!(b.iter().map(|j| j.n_q).collect::<Vec<_>>(), vec![360, 360, 180]);
        // 4 clipes de 2 min: o terceiro não cabe nos 5 min
        let b = dividir(&p, &linha(&[3600; 4]), &alvo_final());
        assert_eq!(b.iter().map(|j| (j.de_q, j.n_q)).collect::<Vec<_>>(), vec![(0, 7200), (7200, 7200)]);
        // clipe de 12 min com escurecimento na entrada e no fecho: cortes a cada 5 min, fora dos escurecimentos
        let p = projeto(json!([titulo("c0", json!({ "tipo": "escurecer", "duracao_s": 1.0, "audio": "acompanhar" }))]), json!({ "tipo": "escurecer", "duracao_s": 2.0, "audio": "acompanhar" }));
        let b = dividir(&p, &linha(&[21_600]), &alvo_final());
        assert_eq!(b.iter().map(|j| (j.de_q, j.n_q)).collect::<Vec<_>>(), vec![(0, 9000), (9000, 9000), (18_000, 3600)]);
        let b = dividir(&p, &linha(&[9030]), &alvo_final());
        // o corte de 5 min cairia no escurecimento do fecho (60 slots): recua para antes dele
        assert_eq!(b.iter().map(|j| (j.de_q, j.n_q)).collect::<Vec<_>>(), vec![(0, 8970), (8970, 60)]);
        // os blocos cobrem o programa sem buraco nem repetição
        let mut q = 0;
        for j in dividir(&p, &linha(&[100, 20_000, 7, 9000, 9001]), &alvo_final()) {
            assert_eq!(j.de_q, q);
            assert!(j.n_q as u64 <= 9000 && j.n_q > 0);
            q += j.n_q as u64;
        }
        assert_eq!(q, 38_108);
    }

    #[test]
    fn chave_muda_com_cada_parte() {
        let base = chave("g", &["-c:v".into(), "libx264".into()], &["m1".into()], &["k1".into()], "ff");
        assert_eq!(base.len(), 64);
        assert_eq!(base, chave("g", &["-c:v".into(), "libx264".into()], &["m1".into()], &["k1".into()], "ff"));
        for outra in [
            chave("g2", &["-c:v".into(), "libx264".into()], &["m1".into()], &["k1".into()], "ff"),
            chave("g", &["-c:v".into(), "h264_nvenc".into()], &["m1".into()], &["k1".into()], "ff"),
            chave("g", &["-c:v".into(), "libx264".into()], &["m2".into()], &["k1".into()], "ff"),
            chave("g", &["-c:v".into(), "libx264".into()], &["m1".into()], &["k2".into()], "ff"),
            chave("g", &["-c:v".into(), "libx264".into()], &["m1".into()], &["k1".into()], "f7"),
            // um item que muda de lista
            chave("g", &["-c:v".into(), "libx264".into(), "m1".into()], &[], &["k1".into()], "ff"),
        ] {
            assert_ne!(base, outra);
        }
    }
}
