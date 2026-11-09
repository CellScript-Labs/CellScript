pub mod token;

use crate::error::{Result, SourceDiagnosticCode, Span};
use crate::syntax::{SyntaxDocument, Trivia, TriviaKind};
use token::{keyword_or_identifier, Token, TokenKind};

pub struct Lexer<'a> {
    input: &'a str,
    chars: std::str::Chars<'a>,
    position: usize,
    line: usize,
    column: usize,
    current: Option<char>,
    trivia: Vec<Trivia>,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        let mut chars = input.chars();
        let current = chars.next();
        Self { input, chars, position: 0, line: 1, column: 1, current, trivia: Vec::new() }
    }

    fn advance(&mut self) -> Option<char> {
        if let Some(c) = self.current {
            self.position += c.len_utf8();
            if c == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
        self.current = self.chars.next();
        self.current
    }

    fn peek(&self) -> Option<char> {
        self.current
    }

    fn peek_next(&self) -> Option<char> {
        self.input[self.position..].chars().nth(1)
    }

    fn skip_whitespace(&mut self) {
        let start = self.position;
        let line = self.line;
        let column = self.column;
        while let Some(c) = self.peek() {
            if c.is_whitespace() && c != '\n' {
                self.advance();
            } else {
                break;
            }
        }
        if self.position != start {
            self.trivia.push(Trivia { kind: TriviaKind::Whitespace, span: Span::new(start, self.position, line, column) });
        }
    }

    fn skip_comment(&mut self) -> Result<()> {
        let opening = Span::new(self.position, self.position + 2, self.line, self.column);
        let kind = if self.peek_next() == Some('/') { TriviaKind::LineComment } else { TriviaKind::BlockComment };
        if self.peek() == Some('/') && self.peek_next() == Some('/') {
            while let Some(c) = self.peek() {
                if c == '\n' {
                    break;
                }
                self.advance();
            }
        } else if self.peek() == Some('/') && self.peek_next() == Some('*') {
            self.advance(); // /
            self.advance(); // *
            let mut depth = 1;
            while depth > 0 {
                match (self.peek(), self.peek_next()) {
                    (Some('/'), Some('*')) => {
                        self.advance();
                        self.advance();
                        depth += 1;
                    }
                    (Some('*'), Some('/')) => {
                        self.advance();
                        self.advance();
                        depth -= 1;
                    }
                    (Some(_), _) => {
                        self.advance();
                    }
                    (None, _) => return Err(SourceDiagnosticCode::UnterminatedComment.error("unterminated block comment", opening)),
                }
            }
        }
        self.trivia.push(Trivia { kind, span: Span::new(opening.start, self.position, opening.line, opening.column) });
        Ok(())
    }

    fn read_identifier(&mut self) -> Token {
        let start = self.position;
        let start_line = self.line;
        let start_col = self.column;

        while let Some(c) = self.peek() {
            if unicode_ident::is_xid_continue(c) {
                self.advance();
            } else {
                break;
            }
        }

        let text = &self.input[start..self.position];
        let kind = keyword_or_identifier(text);
        let span = Span::new(start, self.position, start_line, start_col);

        Token::new(kind, span, text)
    }

    fn read_number(&mut self) -> Result<Token> {
        let start = self.position;
        let start_line = self.line;
        let start_col = self.column;
        // Consume the whole candidate, including invalid separators/suffixes.
        // A malformed number must never split into an integer plus identifier.
        while self.peek().is_some_and(|c| unicode_ident::is_xid_continue(c) || c == '_') {
            self.advance();
        }
        let text = &self.input[start..self.position];
        let span = Span::new(start, self.position, start_line, start_col);
        let (digits, radix) = text.strip_prefix("0x").map_or((text, 10), |digits| (digits, 16));
        let valid_digit = |c: char| c.is_ascii() && c.is_digit(radix);
        let mut previous_digit = false;
        let mut valid = !digits.is_empty();
        for c in digits.chars() {
            if c == '_' {
                valid &= previous_digit;
                previous_digit = false;
            } else {
                valid &= valid_digit(c);
                previous_digit = valid_digit(c);
            }
        }
        valid &= previous_digit;
        if !valid {
            return Err(SourceDiagnosticCode::InvalidIntegerLiteral.error(
                "invalid integer literal; use decimal digits or 0x hex digits with single underscores between digits; suffixes are unsupported",
                span,
            ));
        }
        let normalized = digits.replace('_', "");
        let value = u128::from_str_radix(&normalized, radix).map_err(|_| {
            SourceDiagnosticCode::InvalidIntegerLiteral.error("integer literal exceeds the supported 128-bit source range", span)
        })?;
        Ok(Token::new(TokenKind::Integer(value), span, text))
    }

    fn read_string(&mut self) -> Result<Token> {
        let start = self.position;
        let start_line = self.line;
        let start_col = self.column;

        let quote = '"';
        self.advance();

        let mut content = String::new();
        while let Some(c) = self.peek() {
            if c == quote {
                self.advance();
                let span = Span::new(start, self.position, start_line, start_col);
                return Ok(Token::new(TokenKind::String(content), span, &self.input[start..self.position]));
            } else if c == '\\' {
                let escape = Span::new(self.position, self.position + 1, self.line, self.column);
                self.advance();
                if let Some(byte) = self.peek().and_then(ascii_escape) {
                    content.push(char::from(byte));
                    self.advance();
                    continue;
                }
                match self.peek() {
                    Some('u') => content.push(self.read_unicode_escape(escape)?),
                    Some(c) => {
                        return Err(SourceDiagnosticCode::InvalidEscape.error(
                            format!("unknown escape sequence: \\{}", c),
                            Span::new(self.position, self.position + c.len_utf8(), self.line, self.column),
                        ));
                    }
                    None => return Err(SourceDiagnosticCode::UnterminatedEscape.error("unterminated string escape", escape)),
                }
            } else if c.is_control() {
                return Err(SourceDiagnosticCode::InvalidEscape.error(
                    "raw control characters and line breaks are forbidden in strings; use an explicit escape",
                    Span::new(self.position, self.position + c.len_utf8(), self.line, self.column),
                ));
            } else {
                content.push(c);
                self.advance();
            }
        }

        Err(SourceDiagnosticCode::UnterminatedString
            .error("unterminated string literal", Span::new(start, start + 1, start_line, start_col)))
    }

    fn read_unicode_escape(&mut self, opening: Span) -> Result<char> {
        self.advance(); // u
        let invalid = |end| {
            SourceDiagnosticCode::InvalidEscape.error(
                "expected a Unicode scalar escape \\u{1..6 hex digits}",
                Span::new(opening.start, end, opening.line, opening.column),
            )
        };
        if self.peek() != Some('{') {
            return Err(invalid(self.position + self.peek().map_or(0, char::len_utf8)));
        }
        self.advance();
        let mut scalar = 0u32;
        let mut digits = 0;
        while let Some(character) = self.peek() {
            if character == '}' {
                break;
            }
            if !character.is_ascii_hexdigit() || digits == 6 {
                return Err(invalid(self.position + self.peek().map_or(0, char::len_utf8)));
            }
            scalar = scalar * 16 + character.to_digit(16).expect("hex digit checked");
            digits += 1;
            self.advance();
        }
        if digits == 0 || self.peek() != Some('}') {
            return Err(invalid(self.position + self.peek().map_or(0, char::len_utf8)));
        }
        self.advance();
        char::from_u32(scalar).ok_or_else(|| invalid(self.position))
    }

    fn read_byte_string(&mut self) -> Result<Token> {
        let start = self.position;
        let start_line = self.line;
        let start_col = self.column;

        self.advance(); // b
        self.advance(); // "

        let mut bytes = Vec::new();
        while let Some(c) = self.peek() {
            if c == '"' {
                self.advance();
                let span = Span::new(start, self.position, start_line, start_col);
                return Ok(Token::new(TokenKind::ByteString(bytes), span, &self.input[start..self.position]));
            } else if c == '\\' {
                let escape = Span::new(self.position, self.position + 1, self.line, self.column);
                self.advance();
                if let Some(byte) = self.peek().and_then(ascii_escape) {
                    bytes.push(byte);
                    self.advance();
                    continue;
                }
                match self.peek() {
                    Some('x') => {
                        self.advance();
                        let hi = self.peek().ok_or_else(|| {
                            SourceDiagnosticCode::InvalidByteHexEscape.error(
                                "incomplete hex escape in byte string",
                                Span::new(self.position, self.position, self.line, self.column),
                            )
                        })?;
                        if !hi.is_ascii_hexdigit() {
                            return Err(SourceDiagnosticCode::InvalidByteHexEscape.error(
                                format!("invalid hex escape in byte string: \\x{}", hi),
                                Span::new(self.position, self.position + hi.len_utf8(), self.line, self.column),
                            ));
                        }
                        self.advance();

                        let lo = self.peek().ok_or_else(|| {
                            SourceDiagnosticCode::InvalidByteHexEscape.error(
                                "incomplete hex escape in byte string",
                                Span::new(self.position, self.position, self.line, self.column),
                            )
                        })?;
                        if !lo.is_ascii_hexdigit() {
                            return Err(SourceDiagnosticCode::InvalidByteHexEscape.error(
                                format!("invalid hex escape in byte string: \\x{}{}", hi, lo),
                                Span::new(self.position, self.position + lo.len_utf8(), self.line, self.column),
                            ));
                        }
                        self.advance();

                        let hex = &self.input[self.position - 2..self.position];
                        let val = u8::from_str_radix(hex, 16).map_err(|_| {
                            SourceDiagnosticCode::InvalidByteHexEscape.error(
                                format!("invalid hex escape in byte string: \\x{}", hex),
                                Span::new(self.position - 2, self.position, self.line, self.column),
                            )
                        })?;
                        bytes.push(val);
                    }
                    Some(c) => {
                        return Err(SourceDiagnosticCode::InvalidEscape.error(
                            format!("unknown escape sequence in byte string: \\{}", c),
                            Span::new(self.position, self.position + c.len_utf8(), self.line, self.column),
                        ));
                    }
                    None => {
                        return Err(
                            SourceDiagnosticCode::UnterminatedByteStringEscape.error("unterminated byte string escape", escape)
                        );
                    }
                }
            } else if c.is_ascii() && !c.is_control() {
                bytes.push(c as u8);
                self.advance();
            } else {
                return Err(SourceDiagnosticCode::NonAsciiByteString.error(
                    "non-ASCII character in byte string",
                    Span::new(self.position, self.position + c.len_utf8(), self.line, self.column),
                ));
            }
        }

        Err(SourceDiagnosticCode::UnterminatedByteString
            .error("unterminated byte string literal", Span::new(start, start + 2, start_line, start_col)))
    }

    pub fn next_token(&mut self) -> Result<Token> {
        loop {
            self.skip_whitespace();
            if self.peek() == Some('/') && (self.peek_next() == Some('/') || self.peek_next() == Some('*')) {
                self.skip_comment()?;
            } else {
                break;
            }
        }

        let start = self.position;
        let start_line = self.line;
        let start_col = self.column;

        let c = match self.peek() {
            Some(c) => c,
            None => {
                return Ok(Token::new(TokenKind::Eof, Span::new(start, start, start_line, start_col), ""));
            }
        };

        if c == '\n' {
            self.advance();
            return Ok(Token::new(TokenKind::Newline, Span::new(start, self.position, start_line, start_col), "\n"));
        }

        if c == 'b' && self.peek_next() == Some('"') {
            return self.read_byte_string();
        }

        if unicode_ident::is_xid_start(c) || c == '_' {
            return Ok(self.read_identifier());
        }

        if c.is_ascii_digit() {
            return self.read_number();
        }

        if c == '"' {
            return self.read_string();
        }
        if c == '\'' {
            return Err(SourceDiagnosticCode::InvalidEscape.error(
                "single-quoted strings were removed; use a double-quoted string",
                Span::new(start, start + 1, start_line, start_col),
            ));
        }

        self.advance();
        let span = Span::new(start, self.position, start_line, start_col);

        match c {
            '(' => Ok(Token::new(TokenKind::LParen, span, "(")),
            ')' => Ok(Token::new(TokenKind::RParen, span, ")")),
            '{' => Ok(Token::new(TokenKind::LBrace, span, "{")),
            '}' => Ok(Token::new(TokenKind::RBrace, span, "}")),
            '[' => Ok(Token::new(TokenKind::LBracket, span, "[")),
            ']' => Ok(Token::new(TokenKind::RBracket, span, "]")),
            '#' => Ok(Token::new(TokenKind::Pound, span, "#")),
            ',' => Ok(Token::new(TokenKind::Comma, span, ",")),
            ';' => Ok(Token::new(TokenKind::Semi, span, ";")),
            '_' => Ok(Token::new(TokenKind::Underscore, span, "_")),

            '+' => Ok(Token::new(TokenKind::Plus, span, "+")),
            '-' => {
                if self.peek() == Some('>') {
                    self.advance();
                    Ok(Token::new(TokenKind::Arrow, span.combine(&Span::new(start, self.position, start_line, start_col)), "->"))
                } else {
                    Ok(Token::new(TokenKind::Minus, span, "-"))
                }
            }
            '*' => Ok(Token::new(TokenKind::Star, span, "*")),
            '/' => Ok(Token::new(TokenKind::Slash, span, "/")),
            '%' => Ok(Token::new(TokenKind::Percent, span, "%")),

            '&' => {
                if self.peek() == Some('&') {
                    self.advance();
                    Ok(Token::new(TokenKind::And, span.combine(&Span::new(start, self.position, start_line, start_col)), "&&"))
                } else {
                    Ok(Token::new(TokenKind::Ampersand, span, "&"))
                }
            }
            '|' => {
                if self.peek() == Some('|') {
                    self.advance();
                    Ok(Token::new(TokenKind::Or, span.combine(&Span::new(start, self.position, start_line, start_col)), "||"))
                } else {
                    Ok(Token::new(TokenKind::Pipe, span, "|"))
                }
            }
            '!' => {
                if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::new(TokenKind::NotEq, span.combine(&Span::new(start, self.position, start_line, start_col)), "!="))
                } else {
                    Ok(Token::new(TokenKind::Not, span, "!"))
                }
            }

            '=' => {
                if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::new(TokenKind::EqEq, span.combine(&Span::new(start, self.position, start_line, start_col)), "=="))
                } else if self.peek() == Some('>') {
                    self.advance();
                    Ok(Token::new(TokenKind::FatArrow, span.combine(&Span::new(start, self.position, start_line, start_col)), "=>"))
                } else {
                    Ok(Token::new(TokenKind::Eq, span, "="))
                }
            }
            '<' => {
                if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::new(TokenKind::Le, span.combine(&Span::new(start, self.position, start_line, start_col)), "<="))
                } else {
                    Ok(Token::new(TokenKind::Lt, span, "<"))
                }
            }
            '>' => {
                if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::new(TokenKind::Ge, span.combine(&Span::new(start, self.position, start_line, start_col)), ">="))
                } else {
                    Ok(Token::new(TokenKind::Gt, span, ">"))
                }
            }

            ':' => {
                if self.peek() == Some(':') {
                    self.advance();
                    Ok(Token::new(TokenKind::ColonColon, span.combine(&Span::new(start, self.position, start_line, start_col)), "::"))
                } else {
                    Ok(Token::new(TokenKind::Colon, span, ":"))
                }
            }
            '.' => Ok(Token::new(TokenKind::Dot, span, ".")),
            '^' => Ok(Token::new(TokenKind::Caret, span, "^")),

            _ => Ok(Token::new(TokenKind::Invalid(c), span, c.to_string())),
        }
    }

    pub fn tokenize(self) -> Result<Vec<Token>> {
        self.tokenize_document().map(|document| document.tokens)
    }

    pub fn tokenize_document(mut self) -> Result<SyntaxDocument<'a>> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            if token.kind == TokenKind::Eof {
                tokens.push(token);
                break;
            }
            tokens.push(token);
        }
        Ok(SyntaxDocument { source: self.input, tokens, trivia: self.trivia, nodes: Vec::new() })
    }
}

