//! Effect contracts are inferred from resolved, unoptimized IR. Source spelling
//! only selects the already resolved callable; it never classifies an operation.
mod ir;
pub mod resolved;
use crate::ast::Module;
use crate::error::{CompileError, Result};
use crate::resolve::ModuleResolver;
use cellscript_artifact_checker::effect_facts::EffectFacts;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub(crate) struct InferredEffects {
    module: String,
    facts: BTreeMap<String, EffectFacts>,
    targets: BTreeMap<String, String>,
}
impl InferredEffects {
    pub(crate) fn get(&self, name: &str) -> Option<EffectFacts> {
        let target = self.targets.get(name).map(String::as_str).unwrap_or(name);
        self.facts.get(target).or_else(|| self.facts.get(target.strip_prefix(&format!("{}::", self.module))?)).cloned()
    }
}

pub(crate) fn infer_module(module: &Module, resolver: Option<&ModuleResolver>) -> Result<InferredEffects> {
    infer_module_diagnostics(module, resolver)
        .map_err(|errors| errors.into_iter().next().unwrap_or_else(|| CompileError::without_span("effect lowering failed")))
}

pub(crate) fn infer_module_diagnostics(
    module: &Module,
    resolver: Option<&ModuleResolver>,
) -> std::result::Result<InferredEffects, Vec<CompileError>> {
    // This entry is used after structural typing. The raw lowering entry must
    // not invoke effect analysis or TypeChecker again.
    let lowered = if let Some(resolver) = resolver {
        crate::ir::lower_with_resolver_diagnostics(module, resolver, &module.name)
    } else {
        crate::ir::IrGenerator::new(module.name.clone()).lower_diagnostics(module)
    }?;
    infer_ir(&lowered).map_err(|error| vec![error])
}

pub(crate) fn infer_ir(module: &crate::ir::IrModule) -> Result<InferredEffects> {
    Ok(InferredEffects { module: module.name.clone(), facts: ir::infer(module)?, targets: module.source_callable_targets.clone() })
}

pub(crate) fn populate(module: &mut crate::ir::IrModule) -> Result<InferredEffects> {
    let inferred = infer_ir(module)?;
    for item in &mut module.items {
        let (name, facts) = match item {
            crate::ir::IrItem::Action(value) => (&value.name, &mut value.effects),
            crate::ir::IrItem::Function(value) => (&value.name, &mut value.effects),
            crate::ir::IrItem::Lock(value) => (&value.name, &mut value.effects),
            _ => continue,
        };
        *facts = inferred.get(name).ok_or_else(|| CompileError::without_span(format!("missing effect contract for '{name}'")))?;
    }
    Ok(inferred)
}

#[cfg(test)]
mod tests;
