//! Lossless source information owned by a single document revision.
//! The semantic parser consumes `tokens`; trivia never becomes executable IR.
use crate::error::Span;
use crate::lexer::token::Token;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriviaKind {
    Whitespace,
    LineComment,
    BlockComment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trivia {
    pub kind: TriviaKind,
    pub span: Span,
}

#[derive(Debug)]
pub struct SyntaxDocument<'a> {
    pub source: &'a str,
    pub tokens: Vec<Token>,
    pub trivia: Vec<Trivia>,
    /// Ranges emitted by the same semantic parser, never inferred from case
    /// conventions or obtained by reparsing generated source text.
    pub nodes: Vec<SyntaxNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxKind {
    ModuleHeader,
    Item,
    Block,
    Statement {
        expression: bool,
        terminated: bool,
    },
    Expression,
    PrefixExpression,
    Type,
    TypeParameters,
    Constructor,
    GenericArguments,
    Field,
    LoopBody,
    /// A declaration/policy clause with its own statement delimiter.
    ClauseEnd,
    /// An entry in a comma-delimited record or configuration list.
    ListElement,
    /// A singleton tuple type, distinct from parenthesized grouping.
    SingletonTupleType,
    ComparisonExpression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxNode {
    pub kind: SyntaxKind,
    pub span: Span,
}

impl SyntaxDocument<'_> {
    /// Byte ranges partition the exact source, including comments and spaces.
    pub fn pieces(&self) -> Vec<SyntaxPiece> {
        let mut pieces = self
            .tokens
            .iter()
            .enumerate()
            .filter(|(_, token)| token.span.start != token.span.end)
            .map(|(index, token)| SyntaxPiece { span: token.span, trivia: None, token_index: Some(index) })
            .collect::<Vec<_>>();
        pieces.extend(self.trivia.iter().map(|item| SyntaxPiece { span: item.span, trivia: Some(item.kind), token_index: None }));
        pieces.sort_by_key(|piece| piece.span.start);
        pieces
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SyntaxPiece {
    pub span: Span,
    pub trivia: Option<TriviaKind>,
    pub token_index: Option<usize>,
}
