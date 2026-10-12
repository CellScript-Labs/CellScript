//! Native packed-CKB derivation, cross-checked by the compiler-independent reader.
use anyhow::{ensure, Result};
pub use cellscript_artifact_checker::code_origin::{
    check_counter_set, decode_counter_set, encode_counter_set, CheckedCounterSet, CounterSetLeaf, SuppliedInputCell,
    COUNTER_SET_PROFILE,
};
use ckb_hash::blake2b_256 as hash;
use ckb_types::{
    bytes::Bytes,
    core::TransactionView,
    packed::{CellOutput, OutPoint, Script},
    prelude::*,
};

pub struct CounterSetStatement {
    pub family: [u8; 32],
    pub leaves: Vec<CounterSetLeaf>,
    pub scripts: Vec<Script>,
    pub encoded: Vec<u8>,
}

pub fn counter_set_statement(
    tx: &TransactionView,
    inputs: &[(OutPoint, CellOutput, Bytes)],
    selected: &Script,
) -> Result<CounterSetStatement> {
    ensure!(
        tx.data().as_slice().len() <= 16_384
            && tx.inputs().len() <= 64
            && tx.outputs().len() <= 64
            && tx.cell_deps().len() <= 64
            && inputs.len() == tx.inputs().len(),
        "counter set transaction bounds/resolution"
    );
    let selected_args = selected.args().raw_data();
    ensure!(
        selected_args.len() == 73
            && &selected_args[..8] == b"CSZKSET1"
            && selected_args[40] < 4
            && selected_args[41..] != [0; 32]
            && selected.hash_type().as_slice() == [4],
        "counter set selected Script"
    );
    let family: [u8; 32] = selected_args[8..40].try_into()?;
    let member = |cell: &CellOutput| -> Result<Option<(u8, Script)>> {
        let Some(script) = cell.type_().to_opt() else { return Ok(None) };
        if script.code_hash() != selected.code_hash() {
            return Ok(None);
        }
        let args = script.args().raw_data();
        ensure!(args.len() >= 40 && &args[..8] == b"CSZKSET1", "counter set malformed same-code args");
        if args[8..40] != family {
            return Ok(None);
        }
        ensure!(
            script.hash_type().as_slice() == [4] && args.len() == 73 && args[40] < 4 && args[41..] == selected_args[41..],
            "counter set same-family identity drift"
        );
        Ok(Some((args[40], script)))
    };
    let mut protected = Vec::new();
    let mut points = std::collections::BTreeSet::new();
    for (index, (input, (point, cell, data))) in tx.inputs().into_iter().zip(inputs).enumerate() {
        ensure!(input.previous_output() == *point && points.insert(point.as_slice()), "counter set wrong/duplicate input OutPoint");
        ensure!(cell.as_slice().len() <= 4096 && data.len() <= 4096, "counter set input snapshot byte budget");
        if let Some((lane, script)) = member(cell)? {
            ensure!(
                protected.len() < 4 && protected.last().is_none_or(|(previous, _, _)| lane > *previous),
                "counter set input order/cardinality/duplicate"
            );
            protected.push((lane, index, script));
        }
    }
    ensure!(!protected.is_empty() && protected.iter().any(|(_, _, script)| script == selected), "counter set selected input missing");
    let mut outputs = Vec::new();
    for (index, cell) in tx.outputs().into_iter().enumerate() {
        if let Some((lane, script)) = member(&cell)? {
            ensure!(
                outputs.len() < 4 && outputs.last().is_none_or(|(previous, _, _)| lane > *previous),
                "counter set output order/cardinality/duplicate"
            );
            outputs.push((lane, index, script));
        }
    }
    ensure!(protected.len() == outputs.len(), "counter set unmatched successors");
    let mut leaves = Vec::new();
    let mut scripts = Vec::new();
    for ((lane, i, script), (next_lane, o, next_script)) in protected.into_iter().zip(outputs) {
        ensure!(lane == next_lane && script == next_script, "counter set successor identity");
        let (_, old, old_data) = &inputs[i];
        let next = tx.outputs().get(o).unwrap();
        let next_data = tx.outputs_data().get(o).ok_or_else(|| anyhow::anyhow!("counter set output data missing"))?.raw_data();
        ensure!(
            old_data.len() == 48 && next_data.len() == 48 && &old_data[..8] == b"CSZKCNT1" && &next_data[..8] == b"CSZKCNT1",
            "counter set state encoding"
        );
        ensure!(
            old_data[8..40] == next_data[8..40] && old.lock() == next.lock() && old.capacity() == next.capacity(),
            "counter set owner/custody"
        );
        let before = u64::from_le_bytes(old_data[40..].try_into()?);
        let after = u64::from_le_bytes(next_data[40..].try_into()?);
        ensure!(before.checked_add(1) == Some(after), "counter set increment/overflow");
        let statement = crate::zk::transition_statement(
            tx,
            inputs,
            &script,
            hash(b"cellscript-private-counter-v1/domain"),
            hash(b"cellscript-private-counter-v1/increment"),
        )?;
        leaves.push(CounterSetLeaf { input_index: i as u32, output_index: o as u32, statement });
        scripts.push(script);
    }
    let encoded = encode_counter_set(family, &leaves)?;
    let snapshots = inputs
        .iter()
        .map(|(point, cell, data)| SuppliedInputCell {
            out_point: point.as_slice().try_into().unwrap(),
            output: cell.as_slice(),
            data,
        })
        .collect::<Vec<_>>();
    let checked = check_counter_set(tx.data().as_slice(), &snapshots, selected.as_slice())?;
    checked.check_statement(&encoded)?;
    Ok(CounterSetStatement { family, leaves, scripts, encoded })
}
