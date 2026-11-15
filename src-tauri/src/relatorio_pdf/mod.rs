//! Relatórios em PDF sem programa externo (§10.3; protótipo e medições em `p4/integracao/src-tauri/src/relatorio_pdf/`):
//! modelo Typst embutido (`juri.typ`) + dados em JSON, fontes dentro do binário, nada lê relógio nem sistema
//! (mesma entrada, mesmos bytes). O texto do perito entra como dado, nunca como código Typst. Dono: P1.R.
//! As fontes são as de `edicao/camada/fontes/` (as mesmas do texto do vídeo; licenças lá).

use std::collections::HashMap;

use chrono::{DateTime, Datelike, Timelike, Utc};
use typst::diag::{FileError, FileResult, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Duration, Smart};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;
use typst_pdf::{PdfOptions, PdfStandard, PdfStandards, Timestamp};

use crate::error::{Result, SicroError};

const FONTES: &[&[u8]] = &[
    include_bytes!("../edicao/camada/fontes/LibertinusSerif-Regular.otf"),
    include_bytes!("../edicao/camada/fontes/LibertinusSerif-Bold.otf"),
    include_bytes!("../edicao/camada/fontes/LibertinusSerif-Italic.otf"),
    include_bytes!("../edicao/camada/fontes/LibertinusSerif-BoldItalic.otf"),
    include_bytes!("../edicao/camada/fontes/DejaVuSansMono.ttf"),
    include_bytes!("../edicao/camada/fontes/DejaVuSansMono-Bold.ttf"),
];

const SUBSTITUTO: char = '\u{25A1}'; // □

/// Modelo do documento.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modelo {
    /// Relatório do pacote do júri.
    Juri,
}

impl Modelo {
    fn fonte(self) -> &'static str {
        match self {
            Modelo::Juri => include_str!("juri.typ"),
        }
    }
}

/// O que vira o PDF. `dados` vira `dados.json` (contrato `relatorio::juri::ESQUEMA_JURI`); `arquivos` = (caminho
/// virtual com `/`, bytes): miniaturas (`img/1.jpg`) e anexos (`tecnico.json`, `projeto.json`); `momento` = data do
/// documento (a do render); `ident` = SHA-256 do vídeo. `pdf_a` = arquivável: PDF/A-3a + PDF/UA-1 quando `dados.anexos`
/// traz arquivo anexado, PDF/A-2b + PDF/UA-1 sem anexos; `false` = PDF 1.7 comum (a saída de emergência do §10.3).
pub struct Entrada {
    pub modelo: Modelo,
    pub dados: serde_json::Value,
    pub arquivos: Vec<(String, Vec<u8>)>,
    pub momento: DateTime<Utc>,
    pub ident: String,
    pub pdf_a: bool,
}

/// PDF pronto; `avisos` = o que mostrar ao perito (caractere sem glifo trocado por □; PDF/A que não saiu).
#[derive(Debug)]
pub struct Saida {
    pub pdf: Vec<u8>,
    pub paginas: usize,
    pub avisos: Vec<String>,
}

struct Mundo {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fontes: Vec<Font>,
    principal: Source,
    arquivos: HashMap<FileId, Bytes>,
    hoje: Datetime,
}

fn id(caminho: &str) -> Result<FileId> {
    // No Windows quem chama pode montar o caminho com `\`; o Typst só aceita `/`.
    let caminho = &caminho.replace('\\', "/");
    let v = VirtualPath::new(caminho).map_err(|e| SicroError::Validation(format!("caminho inválido no relatório ({caminho}): {e}")))?;
    Ok(RootedPath::new(VirtualRoot::Project, v).intern())
}

impl World for Mundo {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }
    fn main(&self) -> FileId {
        self.principal.id()
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.principal.id() {
            Ok(self.principal.clone())
        } else {
            Err(FileError::NotSource)
        }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.arquivos.get(&id).cloned().ok_or_else(|| FileError::NotFound(id.vpath().get_without_slash().into()))
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fontes.get(index).cloned()
    }
    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        Some(self.hoje)
    }
}

