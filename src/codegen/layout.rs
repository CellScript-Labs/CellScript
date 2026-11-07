//! Compiler-owned executable representations, call transport and physical storage.
//!
//! Wire widths remain owned by admitted IR/codecs. These plans describe how
//! those values travel through this backend; they are not checker authority.
use super::*;

mod storage;
#[cfg(test)]
mod tests;
pub(super) use storage::{FunctionStoragePlan, StorageBoundaries};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ParameterRepresentation {
    Scalar,
    TemporalScalar,
    FieldlessEnum { bytes: usize },
    PayloadEnum { bytes: usize },
    FixedNamed { bytes: usize },
    ExactHandle,
    DeploymentLineHandle,
    Commitment,
    SchemaSpan { type_hash: bool },
    FixedBytes { bytes: usize },
    FixedAggregate { bytes: usize },
}

impl ParameterRepresentation {
    pub(super) const fn words(self) -> usize {
        match self {
            Self::Scalar | Self::TemporalScalar => 1,
            Self::SchemaSpan { type_hash: true } => 4,
            Self::FieldlessEnum { .. }
            | Self::PayloadEnum { .. }
            | Self::FixedNamed { .. }
            | Self::ExactHandle
            | Self::DeploymentLineHandle
            | Self::Commitment
            | Self::SchemaSpan { type_hash: false }
            | Self::FixedBytes { .. }
            | Self::FixedAggregate { .. } => 2,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ParameterLayout {
    pub(super) first_word: usize,
    pub(super) representation: ParameterRepresentation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ReturnLayout {
    EntryStatus,
    /// Preserve annotated action return transport without adding the ordinary
    /// function ABI's hidden output-buffer argument to an entry call.
    EntryValue(Box<ReturnLayout>),
    Unit,
    SingleWord,
    WidePair,
    EncodedPair {
        bytes: usize,
    },
    Tuple {
        fields: usize,
    },
    CallerOwned {
        bytes: usize,
    },
}

impl ReturnLayout {
    pub(super) fn buffer_bytes(&self) -> Option<usize> {
        match self {
            Self::CallerOwned { bytes } => Some(*bytes),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct CallableLayout {
    pub(super) parameters: Vec<ParameterLayout>,
    pub(super) input_words: usize,
    pub(super) result: ReturnLayout,
    pub(super) outgoing_stack_bytes: usize,
}

impl CallableLayout {
    pub(super) fn new(
        generator: &CodeGenerator,
        params: &[IrParam],
        type_hash_parameters: &BTreeSet<usize>,
        result: ReturnLayout,
    ) -> Result<Self> {
        let mut input_words = 0usize;
        let mut parameters = Vec::with_capacity(params.len());
        for (index, param) in params.iter().enumerate() {
            let representation = generator.parameter_representation(param, type_hash_parameters.contains(&index));
            parameters.push(ParameterLayout { first_word: input_words, representation });
            input_words = input_words
                .checked_add(representation.words())
                .ok_or_else(|| CompileError::without_span("call ABI machine-argument count exceeds the target addressable extent"))?;
        }
        let total_words = input_words
            .checked_add(usize::from(result.buffer_bytes().is_some()))
            .ok_or_else(|| CompileError::without_span("call ABI result-argument count exceeds the target addressable extent"))?;
        let outgoing_stack_bytes = checked_outgoing_stack_bytes(total_words)?;
        Ok(Self { parameters, input_words, result, outgoing_stack_bytes })
    }

    pub(super) fn hidden_result_word(&self) -> Option<usize> {
        self.result.buffer_bytes().map(|_| self.input_words)
    }

    pub(super) fn check_parameter_start(&self, index: usize, actual: usize, callee: &str, phase: &str) -> Result<()> {
        let expected = self
            .parameters
            .get(index)
            .ok_or_else(|| {
                CompileError::without_span(format!(
                    "compiler {phase}-layout mismatch for {callee}: unexpected source parameter {index}"
                ))
            })?
            .first_word;
        if actual != expected {
            return Err(CompileError::without_span(format!(
                "compiler {phase}-layout mismatch for {callee} parameter {index}: expected machine word {expected}, reached {actual}"
            )));
        }
        Ok(())
    }

    pub(super) fn check_input_words(&self, actual: usize, callee: &str, phase: &str) -> Result<()> {
        if actual != self.input_words {
            return Err(CompileError::without_span(format!(
                "compiler {phase}-layout mismatch for {callee}: expected {} ordinary machine words, reached {actual}",
                self.input_words
            )));
        }
        Ok(())
    }
}

pub(super) fn checked_outgoing_stack_bytes(words: usize) -> Result<usize> {
    let bytes = words.saturating_sub(8).checked_mul(8);
    crate::ir::checked_fixed_size(bytes.and_then(|bytes| bytes.checked_add(15)).map(|bytes| bytes & !15))
}

impl CodeGenerator {
    fn parameter_representation(&self, param: &IrParam, needs_type_hash: bool) -> ParameterRepresentation {
        if is_ckb_temporal_scalar_ir_type(&param.ty) {
            ParameterRepresentation::TemporalScalar
        } else if let Some(bytes) = self.payload_enum_width(&param.ty) {
            ParameterRepresentation::PayloadEnum { bytes }
        } else if let Some(bytes) = self.fieldless_enum_width(&param.ty) {
            ParameterRepresentation::FieldlessEnum { bytes }
        } else if let Some(bytes) = self.fixed_named_parameter_width(&param.ty) {
            ParameterRepresentation::FixedNamed { bytes }
        } else if matches!(&param.ty, IrType::Named(name) if name == crate::script_handle_contract::EXACT_SCRIPT_HANDLE_TYPE) {
            ParameterRepresentation::ExactHandle
        } else if matches!(&param.ty, IrType::Named(name) if name == crate::script_handle_contract::DEPLOYMENT_LINE_HANDLE_TYPE) {
            ParameterRepresentation::DeploymentLineHandle
        } else if matches!(&param.ty, IrType::Commitment(_)) {
            ParameterRepresentation::Commitment
        } else if uses_schema_span_abi(&param.ty) {
            ParameterRepresentation::SchemaSpan { type_hash: needs_type_hash }
        } else if let Some(bytes) = fixed_byte_pointer_param_width(&param.ty) {
            ParameterRepresentation::FixedBytes { bytes }
        } else if let Some(bytes) = fixed_aggregate_pointer_param_width(&param.ty) {
            ParameterRepresentation::FixedAggregate { bytes }
        } else {
            ParameterRepresentation::Scalar
        }
    }

    pub(super) fn function_return_layout(&self, ty: Option<&IrType>) -> ReturnLayout {
        match ty {
            None | Some(IrType::Unit) => ReturnLayout::Unit,
            Some(ty) if self.fixed_struct_return_width(ty).is_some() => {
                ReturnLayout::CallerOwned { bytes: self.fixed_struct_return_width(ty).unwrap() }
            }
            Some(ty) if self.payload_enum_width(ty).is_some() => {
                ReturnLayout::EncodedPair { bytes: self.payload_enum_width(ty).unwrap() }
            }
            Some(IrType::U128) => ReturnLayout::WidePair,
            Some(IrType::Tuple(fields)) => ReturnLayout::Tuple { fields: fields.len() },
            _ => ReturnLayout::SingleWord,
        }
    }

    pub(super) fn validate_return_transport(&self, operand: Option<&IrOperand>) -> Result<()> {
        let Some(abi) = self.current_function.as_ref().and_then(|name| self.callable_abis.get(name)) else {
            return Ok(());
        };
        let expected = match &abi.layout.result {
            ReturnLayout::EntryValue(value) => &**value,
            value => value,
        };
        if matches!(operand, Some(IrOperand::Const(IrConst::Array(values))) if values.iter().any(|value| !matches!(value, IrConst::U8(_))))
        {
            return Err(CompileError::without_span(format!(
                "compiler return-layout mismatch for {}: constant arrays require admitted byte elements",
                self.current_function.as_deref().unwrap_or("<unknown>")
            )));
        }
        let actual = operand.map(|operand| match operand {
            IrOperand::Var(var) => self.function_return_layout(Some(&var.ty)),
            IrOperand::Const(IrConst::Unit) => ReturnLayout::Unit,
            IrOperand::Const(IrConst::U128(_)) => ReturnLayout::WidePair,
            IrOperand::Const(IrConst::Hash(_) | IrConst::Address(_)) => ReturnLayout::CallerOwned { bytes: 32 },
            IrOperand::Const(IrConst::Array(values)) => {
                self.function_return_layout(Some(&IrType::Array(Box::new(IrType::U8), values.len())))
            }
            IrOperand::Const(_) => ReturnLayout::SingleWord,
        });
        let valid = match (expected, actual) {
            (ReturnLayout::EntryStatus, None | Some(ReturnLayout::Unit | ReturnLayout::SingleWord)) => true,
            (ReturnLayout::Unit, None) => true,
            (expected, Some(actual)) => *expected == actual,
            _ => false,
        };
        if !valid {
            return Err(CompileError::without_span(format!(
                "compiler return-layout mismatch for {}: operand differs from the classified result transport",
                self.current_function.as_deref().unwrap_or("<unknown>")
            )));
        }
        Ok(())
    }
}
