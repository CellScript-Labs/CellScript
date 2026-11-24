use super::hover_display::{
    action_metadata_hover, capability_hover, function_metadata_hover, generic_params_hover, payload_enum_hover, receipt_flow_hover,
    type_validity_hover, value_abilities_hover,
};
use super::positions::{position_to_offset, word_before_offset};
use super::{item_name, item_span, position_in_range, span_to_range, type_to_string, Hover, LspServer, Position};
use crate::ast::{BindingPattern, Field, Item, ParamSource, Stmt};

impl LspServer {
    pub fn hover(&self, uri: &str, position: Position) -> Option<Hover> {
        let symbol = self.symbol_at_position(uri, position)?;

        // 1. Try top-level item hover (existing logic).
        if let (Some(ast), Some(source)) = (self.ast_cache.get(uri), self.documents.get(uri)) {
            if let Some(item @ Item::Collections(def)) = ast.items.iter().find(|item| item_name(item) == Some(symbol.as_str())) {
                let mut hover = self.item_hover(source, item, None)?;
                match self.collection_budget_hover(uri, &def.name) {
                    Ok(explanation) => hover.contents.push_str(&explanation),
                    Err(_) => hover.contents.push_str(
                        "\n\nBudget estimate unavailable: resolve declaration/type diagnostics first. No capacity fit is assumed.",
                    ),
                }
                return Some(hover);
            }
            // Semantic hover uses the same captured package admission as diagnostics.
            // A fallback edition is useful for syntax, but cannot authorize metadata.
            let metadata = self.view_index(uri).and_then(|index| index.inputs.diagnostics().metadata);
            if let Some(hover) = ast.items.iter().find_map(|item| {
                if item_name(item) == Some(symbol.as_str()) {
                    self.item_hover(source, item, metadata.as_ref())
                } else {
                    None
                }
            }) {
                return Some(hover);
            }
        }

        // 2. Try field hover.
        if let Some(hover) = self.field_hover(uri, position, &symbol) {
            return Some(hover);
        }

        // 3. Try local variable / parameter hover.
        if let Some(hover) = self.local_hover(uri, position, &symbol) {
            return Some(hover);
        }

        // 4. Only admitted workspace modules can contribute semantic metadata.
        let modules = self.view_index(uri).map(|index| index.modules.clone()).unwrap_or_default();
        for module in modules.iter() {
            let mut metadata = None;
            if let Some(hover) = module.ast.items.iter().find_map(|item| {
                if item_name(item) != Some(symbol.as_str()) {
                    return None;
                }
                let metadata = metadata.get_or_insert_with(|| crate::compile_metadata(&module.source, module.edition, None).ok());
                self.item_hover(&module.source, item, metadata.as_ref())
            }) {
                return Some(hover);
            }
        }

        None
    }

    /// Hover information for a field access (e.g. `token.amount`).
    fn field_hover(&self, uri: &str, position: Position, symbol: &str) -> Option<Hover> {
        let content = self.documents.get(uri)?;
        let offset = position_to_offset(content, position)?;
        let line_start = self.line_start_offset(content, position.line);
        let prefix = &content[line_start..offset];
        let dot_pos = prefix.rfind('.')?;
        let type_name = word_before_offset(prefix, dot_pos)?;

        let ast = self.ast_cache.get(uri)?;
        for item in &ast.items {
            let fields: &[Field] = match item {
                Item::Resource(r) if r.name == type_name => &r.fields,
                Item::Shared(s) if s.name == type_name => &s.fields,
                Item::Receipt(r) if r.name == type_name => &r.fields,
                Item::Struct(s) if s.name == type_name => &s.fields,
                _ => continue,
            };
            for field in fields {
                if field.name == symbol {
                    return Some(Hover {
                        contents: format!(
                            "```cellscript\n{}: {}\n```\n\nField of `{}`",
                            field.name,
                            type_to_string(&field.ty),
                            type_name
                        ),
                        range: Some(span_to_range(content, field.span)),
                    });
                }
            }
        }
        None
    }