/// Caractere que não se vê (controle, marca de formatação, seletor de variante): vem de texto colado de outro
/// programa e não é conteúdo.
fn invisivel(c: char) -> bool {
    matches!(c, '\u{00AD}' | '\u{180E}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FE00}'..='\u{FE0F}' | '\u{FEFF}')
}

/// Código de uso privado: a fonte pode ter desenho para ele, mas o PDF/A não aceita texto sem significado público.
fn uso_privado(c: char) -> bool {
    matches!(c, '\u{E000}'..='\u{F8FF}' | '\u{F0000}'..='\u{10FFFF}')
}

/// Troca por □ o que nenhuma fonte embutida desenha (emoji, ideogramas) e o código de uso privado. Sem isso o PDF/A
/// recusa a exportação e o PDF comum sai com um buraco sem aviso. Sai sem troca o que não se vê e o PDF/A recusa
/// mesmo com a fonte dizendo que cobre: controle (menos quebra de linha e tabulação), U+FEFF, U+FFFE, U+FFFF; e o
/// invisível que a fonte não tem.
fn sanear(v: &mut serde_json::Value, fontes: &[Font], trocados: &mut Vec<char>) {
    match v {
        serde_json::Value::String(s) => {
            let fora = |c: char| (c.is_control() && c != '\n' && c != '\t') || matches!(c, '\u{FEFF}' | '\u{FFFE}' | '\u{FFFF}');
            let falta = |c: char| uso_privado(c) || (!c.is_whitespace() && !fontes.iter().any(|f| f.info().coverage.contains(c as u32)));
            if s.chars().any(|c| fora(c) || falta(c)) {
                *s = s
                    .chars()
                    .filter(|&c| !fora(c) && !(invisivel(c) && falta(c)))
                    .map(|c| {
                        if falta(c) {
                            trocados.push(c);
                            SUBSTITUTO
                        } else {
                            c
                        }
                    })
                    .collect();
            }
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(|x| sanear(x, fontes, trocados)),
        serde_json::Value::Object(o) => o.values_mut().for_each(|x| sanear(x, fontes, trocados)),
        _ => {}
    }
}

/// Imagem que o modelo vai desenhar (`.jpg`, `.png`): tem de decodificar e, no JPEG, ir até a marca de fim. O
/// decodificador aceita JPEG cortado no meio (sai meia imagem sem aviso) e o erro dele não diz qual arquivo foi.
fn conferir_imagem(nome: &str, bytes: &[u8]) -> Result<()> {
    let baixo = nome.to_ascii_lowercase();
    let jpeg = baixo.ends_with(".jpg") || baixo.ends_with(".jpeg");
    if !(jpeg || baixo.ends_with(".png")) {
        return Ok(());
    }
    let motivo = if jpeg && !(bytes.starts_with(&[0xFF, 0xD8]) && bytes.ends_with(&[0xFF, 0xD9])) {
        Some("arquivo incompleto".to_string())
    } else {
        image::load_from_memory(bytes).err().map(|e| e.to_string())
    };
    match motivo {
        Some(m) => Err(SicroError::Workspace(format!("relatório em PDF: imagem inválida ({nome}): {}", m.trim()))),
        None => Ok(()),
    }
}

fn erros(lista: &[SourceDiagnostic]) -> String {
    lista
        .iter()
        .map(|d| {
            let dicas: Vec<&str> = d.hints.iter().map(|h| h.v.as_str()).collect();
            if dicas.is_empty() {
                d.message.to_string()
            } else {
                format!("{} ({})", d.message, dicas.join("; "))
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Compila o modelo e devolve o PDF em memória. Bloqueia (dezenas de ms; ~1 s com centenas de miniaturas): chamar de
/// `spawn_blocking`. Se a exportação em PDF/A falhar, sai o PDF comum com o motivo em `avisos`: o pacote não deixa
/// de sair por causa do padrão do PDF.
pub fn gerar(e: &Entrada) -> Result<Saida> {
    let r = compilar(e).and_then(|(doc, avisos)| exportar(e, &doc, avisos));
    // O Typst memoriza a compilação num cache global; sem isto a memória fica presa no app.
    typst::comemo::evict(0);
    r
}

fn data_invalida() -> SicroError {
    SicroError::Validation("data do relatório inválida".into())
}

/// Documento paginado e os avisos (do Typst e dos caracteres trocados).
fn compilar(e: &Entrada) -> Result<(PagedDocument, Vec<String>)> {
    let fontes: Vec<Font> = FONTES.iter().flat_map(|b| Font::iter(Bytes::new(*b))).collect();

    let mut dados = e.dados.clone();
    let mut trocados = Vec::new();
    sanear(&mut dados, &fontes, &mut trocados);

    let mut arquivos = HashMap::new();
    arquivos.insert(id("dados.json")?, Bytes::new(serde_json::to_vec(&dados)?));
    for (caminho, bytes) in &e.arquivos {
        conferir_imagem(caminho, bytes)?;
        arquivos.insert(id(caminho)?, Bytes::new(bytes.clone()));
    }
    let m = e.momento;
    let mundo = Mundo {
        library: LazyHash::new(Library::default()),
        book: LazyHash::new(FontBook::from_fonts(&fontes)),
        fontes,
        principal: Source::new(id("relatorio.typ")?, e.modelo.fonte().to_string()),
        arquivos,
        hoje: Datetime::from_ymd(m.year(), m.month() as u8, m.day() as u8).ok_or_else(data_invalida)?,
    };

    let r = typst::compile::<PagedDocument>(&mundo);
    let mut avisos: Vec<String> = r.warnings.iter().map(|d| d.message.to_string()).collect();
    let doc = r.output.map_err(|x| SicroError::Workspace(format!("relatório em PDF: {}", erros(&x))))?;
    if !trocados.is_empty() {
        trocados.sort_unstable();
        trocados.dedup();
        let lista: String = trocados.iter().collect();
        avisos.push(format!("caracteres que as fontes do relatório não desenham foram trocados por {SUBSTITUTO}: {lista}"));
    }
    Ok((doc, avisos))
}

/// Padrões do PDF arquivável: com anexo só o PDF/A-3 serve; sem anexo, PDF/A-2b (§10.1).
fn padroes(e: &Entrada) -> &'static [PdfStandard] {
    let com_anexos = e.dados.get("anexos").and_then(serde_json::Value::as_array).is_some_and(|a| !a.is_empty());
    match (e.pdf_a, com_anexos) {
        (false, _) => &[],
        (true, true) => &[PdfStandard::A_3a, PdfStandard::Ua_1],
        (true, false) => &[PdfStandard::A_2b, PdfStandard::Ua_1],
    }
}

fn exportar(e: &Entrada, doc: &PagedDocument, avisos: Vec<String>) -> Result<Saida> {
    exportar_com(e, doc, avisos, padroes(e))
}

/// Exporta nos `padroes` dados; recusados, e com `pdf_a` pedido, sai o PDF comum com o motivo em `avisos`.
fn exportar_com(e: &Entrada, doc: &PagedDocument, mut avisos: Vec<String>, padroes: &[PdfStandard]) -> Result<Saida> {
    let m = e.momento;
    let quando = Datetime::from_ymd_hms(m.year(), m.month() as u8, m.day() as u8, m.hour() as u8, m.minute() as u8, m.second() as u8)
        .ok_or_else(data_invalida)?;
    let com = |padroes: &[PdfStandard]| -> std::result::Result<Vec<u8>, String> {
        let opcoes = PdfOptions {
            ident: Smart::Custom(e.ident.clone()),
            creator: Smart::Custom(Some(format!("SICRO {}", env!("CARGO_PKG_VERSION")))),
            timestamp: Some(Timestamp::new_utc(quando)),
            standards: PdfStandards::new(padroes).map_err(|x| x.message().to_string())?,
            ..PdfOptions::default()
        };
        typst_pdf::pdf(doc, &opcoes).map_err(|x| erros(&x))
    };
    let pdf = match com(padroes) {
        Ok(pdf) => pdf,
        // PDF/A recusado por motivo não previsto: sai o PDF comum, com aviso
        Err(motivo) if e.pdf_a => {
            let pdf = com(&[]).map_err(|x| SicroError::Workspace(format!("relatório em PDF (exportação): {x}")))?;
            avisos.push(format!("o relatório saiu em PDF comum, fora do padrão de arquivo (PDF/A): {motivo}"));
            pdf
        }
        Err(motivo) => return Err(SicroError::Workspace(format!("relatório em PDF (exportação): {motivo}"))),
    };
    Ok(Saida { pdf, paginas: doc.pages().len(), avisos })
}

/// Texto corrido do documento, página a página (só para os testes: os fluxos do PDF saem comprimidos).
#[cfg(test)]
pub(crate) fn texto(e: &Entrada) -> Result<String> {
    use typst::layout::{Frame, FrameItem};
    fn junta(f: &Frame, saida: &mut String) {
        for (_, item) in f.items() {
            match item {
                FrameItem::Group(g) => junta(&g.frame, saida),
                FrameItem::Text(t) => {
                    saida.push_str(&t.text);
                    saida.push(' ');
                }
                _ => {}
            }
        }
    }
    let r = compilar(e).map(|(doc, _)| {
        let mut s = String::new();
        for p in doc.pages() {
            junta(&p.frame, &mut s);
            s.push('\n');
        }
        // espaços simples: a quebra de linha do parágrafo não separa a frase procurada
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    });
    typst::comemo::evict(0);
    r
}

#[cfg(test)]
pub(crate) mod testes {
    use super::*;
    use chrono::TimeZone;

    /// O exemplo do protótipo (`p4`), no contrato `/2`.
    pub(crate) fn entrada() -> Entrada {
        let img: &[(&str, &[u8])] = &[
            ("img/1.jpg", include_bytes!("exemplo/img/1.jpg")),
            ("img/2.jpg", include_bytes!("exemplo/img/2.jpg")),
            ("img/4.jpg", include_bytes!("exemplo/img/4.jpg")),
            ("img/5.jpg", include_bytes!("exemplo/img/5.jpg")),
            ("img/6.png", include_bytes!("exemplo/img/6.png")),
            ("tecnico.json", include_bytes!("exemplo/tecnico.json")),
        ];
        Entrada {
            modelo: Modelo::Juri,
            dados: serde_json::from_slice(include_bytes!("exemplo/dados.json")).unwrap(),
            arquivos: img.iter().map(|(n, b)| (n.to_string(), b.to_vec())).collect(),
            momento: Utc.with_ymd_and_hms(2026, 11, 13, 23, 40, 12).unwrap(),
            ident: "teste".into(),
            pdf_a: true,
        }
    }

    pub(crate) fn contem(pdf: &[u8], agulha: &[u8]) -> bool {
        pdf.windows(agulha.len()).any(|w| w == agulha)
    }

    // ---- os 7 testes do protótipo

    #[test]
    fn gera_pdf_a_com_anexo() {
        let s = gerar(&entrada()).unwrap();
        assert!(s.pdf.starts_with(b"%PDF-1.7"));
        assert!(s.paginas >= 5, "páginas: {}", s.paginas);
        assert!(s.avisos.is_empty(), "{:?}", s.avisos);
        // O XMP do PDF/A fica sem compressão.
        assert!(contem(&s.pdf, b"<pdfaid:part>3</pdfaid:part>"));
        assert!(contem(&s.pdf, b"<pdfaid:conformance>A</pdfaid:conformance>"));
        assert!(contem(&s.pdf, b"<pdfuaid:part>1</pdfuaid:part>"));
        assert!(contem(&s.pdf, b"/AFRelationship"));
    }

    #[test]
    fn mesma_entrada_mesmos_bytes() {
        let a = gerar(&entrada()).unwrap().pdf;
        let b = gerar(&entrada()).unwrap().pdf;
        assert!(a == b);
        let mut outra = entrada();
        outra.dados["saida"]["duracao"] = "38,0 segundos".into();
        assert!(gerar(&outra).unwrap().pdf != a);
    }

    #[test]
    fn texto_do_perito_nao_vira_codigo() {
        let mut e = entrada();
        let perigoso = r##"#panic("x") *a* _b_ $c$ @d <e> [f] \ // `g`"##;
        e.dados["roteiro"][1]["descricao"] = perigoso.into();
        e.dados["limitacoes"][0] = "#import \"nada.typ\": *".into();
        assert!(gerar(&e).is_ok());
        // e sai escrito como foi digitado
        let t = texto(&e).unwrap();
        assert!(t.contains(r##"#panic("x") *a* _b_ $c$ @d <e> [f]"##), "{t}");
        assert!(t.contains("#import \"nada.typ\": *"));
    }

    #[test]
    fn caractere_sem_glifo_vira_quadrado_com_aviso() {
        let mut e = entrada();
        e.dados["roteiro"][1]["descricao"] = "emoji 😀 e ideograma 中".into();
        let s = gerar(&e).unwrap();
        assert_eq!(s.avisos.len(), 1);
        assert!(s.avisos[0].contains('😀') && s.avisos[0].contains('中'));
        assert!(texto(&e).unwrap().contains("emoji □ e ideograma □"));
    }

    #[test]
    fn miniatura_ausente_da_erro_legivel() {
        let mut e = entrada();
        e.arquivos.retain(|(n, _)| n != "img/2.jpg");
        let erro = gerar(&e).unwrap_err().to_string();
        assert!(erro.contains("img/2.jpg"), "{erro}");
    }

    #[test]
    fn relatorio_minimo_sem_listas() {
        let mut e = entrada();
        for k in [
            "acrescimos", "mudancas", "sincronizacao", "processamentos", "diferencas", "ocultacoes", "limitacoes", "anexos", "pacote",
            "selos_desligados", "relogios", "medidas", "modelos_ia", "rastreios", "ia_resumo", "referencias",
        ] {
            e.dados[k] = serde_json::json!([]);
        }
        let primeiro = e.dados["roteiro"][0].clone();
        e.dados["roteiro"] = serde_json::json!([primeiro]);
        e.arquivos.clear();
        let s = gerar(&e).unwrap();
        assert!(s.paginas >= 2);
        assert!(s.avisos.is_empty(), "{:?}", s.avisos);
        // sem anexos, o arquivável é o PDF/A-2b
        assert!(contem(&s.pdf, b"<pdfaid:part>2</pdfaid:part>") && contem(&s.pdf, b"<pdfaid:conformance>B</pdfaid:conformance>"));
        assert!(!contem(&s.pdf, b"/AFRelationship"));
    }

    #[test]
    fn sem_pdf_a_tambem_gera() {
        let mut e = entrada();
        e.pdf_a = false;
        let s = gerar(&e).unwrap();
        assert!(!contem(&s.pdf, b"pdfaid:part"));
    }

    // ---- além do protótipo

    /// Anexo pedido no modelo sem o arquivo: o erro diz qual.
    #[test]
    fn anexo_ausente_da_erro_com_o_nome() {
        let mut e = entrada();
        e.arquivos.retain(|(n, _)| n != "tecnico.json");
        let erro = gerar(&e).unwrap_err().to_string();
        assert!(erro.contains("tecnico.json"), "{erro}");
    }

    /// Saída de emergência (§10.3): padrão de arquivo recusado na exportação (aqui, PDF/A-2b com anexo, que o
    /// PDF/A-2 não aceita) não derruba o pacote: sai o PDF comum, com o motivo para o perito.
    #[test]
    fn pdf_a_recusado_sai_em_pdf_comum_com_aviso() {
        let e = entrada();
        let (doc, avisos) = compilar(&e).unwrap();
        assert!(avisos.is_empty());
        let s = exportar_com(&e, &doc, avisos, &[PdfStandard::A_2b]).unwrap();
        typst::comemo::evict(0);
        assert_eq!(s.avisos.len(), 1, "{:?}", s.avisos);
        assert!(s.avisos[0].starts_with("o relatório saiu em PDF comum, fora do padrão de arquivo (PDF/A): "), "{}", s.avisos[0]);
        assert!(s.pdf.starts_with(b"%PDF-") && !contem(&s.pdf, b"pdfaid:part") && s.paginas >= 5);
        // sem PDF/A pedido não há segunda tentativa: o erro sobe
        let mut comum = entrada();
        comum.pdf_a = false;
        let (doc, avisos) = compilar(&comum).unwrap();
        let erro = exportar_com(&comum, &doc, avisos, &[PdfStandard::A_2b]).unwrap_err().to_string();
        typst::comemo::evict(0);
        assert!(erro.contains("relatório em PDF (exportação)"), "{erro}");
    }

    /// Miniatura de 0 bytes, que não é imagem ou cortada no meio (render interrompido): erro com o nome do arquivo,
    /// nunca meia imagem no relatório.
    #[test]
    fn miniatura_invalida_da_erro_com_o_nome() {
        let inteira: &[u8] = include_bytes!("exemplo/img/2.jpg");
        let png: &[u8] = include_bytes!("exemplo/img/6.png");
        for (nome, bytes) in [("img/2.jpg", &inteira[..0]), ("img/2.jpg", b"lixo".as_slice()), ("img/2.jpg", &inteira[..2000]), ("img/2.jpg", &inteira[..inteira.len() - 2]), ("img/6.png", &png[..png.len() / 2])] {
            let mut e = entrada();
            e.arquivos.iter_mut().filter(|a| a.0 == nome).for_each(|a| a.1 = bytes.to_vec());
            let erro = gerar(&e).unwrap_err().to_string();
            assert!(erro.contains(&format!("imagem inválida ({nome})")), "{} bytes: {erro}", bytes.len());
        }
        // o que não é imagem não passa por essa conferência
        assert!(conferir_imagem("tecnico.json", b"").is_ok() && conferir_imagem("img/2.jpg", inteira).is_ok() && conferir_imagem("IMG/6.PNG", png).is_ok());
    }

    /// Texto colado de outro programa: controle e marca invisível não derrubam o PDF/A nem viram □; quebra de linha
    /// e tabulação ficam; o que tem desenho fica como está.
    #[test]
    fn controle_e_invisivel_saem_sem_derrubar_o_pdf_a() {
        let mut e = entrada();
        e.dados["roteiro"][1]["descricao"] = "a\u{0}b\u{7}c\u{2060}d\u{FE0F}e\r\nf\tg \u{200B}h\u{AD}i 😀\u{FE0F}".into();
        e.dados["caso"]["requisitante"] = "\u{FEFF}1ª Vara\u{1b}\u{FFFE} \u{E000}".into();
        let s = gerar(&e).unwrap();
        // só o que tinha desenho a mostrar vira □ com aviso
        assert_eq!(s.avisos, ["caracteres que as fontes do relatório não desenham foram trocados por □: \u{E000}😀"], "{:?}", s.avisos);
        assert!(contem(&s.pdf, b"<pdfaid:part>3</pdfaid:part>"), "saiu fora do PDF/A");
        let mut d = e.dados.clone();
        sanear(&mut d, &FONTES.iter().flat_map(|b| Font::iter(Bytes::new(*b))).collect::<Vec<_>>(), &mut Vec::new());
        assert_eq!(d["roteiro"][1]["descricao"], "abcde\nf\tg \u{200B}h\u{AD}i □");
        assert_eq!(d["caso"]["requisitante"], "1ª Vara □");
    }

    /// A data e o identificador vêm de quem chama: outro momento ou outro vídeo, outros bytes.
    #[test]
    fn momento_e_ident_entram_no_arquivo() {
        let a = gerar(&entrada()).unwrap().pdf;
        let mut e = entrada();
        e.momento = Utc.with_ymd_and_hms(2026, 11, 14, 0, 0, 0).unwrap();
        assert!(gerar(&e).unwrap().pdf != a);
        let mut e = entrada();
        e.ident = "outro".into();
        assert!(gerar(&e).unwrap().pdf != a);
    }
}
