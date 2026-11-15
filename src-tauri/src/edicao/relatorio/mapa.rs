//! `mapa.csv` (§10.1): q, clipe, midia, pts, indice, tipo (novo | repetido), e os quadros descartados por clipe. Dono: P0.R.
//! Colunas: `q,item,clipe,midia,pts,indice,t_s,tipo`. `item` = número do roteiro (o do PDF); `t_s` = pts × base de tempo
//! (tempo de fonte, 6 casas). Item sem fonte (título, imagem): só q, item e clipe. Depois dos slots, os descartados de cada
//! clipe, com `q` vazio e tipo `descartado`.

use std::collections::HashSet;

use crate::edicao::executar::Resultado;
use crate::edicao::modelo::Racional;
use crate::edicao::motor::plano::{Plano, ProvItem, ProvOrigem};
use crate::error::{Result, SicroError};

const CABECALHO: &str = "q,item,clipe,midia,pts,indice,t_s,tipo\n";

/// Texto do `mapa.csv` (UTF-8, `\n`). Com `res`, exige que os pts lidos sejam os previstos; sem ele, só a previsão do plano.
pub fn mapa_csv(plano: &Plano, res: Option<&Resultado>) -> Result<String> {
    let pv = &plano.proveniencia;
    // (q, item, linha)
    let mut slots: Vec<(u64, u32, String)> = Vec::new();
    let mut descartados = String::new();
    for it in &pv.roteiro {
        let clipe = campo(&it.clipe);
        let Some(o) = &it.origem else {
            for k in 0..it.na_saida.n_q as u64 {
                let q = it.na_saida.de_q + k;
                slots.push((q, it.n, format!("{q},{},{clipe},,,,,\n", it.n)));
            }
            continue;
        };
        let tb = pv
            .midias
            .iter()
            .find(|m| m.id == o.midia)
            .map(|m| m.medido.tb)
            .filter(|tb| tb.num > 0 && tb.den > 0)
            .ok_or_else(|| invalido(it, "mídia fora da proveniência"))?;
        conferir_origem(it, o)?;
        if let Some(res) = res {
            conferir_lidos(plano, res, it, o)?;
        }
        let midia = campo(&o.midia);
        let t_s = |pts: i64| t_s(pts, tb);
        let mut usado = vec![false; o.pts.len()];
        for (k, &j) in o.quadro_do_slot.iter().enumerate() {
            let i = (j - o.j_in) as usize;
            let tipo = if usado[i] { "repetido" } else { "novo" };
            usado[i] = true;
            let q = it.na_saida.de_q + k as u64;
            let pts = o.pts[i];
            slots.push((q, it.n, format!("{q},{},{clipe},{midia},{pts},{j},{},{tipo}\n", it.n, t_s(pts))));
        }
        for (i, _) in usado.iter().enumerate().filter(|(_, u)| !**u) {
            let pts = o.pts[i];
            let j = o.j_in as u64 + i as u64;
            descartados.push_str(&format!(",{},{clipe},{midia},{pts},{j},{},descartado\n", it.n, t_s(pts)));
        }
    }
    // na sobreposição de uma transição o mesmo slot aparece nos dois clipes
    slots.sort_by_key(|(q, n, _)| (*q, *n));
    let mut texto = String::from(CABECALHO);
    for (_, _, l) in slots {
        texto.push_str(&l);
    }
    texto.push_str(&descartados);
    Ok(texto)
}

/// O mapa do item é coerente: um quadro por slot, dentro dos quadros lidos e sem voltar.
fn conferir_origem(it: &ProvItem, o: &ProvOrigem) -> Result<()> {
    if o.quadro_do_slot.len() != it.na_saida.n_q as usize {
        return Err(invalido(it, "slots do mapa ≠ slots na saída"));
    }
    let fim = o.j_in as u64 + o.pts.len() as u64;
    let dentro = o.quadro_do_slot.iter().all(|&j| j >= o.j_in && (j as u64) < fim);
    let em_ordem = o.quadro_do_slot.windows(2).all(|w| w[0] <= w[1]);
    if !dentro || !em_ordem {
        return Err(invalido(it, "quadro fora dos lidos"));
    }
    Ok(())
}

