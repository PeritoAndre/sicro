//! Path sanitization helpers.
//!
//! The workspace folder name is user-derived (built from BO number, type,
//! city, etc.), so we must scrub characters that are illegal on Windows or
//! that could cause confusion (reserved device names, leading/trailing dots
//! and spaces, control chars).

/// Replace anything that is not safe for a file/folder name with `_`.
///
/// Rules:
///   - Strip leading/trailing whitespace and dots.
///   - Reject empty results (substitute "ocorrencia").
///   - Replace any of `<>:"/\\|?*` and ASCII control chars with `_`.
///   - Limit length to 100 chars so the full path stays well under Windows' 260.
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
        // Ensure we don't end on a half multi-byte sequence: truncate at char boundary.
        while !out.is_char_boundary(out.len()) {
            out.pop();
        }
    }

    // Avoid Windows reserved device names.
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