    /// Hover information for a local variable or parameter.
    fn local_hover(&self, uri: &str, position: Position, symbol: &str) -> Option<Hover> {
        let content = self.documents.get(uri)?;
        let ast = self.ast_cache.get(uri)?;

        for item in &ast.items {
            let (params, body, item_span_val) = match item {
                Item::Action(a) => (&a.params, &a.body, a.span),
                Item::Function(f) => (&f.params, &f.body, f.span),
                Item::Lock(l) => (&l.params, &l.body, l.span),
                _ => continue,
            };

            let func_range = span_to_range(content, item_span_val);
            if !position_in_range(position, func_range) {
                continue;
            }

            // Check parameters.
            for param in params {
                if param.name == symbol {
                    let note = if param.is_mut {
                        "\n\nLeading `mut` only applies to local-style mutable value bindings; Cell state updates should be modeled with `action(before: T) -> after: T` plus `transition` and `require` constraints."
                    } else if param.source == ParamSource::Input {
                        "\n\n`input` declares a Cell input role. The selected artifact determines its coordinate system."
                    } else if param.source == ParamSource::Output {
                        "\n\n`output` marks a proposed transaction output Cell. Use `transition input.state: Live -> output.state: Filled` for state transitions and `require` for field continuity."
                    } else if param.source == ParamSource::LockArgs {
                        "\n\n`lock_args` is decoded from the executing lock Script.args bytes."
                    } else {
                        ""
                    };
                    return Some(Hover {
                        contents: format!("```cellscript\n{}: {}\n```\n\nParameter{}", param.name, type_to_string(&param.ty), note),
                        range: Some(span_to_range(content, param.span)),
                    });
                }
            }
            if let Item::Action(action) = item {
                for output in &action.outputs {
                    if output.name == symbol {
                        return Some(Hover {
                            contents: format!(
                                "```cellscript\n{}: {}\n```\n\nAction output binding: proposed transaction output Cell.",
                                output.name,
                                type_to_string(&output.ty)
                            ),
                            range: Some(span_to_range(content, output.span)),
                        });
                    }
                }
            }

            // Check local let bindings.
            for stmt in body {
                if let Stmt::Let(let_stmt) = stmt
                    && let BindingPattern::Name(name) = &let_stmt.pattern
                    && name == symbol
                {
                    let ty_str = let_stmt.ty.as_ref().map(type_to_string).unwrap_or_else(|| "_".to_string());
                    return Some(Hover {
                        contents: format!(
                            "```cellscript\n{}{}: {}\n```\n\nLocal variable",
                            if let_stmt.is_mut { "mut " } else { "" },
                            name,
                            ty_str
                        ),
                        range: Some(span_to_range(content, let_stmt.span)),
                    });
                }
            }
        }
        None
    }

    fn item_hover(&self, source: &str, item: &Item, metadata: Option<&crate::CompileMetadata>) -> Option<Hover> {
        let range = span_to_range(source, item_span(item));
        match item {
            Item::Collections(def) => Some(Hover {
                contents: format!("collections {}\n\nShared arena: {} bytes and {} allocation events; {} named members; local operations and read-only helpers require allocation, bounds and lifetime evidence.", def.name, def.bytes, def.allocations, def.members.len()),
                range: Some(range),
            }),
            Item::DataShape(def) => Some(Hover {
                contents: format!("data_shape {}\n\nElement: {}. Limits: {} items, {} payload bytes, {} encoded bytes. Bounded read-only codec shape; no new wire format or entry-witness budget.", def.name, type_to_string(&def.element), def.limits.items, def.limits.payload_bytes, def.limits.encoded_bytes),
                range: Some(span_to_range(source, def.span)),
            }),
            Item::CellRoles(def) => Some(Hover {
                contents: format!("cell_roles {}\n\n{} named roles; exact identities, authority, correspondence and bounded scans. Bounded verification and immutable named-role selection; native artifact checking requires source, authority and lifetime evidence.", def.name, def.roles.len()),
                range: Some(range),
            }),
            Item::Resource(r) => Some(Hover {
                contents: format!(
                    "```cellscript\nresource {}\n```{}{}",
                    r.name,
                    capability_hover(&r.capabilities),
                    type_validity_hover(&r.name, metadata)
                ),
                range: Some(range),
            }),
            Item::Shared(s) => Some(Hover {
                contents: format!(
                    "```cellscript\nshared {}\n```{}{}",
                    s.name,
                    capability_hover(&s.capabilities),
                    type_validity_hover(&s.name, metadata)
                ),
                range: Some(range),
            }),
            Item::Receipt(r) => Some(Hover {
                contents: format!(
                    "```cellscript\nreceipt {}\n```{}{}{}",
                    r.name,
                    capability_hover(&r.capabilities),
                    receipt_flow_hover(r, metadata),
                    type_validity_hover(&r.name, metadata)
                ),
                range: Some(range),
            }),
            Item::Struct(s) => Some(Hover {
                contents: format!(
                    "```cellscript\nstruct {}{}{}\n```{}",
                    s.name,
                    generic_params_hover(&s.type_params),
                    value_abilities_hover(&s.abilities),
                    type_validity_hover(&s.name, metadata)
                ),
                range: Some(range),
            }),
            Item::Enum(e) => Some(Hover { contents: payload_enum_hover(e, metadata), range: Some(range) }),
            Item::Action(a) => Some(Hover {
                contents: format!(
                    "```cellscript\naction {}\n```\n\n{}{}",
                    a.name,
                    a.doc_comment.as_deref().unwrap_or("No documentation"),
                    action_metadata_hover(&a.name, metadata)
                ),
                range: Some(range),
            }),
            Item::Function(f) => Some(Hover {
                contents: format!(
                    "```cellscript\nfn {}{}\n```\n\n{}{}",
                    f.name,
                    generic_params_hover(&f.type_params),
                    f.doc_comment.as_deref().unwrap_or("No documentation"),
                    function_metadata_hover(&f.name, f, metadata)
                ),
                range: Some(range),
            }),
            Item::Lock(l) => Some(Hover { contents: format!("```cellscript\nlock {}\n```", l.name), range: Some(range) }),
            Item::Invariant(i) => Some(Hover {
                contents: format!(
                    "```cellscript\ninvariant {}\n```\n\nReads: {}\n\nBounded quantifiers: {}{}",
                    i.name,
                    i.reads.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "),
                    i.quantifiers.len(),
                    if i.quantifiers.is_empty() {
                        ""
                    } else {
                        " (runtime-helper-required; scan cost and vacuous/count policy are emitted in ProofPlan)"
                    }
                ),
                range: Some(range),
            }),
            _ => None,
        }
    }
}