/// pts lidos do clipe (todos os registros dele, na ordem) = os previstos pela conferência dos blocos
/// (ou, sem ela, os quadros do trecho); e todo quadro do trecho foi lido.
fn conferir_lidos(plano: &Plano, res: &Resultado, it: &ProvItem, o: &ProvOrigem) -> Result<()> {
    let mut previstos: Vec<i64> =
        plano.blocos.iter().flat_map(|b| &b.conferir).filter(|c| c.clipe == it.clipe).flat_map(|c| c.pts.iter().copied()).collect();
    if previstos.is_empty() {
        previstos = o.pts.clone();
    }
    let lidos: Vec<i64> = res.lidos.iter().filter(|l| l.clipe == it.clipe).flat_map(|l| l.pts.iter().copied()).collect();
    let erro = |d: &str| SicroError::Validation(format!("leitura_divergente: item {} ({d})", it.n));
    if lidos.is_empty() {
        return Err(erro("sem pts lidos"));
    }
    if lidos != previstos {
        let k = lidos.iter().zip(&previstos).position(|(a, b)| a != b).unwrap_or(lidos.len().min(previstos.len()));
        return Err(erro(&format!("{} lidos, {} previstos, primeiro diferente no {k}º", lidos.len(), previstos.len())));
    }
    let lidos: HashSet<i64> = lidos.into_iter().collect();
    // por campo, só o primeiro campo de cada quadro (índice par) tem pts de quadro lido
    let de_quadro = |i: usize| !o.campos || (o.j_in as usize + i) % 2 == 0;
    if let Some((_, p)) = o.pts.iter().enumerate().find(|(i, p)| de_quadro(*i) && !lidos.contains(p)) {
        return Err(erro(&format!("pts {p} não lido")));
    }
    Ok(())
}

/// pts × base em segundos com 6 casas, por conta inteira (meio para longe do zero; sempre ponto decimal).
/// O que arredonda para zero sai sem sinal. `tb` positiva (conferida por quem chama).
fn t_s(pts: i64, tb: Racional) -> String {
    let den = tb.den as i128;
    match (pts as i128).checked_mul(tb.num as i128).and_then(|x| x.checked_mul(1_000_000)) {
        Some(num) if den > 0 && num.unsigned_abs() < 1 << 120 => {
            let micro = (2 * num.unsigned_abs() + den as u128) / (2 * den as u128);
            let sinal = if num < 0 && micro != 0 { "-" } else { "" };
            format!("{sinal}{}.{:06}", micro / 1_000_000, micro % 1_000_000)
        }
        // fora do alcance do i128 (não acontece com tabela válida)
        _ => format!("{:.6}", pts as f64 * tb.num as f64 / tb.den as f64),
    }
}

fn invalido(it: &ProvItem, d: &str) -> SicroError {
    SicroError::Validation(format!("plano_invalido: item {} ({d})", it.n))
}

