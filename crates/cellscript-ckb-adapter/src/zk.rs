//! Builder/prover boundary for the exact single Type-group transition profile.
use anyhow::{bail, Result};
use cellscript_artifact_checker::zk::{Request, Statement, PROOF_BYTES};
use ckb_hash::blake2b_256;
use ckb_types::{
    bytes::Bytes,
    core::TransactionView,
    packed::{CellOutput, OutPoint, Script},
    prelude::*,
};

/// Derive from the finalized raw transaction and its resolved input Cells.
/// Input resolution must be supplied by the caller's chain provider. This
/// function checks correspondence; it does not assert liveness or confirmation.
pub fn transition_statement(
    transaction: &TransactionView,
    resolved_inputs: &[(OutPoint, CellOutput, Bytes)],
    script: &Script,
    domain: [u8; 32],
    action: [u8; 32],
) -> Result<Statement> {
    if transaction.inputs().len() != resolved_inputs.len() {
        bail!("ZK resolved input count differs from transaction");
    }
    let mut selected = None;
    for (input, (outpoint, cell, data)) in transaction.inputs().into_iter().zip(resolved_inputs) {
        if input.previous_output() != *outpoint {
            bail!("ZK resolved input outpoint differs from transaction");
        }
        if cell.type_().to_opt().as_ref() == Some(script) {
            if selected.is_some() {
                bail!("ZK profile requires exactly one group input");
            }
            selected = Some((outpoint, data));
        }
    }
    let (input, old_data) = selected.ok_or_else(|| anyhow::anyhow!("ZK group input missing"))?;
    let outputs: Vec<_> =
        transaction.outputs().into_iter().enumerate().filter(|(_, cell)| cell.type_().to_opt().as_ref() == Some(script)).collect();
    let [(output_index, _)] = outputs.as_slice() else {
        bail!("ZK profile requires exactly one group output");
    };
    let new_data = transaction.outputs_data().get(*output_index).ok_or_else(|| anyhow::anyhow!("ZK output data missing"))?.raw_data();
    Ok(Statement {
        domain,
        action,
        script_hash: script.calc_script_hash().as_slice().try_into()?,
        old_data_hash: blake2b_256(old_data),
        new_data_hash: blake2b_256(&new_data),
        input_transaction_hash: input.tx_hash().as_slice().try_into()?,
        input_output_index: input.index().unpack(),
        transaction_hash: transaction.hash().as_slice().try_into()?,
    })
}

/// Reject prover output for any other statement before witness construction.
/// The caller then uses the compiler's entry_witness_args encoder and must
/// dry-run the final signed transaction; this is not a cryptographic verifier.
pub fn bind_prover_output(statement: Statement, expected_key: [u8; 32], proof: &[u8], public_inputs: &[u8]) -> Result<Request> {
    if proof.len() != PROOF_BYTES || public_inputs != statement.public_inputs() {
        bail!("ZK prover output does not match exact proof width and transaction-derived public inputs");
    }
    Ok(Request { verification_key: expected_key, proof: proof.try_into()?, statement })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ckb_types::{core::TransactionBuilder, packed::CellInput};

    #[test]
    fn finalized_transaction_binding_matches_wire_and_rejects_wrong_resolution() {
        let script = Script::new_builder().code_hash([3u8; 32]).build();
        let cell = CellOutput::new_builder().capacity(100_000_000_000u64).type_(Some(script.clone()).pack()).build();
        let outpoint = OutPoint::new_builder().tx_hash([4u8; 32]).index(7u32).build();
        let old = Bytes::from_static(b"old");
        let new = Bytes::from_static(b"new");
        let tx = TransactionBuilder::default()
            .input(CellInput::new_builder().previous_output(outpoint.clone()).build())
            .output(cell.clone())
            .output_data(new.pack())
            .build();
        let inputs = vec![(outpoint.clone(), cell.clone(), old.clone())];
        let statement = transition_statement(&tx, &inputs, &script, [1; 32], [2; 32]).unwrap();
        assert_eq!(statement.old_data_hash, blake2b_256(&old));
        assert_eq!(statement.new_data_hash, blake2b_256(&new));
        assert_eq!(statement.input_output_index, 7);
        assert_eq!(statement.transaction_hash.as_slice(), tx.hash().as_slice());
        let pi = statement.public_inputs();
        let request = bind_prover_output(statement.clone(), [5; 32], &[9; 128], &pi).unwrap();
        assert_eq!(Request::decode(&request.encode(), &[5; 32]).unwrap(), request);
        let changed = tx.as_advanced_builder().set_outputs_data(vec![old.pack()]).build();
        let changed_statement = transition_statement(&changed, &inputs, &script, [1; 32], [2; 32]).unwrap();
        assert!(bind_prover_output(changed_statement, [5; 32], &[9; 128], &pi).is_err());
        assert!(bind_prover_output(statement, [5; 32], &[9; 127], &pi).is_err());
        let wrong = vec![(OutPoint::default(), cell, old)];
        assert!(transition_statement(&tx, &wrong, &script, [1; 32], [2; 32]).is_err());
    }
}
