//! Todos os metadados de um arquivo: exiftool quando existe (o mesmo `-a -u -G1` do yazi),
//! senão o leitor interno (EXIF completo + XMP + arquivo).

use std::io::Read;
use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct MetaEntry {
    pub group: String,
    pub tag: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FullMetadata {
    /// "exiftool 13.10" ou "leitor interno".
    pub source: String,
    pub entries: Vec<MetaEntry>,
}

pub fn read_all(path: &Path) -> FullMetadata {
    if let Some(m) = via_exiftool(path) {
        return m;
    }
    FullMetadata { source: "leitor interno".into(), entries: internal(path) }
}

fn via_exiftool(path: &Path) -> Option<FullMetadata> {
    let out = crate::tools::command(crate::tools::find_exiftool()?)
        .args(["-j", "-a", "-u", "-G1", "-s", "-api", "largefilesupport=1", "-charset", "filename=utf8"])
        .arg(path)
        .output()
        .ok()?;
    if !out.status.success() && out.stdout.is_empty() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let obj = v.as_array()?.first()?.as_object()?;
    let mut version = String::new();
    let mut entries = Vec::with_capacity(obj.len());
    for (k, val) in obj {
        if k == "SourceFile" {
            continue;
        }
        let (group, tag) = k.split_once(':').unwrap_or(("", k));
        let value = match val {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(a) => a
                .iter()
                .map(|x| x.as_str().map(str::to_string).unwrap_or_else(|| x.to_string()))
                .collect::<Vec<_>>()
                .join(", "),
            other => other.to_string(),
        };
        if tag == "ExifToolVersion" {
            version = value.clone();
        }
        entries.push(MetaEntry { group: group.into(), tag: tag.into(), value });
    }
    Some(FullMetadata {
        source: if version.is_empty() { "exiftool".into() } else { format!("exiftool {version}") },
        entries,
    })
}

fn internal(path: &Path) -> Vec<MetaEntry> {
    let mut v = Vec::new();
    let push = |v: &mut Vec<MetaEntry>, g: &str, t: &str, val: String| {
        v.push(MetaEntry { group: g.into(), tag: t.into(), value: val })
    };
    if let Ok(m) = std::fs::metadata(path) {
        push(&mut v, "System", "FileName", path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default());
        push(&mut v, "System", "FileSize", format!("{} bytes", m.len()));
    }
    if let Ok((w, h)) = image::image_dimensions(path) {
        push(&mut v, "File", "ImageWidth", w.to_string());
        push(&mut v, "File", "ImageHeight", h.to_string());
    }
    // EXIF completo (todas as IFDs).
    if let Ok(file) = std::fs::File::open(path) {
        let mut r = std::io::BufReader::new(file);
        if let Ok(exif) = exif::Reader::new().read_from_container(&mut r) {
            for f in exif.fields() {
                let group = match (f.ifd_num, f.tag.context()) {
                    (_, exif::Context::Gps) => "GPS",
                    (_, exif::Context::Interop) => "InteropIFD",
                    (_, exif::Context::Exif) => "ExifIFD",
                    (exif::In::THUMBNAIL, _) => "IFD1",
                    _ => "IFD0",
                };
                let val = f.display_value().with_unit(&exif).to_string();
                push(&mut v, group, &f.tag.to_string(), val);
            }
        }
    }
    // XMP: atributos e elementos simples do pacote (DJI, Adobe…).
    if let Some(xmp) = xmp_packet(path) {
        for (prefix, name, val) in xmp_pairs(&xmp) {
            push(&mut v, &format!("XMP-{prefix}"), &name, val);
        }
    }
    v
}

/// Primeiro pacote `<x:xmpmeta …>…</x:xmpmeta>` nos primeiros 4 MB do arquivo.
fn xmp_packet(path: &Path) -> Option<String> {
    let mut buf = Vec::new();
    std::fs::File::open(path).ok()?.take(4 * 1024 * 1024).read_to_end(&mut buf).ok()?;
    let start = find(&buf, b"<x:xmpmeta")?;
    let end = find(&buf[start..], b"</x:xmpmeta>")? + start + 12;
    Some(String::from_utf8_lossy(&buf[start..end]).into_owned())
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// `prefix:Nome="valor"` e `<prefix:Nome>valor</prefix:Nome>` (sem rdf/xmlns/x).
fn xmp_pairs(xml: &str) -> Vec<(String, String, String)> {
    let skip = |p: &str| matches!(p, "rdf" | "xmlns" | "x" | "xml");
    let mut out: Vec<(String, String, String)> = Vec::new();
    let b = xml.as_bytes();
    let mut i = 0;
    while i < b.len() {
        // atributo  prefix:Name="value"
        if b[i].is_ascii_alphabetic() && (i == 0 || b[i - 1].is_ascii_whitespace()) {
            let s = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b':' || b[i] == b'-' || b[i] == b'_') {
                i += 1;
            }
            let key = &xml[s..i];
            if i + 1 < b.len() && b[i] == b'=' && (b[i + 1] == b'"' || b[i + 1] == b'\'') {
                let q = b[i + 1];
                let vs = i + 2;
                let mut ve = vs;
                while ve < b.len() && b[ve] != q {
                    ve += 1;
                }
                if let Some((p, n)) = key.split_once(':') {
                    if !skip(p) {
                        out.push((p.into(), n.into(), unescape(&xml[vs..ve.min(b.len())])));
                    }
                }
                i = ve + 1;
                continue;
            }
            continue;
        }
        // elemento simples  <prefix:Name>value</prefix:Name>
        if b[i] == b'<' && i + 1 < b.len() && b[i + 1].is_ascii_alphabetic() {
            let s = i + 1;
            let mut e = s;
            while e < b.len() && b[e] != b'>' && !b[e].is_ascii_whitespace() && b[e] != b'/' {
                e += 1;
            }
            let key = &xml[s..e];
            if e < b.len() && b[e] == b'>' {
                let close = format!("</{key}>");
                if let Some(rel) = xml[e + 1..].find(&close) {
                    let inner = &xml[e + 1..e + 1 + rel];
                    if !inner.contains('<') {
                        if let Some((p, n)) = key.split_once(':') {
                            if !skip(p) && !inner.trim().is_empty() {
                                out.push((p.into(), n.into(), unescape(inner.trim())));
                            }
                        }
                    }
                }
            }
            i = e;
            continue;
        }
        i += 1;
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::{internal, read_all, xmp_pairs};

    /// `SICRO_TEST_IMAGE=foto.jpg cargo test -- --ignored contagem` mostra quantos campos cada leitor acha.
    #[test]
    #[ignore]
    fn contagem() {
        let p = std::path::PathBuf::from(std::env::var("SICRO_TEST_IMAGE").expect("SICRO_TEST_IMAGE"));
        let all = read_all(&p);
        let int = internal(&p);
        println!("{}: {} campos | leitor interno: {} campos", all.source, all.entries.len(), int.len());
        for e in int.iter().filter(|e| e.group.starts_with("XMP-drone")).take(5) {
            println!("  {} {} = {}", e.group, e.tag, e.value);
        }
    }

    #[test]
    fn xmp_dji_atributos_e_elementos() {
        let xml = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF><rdf:Description rdf:about="DJI Meta Data"
            xmlns:drone-dji="http://www.dji.com/drone-dji/1.0/"
            drone-dji:AbsoluteAltitude="+27.10" drone-dji:GimbalYawDegree="-90.30">
            <tiff:Make>DJI</tiff:Make></rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let p = xmp_pairs(xml);
        assert!(p.contains(&("drone-dji".into(), "AbsoluteAltitude".into(), "+27.10".into())));
        assert!(p.contains(&("drone-dji".into(), "GimbalYawDegree".into(), "-90.30".into())));
        assert!(p.contains(&("tiff".into(), "Make".into(), "DJI".into())));
        assert!(!p.iter().any(|(pre, _, _)| pre == "xmlns" || pre == "rdf"));
    }
}
