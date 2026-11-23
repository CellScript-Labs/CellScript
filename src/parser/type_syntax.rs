use super::{Parser, MAX_PARSE_RECURSION_DEPTH};
use crate::ast::{Type, TypeArgument};
use crate::error::{CompileError, Result};
use crate::lexer::token::TokenKind;
use crate::syntax::SyntaxKind;

impl Parser<'_> {
    pub(super) fn parse_type(&mut self) -> Result<Type> {
        self.skip_newlines();
        let syntax_start = self.current().span;
        if self.type_depth >= MAX_PARSE_RECURSION_DEPTH {
            return Err(CompileError::new(
                format!("type nesting exceeds the parser limit of {}", MAX_PARSE_RECURSION_DEPTH),
                self.current().span,
            ));
        }
        self.type_depth += 1;
        let result = self.parse_type_inner();
        self.type_depth -= 1;
        if let Ok(ty) = &result {
            self.record_syntax_node(SyntaxKind::Type, syntax_start);
            if matches!(ty, Type::Tuple(items) if items.len() == 1) {
                self.record_syntax_node(SyntaxKind::SingletonTupleType, syntax_start);
            }
        }
        result
    }

    fn parse_type_inner(&mut self) -> Result<Type> {
        let ty = match &self.current().kind {
            TokenKind::U8 => {
                self.advance();
                Type::U8
            }
            TokenKind::U16 => {
                self.advance();
                Type::U16
            }
            TokenKind::U32 => {
                self.advance();
                Type::U32
            }
            TokenKind::I32 => {
                self.advance();
                Type::I32
            }
            TokenKind::U64 => {
                self.advance();
                Type::U64
            }
            TokenKind::U128 => {
                self.advance();
                Type::U128
            }
            TokenKind::Bool => {
                self.advance();
                Type::Bool
            }
            TokenKind::Address => {
                self.advance();
                Type::Address
            }
            TokenKind::Hash => {
                self.advance();
                Type::Hash
            }
            TokenKind::Identifier(_) | TokenKind::Launch => {
                let name = self.parse_name_path()?;
                if self.check(&TokenKind::Lt) {
                    self.advance();
                    self.skip_newlines();
                    let mut arguments = Vec::new();
                    while !self.check(&TokenKind::Gt) && !self.check(&TokenKind::Eof) {
                        if name == "WitnessBytesView" && arguments.is_empty() && self.check(&TokenKind::Lock) {
                            // `lock` is an owner designation in this family,
                            // not a general nominal type or value argument.
                            arguments.push(TypeArgument::Type(Type::Named("lock".into())));
                            self.advance();
                        } else if let TokenKind::Integer(value) = self.current().kind {
                            arguments.push(TypeArgument::UnsignedConstant(value));
                            self.advance();
                        } else {
                            arguments.push(TypeArgument::Type(self.parse_type()?));
                        }
                        if self.check(&TokenKind::Comma) {
                            self.advance();
                            self.skip_newlines();
                        } else {
                            break;
                        }
                    }
                    self.expect(TokenKind::Gt)?;
                    Type::applied(name, arguments)
                } else {
                    Type::Named(name)
                }
            }
            TokenKind::LBracket => {
                self.advance();
                let elem_ty = self.parse_type()?;
                self.expect(TokenKind::Semi)?;
                let size = match &self.current().kind {
                    TokenKind::Integer(n) => {
                        let size = usize::try_from(*n)
                            .map_err(|_| CompileError::new("array size exceeds the supported platform range", self.current().span))?;
                        self.advance();
                        size
                    }
                    _ => {
                        return Err(CompileError::new("expected array size", self.current().span));
                    }
                };
                self.expect(TokenKind::RBracket)?;
                Type::Array(Box::new(elem_ty), size)
            }
            TokenKind::LParen => {
                self.advance();
                self.skip_newlines();
                if self.check(&TokenKind::RParen) {
                    self.advance();
                    Type::Unit
                } else {
                    let first = self.parse_type()?;
                    self.skip_newlines();
                    if self.check(&TokenKind::Comma) {
                        let mut elems = vec![first];
                        while self.check(&TokenKind::Comma) {
                            self.advance();
                            self.skip_newlines();
                            if self.check(&TokenKind::RParen) {
                                break;
                            }
                            elems.push(self.parse_type()?);
                            self.skip_newlines();
                        }
                        self.expect(TokenKind::RParen)?;
                        Type::Tuple(elems)
                    } else {
                        self.expect(TokenKind::RParen)?;
                        first
                    }
                }
            }
            TokenKind::Ampersand => {
                self.advance();
                if self.check(&TokenKind::Mut) {
                    self.advance();
                    Type::MutRef(Box::new(self.parse_type()?))
                } else {
                    Type::Ref(Box::new(self.parse_type()?))
                }
            }
            TokenKind::ReadRef => {
                return Err(CompileError::new(
                    "`read_ref` is not a type qualifier; use `read name: T` for action/lock parameters or `read_ref::<T>()` as an expression",
                    self.current().span,
                ));
            }
            _ => {
                return Err(CompileError::new(format!("expected type, found {}", self.current().kind), self.current().span));
            }
        };

        self.skip_newlines();
        Ok(ty)
    }
}
