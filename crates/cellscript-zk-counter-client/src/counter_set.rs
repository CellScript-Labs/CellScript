//! Prepared bounded counter-set transition. Live input/code resolution and
//! consensus acceptance remain obligations of the caller's chain provider.
use crate::{check_witness_fields, counter, PreparedIncrement};
use anyhow::{ensure, Context, Result};
use cellscript::zk_client::TransitionParent;
use cellscript_ckb_adapter::counter_set::{counter_set_statement, CounterSetStatement};
use ckb_types::{bytes::Bytes, core::TransactionView, packed, prelude::*};

pub fn family_id(first_input: &packed::CellInput, first_output_index: u64) -> [u8; 32] {
    let mut bytes = first_input.as_slice().to_vec();
    bytes.extend(first_output_index.to_le_bytes());
    counter::hash(&bytes)
}

pub fn script(lifecycle_hash: [u8; 32], family: [u8; 32], lane: u8, parent_hash: [u8; 32]) -> Result<packed::Script> {
    ensure!(lane < 4 && parent_hash != [0; 32], "counter set lane/parent bound");
    let mut args = b"CSZKSET1".to_vec();
    args.extend(family);
    args.push(lane);
    args.extend(parent_hash);
    Ok(packed::Script::new_builder().code_hash(lifecycle_hash).hash_type(4u8).args(Bytes::from(args).pack()).build())
}

pub struct PreparedCounterSet {
    transaction: TransactionView,
    inputs: Vec<(packed::OutPoint, packed::CellOutput, Bytes)>,
    derived: CounterSetStatement,
    statements: Vec<counter::wire::Statement>,
    proved: Option<TransactionView>,
}
impl PreparedCounterSet {
    pub fn new(
        transaction: TransactionView,
        inputs: &[(packed::OutPoint, packed::CellOutput, Bytes)],
        selected: &packed::Script,
    ) -> Result<Self> {
        let derived = counter_set_statement(&transaction, inputs, selected)?;
        let statements = derived
            .leaves
            .iter()
            .map(|leaf| {
                counter::wire::Statement::decode(&leaf.statement.encode())
                    .map_err(|_| anyhow::anyhow!("counter set leaf codec mismatch"))
            })
            .collect::<Result<_>>()?;
        Ok(Self { transaction, inputs: inputs.into(), derived, statements, proved: None })
    }
    pub fn statements(&self) -> &[counter::wire::Statement] {
        &self.statements
    }
    pub fn encoded_statement(&self) -> &[u8] {
        &self.derived.encoded
    }

    /// Verify every proof against its own actual input and the identical frozen
    /// raw transaction. A failure invalidates the entire prepared proof result.
    pub fn attach_proofs(&mut self, parent: &TransitionParent, key: &[u8], proofs: &[Vec<u8>]) -> Result<TransactionView> {
        self.proved = None;
        ensure!(proofs.len() == self.statements.len(), "counter set complete proof count required");
        parent.compiled.validate()?;
        let parent_hash = counter::hash(cellscript::strip_vm_abi_trailer(&parent.compiled.artifact_bytes));
        let mut tx = self.transaction.clone();
        for (script, proof) in self.derived.scripts.iter().zip(proofs) {
            ensure!(script.args().raw_data()[41..] == parent_hash, "counter set parent differs from immutable Script args");
            let mut leaf = PreparedIncrement::new(tx, &self.inputs, script)?;
            tx = leaf.attach_proof(&parent.compiled, &parent.handle, key, proof)?;
        }
        ensure!(tx.data().as_slice().len() <= 16_384, "counter set witness/transaction byte budget");
        self.proved = Some(tx.clone());
        Ok(tx)
    }
    pub fn check_signed(&self, supplied_proved: &TransactionView, signed: &TransactionView) -> Result<()> {
        let accepted = self.proved.as_ref().context("counter set has no complete proof result")?;
        ensure!(
            supplied_proved.hash() == self.transaction.hash() && signed.hash() == self.transaction.hash(),
            "counter set raw transaction changed; rebuild and reprove every leaf"
        );
        ensure!(signed.data().as_slice().len() <= 16_384, "counter set signed transaction byte budget");
        check_witness_fields(accepted, supplied_proved)?;
        check_witness_fields(supplied_proved, signed)
    }
}
