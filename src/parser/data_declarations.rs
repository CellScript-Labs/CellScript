use super::{normalize_hash_type_decl, Parser, TypePolicyDecls};
use crate::ast::{
    Capability, CapacityFloorDecl, ConstDef, DefinitionOrigin, EnumDef, EnumVariant, Expr, Field, FlowDef, HashTypeDecl,
    IdentityPolicy, ReceiptDef, ResourceDef, SharedDef, StateFieldPath, StateTransition, StructDef, TypeIdentity, TypeParam,
    UseImport, UseStmt, ValidityBlock, ValueAbility,
};
use crate::error::{CompileError, Result, Span};
use crate::lexer::token::TokenKind;
use crate::syntax::SyntaxKind;

impl Parser<'_> {
    pub(super) fn parse_type_params(&mut self) -> Result<Vec<TypeParam>> {
        if !self.check(&TokenKind::Lt) {
            return Ok(Vec::new());
        }

        let syntax_start = self.current().span;
        self.advance();
        self.skip_newlines();
        let mut params = Vec::new();
        while !self.check(&TokenKind::Gt) && !self.check(&TokenKind::Eof) {
            let start = self.current().span;
            let phantom = self.current().text == "phantom";
            if phantom {
                self.advance();
            }
            let name = self.parse_name()?;
            let mut constraints = Vec::new();
            let mut uses_fixed_value_profile = false;
            if self.check(&TokenKind::Colon) {
                self.advance();
                loop {
                    let source_name = self.current().text.as_str();
                    let expanded = if source_name == ValueAbility::FIXED_VALUE_PROFILE_NAME {
                        uses_fixed_value_profile = true;
                        ValueAbility::FIXED_VALUE_PROFILE.to_vec()
                    } else if let Some(ability) = ValueAbility::from_source_name(source_name) {
                        vec![ability]
                    } else {
                        return Err(CompileError::new(
                            "expected value constraint: fixed_value, copy, drop, store, fixed, serializable, non_linear, or cell",
                            self.current().span,
                        ));
                    };
                    for ability in expanded {
                        if constraints.contains(&ability) {
                            return Err(CompileError::new(
                                format!("duplicate '{}' constraint on type parameter '{}'", ability.as_str(), name),
                                self.current().span,
                            ));
                        }
                        constraints.push(ability);
                    }
                    self.advance();
                    if self.check(&TokenKind::Plus) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                constraints.sort_unstable();
            }
            params.push(TypeParam { name, constraints, uses_fixed_value_profile, phantom, span: start });
            if self.check(&TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
            } else {
                break;
            }
        }
        self.expect(TokenKind::Gt)?;
        self.record_syntax_node(SyntaxKind::TypeParameters, syntax_start);
        Ok(params)
    }

    fn reject_generic_type_params(&self, type_name: &str) -> Result<()> {
        if self.check(&TokenKind::Lt) {
            Err(CompileError::new(
                format!(
                    "Cell-backed type '{}' cannot declare generic parameters; use concrete resource/shared/receipt schemas and generic non-Cell value types",
                    type_name
                ),
                self.current().span,
            ))
        } else {
            Ok(())
        }
    }

    fn parse_value_abilities(&mut self) -> Result<Vec<ValueAbility>> {
        if !self.check(&TokenKind::Has) {
            return Ok(Vec::new());
        }
        self.advance();
        let mut abilities = Vec::new();
        loop {
            let Some(ability) = ValueAbility::from_source_name(&self.current().text) else {
                return Err(CompileError::new(
                    "expected value ability: copy, drop, store, fixed, serializable, non_linear, or cell",
                    self.current().span,
                ));
            };
            if abilities.contains(&ability) {
                return Err(CompileError::new(format!("duplicate '{}' value ability", ability.as_str()), self.current().span));
            }
            abilities.push(ability);
            self.advance();
            self.skip_newlines();
            if self.check(&TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
                if self.check(&TokenKind::LBrace) {
                    break;
                }
            } else {
                break;
            }
        }
        abilities.sort_unstable();
        Ok(abilities)
    }

    pub(super) fn parse_use(&mut self) -> Result<UseStmt> {
        let start_span = self.current().span;
        self.expect(TokenKind::Use)?;

        let path = self.parse_name_path()?.split("::").map(ToString::to_string).collect::<Vec<_>>();

        let (module_path, imports) = if self.check(&TokenKind::ColonColon) {
            self.advance();
            if self.check(&TokenKind::LBrace) {
                self.advance();
                self.skip_newlines();
                let mut imports = Vec::new();
                while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
                    let name = self.parse_name_path()?;
                    let alias = if self.current().text == "as" {
                        self.advance();
                        Some(self.parse_name()?)
                    } else {
                        None
                    };
                    imports.push(UseImport { name, alias });
                    if self.check(&TokenKind::Comma) {
                        self.advance();
                        self.skip_newlines();
                    } else {
                        break;
                    }
                }
                self.expect(TokenKind::RBrace)?;
                (path, imports)
            } else {
                let import_name = self.parse_name_path()?;
                let alias = match &self.current().kind {
                    TokenKind::Identifier(s) if s == "as" => {
                        self.advance();
                        match &self.current().kind {
                            TokenKind::Identifier(n) => {
                                let a = n.clone();
                                self.advance();
                                Some(a)
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                };
                (path, vec![UseImport { name: import_name, alias }])
            }
        } else {
            let mut module_path = path;
            let import_name = module_path.pop().ok_or_else(|| CompileError::new("expected import path", self.current().span))?;
            let alias = match &self.current().kind {
                TokenKind::Identifier(s) if s == "as" => {
                    self.advance();
                    match &self.current().kind {
                        TokenKind::Identifier(n) => {
                            let a = n.clone();
                            self.advance();
                            Some(a)
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            (module_path, vec![UseImport { name: import_name, alias }])
        };

        self.record_end(SyntaxKind::ClauseEnd);
        self.expect_statement_end()?;

        let end_span = self.current().span;
        Ok(UseStmt { module_path, imports, span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column) })
    }

    pub(super) fn parse_resource(&mut self, type_id: Option<TypeIdentity>) -> Result<ResourceDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Resource)?;

        let name = self.parse_name()?;
        self.reject_generic_type_params(&name)?;

        let capabilities = self.parse_capabilities()?;
        self.skip_newlines();
        let type_policy = self.parse_type_policy_decls()?;
        self.skip_newlines();

        let (fields, validity) = self.parse_fields_and_validity()?;

        let end_span = self.current().span;
        Ok(ResourceDef {
            name,
            type_id,
            identity: type_policy.identity.unwrap_or_default(),
            default_hash_type: type_policy.default_hash_type,
            capacity_floor: type_policy.capacity_floor,
            capabilities,
            fields,
            validity,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    pub(super) fn parse_shared(&mut self, type_id: Option<TypeIdentity>) -> Result<SharedDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Shared)?;

        let name = self.parse_name()?;
        self.reject_generic_type_params(&name)?;

        let capabilities = self.parse_capabilities()?;
        self.skip_newlines();
        let type_policy = self.parse_type_policy_decls()?;
        self.skip_newlines();
        let (fields, validity) = self.parse_fields_and_validity()?;

        let end_span = self.current().span;
        Ok(SharedDef {
            name,
            type_id,
            identity: type_policy.identity.unwrap_or_default(),
            default_hash_type: type_policy.default_hash_type,
            capacity_floor: type_policy.capacity_floor,
            capabilities,
            fields,
            validity,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    pub(super) fn parse_receipt(&mut self, type_id: Option<TypeIdentity>) -> Result<ReceiptDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Receipt)?;

        let name = self.parse_name()?;
        self.reject_generic_type_params(&name)?;
        let claim_output = if self.check(&TokenKind::Arrow) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };

        let capabilities = self.parse_capabilities()?;
        self.skip_newlines();
        let type_policy = self.parse_type_policy_decls()?;
        self.skip_newlines();
        let (fields, validity) = self.parse_fields_and_validity()?;

        let end_span = self.current().span;
        Ok(ReceiptDef {
            name,
            type_id,
            identity: type_policy.identity.unwrap_or_default(),
            default_hash_type: type_policy.default_hash_type,
            capacity_floor: type_policy.capacity_floor,
            claim_output,
            capabilities,
            fields,
            validity,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    pub(super) fn parse_struct(&mut self, type_id: Option<TypeIdentity>) -> Result<StructDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Struct)?;

        let name = self.parse_name()?;
        let type_params = self.parse_type_params()?;
        self.skip_newlines();
        let abilities = self.parse_value_abilities()?;
        if !type_params.is_empty() && type_id.is_some() {
            return Err(CompileError::new(
                "generic value structs cannot declare #[type_id]; Cell identity belongs to concrete Cell-backed types",
                start_span,
            ));
        }

        self.skip_newlines();
        let type_policy = self.parse_type_policy_decls()?;
        self.skip_newlines();
        let (fields, validity) = self.parse_fields_and_validity()?;

        let end_span = self.current().span;
        Ok(StructDef {
            origin: DefinitionOrigin::Source,
            name,
            type_params,
            abilities,
            type_id,
            default_hash_type: type_policy.default_hash_type,
            capacity_floor: type_policy.capacity_floor,
            fields,
            validity,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    pub(super) fn parse_flow(&mut self) -> Result<FlowDef> {
        let start_span = self.current().span;
        let keyword = self.parse_name()?;
        debug_assert_eq!(keyword, "flow");

        let first = self.parse_name_path()?;
        let (name, target) =
            if self.check(&TokenKind::For) || matches!(&self.current().kind, TokenKind::Identifier(name) if name == "for") {
                self.advance();
                (Some(first), self.parse_state_field_path()?)
            } else if self.check(&TokenKind::Dot) {
                self.advance();
                let field_start = self.current().span;
                let field = self.parse_name()?;
                let field_end = self.current().span;
                (
                    None,
                    StateFieldPath {
                        base: first,
                        field,
                        span: Span::new(start_span.start, field_end.end, field_start.line, field_start.column),
                    },
                )
            } else {
                return Err(CompileError::new("expected 'for' or '.field' in flow declaration", self.current().span));
            };
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();

        let mut initial_states = Vec::new();
        let mut terminal_states = Vec::new();
        let mut transitions = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            if self.current_identifier().as_deref() == Some("initial") {
                self.advance();
                initial_states.extend(self.parse_flow_state_list("initial")?);
                self.expect(TokenKind::Semi)?;
                self.skip_newlines();
                continue;
            }
            if self.current_identifier().as_deref() == Some("terminal") {
                self.advance();
                terminal_states.extend(self.parse_flow_state_list("terminal")?);
                self.expect(TokenKind::Semi)?;
                self.skip_newlines();
                continue;
            }
            let transition_start = self.current().span;
            let from = self.parse_name_path()?;
            self.expect(TokenKind::Arrow)?;
            let to = self.parse_name_path()?;
            let action = if matches!(&self.current().kind, TokenKind::Identifier(name) if name == "by") {
                self.advance();
                Some(self.parse_name_path()?)
            } else {
                None
            };
            let transition_end = self.current().span;
            transitions.push(StateTransition {
                from,
                to,
                action,
                span: Span::new(transition_start.start, transition_end.end, transition_start.line, transition_start.column),
            });

            self.record_end(SyntaxKind::ClauseEnd);
            if self.check(&TokenKind::Comma) || self.check(&TokenKind::Semi) {
                self.advance();
            }
            self.skip_newlines();
        }

        self.expect(TokenKind::RBrace)?;

        let end_span = self.current().span;
        Ok(FlowDef {
            name,
            target,
            initial_states,
            terminal_states,
            transitions,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_flow_state_list(&mut self, declaration: &str) -> Result<Vec<String>> {
        let mut states = Vec::new();
        while !self.check(&TokenKind::Semi) && !self.check(&TokenKind::Eof) {
            states.push(self.parse_name_path()?);
            if self.check(&TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
            } else {
                break;
            }
        }
        if states.is_empty() {
            return Err(CompileError::new(format!("{} declaration must name at least one state", declaration), self.current().span));
        }
        Ok(states)
    }

    fn parse_state_field_path(&mut self) -> Result<StateFieldPath> {
        let start_span = self.current().span;
        let base = self.parse_name_path()?;
        self.expect(TokenKind::Dot)?;
        let field = self.parse_name()?;
        let end_span = self.current().span;
        Ok(StateFieldPath { base, field, span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column) })
    }

    fn parse_type_policy_decls(&mut self) -> Result<TypePolicyDecls> {
        let mut policy = TypePolicyDecls::default();
        loop {
            self.skip_newlines();
            let Some(name) = self.current_identifier() else {
                break;
            };
            match name.as_str() {
                "with_default_hash_type" => {
                    if policy.default_hash_type.is_some() {
                        return Err(crate::error::CompileError::new(
                            "duplicate with_default_hash_type declaration",
                            self.current().span,
                        ));
                    }
                    policy.default_hash_type = Some(self.parse_default_hash_type_decl()?);
                }
                "with_capacity_floor" => {
                    if policy.capacity_floor.is_some() {
                        return Err(crate::error::CompileError::new("duplicate with_capacity_floor declaration", self.current().span));
                    }
                    policy.capacity_floor = Some(self.parse_capacity_floor_decl()?);
                }
                "identity" => {
                    if policy.identity.is_some() {
                        return Err(crate::error::CompileError::new("duplicate identity declaration", self.current().span));
                    }
                    policy.identity = Some(self.parse_identity_decl()?);
                }
                _ => break,
            }
        }
        Ok(policy)
    }

    fn parse_default_hash_type_decl(&mut self) -> Result<HashTypeDecl> {
        let start_span = self.current().span;
        self.advance();
        self.expect(TokenKind::LParen)?;
        let value = self.parse_name()?;
        self.expect(TokenKind::RParen)?;
        let normalized = normalize_hash_type_decl(&value).ok_or_else(|| {
            crate::error::CompileError::new(
                format!("unsupported CKB hash_type '{}'; expected Data, Data1, Data2, or Type", value),
                start_span,
            )
        })?;
        Ok(HashTypeDecl { value: normalized, span: start_span })
    }

    fn parse_capacity_floor_decl(&mut self) -> Result<CapacityFloorDecl> {
        let start_span = self.current().span;
        self.advance();
        self.expect(TokenKind::LParen)?;
        let shannons = match &self.current().kind {
            TokenKind::Integer(value) => {
                let value = u64::try_from(*value)
                    .map_err(|_| CompileError::new("capacity floor must fit in u64 shannons", self.current().span))?;
                self.advance();
                value
            }
            _ => {
                return Err(crate::error::CompileError::new(
                    "expected integer shannon value in with_capacity_floor(...)",
                    self.current().span,
                ));
            }
        };
        self.expect(TokenKind::RParen)?;
        if shannons == 0 {
            return Err(crate::error::CompileError::new("capacity floor must be greater than zero shannons", start_span));
        }
        Ok(CapacityFloorDecl { shannons, span: start_span })
    }

    fn parse_identity_decl(&mut self) -> Result<IdentityPolicy> {
        self.advance(); // consume "identity"
        if self.check(&TokenKind::LParen) {
            self.advance();
            let kind = self.parse_name()?;
            let policy = match kind.as_str() {
                "ckb_type_id" => IdentityPolicy::CkbTypeId,
                "field" => {
                    self.expect(TokenKind::LParen)?;
                    let path = self.parse_name()?;
                    self.expect(TokenKind::RParen)?;
                    IdentityPolicy::Field(path)
                }
                "script_args" => IdentityPolicy::ScriptArgs,
                "singleton_type" => IdentityPolicy::SingletonType,
                "none" => IdentityPolicy::None,
                other => {
                    return Err(crate::error::CompileError::new(
                        format!(
                            "unsupported identity policy '{}'; expected none, ckb_type_id, field(...), script_args, or singleton_type",
                            other
                        ),
                        self.current().span,
                    ));
                }
            };
            self.expect(TokenKind::RParen)?;
            Ok(policy)
        } else {
            // `identity ckb_type_id` without parens
            let kind = self.parse_name()?;
            match kind.as_str() {
                "ckb_type_id" => Ok(IdentityPolicy::CkbTypeId),
                "script_args" => Ok(IdentityPolicy::ScriptArgs),
                "singleton_type" => Ok(IdentityPolicy::SingletonType),
                "none" => Ok(IdentityPolicy::None),
                other => Err(crate::error::CompileError::new(
                    format!("unsupported identity policy '{}'; expected none, ckb_type_id, script_args, or singleton_type", other),
                    self.current().span,
                )),
            }
        }
    }

    pub(super) fn parse_const(&mut self) -> Result<ConstDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Const)?;
        let name = self.parse_name_path()?;
        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type()?;
        self.expect(TokenKind::Eq)?;
        let value = self.parse_expr()?;
        self.record_end(SyntaxKind::ClauseEnd);
        self.expect_statement_end()?;

        let end_span = self.current().span;
        Ok(ConstDef { name, ty, value, span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column) })
    }

    pub(super) fn parse_enum(&mut self) -> Result<EnumDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Enum)?;
        let name = self.parse_name_path()?;
        let type_params = self.parse_type_params()?;
        self.skip_newlines();
        let abilities = self.parse_value_abilities()?;
        self.skip_newlines();
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();

        let mut variants = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let variant_start = self.current().span;
            let variant_name = self.parse_name_path()?;
            let fields = if self.check(&TokenKind::LParen) {
                self.advance();
                let items = self.parse_type_list(TokenKind::RParen)?;
                self.expect(TokenKind::RParen)?;
                items
            } else {
                Vec::new()
            };
            self.record_end(SyntaxKind::ListElement);
            let variant_end = self.current().span;
            variants.push(EnumVariant {
                name: variant_name,
                fields,
                span: Span::new(variant_start.start, variant_end.end, variant_start.line, variant_start.column),
            });

            self.expect_list_separator(TokenKind::RBrace)?;
        }

        self.expect(TokenKind::RBrace)?;

        let end_span = self.current().span;
        Ok(EnumDef {
            origin: DefinitionOrigin::Source,
            name,
            type_params,
            abilities,
            variants,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_capabilities(&mut self) -> Result<Vec<Capability>> {
        let mut caps = Vec::new();

        if self.check(&TokenKind::Has) {
            self.advance();

            while let Some(capability) = Capability::from_source_name(&self.current().text) {
                if caps.contains(&capability) {
                    return Err(CompileError::new("duplicate lifecycle capability", self.current().span));
                }
                caps.push(capability);
                self.advance();

                if self.check(&TokenKind::Comma) {
                    self.advance();
                    self.skip_newlines();
                } else {
                    break;
                }
            }
        }

        Ok(caps)
    }

    fn parse_fields_and_validity(&mut self) -> Result<(Vec<Field>, Option<ValidityBlock>)> {
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();

        let mut fields = Vec::new();
        let mut validity = None;
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            if self.current_identifier().as_deref() == Some("validity") && self.peek(1).kind != TokenKind::Colon {
                if validity.is_some() {
                    return Err(CompileError::new("type declaration contains more than one validity block", self.current().span));
                }
                let start = self.current().span;
                self.advance();
                self.skip_newlines();
                let mut predicates = Vec::new();
                while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
                    if !self.check(&TokenKind::Require) {
                        return Err(CompileError::new(
                            "validity block only accepts pure require predicates and must be the final section of the type body",
                            self.current().span,
                        ));
                    }
                    predicates.push(self.parse_require()?);
                    self.record_end(SyntaxKind::ClauseEnd);
                    self.expect_statement_end()?;
                    self.skip_newlines();
                }
                if predicates.is_empty() {
                    return Err(CompileError::new("validity block must contain at least one require predicate", start));
                }
                let end = predicates.last().map(Expr::span).unwrap_or(start);
                validity = Some(ValidityBlock { predicates, span: Span::new(start.start, end.end, start.line, start.column) });
                break;
            }
            let field = self.parse_field()?;
            fields.push(field);
            self.expect_list_separator(TokenKind::RBrace)?;
        }

        self.expect(TokenKind::RBrace)?;
        Ok((fields, validity))
    }

    fn parse_field(&mut self) -> Result<Field> {
        let start_span = self.current().span;

        let name = self.parse_name()?;

        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type()?;
        if self.current_identifier().as_deref() == Some("where") {
            return Err(CompileError::new(
                "field-level 'where' refinements are deferred; declare predicates in the type validity block",
                self.current().span,
            ));
        }

        let end_span = self.current().span;
        self.record_syntax_node(SyntaxKind::Field, start_span);
        self.record_end(SyntaxKind::ListElement);
        Ok(Field { name, ty, span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column) })
    }
}
