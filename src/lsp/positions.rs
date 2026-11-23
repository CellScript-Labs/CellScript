use super::{Position, Range};
use crate::lexer::token::TokenKind;

pub(super) fn position_to_offset(source: &str, position: Position) -> Option<usize> {
    let mut line = 0u32;
    let mut col = 0u32;

    for (idx, ch) in source.char_indices() {
        if line == position.line && col == position.character {
            return Some(idx);
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col = col.checked_add(ch.len_utf16() as u32)?;
            if line == position.line && col == position.character {
                return Some(idx + ch.len_utf8());
            }
            if line == position.line && col > position.character {
                return None;
            }
        }
    }

    if line == position.line && col == position.character {
        Some(source.len())
    } else {
        None
    }
}

pub(super) fn offset_to_position(source: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut col = 0u32;
    for (idx, ch) in source.char_indices() {
        if idx >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16() as u32;
        }
    }
    Position { line, character: col }
}

pub(super) fn end_position(source: &str) -> Position {
    offset_to_position(source, source.len())
}

pub(super) fn ranges_overlap(left: Range, right: Range) -> bool {
    position_le(left.start, right.end) && position_le(right.start, left.end)
}

pub(super) fn position_le(left: Position, right: Position) -> bool {
    left.line < right.line || (left.line == right.line && left.character <= right.character)
}

pub(super) fn is_ident_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

pub(super) fn word_at_offset(source: &str, offset: usize) -> Option<String> {
    if source.is_empty() || offset > source.len() {
        return None;
    }
    let mut start = offset;
    while start > 0 {
        let prev_idx = source[..start].char_indices().last()?.0;
        let ch = source[prev_idx..start].chars().next()?;
        if !is_ident_char(ch) {
            break;
        }
        start = prev_idx;
    }

    let mut end = offset;
    while end < source.len() {
        let ch = source[end..].chars().next()?;
        if !is_ident_char(ch) {
            break;
        }
        end += ch.len_utf8();
    }

    if start == end {
        None
    } else {
        Some(source[start..end].to_string())
    }
}

/// Get the word immediately before the given offset in `source`.
/// Unlike `word_at_offset`, this scans backwards from `offset` and stops at
/// the first non-identifier character, returning the identifier that ends
/// just before `offset`.
pub(super) fn word_before_offset(source: &str, offset: usize) -> Option<String> {
    if source.is_empty() || offset == 0 || offset > source.len() {
        return None;
    }
    // Skip trailing whitespace.
    let mut end = offset;
    while end > 0 {
        let prev_idx = source[..end].char_indices().last()?.0;
        let ch = source[prev_idx..end].chars().next()?;
        if !ch.is_whitespace() {
            break;
        }
        end = prev_idx;
    }
    if end == 0 {
        return None;
    }
    // Scan the identifier backwards.
    let mut start = end;
    while start > 0 {
        let prev_idx = source[..start].char_indices().last()?.0;
        let ch = source[prev_idx..start].chars().next()?;
        if !is_ident_char(ch) {
            break;
        }
        start = prev_idx;
    }
    if start == end {
        None
    } else {
        Some(source[start..end].to_string())
    }
}

pub(super) fn word_occurrences(source: &str, symbol: &str) -> Vec<(usize, usize)> {
    let mut matches = Vec::new();
    if symbol.is_empty() {
        return matches;
    }

    let Ok(tokens) = crate::lexer::lex(source) else {
        return matches;
    };
    for token in tokens {
        if let TokenKind::Identifier(name) = token.kind
            && name == symbol
        {
            matches.push((token.span.start, token.span.end));
        }
    }
    matches
}
