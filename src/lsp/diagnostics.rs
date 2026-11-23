use super::{span_to_range, Diagnostic, DiagnosticSeverity};
use crate::ast::{Item, Module};
use crate::error::{CompileError, DiagnosticSeverity as CompilerDiagnosticSeverity};
use camino::Utf8PathBuf;

pub(super) fn diagnostic_from_error(source: &str, error: &CompileError) -> Diagnostic {
    let code = error.code.map(|code| code.as_str().to_owned());
    let code_description = code.as_deref().and_then(|code| {
        crate::error::compiler_error_info_by_code(code).map(|_| {
            format!(
                "https://github.com/CellScript-Labs/CellScript/blob/nightly-0.22/docs/CELLSCRIPT_COMPILER_ERROR_CODES.md#{}",
                code.to_ascii_lowercase()
            )
        })
    });
    Diagnostic {
        range: span_to_range(source, error.span),
        severity: match error.severity {
            CompilerDiagnosticSeverity::Error => DiagnosticSeverity::Error,
            CompilerDiagnosticSeverity::Warning => DiagnosticSeverity::Warning,
        },
        code,
        code_description,
        message: error.message.clone(),
        source: "cellscript".to_string(),
    }
}

pub(super) fn diagnostic_belongs_to_uri(error: &CompileError, uri_path: Option<&Utf8PathBuf>) -> bool {
    match (&error.file, uri_path) {
        // Both paths belong to the captured graph and are already canonical.
        (Some(file), Some(path)) => file == path,
        (Some(_), None) => false,
        (None, _) => true,
    }
}

pub(super) fn lowering_diagnostics(source: &str, module: &Module, metadata: &crate::CompileMetadata) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for action in &metadata.actions {
        if action.elf_compatible && action.fail_closed_runtime_features.is_empty() {
            continue;
        }
        let span = module
            .items
            .iter()
            .find_map(|item| match item {
                Item::Action(def) if def.name == action.name => Some(def.span),
                _ => None,
            })
            .unwrap_or_default();
        diagnostics.push(Diagnostic {
            range: span_to_range(source, span),
            severity: DiagnosticSeverity::Warning,
            code: None,
            code_description: None,
            message: format!(
                "action '{}' {}; fail-closed runtime features: {}; CKB runtime features: {}; CKB accesses: {}",
                action.name,
                lowering_status(action.elf_compatible, &action.fail_closed_runtime_features),
                diagnostic_fail_closed_features(&action.fail_closed_runtime_features, &action.verifier_obligations),
                diagnostic_list(&action.ckb_runtime_features),
                diagnostic_access_list(&action.ckb_runtime_accesses)
            ),
            source: "cellscript-lowering".to_string(),
        });
    }

    for lock in &metadata.locks {
        if lock.elf_compatible && lock.fail_closed_runtime_features.is_empty() {
            continue;
        }
        let span = module
            .items
            .iter()
            .find_map(|item| match item {
                Item::Lock(def) if def.name == lock.name => Some(def.span),
                _ => None,
            })
            .unwrap_or_default();
        diagnostics.push(Diagnostic {
            range: span_to_range(source, span),
            severity: DiagnosticSeverity::Warning,
            code: None,
            code_description: None,
            message: format!(
                "lock '{}' {}; fail-closed runtime features: {}; CKB runtime features: {}; CKB accesses: {}",
                lock.name,
                lowering_status(lock.elf_compatible, &lock.fail_closed_runtime_features),
                diagnostic_fail_closed_features(&lock.fail_closed_runtime_features, &lock.verifier_obligations),
                diagnostic_list(&lock.ckb_runtime_features),
                diagnostic_access_list(&lock.ckb_runtime_accesses)
            ),
            source: "cellscript-lowering".to_string(),
        });
    }

    for function in &metadata.functions {
        if function.elf_compatible && function.fail_closed_runtime_features.is_empty() {
            continue;
        }
        let span = module
            .items
            .iter()
            .find_map(|item| match item {
                Item::Function(def) if def.name == function.name => Some(def.span),
                _ => None,
            })
            .unwrap_or_default();
        diagnostics.push(Diagnostic {
            range: span_to_range(source, span),
            severity: DiagnosticSeverity::Warning,
            code: None,
            code_description: None,
            message: format!(
                "fn '{}' {}; fail-closed runtime features: {}; CKB runtime features: {}; CKB accesses: {}",
                function.name,
                lowering_status(function.elf_compatible, &function.fail_closed_runtime_features),
                diagnostic_fail_closed_features(&function.fail_closed_runtime_features, &function.verifier_obligations),
                diagnostic_list(&function.ckb_runtime_features),
                diagnostic_access_list(&function.ckb_runtime_accesses)
            ),
            source: "cellscript-lowering".to_string(),
        });
    }

    diagnostics
}

fn lowering_status(elf_compatible: bool, features: &[String]) -> &'static str {
    if !elf_compatible {
        "is not currently ELF-compatible"
    } else if !features.is_empty()
        && features.iter().all(|feature| crate::executable_surface::experimental_artifact_admission_detail(feature).is_some())
    {
        "requires experimental artifact admission"
    } else {
        "emits fail-closed runtime traps"
    }
}

fn diagnostic_fail_closed_features(features: &[String], obligations: &[crate::VerifierObligationMetadata]) -> String {
    let descriptions = features
        .iter()
        .map(|feature| {
            obligations
                .iter()
                .find(|obligation| obligation.feature == *feature && obligation.status == "fail-closed")
                .map(|obligation| format!("{} ({})", feature, obligation.detail))
                .unwrap_or_else(|| feature.clone())
        })
        .collect::<Vec<_>>();
    diagnostic_list(&descriptions)
}

fn diagnostic_list(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(", ")
    }
}

fn diagnostic_access_list(accesses: &[crate::CkbRuntimeAccessMetadata]) -> String {
    if accesses.is_empty() {
        return "none".to_string();
    }
    accesses
        .iter()
        .map(|access| {
            let index = match access.provenance.index.kind.as_str() {
                "dynamic" => format!("${}", access.provenance.index.binding.as_deref().unwrap_or("?")),
                "bounded-scan" => format!("0..={}", access.provenance.index.max_inclusive.unwrap_or_default()),
                "not-applicable" => "n/a".to_string(),
                _ => access.provenance.index.value.unwrap_or(access.index as u64).to_string(),
            };
            format!("{}:{}#{} ({})", access.operation, access.source, index, access.binding)
        })
        .collect::<Vec<_>>()
        .join(", ")
}
