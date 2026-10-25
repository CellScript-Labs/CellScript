//! Budget hints use the same checked layouts and scenarios as the CLI. They
//! do not infer execution paths or change allocation policy.
use super::*;
use crate::allocated_values::budget;
use std::collections::HashSet;

impl LspServer {
    pub(super) fn collection_budget_hover(&self, uri: &str, name: &str) -> crate::error::Result<String> {
        let source = self.ast_cache.get(uri).ok_or_else(|| CompileError::without_span("missing parsed document"))?;
        let (module, resolver) = if source.items.iter().any(|item| matches!(item, Item::Use(_))) {
            // Resolve and specialize the opened dependency buffers together,
            // preserving package visibility and imported record widths.
            let mut modules = self.workspace_modules(uri);
            let index = modules
                .iter()
                .position(|module| module.ast.name == source.name)
                .ok_or_else(|| CompileError::without_span("missing project entry"))?;
            let resolver = crate::build_module_resolver_from_loaded_modules(&modules)?;
            crate::monomorphize_loaded_project(&mut modules, index, &resolver)?;
            let resolver = crate::build_module_resolver_from_loaded_modules(&modules)?;
            (modules[index].ast.clone(), Some(resolver))
        } else {
            // In-memory editor/browser documents need no filesystem project.
            (crate::generics::monomorphize_for_edition(source, self.document_edition(uri))?, None)
        };
        let mut checker = match &resolver {
            Some(resolver) => crate::types::TypeChecker::with_resolver(resolver, &module.name),
            None => crate::types::TypeChecker::new(),
        };
        checker.check_module(&module)?;
        let declaration = module
            .items
            .iter()
            .find_map(|item| match item {
                Item::Collections(def) if def.name == name => Some(def),
                _ => None,
            })
            .ok_or_else(|| CompileError::without_span("missing collection declaration"))?;
        let report = budget::analyze(&module.name, declaration, |ty| checker.allocated_value_width(ty, &mut HashSet::new()))?;
        Ok(format!(
            "\n\n`items` is a capacity limit, not an initial reservation.\n\n```text\n{}\n```\n\n{}",
            report.lines().join("\n"),
            budget::ASSUMPTIONS.join("\n\n"),
        ))
    }
}