pub fn lex(input: &str) -> Result<Vec<Token>> {
    lex_document(input).map(|document| document.tokens)
}

pub fn lex_document(input: &str) -> Result<SyntaxDocument<'_>> {
    if input.len() > crate::MAX_SOURCE_BYTES {
        return Err(SourceDiagnosticCode::SourceSizeLimit
            .error(format!("source exceeds the {} byte compiler limit", crate::MAX_SOURCE_BYTES), Span::new(0, input.len(), 1, 1)));
    }
    Lexer::new(input).tokenize_document()
}

/// Text and byte strings share the admitted ASCII escape vocabulary.
/// Hex escapes are byte-only and are validated by read_byte_string.
fn ascii_escape(character: char) -> Option<u8> {
    match character {
        'n' => Some(b'\n'),
        't' => Some(b'\t'),
        'r' => Some(b'\r'),
        '\\' => Some(b'\\'),
        '\'' => Some(b'\''),
        '"' => Some(b'"'),
        '0' => Some(0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keywords() {
        let input = "module resource action consume create launch";
        let tokens = lex(input).unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Module);
        assert_eq!(tokens[1].kind, TokenKind::Resource);
        assert_eq!(tokens[2].kind, TokenKind::Action);
        assert_eq!(tokens[3].kind, TokenKind::Consume);
        assert_eq!(tokens[4].kind, TokenKind::Create);
        assert_eq!(tokens[5].kind, TokenKind::Launch);
    }

    #[test]
    fn test_identifiers() {
        let input = "foo bar_baz MyType";
        let tokens = lex(input).unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Identifier(ref s) if s == "foo"));
        assert!(matches!(tokens[1].kind, TokenKind::Identifier(ref s) if s == "bar_baz"));
        assert!(matches!(tokens[2].kind, TokenKind::Identifier(ref s) if s == "MyType"));
    }

    #[test]
    fn test_numbers() {
        let input = "42 0xFF 100_000";
        let tokens = lex(input).unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Integer(42)));
        assert!(matches!(tokens[1].kind, TokenKind::Integer(255)));
    }

    #[test]
    fn accepts_the_full_u128_decimal_literal_range() {
        let tokens = lex("340282366920938463463374607431768211455").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Integer(value) if value == u128::MAX));
    }

    #[test]
    fn rejects_decimal_literals_above_u128() {
        let error = lex("340282366920938463463374607431768211456").expect_err("u128 overflow must be rejected by the lexer");
        assert!(error.message.contains("exceeds the supported 128-bit source range"), "unexpected error: {}", error.message);
    }

    #[test]
    fn rejects_hex_literal_without_digits() {
        for source in ["0x", "0xG"] {
            let error = lex(source).expect_err("empty hex payload must be rejected");
            assert!(error.message.contains("invalid integer literal"), "unexpected error: {}", error.message);
        }
    }

    #[test]
    fn rejects_source_over_byte_budget() {
        let source = " ".repeat(crate::MAX_SOURCE_BYTES + 1);
        let error = lex(&source).expect_err("oversized source must be rejected before token allocation");
        assert!(error.message.contains("source exceeds"), "unexpected error: {}", error.message);
    }

    #[test]
    fn test_operators() {
        let input = "+ - * / == != <= >= && || & | ^";
        let tokens = lex(input).unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Plus);
        assert_eq!(tokens[1].kind, TokenKind::Minus);
        assert_eq!(tokens[2].kind, TokenKind::Star);
        assert_eq!(tokens[3].kind, TokenKind::Slash);
        assert_eq!(tokens[4].kind, TokenKind::EqEq);
        assert_eq!(tokens[5].kind, TokenKind::NotEq);
        assert_eq!(tokens[6].kind, TokenKind::Le);
        assert_eq!(tokens[7].kind, TokenKind::Ge);
        assert_eq!(tokens[8].kind, TokenKind::And);
        assert_eq!(tokens[9].kind, TokenKind::Or);
        assert_eq!(tokens[10].kind, TokenKind::Ampersand);
        assert_eq!(tokens[11].kind, TokenKind::Pipe);
        assert_eq!(tokens[12].kind, TokenKind::Caret);
    }

    #[test]
    fn test_punctuation() {
        let input = "() {} [] :: -> =>";
        let tokens = lex(input).unwrap();
        assert_eq!(tokens[0].kind, TokenKind::LParen);
        assert_eq!(tokens[1].kind, TokenKind::RParen);
        assert_eq!(tokens[2].kind, TokenKind::LBrace);
        assert_eq!(tokens[3].kind, TokenKind::RBrace);
        assert_eq!(tokens[4].kind, TokenKind::LBracket);
        assert_eq!(tokens[5].kind, TokenKind::RBracket);
        assert_eq!(tokens[6].kind, TokenKind::ColonColon);
        assert_eq!(tokens[7].kind, TokenKind::Arrow);
        assert_eq!(tokens[8].kind, TokenKind::FatArrow);
    }

    #[test]
    fn test_string() {
        let input = r#""hello world" "escaped \"quote\"""#;
        let tokens = lex(input).unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::String(ref s) if s == "hello world"));
        assert!(matches!(tokens[1].kind, TokenKind::String(ref s) if s == "escaped \"quote\""));
    }

    #[test]
    fn test_byte_string() {
        let input = r#"b"hello" b"\x00\xFF""#;
        let tokens = lex(input).unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::ByteString(ref b) if b == b"hello"));
    }

    #[test]
    fn closed_byte_strings_preserve_empty_values_quotes_and_escapes() {
        for (input, expected) in [
            (r#"b"""#, Vec::new()),
            (r#"b"\x00\xFF""#, vec![0, 255]),
            (r#"b"\n\t\r\\\"\0\x41""#, vec![b'\n', b'\t', b'\r', b'\\', b'"', 0, b'A']),
        ] {
            let tokens = lex(input).unwrap();
            assert_eq!(tokens[0].kind, TokenKind::ByteString(expected));
            assert_eq!(tokens[0].span, Span::new(0, input.len(), 1, 1));
            assert_eq!(tokens[1].kind, TokenKind::Eof);
        }
    }

    #[test]
    fn malformed_byte_string_escapes_keep_their_specific_diagnostics() {
        for (input, message, code) in [
            (r#"b"\x"#, "incomplete hex escape in byte string", "E1026"),
            (r#"b"\x0"#, "incomplete hex escape in byte string", "E1026"),
            (r#"b"\xQ0""#, "invalid hex escape in byte string", "E1026"),
            (r#"b"\x0Q""#, "invalid hex escape in byte string", "E1026"),
            (r#"b"\q""#, "unknown escape sequence in byte string", "E1025"),
        ] {
            let error = lex(input).unwrap_err();
            assert_eq!(error.code.as_deref(), Some(code), "{input:?}: {error}");
            assert!(error.message.contains(message), "{input:?}: {error}");
        }
    }

    #[test]
    fn test_comment() {
        let input = "foo // this is a comment\nbar /* block */ baz";
        let tokens = lex(input).unwrap();
        let semantic_tokens: Vec<_> =
            tokens.into_iter().filter(|token| !matches!(token.kind, TokenKind::Newline | TokenKind::Eof)).collect();
        assert!(matches!(semantic_tokens[0].kind, TokenKind::Identifier(ref s) if s == "foo"));
        assert!(matches!(semantic_tokens[1].kind, TokenKind::Identifier(ref s) if s == "bar"));
        assert!(matches!(semantic_tokens[2].kind, TokenKind::Identifier(ref s) if s == "baz"));
    }
}

#[cfg(test)]
mod v3_literals {
    use super::*;

    #[test]
    fn xid_tables_and_whole_numeric_literal_admission_are_pinned() {
        assert_eq!(unicode_ident::UNICODE_VERSION, (17, 0, 0));
        for (source, expected) in
            [("1_000", 1000), ("0xff_FF", 65535), ("0x0", 0), ("0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff", u128::MAX)]
        {
            assert!(matches!(lex(source).unwrap()[0].kind, TokenKind::Integer(value) if value == expected));
        }
        for source in
            ["1__0", "123_", "0x_FF", "0x", "0xG", "0xFG", "123u64", "0b10", "0XFF", "1_λ", "0x100000000000000000000000000000000"]
        {
            let error = lex(source).unwrap_err();
            assert_eq!((error.span.start, error.span.end), (0, source.len()), "{source}: {error}");
        }
        let tokens = lex("变量 café e\u{301} _value").unwrap();
        for token in &tokens[..4] {
            assert!(matches!(token.kind, TokenKind::Identifier(_)), "{token:?}");
        }
        assert!(!matches!(lex("\u{301}x").unwrap()[0].kind, TokenKind::Identifier(_)));
    }

    #[test]
    fn numeric_printing_is_an_inverse_for_both_radices() {
        let mut state = 0x43656c6c536372697074u128;
        for _ in 0..10_000 {
            state = state.wrapping_mul(0xda942042e4dd58b5).wrapping_add(1);
            for source in [state.to_string(), format!("0x{state:x}")] {
                assert!(matches!(lex(&source).unwrap()[0].kind, TokenKind::Integer(value) if value == state));
            }
        }
    }

    #[test]
    fn strings_reject_raw_controls_and_non_byte_escapes() {
        for source in ["'alias'", "\"line\nbreak\"", "\"tab\there\"", "\"\u{7}\"", "b\"line\nbreak\"", "b\"\\u{41}\"", "b\"é\""] {
            assert!(lex(source).is_err(), "{source:?}");
        }
        for source in [r#""\u{0}\u{7}\u{10ffff}""#, r#""\n\r\t\0\\\"""#, r#"b"\x00\xff\"""#] {
            lex(source).unwrap();
        }
    }
}
