use crate::lexer::token::{keyword_or_identifier, TokenKind};
use camino::Utf8PathBuf;

pub(super) fn file_uri_to_utf8_path(uri: &str) -> Option<Utf8PathBuf> {
    let path = uri.strip_prefix("file://")?;
    let decoded = percent_decode(path)?;
    let candidate = Utf8PathBuf::from(decoded);
    std::fs::canonicalize(&candidate).ok().and_then(|path| Utf8PathBuf::from_path_buf(path).ok()).or(Some(candidate))
}

pub(super) fn is_valid_rename_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_alphabetic() || first == '_') {
        return false;
    }
    if !chars.all(|ch| ch.is_alphanumeric() || ch == '_') {
        return false;
    }
    matches!(keyword_or_identifier(name), TokenKind::Identifier(_))
}

pub(super) fn utf8_path_to_file_uri(path: &camino::Utf8Path) -> String {
    format!("file://{}", path)
}

pub(super) fn same_workspace_path(left: &camino::Utf8Path, right: &camino::Utf8Path) -> bool {
    left == right
        || std::fs::canonicalize(left).ok().zip(std::fs::canonicalize(right).ok()).map(|(left, right)| left == right).unwrap_or(false)
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx] == b'%' {
            if idx + 2 >= bytes.len() {
                return None;
            }
            let hi = hex_nibble(bytes[idx + 1])?;
            let lo = hex_nibble(bytes[idx + 2])?;
            out.push((hi << 4) | lo);
            idx += 3;
        } else {
            out.push(bytes[idx]);
            idx += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(10 + byte - b'a'),
        b'A'..=b'F' => Some(10 + byte - b'A'),
        _ => None,
    }
}
