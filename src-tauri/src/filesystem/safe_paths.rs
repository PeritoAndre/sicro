//! Nome de pasta derivado de dados do usuário (BO, tipo, município): limpa o
//! que é ilegal no Windows (chars, nomes reservados, pontos/espaços nas pontas).

/// Troca o que não é seguro por `_`; apara espaços e pontos; vazio → "ocorrencia";
/// corta em 100 chars (caminho completo fica longe dos 260 do Windows).
pub fn sanitize_folder_name(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches('.');
    if trimmed.is_empty() {
        return "ocorrencia".to_string();
    }

    let mut out = String::with_capacity(trimmed.len());
    for c in trimmed.chars() {
        let safe = match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        };
        out.push(safe);
    }

    if out.len() > 100 {
        out.truncate(100);
        // Não cortar no meio de um char multibyte.
        while !out.is_char_boundary(out.len()) {
            out.pop();
        }
    }

    // Nomes reservados do Windows.
    let upper = out.to_uppercase();
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
        "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.iter().any(|r| upper == *r) {
        out.push('_');
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_illegal_chars() {
        assert_eq!(sanitize_folder_name("BO 12/2026"), "BO 12_2026");
        assert_eq!(sanitize_folder_name("a<b>c"), "a_b_c");
    }

    #[test]
    fn empty_falls_back() {
        assert_eq!(sanitize_folder_name(""), "ocorrencia");
        assert_eq!(sanitize_folder_name("...   "), "ocorrencia");
    }

    #[test]
    fn reserved_names_get_suffix() {
        assert_eq!(sanitize_folder_name("CON"), "CON_");
        assert_eq!(sanitize_folder_name("aux"), "aux_");
    }

    #[test]
    fn truncates_long_names() {
        let name = "x".repeat(250);
        assert!(sanitize_folder_name(&name).len() <= 100);
    }
}
