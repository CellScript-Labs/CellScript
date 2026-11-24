//! Parser-independent declarations for a persistent, explicitly dispatched policy.
//!
//! A declaration is an artifact boundary, not an action-call convention. Tags
//! belong to the declaration and never depend on source order or action names.

use crate::error::{CompileError, Result};
use crate::ir::{IrItem, IrModule, IrType};
use crate::CompileEntryScope;
pub use cellscript_artifact_checker::{EntryCellBindings, EntryCoverage};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

mod compile;
mod metadata;
#[cfg(all(test, not(feature = "wasm")))]
mod vm_tests;
#[cfg(not(feature = "wasm"))]
pub use compile::{compile_artifact, compile_sources_artifact};
pub use compile::{compile_artifact_metadata, compile_path_artifact_metadata, compile_sources_artifact_metadata};
#[cfg(not(feature = "wasm"))]
pub(crate) use metadata::bind_policy_metadata;
pub use metadata::{
    encode_policy_action_record, PolicyArtifactMetadata, POLICY_ARTIFACT_METADATA_SCHEMA, POLICY_WITNESS_PLACEMENT_ABI,
    POLICY_WITNESS_PLACEMENT_FIELD, POLICY_WITNESS_PLACEMENT_SOURCE,
};

pub const ARTIFACT_DECLARATION_SCHEMA: &str = "cellscript-artifact-declaration-v2";
pub const MAX_ARTIFACT_COMMON_CHECKS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactDeclaration {
    pub name: String,
    pub context: ArtifactContext,
    pub bindings: EntryCellBindings,
    pub coverage: EntryCoverage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    pub dispatch: ArtifactDispatch,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ArtifactAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub common_checks: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ArtifactContext {
    TypeGroup {},
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactDispatch {
    SingleEntry,
    PolicyWitnessV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactAction {
    pub tag: u32,
    pub action: String,
}

impl ArtifactDeclaration {
    /// Configure one fixed Type-group action with the ordinary entry witness ABI.
    pub fn single_type_group(name: impl Into<String>, action: impl Into<String>, layout: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            context: ArtifactContext::TypeGroup {},
            bindings: EntryCellBindings::GroupRelative,
            coverage: EntryCoverage::FixedGroup,
            layout: Some(layout.into()),
            dispatch: ArtifactDispatch::SingleEntry,
            action: Some(action.into()),
            actions: Vec::new(),
            common_checks: Vec::new(),
        }
    }

    /// Validate declaration-local properties before resolving any source.
    pub fn validate(&self) -> Result<()> {
        validate_name(&self.name, "artifact")?;
        match (self.bindings, self.coverage, self.layout.as_deref()) {
            (EntryCellBindings::GroupRelative, EntryCoverage::FixedGroup, Some(layout)) => validate_name(layout, "group layout")?,
            (EntryCellBindings::TransactionAbsolute, EntryCoverage::ActionDefined, None) => {}
            _ => return Err(policy_error("fixed-group coverage requires group-relative bindings and a layout; action-defined coverage uses transaction-absolute bindings")),
        }
        if self.dispatch == ArtifactDispatch::SingleEntry {
            if !self.actions.is_empty() || !self.common_checks.is_empty() {
                return Err(policy_error("single-entry declares one action and no tags or common checks"));
            }
            let action = self.action.as_deref().ok_or_else(|| policy_error("single-entry requires an action"))?;
            return validate_name(action, "artifact action");
        }
        if self.coverage != EntryCoverage::FixedGroup {
            return Err(policy_error("policy-witness-v1 requires fixed-group coverage"));
        }
        if self.action.is_some() {
            return Err(policy_error("policy-witness-v1 declares tagged actions; use the actions list"));
        }
        if self.actions.is_empty() || self.actions.len() > crate::policy_witness::MAX_POLICY_ARTIFACT_VARIANTS {
            return Err(policy_error(format!(
                "artifact '{}' must declare between 1 and {} explicitly tagged actions",
                self.name,
                crate::policy_witness::MAX_POLICY_ARTIFACT_VARIANTS
            )));
        }
        let mut tags = BTreeSet::new();
        let mut actions = BTreeSet::new();
        for variant in &self.actions {
            validate_name(&variant.action, "artifact action")?;
            if !tags.insert(variant.tag) {
                return Err(policy_error(format!("artifact '{}' repeats numeric tag {}", self.name, variant.tag)));
            }
            if !actions.insert(variant.action.as_str()) {
                return Err(policy_error(format!("artifact '{}' maps action '{}' more than once", self.name, variant.action)));
            }
        }
        if self.common_checks.len() > MAX_ARTIFACT_COMMON_CHECKS {
            return Err(policy_error(format!(
                "artifact '{}' exceeds the {} common-check limit",
                self.name, MAX_ARTIFACT_COMMON_CHECKS
            )));
        }
        let mut common = BTreeSet::new();
        for name in &self.common_checks {
            validate_name(name, "common check")?;
            if !common.insert(name.as_str()) {
                return Err(policy_error(format!("artifact '{}' repeats common check '{name}'", self.name)));
            }
            if actions.contains(name.as_str()) {
                return Err(policy_error(format!(
                    "artifact '{}' cannot expose common check '{name}' as a selectable action",
                    self.name
                )));
            }
        }
        Ok(())
    }

    /// Numeric dispatch order is canonical. Common-check order is deliberately
    /// preserved: it specifies evaluation and first-failure order.
    pub fn canonicalized(&self) -> Result<Self> {
        self.validate()?;
        let mut declaration = self.clone();
        declaration.actions.sort_by_key(|variant| variant.tag);
        Ok(declaration)
    }

    pub fn action(&self, name: &str) -> Option<&ArtifactAction> {
        self.actions.iter().find(|variant| variant.action == name)
    }
}

/// A single-entry declaration may provide the package's default binding.
/// Policy dispatch is always selected by its artifact name.
pub(crate) fn resolve_single_entry_scope<'a>(
    declarations: impl IntoIterator<Item = &'a ArtifactDeclaration>,
    scope: Option<CompileEntryScope>,
) -> Result<Option<CompileEntryScope>> {
    let mut candidates = declarations.into_iter().filter(|declaration| {
        declaration.dispatch == ArtifactDispatch::SingleEntry
            && match &scope {
                None => true,
                Some(CompileEntryScope::Action(name)) => declaration.action.as_ref() == Some(name),
                _ => false,
            }
    });
    let first = candidates.next();
    if first.is_some() && candidates.next().is_some() {
        return Err(policy_error("multiple single-entry declarations match; select one with --artifact"));
    }
    Ok(first.map(|declaration| CompileEntryScope::Artifact(declaration.clone())).or(scope))
}

pub(crate) fn validate_declarations(declarations: &[ArtifactDeclaration]) -> Result<()> {
    let mut names = BTreeSet::new();
    for declaration in declarations {
        declaration.validate()?;
        if !names.insert(declaration.name.as_str()) {
            return Err(policy_error(format!("artifact name '{}' is declared more than once", declaration.name)));
        }
    }
    Ok(())
}

/// Keep exactly the union of exported action and common-check dependencies.
/// Existing single-entry scoping remains the authority for each closure; its
/// retained actions are dependencies, never additional dispatch variants.
pub(crate) fn scope_ir_to_artifact(ir: &IrModule, declaration: &ArtifactDeclaration) -> Result<IrModule> {
    let declaration = declaration.canonicalized()?;
    if let Some(name) = &declaration.action {
        let mut scoped = crate::scope_ir_to_entry(ir, &CompileEntryScope::Action(name.clone()))?;
        crate::ir::bind_single_entry_cells(&mut scoped, &declaration)?;
        return Ok(scoped);
    }
    let mut retained = BTreeSet::new();
    let mut external_types = BTreeSet::new();
    let mut external_callables = BTreeSet::new();
    let mut enum_names = BTreeSet::new();
    for name in declaration.actions.iter().map(|variant| &variant.action).chain(&declaration.common_checks) {
        let matches = ir
            .items
            .iter()
            .filter_map(|item| match item {
                IrItem::Action(action) if action.name == *name => Some(action),
                _ => None,
            })
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(policy_error(format!(
                "artifact '{}' must resolve action '{name}' exactly once; found {}",
                declaration.name,
                matches.len()
            )));
        }
        let action = matches[0];
        if action.return_type.as_ref().is_some_and(|ty| *ty != IrType::Unit) {
            return Err(policy_error(format!(
                "artifact action '{name}' must return unit; policy success is the zero-status verification contract"
            )));
        }
        if declaration.common_checks.contains(name) && !action.params.is_empty() {
            return Err(policy_error(format!("artifact common check '{name}' must have no parameters")));
        }
        let scoped = crate::scope_ir_to_entry(ir, &CompileEntryScope::Action(name.clone()))?;
        retained.extend(scoped.items.iter().map(item_key));
        external_types.extend(scoped.external_type_defs.iter().map(|item| item.name.clone()));
        external_callables.extend(scoped.external_callable_abis.iter().map(|item| item.name.clone()));
        enum_names.extend(scoped.enum_layouts.keys().cloned());
        enum_names.extend(scoped.enum_fixed_sizes.keys().cloned());
    }
    // The policy schema participates in group membership even if a particular
    // action does not materialize every declared field.
    let resource = declaration.layout.as_deref().expect("validated fixed group layout");
    retained.insert(("type", resource.to_string()));
    external_types.insert(resource.to_string());
    let mut scoped = ir.clone();
    scoped.items.retain(|item| retained.contains(&item_key(item)));
    scoped.external_type_defs.retain(|item| external_types.contains(&item.name));
    scoped.external_callable_abis.retain(|item| external_callables.contains(&item.name));
    scoped.enum_layouts.retain(|name, _| enum_names.contains(name));
    scoped.enum_fixed_sizes.retain(|name, _| enum_names.contains(name));
    crate::ir::bind_artifact_policy(&mut scoped, &declaration)?;
    scoped.entry_selection = crate::ir::IrEntrySelection::Artifact(declaration);
    Ok(scoped)
}

