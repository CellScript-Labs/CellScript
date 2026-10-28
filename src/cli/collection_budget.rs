//! Native project loading for the shared declaration-only budget explanation.
use crate::allocated_values::budget;
use crate::ast::Item;
use crate::error::Result;
use camino::Utf8Path;
use std::collections::HashSet;

pub(super) fn explain(input: &Utf8Path) -> Result<(serde_json::Value, Vec<String>)> {
    let path = crate::resolve_input_path(input)?;
    let project = crate::load_project_for_entry(&path, None)?;
    let options = crate::CompileOptions {
        edition: project.entry().edition,
        diagnostics: crate::CompilerDiagnostics::default(),
        ..Default::default()
    };
    let diagnostics = crate::project_frontend_diagnostics(&project, &options, true);
    if diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::error::DiagnosticSeverity::Error) {
        return Err(crate::diagnostics_to_compile_error(diagnostics));
    }
    let mut owners = Vec::new();
    for module in &project.modules {
        let mut checker = crate::types::TypeChecker::with_resolver(&project.resolver, &module.ast.name);
        checker.check_module(&module.ast)?;
        for item in &module.ast.items {
            if let Item::Collections(def) = item {
                owners.push(budget::analyze(&module.ast.name, def, |ty| checker.allocated_value_width(ty, &mut HashSet::new()))?);
            }
        }
    }
    owners.sort_by(|left, right| (&left.module, &left.name).cmp(&(&right.module, &right.name)));
    let mut lines = vec!["Experimental collection budgets (declaration scenarios, not execution guarantees)".into()];
    if owners.is_empty() {
        lines.push("No named collections declarations found.".into());
    }
    for owner in &owners {
        lines.extend(owner.lines());
    }
    let assumptions = budget::ASSUMPTIONS;
    lines.extend(assumptions.iter().map(|text| text.to_string()));
    Ok((
        serde_json::json!({
            "schema": "cellscript-collection-budget-report-v1", "status": "ok", "experimental": true,
            "scope": "loaded-project-declarations", "owners": owners, "assumptions": assumptions,
        }),
        lines,
    ))
}
