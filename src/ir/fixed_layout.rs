//! Checked intrinsic extents for the target's fixed-value representation.
//! Nominal record layouts and dynamic codecs retain their own admission.
use super::*;
use crate::error::SourceDiagnosticCode;

pub(crate) fn checked_fixed_size(size: Option<usize>) -> Result<usize> {
    // Typed artifact widths and captured frame extents are u32. Reject before
    // metadata or emission can truncate an admitted source extent to that wire
    // representation. This is a representation bound, not a memory budget.
    size.filter(|size| u32::try_from(*size).is_ok()).ok_or_else(|| {
        SourceDiagnosticCode::FixedLayoutBounds
            .error("fixed-value layout exceeds the checked compiler or target addressable extent", Span::default())
    })
}

pub(crate) fn validate_fixed_layout(ty: &IrType) -> Result<Option<usize>> {
    let overflow = || {
        SourceDiagnosticCode::FixedLayoutBounds
            .error("fixed-value layout exceeds the checked compiler or target addressable extent", Span::default())
    };
    let width = match ty {
        IrType::Array(inner, length) => {
            validate_fixed_layout(inner)?.map(|width| width.checked_mul(*length).ok_or_else(overflow)).transpose()?
        }
        IrType::Tuple(items) => {
            let mut width = 0usize;
            let mut fixed = true;
            for item in items {
                match validate_fixed_layout(item)? {
                    Some(size) => width = width.checked_add(size).ok_or_else(overflow)?,
                    None => fixed = false,
                }
            }
            fixed.then_some(width)
        }
        IrType::Ref(inner) | IrType::MutRef(inner) => validate_fixed_layout(inner)?,
        IrType::Bool | IrType::U8 => Some(1),
        IrType::U16 => Some(2),
        IrType::U32 | IrType::I32 => Some(4),
        IrType::U64 => Some(8),
        IrType::U128 => Some(16),
        IrType::Address | IrType::Hash => Some(32),
        IrType::Unit => Some(0),
        IrType::Named(name) if is_ckb_temporal_scalar_name(name) => Some(8),
        IrType::Named(name) if is_ckb_fixed_hash_domain_name(name) => Some(32),
        IrType::Named(name) if crate::commitment_contract::commitment_inner_type(name).is_some() => Some(32),
        IrType::Named(name) if crate::zk_contract::fixed_width(name).is_some() => crate::zk_contract::fixed_width(name),
        IrType::Named(name) if name == crate::script_handle_contract::EXACT_SCRIPT_HANDLE_TYPE => {
            Some(crate::script_handle_contract::EXACT_SCRIPT_HANDLE_BYTES)
        }
        IrType::Named(name) if name == crate::script_handle_contract::DEPLOYMENT_LINE_HANDLE_TYPE => {
            Some(crate::script_handle_contract::DEPLOYMENT_LINE_HANDLE_BYTES)
        }
        IrType::Named(name) if name.starts_with("InstanceScriptHandle<") => {
            Some(crate::script_handle_contract::INSTANCE_SCRIPT_HANDLE_BYTES)
        }
        IrType::Named(name)
            if matches!(
                name.split('<').next().unwrap_or(name),
                crate::script_handle_contract::OPEN_SCRIPT_HANDLE_TYPE | crate::script_handle_contract::OPEN_VERIFIER_HANDLE_TYPE
            ) =>
        {
            Some(crate::script_handle_contract::OPEN_HANDLE_SELECTION_BYTES)
        }
        IrType::Named(_) | IrType::Nested(_) => None,
    };
    width.map(|width| checked_fixed_size(Some(width))).transpose()
}

/// A shape query for already admitted types, not a substitute for validation.
/// Source checking and executable-surface validation report invalid extents
/// before metadata construction or artifact emission can consume these widths.
pub(crate) fn fixed_layout_width(ty: &IrType) -> Option<usize> {
    validate_fixed_layout(ty).ok().flatten()
}
