use super::{
    match_pattern_variant, type_repr, MatchConstructor, MatchPatternAnalysis, MatchPayloadBindings, TypeChecker,
    MATCH_EXHAUSTIVENESS_BUDGET,
};
use crate::ast::{MatchExpr, MatchPattern, Type};
use crate::error::{CompileError, Result, Span};
use crate::resolve::TypeDef;
use std::collections::{HashMap, HashSet};

impl TypeChecker<'_> {
    pub(super) fn check_match_patterns(&self, scrutinee_ty: &Type, match_expr: &MatchExpr) -> Result<Vec<MatchPayloadBindings>> {
        let enum_shape = if let Type::Named(enum_name) = scrutinee_ty {
            self.resolve_enum_variants(enum_name).map(|variants| (enum_name.as_str(), variants))
        } else {
            None
        };
        let mut seen = HashSet::new();
        let mut has_catch_all = false;
        let mut payloads = Vec::with_capacity(match_expr.arms.len());
        let mut coverage_rows = Vec::with_capacity(match_expr.arms.len());

        for arm in &match_expr.arms {
            if has_catch_all {
                return Err(CompileError::new(
                    "wildcard pattern '_' or another irrefutable pattern must be the last match arm",
                    arm.span,
                ));
            }
            let analysis = self.analyze_match_pattern(scrutinee_ty, &arm.pattern, arm.span)?;
            for variant in &analysis.covered_variants {
                if !seen.insert(variant.clone()) {
                    return Err(CompileError::new(
                        format!("duplicate exhaustive match coverage for enum variant '{}'", variant),
                        arm.span,
                    ));
                }
            }
            has_catch_all = analysis.irrefutable;
            coverage_rows.push(vec![arm.pattern.clone()]);
            payloads.push(MatchPayloadBindings { bindings: analysis.bindings });
        }

        let matrix_exhaustive = has_catch_all
            || self.match_pattern_matrix_is_exhaustive(std::slice::from_ref(scrutinee_ty), coverage_rows, MATCH_EXHAUSTIVENESS_BUDGET);
        if !matrix_exhaustive {
            if let Some((enum_name, variants)) = enum_shape {
                if seen.len() != variants.len() {
                    let missing = variants.iter().filter(|variant| !seen.contains(*variant)).cloned().collect::<Vec<_>>().join(", ");
                    return Err(CompileError::new(
                        format!("non-exhaustive match for enum '{}'; missing {}", enum_name, missing),
                        match_expr.span,
                    ));
                }
            } else {
                return Err(CompileError::new(
                    "non-enum match requires a final irrefutable binding, tuple/struct pattern, or wildcard arm",
                    match_expr.span,
                ));
            }
        }

        Ok(payloads)
    }

    fn analyze_match_pattern(&self, expected: &Type, pattern: &MatchPattern, span: Span) -> Result<MatchPatternAnalysis> {
        let mut analysis = match pattern {
            MatchPattern::Wildcard => {
                if self.is_linear_type(expected) {
                    return Err(CompileError::new(
                        "wildcard match pattern cannot discard a linear Cell-backed value; bind it and discharge ownership",
                        span,
                    ));
                }
                MatchPatternAnalysis { bindings: Vec::new(), covered_variants: Vec::new(), irrefutable: true }
            }
            MatchPattern::Binding(name) => MatchPatternAnalysis {
                bindings: vec![(name.clone(), expected.clone())],
                covered_variants: Vec::new(),
                irrefutable: true,
            },
            MatchPattern::Tuple(items) => {
                let Type::Tuple(types) = expected else {
                    return Err(CompileError::new(
                        format!("tuple pattern '{}' requires a tuple scrutinee, got {}", pattern, type_repr(expected)),
                        span,
                    ));
                };
                if items.len() != types.len() {
                    return Err(CompileError::new(
                        format!("tuple pattern expects {} field(s), got {}", types.len(), items.len()),
                        span,
                    ));
                }
                self.combine_pattern_children(items.iter().zip(types).map(|(item, ty)| self.analyze_match_pattern(ty, item, span)))?
            }
            MatchPattern::Struct { path, fields } => {
                let Type::Named(type_name) = expected else {
                    return Err(CompileError::new(
                        format!("struct pattern '{}' requires a named value, got {}", pattern, type_repr(expected)),
                        span,
                    ));
                };
                if path != type_name && path.rsplit_once("::").map(|(_, name)| name) != Some(type_name.as_str()) {
                    return Err(CompileError::new(format!("struct pattern '{}' does not match type '{}'", path, type_name), span));
                }
                let declared = self
                    .resolve_named_type_fields(type_name)
                    .ok_or_else(|| CompileError::new(format!("struct pattern type '{}' has no declared fields", type_name), span))?;
                let mut seen_fields = HashSet::new();
                let mut children = Vec::with_capacity(fields.len());
                for (field, child) in fields {
                    if !seen_fields.insert(field.clone()) {
                        return Err(CompileError::new(format!("duplicate struct pattern field '{}'", field), span));
                    }
                    let field_ty = declared.get(field).ok_or_else(|| {
                        CompileError::new(format!("unknown field '{}' in struct pattern for '{}'", field, type_name), span)
                    })?;
                    children.push(self.analyze_match_pattern(field_ty, child, span));
                }
                let missing = declared.keys().filter(|field| !seen_fields.contains(*field)).cloned().collect::<Vec<_>>();
                if !missing.is_empty() {
                    return Err(CompileError::new(
                        format!("struct pattern for '{}' must name every field; missing {}", type_name, missing.join(", ")),
                        span,
                    ));
                }
                self.combine_pattern_children(children.into_iter())?
            }
            MatchPattern::Variant { path, fields } => {
                let Type::Named(enum_name) = expected else {
                    return Err(CompileError::new(
                        format!("enum pattern '{}' requires an enum scrutinee, got {}", pattern, type_repr(expected)),
                        span,
                    ));
                };
                let variants = self.resolve_enum_variants(enum_name).ok_or_else(|| {
                    CompileError::new(format!("match pattern '{}' targets non-enum type '{}'", pattern, enum_name), span)
                })?;
                let Some(variant) = match_pattern_variant(enum_name, path) else {
                    return Err(CompileError::new(format!("match pattern '{}' does not match enum '{}'", pattern, enum_name), span));
                };
                if !variants.iter().any(|candidate| candidate == variant) {
                    return Err(CompileError::new(
                        format!("unknown enum variant '{}::{}' in match pattern", enum_name, variant),
                        span,
                    ));
                }
                let declared = self
                    .resolve_enum_variant_fields(enum_name)
                    .and_then(|variants| variants.get(variant).cloned())
                    .unwrap_or_default();
                if fields.len() != declared.len() {
                    return Err(CompileError::new(
                        format!(
                            "match pattern '{}::{}' expects {} payload pattern(s), got {}",
                            enum_name,
                            variant,
                            declared.len(),
                            fields.len()
                        ),
                        span,
                    ));
                }
                let children = self.combine_pattern_children(
                    fields.iter().zip(&declared).map(|(field, ty)| self.analyze_match_pattern(ty, field, span)),
                )?;
                MatchPatternAnalysis {
                    bindings: children.bindings,
                    covered_variants: if children.irrefutable { vec![variant.to_string()] } else { Vec::new() },
                    irrefutable: variants.len() == 1 && children.irrefutable,
                }
            }
            MatchPattern::Or(patterns) => {
                if patterns.len() < 2 {
                    return Err(CompileError::new("or-pattern requires at least two alternatives", span));
                }
                let alternatives =
                    patterns.iter().map(|pattern| self.analyze_match_pattern(expected, pattern, span)).collect::<Result<Vec<_>>>()?;
                if alternatives.iter().any(|alternative| !alternative.bindings.is_empty()) {
                    return Err(CompileError::new(
                        "or-pattern alternatives cannot bind values in the 0.25 deterministic pattern kernel",
                        span,
                    ));
                }
                let covered_variants =
                    alternatives.iter().flat_map(|alternative| alternative.covered_variants.iter().cloned()).collect::<Vec<_>>();
                MatchPatternAnalysis {
                    bindings: Vec::new(),
                    covered_variants,
                    irrefutable: alternatives.iter().any(|alternative| alternative.irrefutable)
                        || self.match_pattern_matrix_is_exhaustive(
                            std::slice::from_ref(expected),
                            patterns.iter().cloned().map(|pattern| vec![pattern]).collect(),
                            MATCH_EXHAUSTIVENESS_BUDGET,
                        ),
                }
            }
        };

        let mut names = HashSet::new();
        for (name, _) in &analysis.bindings {
            if name == "_" || !names.insert(name.clone()) {
                return Err(CompileError::new(format!("duplicate match binding '{}'", name), span));
            }
        }
        analysis.bindings.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(analysis)
    }

    fn combine_pattern_children(&self, children: impl Iterator<Item = Result<MatchPatternAnalysis>>) -> Result<MatchPatternAnalysis> {
        let mut bindings = Vec::new();
        let mut irrefutable = true;
        for child in children {
            let child = child?;
            bindings.extend(child.bindings);
            irrefutable &= child.irrefutable;
        }
        Ok(MatchPatternAnalysis { bindings, covered_variants: Vec::new(), irrefutable })
    }

    fn match_pattern_matrix_is_exhaustive(&self, types: &[Type], rows: Vec<Vec<MatchPattern>>, budget: usize) -> bool {
        if budget == 0 {
            return false;
        }
        if types.is_empty() {
            return !rows.is_empty();
        }

        let mut expanded = Vec::new();
        for row in rows {
            let Some((head, tail)) = row.split_first() else {
                continue;
            };
            match head {
                MatchPattern::Or(alternatives) => {
                    for alternative in alternatives {
                        let mut next = Vec::with_capacity(row.len());
                        next.push(alternative.clone());
                        next.extend_from_slice(tail);
                        expanded.push(next);
                    }
                }
                _ => expanded.push(row),
            }
        }

        let head_ty = &types[0];
        let tail_types = &types[1..];
        let Some(constructors) = self.match_constructors_for_type(head_ty) else {
            let default_rows = expanded
                .into_iter()
                .filter_map(|row| {
                    matches!(row.first(), Some(MatchPattern::Wildcard | MatchPattern::Binding(_))).then(|| row[1..].to_vec())
                })
                .collect();
            return self.match_pattern_matrix_is_exhaustive(tail_types, default_rows, budget - 1);
        };

        constructors.into_iter().all(|constructor| {
            let mut specialized_rows = Vec::new();
            for row in &expanded {
                let Some((head, tail)) = row.split_first() else {
                    continue;
                };
                let fields = match head {
                    MatchPattern::Wildcard | MatchPattern::Binding(_) => {
                        vec![MatchPattern::Wildcard; constructor.fields.len()]
                    }
                    _ => {
                        let Some(fields) = self.specialize_match_pattern(head_ty, &constructor, head) else {
                            continue;
                        };
                        fields
                    }
                };
                let mut specialized = Vec::with_capacity(fields.len() + tail.len());
                specialized.extend(fields);
                specialized.extend_from_slice(tail);
                specialized_rows.push(specialized);
            }

            let mut specialized_types = constructor.fields.iter().map(|(_, ty)| ty.clone()).collect::<Vec<_>>();
            specialized_types.extend_from_slice(tail_types);
            self.match_pattern_matrix_is_exhaustive(&specialized_types, specialized_rows, budget - 1)
        })
    }

    fn match_constructors_for_type(&self, ty: &Type) -> Option<Vec<MatchConstructor>> {
        match ty {
            Type::Tuple(items) => Some(vec![MatchConstructor {
                name: "tuple".to_string(),
                fields: items.iter().cloned().map(|ty| (None, ty)).collect(),
            }]),
            Type::Named(name) => {
                if let (Some(variants), Some(fields)) = (self.resolve_enum_variants(name), self.resolve_enum_variant_fields(name)) {
                    return Some(
                        variants
                            .into_iter()
                            .map(|variant| MatchConstructor {
                                fields: fields.get(&variant).cloned().unwrap_or_default().into_iter().map(|ty| (None, ty)).collect(),
                                name: variant,
                            })
                            .collect(),
                    );
                }
                let fields = self.resolve_named_type_fields(name)?;
                let mut fields = fields.into_iter().collect::<Vec<_>>();
                fields.sort_by(|left, right| left.0.cmp(&right.0));
                Some(vec![MatchConstructor {
                    name: name.clone(),
                    fields: fields.into_iter().map(|(name, ty)| (Some(name), ty)).collect(),
                }])
            }
            _ => None,
        }
    }

    fn specialize_match_pattern(
        &self,
        expected: &Type,
        constructor: &MatchConstructor,
        pattern: &MatchPattern,
    ) -> Option<Vec<MatchPattern>> {
        match (expected, pattern) {
            (Type::Tuple(_), MatchPattern::Tuple(items)) if constructor.name == "tuple" => Some(items.clone()),
            (Type::Named(enum_name), MatchPattern::Variant { path, fields })
                if match_pattern_variant(enum_name, path) == Some(constructor.name.as_str()) =>
            {
                Some(fields.clone())
            }
            (Type::Named(type_name), MatchPattern::Struct { path, fields })
                if path == type_name || path.rsplit_once("::").map(|(_, name)| name) == Some(type_name.as_str()) =>
            {
                let fields = fields.iter().cloned().collect::<HashMap<_, _>>();
                constructor.fields.iter().map(|(name, _)| name.as_ref().and_then(|name| fields.get(name)).cloned()).collect()
            }
            _ => None,
        }
    }

    pub(super) fn resolve_enum_variants(&self, enum_name: &str) -> Option<Vec<String>> {
        if let Some(variants) = self.enum_variants.get(enum_name) {
            return Some(variants.clone());
        }
        self.resolver
            .zip(self.current_module.as_deref())
            .and_then(|(resolver, module)| resolver.resolve_type(module, enum_name))
            .and_then(|ty| match ty {
                TypeDef::Enum(enum_def) => Some(enum_def.variants.into_iter().map(|variant| variant.name).collect()),
                _ => None,
            })
    }

    pub(super) fn resolve_enum_variant_fields(&self, enum_name: &str) -> Option<HashMap<String, Vec<Type>>> {
        if let Some(variants) = self.enum_variant_fields.get(enum_name) {
            return Some(variants.clone());
        }
        self.resolver
            .zip(self.current_module.as_deref())
            .and_then(|(resolver, module)| resolver.resolve_type(module, enum_name))
            .and_then(|ty| match ty {
                TypeDef::Enum(enum_def) => Some(enum_def.variants.into_iter().map(|variant| (variant.name, variant.fields)).collect()),
                _ => None,
            })
    }
}
