use super::{Parser, MAX_PARSE_RECURSION_DEPTH};
use crate::ast::{BoundPolicy, BoundReference, Item, Type, TypeArgument, UnsignedBound};
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
                        } else if crate::source_bounds::is_unsigned_argument(&name, arguments.len()) {
                            arguments.push(match self.parse_unsigned_bound()? {
                                UnsignedBound::Literal(value) => TypeArgument::UnsignedConstant(value),
                                UnsignedBound::Reference(reference) => TypeArgument::UnsignedReference(reference),
                            });
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
                let bound_span = self.current().span;
                let bound = self.parse_unsigned_bound()?;
                self.expect(TokenKind::RBracket)?;
                match bound {
                    UnsignedBound::Literal(value) => Type::Array(
                        Box::new(elem_ty),
                        usize::try_from(value)
                            .map_err(|_| CompileError::new("array size exceeds the supported platform range", bound_span))?,
                    ),
                    reference => Type::ArrayBound(Box::new(elem_ty), reference),
                }
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

    pub(super) fn parse_unsigned_bound(&mut self) -> Result<UnsignedBound> {
        let start = self.current().span;
        match self.current().kind {
            TokenKind::Integer(value) => {
                self.advance();
                Ok(UnsignedBound::Literal(value))
            }
            _ if self.ident_like_name().is_some() => {
                let path = self.parse_name_path()?;
                Ok(UnsignedBound::Reference(BoundReference { path, span: start.combine(&self.previous_non_newline().span) }))
            }
            _ => Err(CompileError::new("expected an unsigned integer literal or constant name for this bound", start)),
        }
    }

    pub(super) fn parse_u32_bound(&mut self) -> Result<UnsignedBound> {
        let span = self.current().span;
        let bound = self.parse_unsigned_bound()?;
        if matches!(&bound, UnsignedBound::Literal(value) if *value > u32::MAX as u128) {
            return Err(CompileError::new("named policy limit must fit in u32", span));
        }
        Ok(bound)
    }

    pub(super) fn finish_bound_policy(&self, policy: BoundPolicy) -> Result<Item> {
        if policy.bounds().iter().any(|bound| matches!(bound, UnsignedBound::Reference(_))) {
            return Ok(Item::BoundPolicy(policy));
        }
        let span = policy.span();
        let item = policy.resolve(|bound| match bound {
            UnsignedBound::Literal(value) => {
                u32::try_from(*value).map_err(|_| CompileError::new("named policy limit must fit in u32", span))
            }
            UnsignedBound::Reference(reference) => Err(CompileError::new("named bound requires constant resolution", reference.span)),
        })?;
        crate::source_bounds::validate_policy_bounds(&item)?;
        if let Item::Collections(def) = &item
            && !crate::source_bounds::collection_has_unresolved_types(def)
        {
            crate::allocated_values::validate_declaration(def)?;
        }
        Ok(item)
    }
}
