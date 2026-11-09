//! Canonical layout over the lossless token/trivia document. Comments and
//! grouping remain source-owned; semantic AST comparison guards every rewrite.
use super::*;
use crate::lexer::token::TokenKind;
use crate::syntax::{SyntaxDocument, SyntaxKind, TriviaKind};
use std::collections::BTreeSet;

struct Layout<'a> {
    config: &'a FormatConfig,
    output: String,
    indent: usize,
    column: usize,
    pending_break: bool,
    line_start: bool,
}

impl Layout<'_> {
    fn newline(&mut self) {
        while self.output.ends_with(' ') {
            self.output.pop();
        }
        if !self.output.is_empty() && !self.output.ends_with('\n') {
            self.output.push('\n');
        }
        self.column = 0;
        self.line_start = true;
        self.pending_break = false;
    }

    fn blank_lines(&mut self, count: usize) {
        self.newline();
        if !self.output.is_empty() {
            for _ in 0..count {
                self.output.push('\n');
            }
        }
    }

    fn write(&mut self, text: &str, space: bool) {
        if self.line_start {
            let width = self.indent.saturating_mul(self.config.indent_width);
            self.output.extend(std::iter::repeat_n(' ', width));
            self.column = width;
        } else if space && !self.output.ends_with([' ', '\n']) {
            self.output.push(' ');
            self.column += 1;
        }
        self.output.push_str(text);
        self.line_start = text.ends_with('\n');
        self.column = text.rsplit_once('\n').map_or(self.column + text.chars().count(), |(_, tail)| tail.chars().count());
    }
}

fn joined_punctuation(previous: Option<&TokenKind>, current: &TokenKind, contiguous: bool) -> bool {
    use TokenKind::*;
    matches!(current, Comma | Semi | RParen | RBracket | Dot | ColonColon | Colon)
        || matches!(previous, Some(LParen | LBracket | Dot | ColonColon))
        || contiguous && matches!((previous, current), (Some(Lt), Lt) | (Some(Gt), Gt) | (Some(Plus), Eq) | (Some(Dot), Eq))
}

fn continues_after_brace(kind: &TokenKind, text: &str) -> bool {
    use TokenKind::*;
    matches!(
        kind,
        Comma
            | Semi
            | RParen
            | RBracket
            | Dot
            | ColonColon
            | LParen
            | LBracket
            | Plus
            | Minus
            | Star
            | Slash
            | Percent
            | Eq
            | EqEq
            | NotEq
            | Lt
            | Le
            | Gt
            | Ge
            | And
            | Or
            | Ampersand
            | Pipe
            | Caret
            | Else
    ) || matches!(text, "as" | "with_lock")
}