fn item_key(item: &IrItem) -> (&'static str, String) {
    match item {
        IrItem::TypeDef(item) => ("type", item.name.clone()),
        IrItem::Action(item) => ("action", item.name.clone()),
        IrItem::Function(item) => ("helper", item.name.clone()),
        IrItem::Lock(item) => ("lock", item.name.clone()),
        IrItem::Invariant(item) => ("invariant", item.name.clone()),
    }
}

fn validate_name(name: &str, kind: &str) -> Result<()> {
    if name.is_empty() || name.len() > 256 || name.chars().any(|character| character.is_control() || character.is_whitespace()) {
        return Err(policy_error(format!(
            "{kind} name must be nonempty, at most 256 bytes, and contain no whitespace/control characters"
        )));
    }
    Ok(())
}

fn policy_error(message: impl Into<String>) -> CompileError {
    CompileError::without_span(message).with_code(crate::error::DiagnosticCode::E2101)
}

#[cfg(test)]
pub(crate) fn fixture_metadata(
    source: &str,
    edition: crate::CellScriptEdition,
    target: Option<String>,
) -> Result<crate::CompileMetadata> {
    let module = crate::frontend::parse(source, edition)?;
    for item in &module.items {
        if let crate::ast::Item::Action(action) = item
            && action.next_surface.is_some()
            && let Some(crate::ast::Type::Named(schema)) =
                action.outputs.first().map(|output| &output.ty).or_else(|| {
                    action.params.iter().find(|param| param.source == crate::ast::ParamSource::Input).map(|param| &param.ty)
                })
        {
            return compile_artifact_metadata(
                source,
                crate::CompileOptions { edition, target, ..Default::default() },
                ArtifactDeclaration::single_type_group("fixture", &action.name, schema),
            );
        }
    }
    crate::compile_metadata(source, edition, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration() -> ArtifactDeclaration {
        ArtifactDeclaration {
            name: "token".into(),
            context: ArtifactContext::TypeGroup {},
            bindings: crate::artifact::EntryCellBindings::GroupRelative,
            coverage: crate::artifact::EntryCoverage::FixedGroup,
            layout: Some("Token".into()),
            dispatch: ArtifactDispatch::PolicyWitnessV1,
            action: None,
            actions: vec![ArtifactAction { tag: 17, action: "transfer".into() }, ArtifactAction { tag: 0, action: "mint".into() }],
            common_checks: vec!["require_network".into(), "require_epoch".into()],
        }
    }

    #[test]
    fn numeric_tags_are_explicit_and_common_check_order_is_semantic() {
        let original = declaration();
        let canonical = original.canonicalized().unwrap();
        assert_eq!(canonical.actions.iter().map(|variant| variant.tag).collect::<Vec<_>>(), vec![0, 17]);
        assert_eq!(canonical.common_checks, original.common_checks);
        assert_eq!(canonical.action("transfer").unwrap().tag, 17);
        assert_eq!(canonical.canonicalized().unwrap(), canonical);
    }

    #[test]
    fn duplicate_and_ambiguous_declarations_are_rejected() {
        let base = declaration();
        let mut duplicate_tag = base.clone();
        duplicate_tag.actions[1].tag = 17;
        assert!(duplicate_tag.validate().unwrap_err().message.contains("numeric tag"));
        let mut duplicate_action = base.clone();
        duplicate_action.actions[1].action = "transfer".into();
        assert!(duplicate_action.validate().unwrap_err().message.contains("more than once"));
        let mut duplicate_common = base.clone();
        duplicate_common.common_checks.push("require_epoch".into());
        assert!(duplicate_common.validate().unwrap_err().message.contains("repeats common"));
        let mut exported_common = base.clone();
        exported_common.common_checks.push("mint".into());
        assert!(exported_common.validate().unwrap_err().message.contains("selectable action"));
        assert!(validate_declarations(&[base.clone(), base]).unwrap_err().message.contains("declared more than once"));
    }

    #[test]
    fn declarations_require_an_explicit_dispatch_and_its_own_action_selection() {
        let value = serde_json::to_value(declaration()).unwrap();
        assert_eq!(serde_json::from_value::<ArtifactDeclaration>(value.clone()).unwrap(), declaration());
        for field in ["context", "dispatch"] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<ArtifactDeclaration>(missing).is_err(), "missing {field}");
        }
        let mut missing_actions = value.clone();
        missing_actions.as_object_mut().unwrap().remove("actions");
        assert!(serde_json::from_value::<ArtifactDeclaration>(missing_actions).unwrap().validate().is_err());
        let mut missing_tag = value;
        missing_tag["actions"][0].as_object_mut().unwrap().remove("tag");
        assert!(serde_json::from_value::<ArtifactDeclaration>(missing_tag).is_err());

        let single = ArtifactDeclaration::single_type_group("single", "transfer", "Token");
        let mut encoded = serde_json::to_value(&single).unwrap();
        assert!(encoded.get("actions").is_none());
        let decoded = serde_json::from_value::<ArtifactDeclaration>(encoded.clone()).unwrap();
        assert_eq!(decoded.canonicalized().unwrap(), single);
        encoded.as_object_mut().unwrap().remove("action");
        assert!(serde_json::from_value::<ArtifactDeclaration>(encoded).unwrap().validate().is_err());
    }
}
