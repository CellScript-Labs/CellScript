//! Named allocation policy. Keep member types and budgets structured; no
//! positional generic budget encoding or expansion into synthetic source.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

impl Parser<'_> {
    pub(super) fn parse_collections(&mut self) -> Result<CollectionsDef> {
        let start = self.advance().span;
        let name = self.parse_name()?;
        self.skip_newlines();
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();
        let mut limits = None;
        let mut members = Vec::new();
        let mut seen = BTreeSet::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let span = self.current().span;
            let field = self.parse_name()?;
            if !seen.insert(field.clone()) {
                return Err(CompileError::new(format!("duplicate collections member '{field}'"), span));
            }
            if field == "limits" {
                limits = Some(self.parse_collection_limits(&["bytes", "allocations"])?);
            } else {
                self.expect(TokenKind::Colon)?;
                let kind_name = self.parse_name()?;
                self.expect(TokenKind::Lt)?;
                let first = self.parse_type()?;
                let kind = match kind_name.as_str() {
                    "Vec" => ValueCollectionKind::Vec(first),
                    "OrderedSet" => ValueCollectionKind::OrderedSet(first),
                    "OrderedMap" => {
                        self.expect(TokenKind::Comma)?;
                        ValueCollectionKind::OrderedMap { key: first, value: self.parse_type()? }
                    }
                    _ => return Err(CompileError::new("collections members must be Vec, OrderedMap or OrderedSet", span)),
                };
                self.expect(TokenKind::Gt)?;
                let bounds = self.parse_collection_limits(&["items"])?;
                members.push(CollectionMember { name: field, kind, items: bounds["items"], span });
            }
            self.collection_separator()?;
        }
        self.expect(TokenKind::RBrace)?;
        let limits = limits.ok_or_else(|| CompileError::new("collections requires limits { bytes, allocations }", start))?;
        let def = CollectionsDef {
            name,
            bytes: limits["bytes"],
            allocations: limits["allocations"],
            members,
            span: Span::new(start.start, self.previous_non_newline().span.end, start.line, start.column),
        };
        crate::allocated_values::validate_declaration(&def)?;
        Ok(def)
    }

    fn collection_separator(&mut self) -> Result<()> {
        if self.check(&TokenKind::Comma) {
            self.advance();
        } else if !self.check(&TokenKind::Newline) && !self.check(&TokenKind::RBrace) {
            return Err(CompileError::new("expected a comma or newline after collections member", self.current().span));
        }
        self.skip_newlines();
        Ok(())
    }

    fn parse_collection_limits(&mut self, fields: &[&str]) -> Result<BTreeMap<String, u32>> {
        self.skip_newlines();
        let span = self.expect(TokenKind::LBrace)?.span;
        self.skip_newlines();
        let mut values = BTreeMap::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let field_span = self.current().span;
            let name = self.parse_name()?;
            if !fields.contains(&name.as_str()) {
                return Err(CompileError::new(format!("unknown collections limit '{name}'"), field_span));
            }
            self.expect(TokenKind::Colon)?;
            let token = self.advance();
            let TokenKind::Integer(value) = token.kind else {
                return Err(CompileError::new("collections limits require unsigned integer literals", token.span));
            };
            let value = u32::try_from(value).map_err(|_| CompileError::new("collections limit must fit in u32", token.span))?;
            if values.insert(name.clone(), value).is_some() {
                return Err(CompileError::new(format!("duplicate collections limit '{name}'"), field_span));
            }
            self.collection_separator()?;
        }
        self.expect(TokenKind::RBrace)?;
        for field in fields {
            if !values.contains_key(*field) {
                return Err(CompileError::new(format!("missing collections limit '{field}'"), span));
            }
        }
        Ok(values)
    }
}