pub(super) fn format_document(document: &SyntaxDocument<'_>, config: &FormatConfig) -> Result<String> {
    if config.indent_width > 16 || config.max_line_width == 0 || config.max_line_width > 4096 || config.blank_lines_between_items > 16
    {
        return Err(CompileError::without_span(
            "formatter configuration exceeds its bounded layout limits (indent 0..16, width 1..4096, blank lines 0..16)",
        ));
    }
    let items =
        document.nodes.iter().filter(|node| node.kind == SyntaxKind::Item).map(|node| node.span.start).collect::<BTreeSet<_>>();
    let mut ranges = document
        .nodes
        .iter()
        .filter(|node| matches!(node.kind, SyntaxKind::Type | SyntaxKind::TypeParameters | SyntaxKind::GenericArguments))
        .map(|node| (node.span.start, node.span.end))
        .collect::<Vec<_>>();
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut()
            && start <= last.1
        {
            last.1 = last.1.max(end);
        } else {
            merged.push((start, end));
        }
    }
    let mut range_index = 0;
    let mut type_angles = BTreeSet::new();
    for token in &document.tokens {
        while range_index < merged.len() && merged[range_index].1 <= token.span.start {
            range_index += 1;
        }
        if range_index < merged.len()
            && merged[range_index].0 <= token.span.start
            && token.span.end <= merged[range_index].1
            && matches!(token.kind, TokenKind::Lt | TokenKind::Gt)
        {
            type_angles.insert(token.span.start);
        }
    }
    let mut layout = Layout {
        config,
        output: String::with_capacity(document.source.len()),
        indent: 0,
        column: 0,
        pending_break: false,
        line_start: true,
    };
    let mut cursor = 0;
    let mut original_newline = true;
    let prefixes = document
        .nodes
        .iter()
        .filter(|node| node.kind == SyntaxKind::PrefixExpression)
        .map(|node| node.span.start)
        .collect::<BTreeSet<_>>();
    let mut previous: Option<&TokenKind> = None;
    let mut previous_prefix = false;
    let mut previous_type_angle = false;
    let mut previous_end = 0;
    let mut delimiters = Vec::new();
    let mut last_comment = false;
    let mut item_prepared = None;
    for piece in document.pieces() {
        if piece.span.start != cursor {
            return Err(CompileError::without_span("incomplete lossless syntax information; source was not modified"));
        }
        cursor = piece.span.end;
        let source = &document.source[piece.span.start..piece.span.end];
        if piece.trivia == Some(TriviaKind::Whitespace) {
            continue;
        }
        let kind = piece.token_index.map(|index| &document.tokens[index].kind);
        if matches!(kind, Some(TokenKind::Newline)) {
            original_newline = true;
            continue;
        }
        if matches!(kind, Some(TokenKind::Eof)) {
            continue;
        }
        if matches!(piece.trivia, Some(TriviaKind::LineComment | TriviaKind::BlockComment)) {
            if original_newline || layout.pending_break && layout.line_start {
                layout.newline();
            }
            // Ordinary and doc comments retain their exact byte contents and
            // their order relative to every executable token and scope brace.
            layout.write(source, !layout.line_start);
            if piece.trivia == Some(TriviaKind::LineComment) || source.contains('\n') {
                layout.newline();
            }
            last_comment = true;
            original_newline = false;
            continue;
        }
        let Some(kind) = kind else {
            return Err(CompileError::without_span("unknown lossless syntax piece"));
        };
        if items.contains(&piece.span.start) && item_prepared != Some(piece.span.start) {
            if !last_comment {
                layout.blank_lines(config.blank_lines_between_items);
            }
            item_prepared = Some(piece.span.start);
        }
        let closing_brace = matches!(kind, TokenKind::RBrace);
        let empty_brace = closing_brace && matches!(previous, Some(TokenKind::LBrace)) && !last_comment;
        if closing_brace {
            layout.indent = layout.indent.saturating_sub(1);
            if !empty_brace {
                layout.newline();
            } else {
                layout.pending_break = false;
            }
        } else if layout.pending_break {
            if matches!(previous, Some(TokenKind::RBrace)) && continues_after_brace(kind, source) {
                layout.pending_break = false;
            } else {
                layout.newline();
            }
        }
        let angle = type_angles.contains(&piece.span.start);
        let postfix = matches!(
            previous,
            Some(
                TokenKind::Identifier(_)
                    | TokenKind::RParen
                    | TokenKind::RBracket
                    | TokenKind::RBrace
                    | TokenKind::Address
                    | TokenKind::Hash
            )
        ) || previous_type_angle && matches!(previous, Some(TokenKind::Gt));
        let space = !joined_punctuation(previous, kind, previous_end == piece.span.start)
            && !matches!((previous, kind), (Some(TokenKind::Public), TokenKind::LParen))
            && !empty_brace
            && !angle
            && !(previous_type_angle && matches!(previous, Some(TokenKind::Lt)))
            && (!previous_prefix || last_comment)
            && !(postfix && matches!(kind, TokenKind::LParen | TokenKind::LBracket));
        match kind {
            TokenKind::String(value) => layout.write(&format_string_literal(value), space),
            _ => layout.write(source, space),
        }
        match kind {
            TokenKind::Lt if angle => delimiters.push(kind),
            TokenKind::Gt if angle => {
                delimiters.pop();
            }
            TokenKind::LBrace => {
                delimiters.push(kind);
                layout.indent += 1;
                layout.pending_break = true;
            }
            TokenKind::LParen | TokenKind::LBracket => delimiters.push(kind),
            TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket => {
                delimiters.pop();
                if closing_brace {
                    layout.pending_break = true;
                }
            }
            TokenKind::Semi if !matches!(delimiters.last(), Some(TokenKind::LBracket)) => layout.pending_break = true,
            TokenKind::Comma if matches!(delimiters.last(), Some(TokenKind::LBrace)) || layout.column >= config.max_line_width => {
                layout.pending_break = true
            }
            _ => {}
        }
        previous = Some(kind);
        previous_prefix = prefixes.contains(&piece.span.start);
        previous_type_angle = angle;
        previous_end = piece.span.end;
        original_newline = false;
        last_comment = false;
        if layout.output.len() > crate::MAX_SOURCE_BYTES.saturating_mul(4) {
            return Err(CompileError::without_span("formatted source exceeds the bounded output budget; source was not modified"));
        }
    }
    if cursor != document.source.len() {
        return Err(CompileError::without_span("lossless syntax did not cover the document; source was not modified"));
    }
    while layout.output.ends_with([' ', '\n']) {
        layout.output.pop();
    }
    if config.trailing_newline {
        layout.output.push('\n');
    }
    Ok(layout.output)
}