/// Campo de texto do CSV (RFC 4180): entre aspas quando tem vírgula, aspas ou quebra.
fn campo(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::executar::LidosClipe;
    use crate::edicao::modelo::Velocidade;
    use crate::edicao::relatorio::apoio::{self, Cena};
    use crate::edicao::tempo;

    /// Linha do mapa lida de volta.
    #[derive(Debug, PartialEq)]
    struct Linha {
        q: Option<u64>,
        item: u32,
        clipe: String,
        midia: String,
        pts: Option<i64>,
        indice: Option<usize>,
        t_s: String,
        tipo: String,
    }

    fn ler(texto: &str) -> Vec<Linha> {
        let mut l = texto.lines();
        assert_eq!(l.next(), Some(CABECALHO.trim_end()));
        l.map(|x| {
            let c: Vec<&str> = x.split(',').collect();
            assert_eq!(c.len(), 8, "{x}");
            Linha {
                q: c[0].parse().ok(),
                item: c[1].parse().unwrap(),
                clipe: c[2].into(),
                midia: c[3].into(),
                pts: c[4].parse().ok(),
                indice: c[5].parse().ok(),
                t_s: c[6].into(),
                tipo: c[7].into(),
            }
        })
        .collect()
    }

    /// Confere o mapa contra um mapa de quadros real, slot a slot, e os descartados.
    fn confere(fps: Racional, v: f64) {
        let c = Cena::nova("mapa");
        let plano = apoio::plano(&c, fps, v);
        let tab = apoio::tabela();
        let m = tempo::mapa_trecho(&tab, 30, 90, &Velocidade::Constante { fator: v }, fps).unwrap();
        let texto = mapa_csv(&plano, None).unwrap();
        assert!(texto.ends_with('\n') && !texto.contains('\r'));
        let linhas = ler(&texto);
        let total = plano.total_q;
        // um slot por linha, em ordem, de 0 a total_q − 1
        let slots: Vec<_> = linhas.iter().filter(|l| l.q.is_some()).collect();
        assert_eq!(slots.iter().map(|l| l.q.unwrap()).collect::<Vec<_>>(), (0..total).collect::<Vec<_>>());
        // título: sem fonte
        assert!(slots[..30].iter().all(|l| l.item == 1 && l.clipe == "c1" && l.midia.is_empty() && l.pts.is_none() && l.tipo.is_empty()));
        // trecho: o quadro de cada slot é o do mapa, o pts é o da tabela, o tempo é pts × base
        let trecho: Vec<_> = slots.iter().filter(|l| l.item == 2).collect();
        assert_eq!(trecho.len(), m.n_q as usize);
        for (k, l) in trecho.iter().enumerate() {
            let j = m.quadro_do_slot[k] as usize;
            assert_eq!((l.q, l.indice, l.pts, l.midia.as_str()), (Some(30 + k as u64), Some(j), Some(tab.pts[j]), "m1"), "slot {k}");
            assert_eq!(l.t_s, format!("{:.6}", tab.tempo(j)));
            let novo = k == 0 || m.quadro_do_slot[k - 1] as usize != j;
            assert_eq!(l.tipo, if novo { "novo" } else { "repetido" }, "slot {k}");
        }
        let repetidos = trecho.iter().filter(|l| l.tipo == "repetido").count();
        assert_eq!(repetidos, m.repetidos as usize);
        // descartados: os quadros lidos que nenhum slot mostra
        let desc: Vec<_> = linhas.iter().filter(|l| l.tipo == "descartado").collect();
        assert_eq!(desc.len(), m.descartados as usize);
        let mostrados: HashSet<u32> = m.quadro_do_slot.iter().copied().collect();
        for l in &desc {
            let j = l.indice.unwrap();
            assert!(l.q.is_none() && l.item == 2 && !mostrados.contains(&(j as u32)) && (30..90).contains(&j) && l.pts == Some(tab.pts[j]));
        }
        // congelado: o mesmo quadro em 15 slots
        let cg: Vec<_> = slots.iter().filter(|l| l.item == 3).collect();
        assert_eq!(cg.len(), 15);
        assert!(cg.iter().all(|l| l.indice == Some(100) && l.pts == Some(512 * 100)));
        assert_eq!(cg.iter().filter(|l| l.tipo == "novo").count(), 1);
        // com os pts lidos iguais aos previstos: o mesmo texto
        let res = apoio::resultado(&c, &plano, "t1");
        assert_eq!(mapa_csv(&plano, Some(&res)).unwrap(), texto);
    }

    #[test]
    fn mapa_confere_com_o_mapa_de_quadros() {
        let f = |num, den| Racional { num, den };
        // própria taxa, 30 → 25 (descarta), 0,5× (repete), 30 → 29,97, 4×
        for (fps, v, desc, rep) in [(f(30, 1), 1.0, false, false), (f(25, 1), 1.0, true, false), (f(30, 1), 0.5, false, true), (f(30000, 1001), 1.0, false, false), (f(30, 1), 4.0, true, false)] {
            let m = tempo::mapa_trecho(&apoio::tabela(), 30, 90, &Velocidade::Constante { fator: v }, fps).unwrap();
            assert_eq!((m.descartados > 0, m.repetidos > 0), (desc, rep), "{fps:?} {v}");
            confere(fps, v);
        }
    }

    #[test]
    fn exige_os_pts_lidos() {
        let c = Cena::nova("lidos");
        let (plano, res) = apoio::plano_e_resultado(&c);
        let msg = |r: Result<String>| match r {
            Err(SicroError::Validation(m)) => m,
            outro => panic!("{outro:?}"),
        };
        // um pts trocado
        let mut r = res.clone();
        r.lidos[0].pts[5] += 1;
        assert!(msg(mapa_csv(&plano, Some(&r))).starts_with("leitura_divergente: item 2"));
        // os mesmos pts fora de ordem
        let mut r = res.clone();
        r.lidos[0].pts.swap(3, 4);
        assert!(msg(mapa_csv(&plano, Some(&r))).contains("primeiro diferente no 3º"));
        // um quadro a menos
        let mut r = res.clone();
        r.lidos[0].pts.pop();
        assert!(msg(mapa_csv(&plano, Some(&r))).starts_with("leitura_divergente"));
        // clipe sem leitura
        let mut r = res.clone();
        r.lidos.retain(|l| l.clipe != "c3");
        assert!(msg(mapa_csv(&plano, Some(&r))).starts_with("leitura_divergente: item 3"));
        // lido em dois blocos: os registros do clipe se juntam na ordem
        let mut r = res.clone();
        let resto = r.lidos[0].pts.split_off(20);
        r.lidos.push(LidosClipe { clipe: "c2".into(), pts: resto });
        assert_eq!(mapa_csv(&plano, Some(&r)).unwrap(), mapa_csv(&plano, None).unwrap());
        // previsto por bloco com o quadro da emenda lido duas vezes
        let mut p2 = plano.clone();
        let mut b2 = p2.blocos[0].clone();
        b2.indice = 1;
        let k = &mut p2.blocos[0].conferir[0];
        let cauda = k.pts.split_off(20);
        let mut segundo = vec![*k.pts.last().unwrap()];
        segundo.extend(cauda);
        b2.conferir = vec![crate::edicao::motor::plano::ConferirClipe { rotulo: "c001".into(), clipe: "c2".into(), lidos: segundo.len() as u32, pts: segundo.clone() }];
        p2.blocos.push(b2);
        let mut r = res.clone();
        r.lidos[0].pts.truncate(20);
        r.lidos.push(LidosClipe { clipe: "c2".into(), pts: segundo });
        assert_eq!(mapa_csv(&p2, Some(&r)).unwrap(), mapa_csv(&plano, None).unwrap());
    }

    #[test]
    fn plano_incoerente_e_recusado() {
        let c = Cena::nova("x");
        let base = apoio::plano(&c, Racional { num: 30, den: 1 }, 1.0);
        let erro = |p: &Plano| matches!(mapa_csv(p, None), Err(SicroError::Validation(m)) if m.starts_with("plano_invalido"));
        let mut p = base.clone();
        p.proveniencia.roteiro[1].origem.as_mut().unwrap().quadro_do_slot.pop();
        assert!(erro(&p));
        let mut p = base.clone();
        p.proveniencia.roteiro[1].origem.as_mut().unwrap().quadro_do_slot[3] = 10;
        assert!(erro(&p));
        let mut p = base.clone();
        p.proveniencia.roteiro[1].origem.as_mut().unwrap().quadro_do_slot.swap(3, 40);
        assert!(erro(&p));
        let mut p = base;
        p.proveniencia.midias.clear();
        assert!(erro(&p));
    }

    /// Plano com o trecho (item 2) trocado por um de outra tabela.
    fn plano_com(c: &Cena, tb: (i64, i64), pts: Vec<i64>, fps: Racional) -> (Plano, crate::edicao::quadros::TabelaQuadros) {
        let tab = crate::edicao::quadros::apoio::tabela(tb, pts);
        let m = tempo::mapa_trecho(&tab, 0, tab.pts.len(), &Velocidade::Constante { fator: 1.0 }, fps).unwrap();
        let mut p = apoio::plano(c, fps, 1.0);
        p.proveniencia.roteiro.truncate(2);
        p.proveniencia.roteiro[1].na_saida.n_q = m.n_q;
        p.proveniencia.roteiro[1].origem = Some(apoio::origem(&tab, &m));
        p.proveniencia.midias[0].medido.tb = Racional { num: tb.0, den: tb.1 };
        (p, tab)
    }

    fn trecho(texto: &str) -> Vec<Linha> {
        ler(texto).into_iter().filter(|l| l.item == 2 && l.q.is_some()).collect()
    }

    /// Vídeo que começa antes do zero, em base de tempo grossa (29,97 em 1001/30000): o tempo sai de pts × base, exato.
    #[test]
    fn base_grossa_e_pts_negativo() {
        let c = Cena::nova("tb");
        let (p, _) = plano_com(&c, (1001, 30000), (-3..57).collect(), Racional { num: 30000, den: 1001 });
        let texto = mapa_csv(&p, None).unwrap();
        let l = trecho(&texto);
        assert_eq!(l.len(), 60);
        let par = |k: usize| (l[k].pts.unwrap(), l[k].indice.unwrap(), l[k].t_s.as_str());
        assert_eq!(par(0), (-3, 0, "-0.100100"));
        assert_eq!(par(2), (-1, 2, "-0.033367"));
        assert_eq!(par(3), (0, 3, "0.000000"));
        assert_eq!(par(4), (1, 4, "0.033367"));
        assert_eq!(par(59), (56, 59, "1.868533"));
        assert!(l.iter().all(|x| x.tipo == "novo"));
        // base de 1 quadro por tick (AVI 25): 1/25
        let (p, _) = plano_com(&c, (1, 25), (0..50).collect(), Racional { num: 25, den: 1 });
        let l = trecho(&mapa_csv(&p, None).unwrap());
        assert_eq!((l[1].pts, l[1].t_s.as_str(), l[49].t_s.as_str()), (Some(1), "0.040000", "1.960000"));
        // base de nanossegundo: o que arredonda para zero não sai com sinal; ponto decimal, nunca vírgula
        let (p, _) = plano_com(&c, (1, 1_000_000_000), (0..30).map(|k| -1 + k * 33_333_333).collect(), Racional { num: 30, den: 1 });
        let texto = mapa_csv(&p, None).unwrap();
        let l = trecho(&texto);
        assert_eq!((l[0].pts, l[0].t_s.as_str()), (Some(-1), "0.000000"));
        assert_eq!(l[1].t_s, "0.033333");
        assert!(!texto.contains("-0.000000") && !texto.contains(';'));
        // meio exato (0,0000005 s) arredonda para longe do zero, nos dois sinais
        assert_eq!((t_s(1, Racional { num: 1, den: 2_000_000 }), t_s(-1, Racional { num: 1, den: 2_000_000 })), ("0.000001".to_string(), "-0.000001".to_string()));
        assert_eq!(t_s(-90_000 * 3 - 45_000, Racional { num: 1, den: 90_000 }), "-3.500000");
    }

    #[test]
    fn texto_com_virgula_vai_entre_aspas() {
        assert_eq!(campo("c-1"), "c-1");
        assert_eq!(campo("a,b"), "\"a,b\"");
        assert_eq!(campo("a\"b"), "\"a\"\"b\"");
    }
}
