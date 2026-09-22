//! Explicit availability and accounting for the pinned CKB Script scheduler.
//!
//! Context::verify_tx remains the transaction accept/reject oracle. The replay
//! below mirrors ckb-testtool 1.1.1's resolver and development consensus, then
//! uses ckb-script 1.1.0's detailed_run to retain nonzero-exit cycle counts.

use std::sync::Arc;

use ckb_testtool::{
    ckb_chain_spec::consensus::ConsensusBuilder,
    ckb_script::{ScriptError, ScriptGroupType, TransactionScriptsVerifier, TxVerifyEnv},
    ckb_types::{
        core::{
            cell::{CellMeta, CellMetaBuilder, ResolvedTransaction},
            hardfork::{HardForks, CKB2021, CKB2023},
            DepType, HeaderBuilder, TransactionView,
        },
        packed,
        prelude::*,
    },
    context::Context,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CycleScope {
    TransactionScripts,
    ScriptGroupSchedulerIncludingChildren,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExitCategory {
    Success,
    NonzeroExit,
    CycleLimit,
    VmTrap,
    SetupFailure,
    TransactionRejected,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum CycleObservation {
    Measured { cycles: u64, exit_code: i8 },
    Unavailable { reason: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GroupIdentity {
    pub script_hash: String,
    pub role: String,
    pub input_indices: Vec<usize>,
    pub output_indices: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CycleMeasurement {
    pub scope: CycleScope,
    pub group: Option<GroupIdentity>,
    pub exit_category: ExitCategory,
    pub observation: CycleObservation,
}

impl CycleMeasurement {
    pub fn required_cycles(&self) -> u64 {
        match &self.observation {
            CycleObservation::Measured { cycles, .. } if *cycles > 0 => *cycles,
            other => panic!("required cycle measurement is unavailable or empty: {other:?}"),
        }
    }

    pub fn transaction(exit_code: i64, cycles: u64) -> Self {
        let (exit_category, observation) = if exit_code == 0 {
            assert!(cycles > 0, "successful corpus execution must consume cycles");
            (ExitCategory::Success, CycleObservation::Measured { cycles, exit_code: 0 })
        } else {
            (
                ExitCategory::TransactionRejected,
                CycleObservation::Unavailable { reason: "ordinary transaction verifier does not return cycles on rejection".into() },
            )
        };
        Self { scope: CycleScope::TransactionScripts, group: None, exit_category, observation }
    }
}

fn resolve_cell(context: &Context, out_point: &packed::OutPoint) -> Result<CellMeta, String> {
    let (output, data) = context.cells.get(out_point).ok_or_else(|| format!("unresolved Cell {out_point:?}"))?;
    let mut builder = CellMetaBuilder::from_cell_output(output.clone(), data.clone()).out_point(out_point.clone());
    if let Some(info) = context.transaction_infos.get(out_point) {
        builder = builder.transaction_info(info.clone());
    }
    Ok(builder.build())
}

fn resolve_transaction(context: &Context, tx: &TransactionView) -> Result<ResolvedTransaction, String> {
    let resolved_inputs =
        tx.inputs().into_iter().map(|input| resolve_cell(context, &input.previous_output())).collect::<Result<_, _>>()?;
    let mut resolved_cell_deps = Vec::new();
    let mut resolved_dep_groups = Vec::new();
    for dep in tx.cell_deps() {
        let cell = resolve_cell(context, &dep.out_point())?;
        if dep.dep_type() == DepType::DepGroup.into() {
            let data = cell.mem_cell_data.as_ref().ok_or("missing dependency group data")?;
            let members = packed::OutPointVec::from_slice(data).map_err(|error| format!("malformed dependency group: {error}"))?;
            resolved_dep_groups.push(cell);
            for member in members {
                resolved_cell_deps.push(resolve_cell(context, &member)?);
            }
        } else if dep.dep_type() == DepType::Code.into() {
            resolved_cell_deps.push(cell);
        } else {
            return Err("unsupported dependency type".into());
        }
    }
    Ok(ResolvedTransaction { transaction: tx.clone(), resolved_inputs, resolved_cell_deps, resolved_dep_groups })
}

pub fn measure_group(
    context: &Context,
    tx: &TransactionView,
    script: &packed::Script,
    role: ScriptGroupType,
    max_cycles: u64,
) -> CycleMeasurement {
    let mut measurement = CycleMeasurement {
        scope: CycleScope::ScriptGroupSchedulerIncludingChildren,
        group: Some(GroupIdentity {
            script_hash: format!("0x{}", hex::encode(script.calc_script_hash().as_slice())),
            role: role.to_string().to_lowercase(),
            input_indices: Vec::new(),
            output_indices: Vec::new(),
        }),
        exit_category: ExitCategory::SetupFailure,
        observation: CycleObservation::Unavailable { reason: "selected Script group is absent".into() },
    };
    let resolved = match resolve_transaction(context, tx) {
        Ok(resolved) => resolved,
        Err(reason) => {
            measurement.observation = CycleObservation::Unavailable { reason };
            return measurement;
        }
    };
    // Keep this configuration identical to Context::verify_tx in the locked
    // ckb-testtool source. In particular, don't substitute a mainnet tip.
    let consensus = ConsensusBuilder::default()
        .hardfork_switch(HardForks { ckb2021: CKB2021::new_dev_default(), ckb2023: CKB2023::new_dev_default() })
        .build();
    let tip = HeaderBuilder::default().number(0).build();
    let verifier = TransactionScriptsVerifier::new(
        Arc::new(resolved),
        context.clone(),
        Arc::new(consensus),
        Arc::new(TxVerifyEnv::new_submit(&tip)),
    );
    let Some(group) = verifier.find_script_group(role, &script.calc_script_hash()) else {
        return measurement;
    };
    let identity = measurement.group.as_mut().expect("group measurement identity");
    identity.input_indices = group.input_indices.clone();
    identity.output_indices = group.output_indices.clone();
    match verifier.detailed_run(group, max_cycles) {
        Ok(result) => {
            measurement.exit_category = if result.exit_code == 0 { ExitCategory::Success } else { ExitCategory::NonzeroExit };
            measurement.observation = CycleObservation::Measured { cycles: result.consumed_cycles, exit_code: result.exit_code };
        }
        Err(error) => {
            measurement.exit_category = match error {
                ScriptError::ExceededMaximumCycles(_) => ExitCategory::CycleLimit,
                ScriptError::VMInternalError(_) => ExitCategory::VmTrap,
                _ => ExitCategory::SetupFailure,
            };
            measurement.observation = CycleObservation::Unavailable { reason: error.to_string() };
        }
    }
    measurement
}
