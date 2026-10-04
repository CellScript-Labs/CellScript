//! Private-counter client: checked deployment, final transaction binding and witness placement.
use anyhow::{ensure, Context, Result};
use cellscript::zk_client::{compile_transition_parent, TransitionParent, TransitionParentManifest};
use cellscript_zk_private_counter::{self as counter, wire};
use ckb_types::{bytes::Bytes, core::TransactionView, packed, prelude::*};
use serde::{Deserialize, Serialize};

/// Supplied identities are checked against bytes here and against live Cells by the RPC client.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deployment {
    pub chain_id: String,
    pub genesis_hash: String,
    pub child_tx_hash: String,
    pub child_index: u32,
    pub child_data_hash: String,
    pub verification_key_hash: String,
}

pub fn hash32(value: &str, field: &str) -> Result<[u8; 32]> {
    hex::decode(value.strip_prefix("0x").unwrap_or(value))?
        .try_into()
        .map_err(|_| anyhow::anyhow!("{field}: expected a 32-byte hex hash"))
}

pub fn parent(deployment: &Deployment, child: &[u8], key: &[u8]) -> Result<TransitionParent> {
    compile_transition_parent(
        &TransitionParentManifest {
            chain_id: deployment.chain_id.clone(),
            genesis_hash: hash32(&deployment.genesis_hash, "genesis_hash")?,
            child_tx_hash: hash32(&deployment.child_tx_hash, "child_tx_hash")?,
            child_index: deployment.child_index,
            child_data_hash: hash32(&deployment.child_data_hash, "child_data_hash")?,
            verification_key_hash: hash32(&deployment.verification_key_hash, "verification_key_hash")?,
            package_coordinate: "test/counter-verifier@0.32.0".into(),
            lock_node_id: "counter-verifier-v1".into(),
            module_name: "private_counter".into(),
            action_name: "increment".into(),
            policy: "private_counter".into(),
            domain: counter::domain(),
            action: counter::action(),
        },
        child,
        key,
    )
}

pub fn type_args(input: &packed::CellInput, index: u64, parent: &[u8]) -> Vec<u8> {
    let mut preimage = input.as_slice().to_vec();
    preimage.extend(index.to_le_bytes());
    let mut args = counter::hash(&preimage).to_vec();
    args.extend(counter::hash(parent));
    args
}

/// Holds an immutable snapshot after dependency and fee completion. A wallet may
/// change Lock witnesses only; changing raw fields requires a new proof.
pub struct PreparedIncrement {
    transaction: TransactionView,
    statement: wire::Statement,
    witness_index: usize,
    proved: Option<TransactionView>,
}

impl PreparedIncrement {
    pub fn new(
        transaction: TransactionView,
        resolved_inputs: &[(packed::OutPoint, packed::CellOutput, Bytes)],
        script: &packed::Script,
    ) -> Result<Self> {
        let statement = cellscript_ckb_adapter::zk::transition_statement(
            &transaction,
            resolved_inputs,
            script,
            counter::domain(),
            counter::action(),
        )?;
        let witness_index = resolved_inputs
            .iter()
            .position(|(_, cell, _)| cell.type_().to_opt().as_ref() == Some(script))
            .context("counter input missing")?;
        let (_, old, old_data) = &resolved_inputs[witness_index];
        let output_index = transaction
            .outputs()
            .into_iter()
            .position(|cell| cell.type_().to_opt().as_ref() == Some(script))
            .context("counter output missing")?;
        let new = transaction.outputs().get(output_index).context("counter output missing")?;
        let new_data = transaction.outputs_data().get(output_index).context("counter data missing")?.raw_data();
        ensure!(
            old.lock() == new.lock() && old.capacity() == new.capacity(),
            "counter update must preserve Lock and capacity; use a separate fee input"
        );
        ensure!(
            old_data.len() == 48 && new_data.len() == 48 && &old_data[..8] == b"CSZKCNT1" && old_data[..40] == new_data[..40],
            "counter update must preserve the 48-byte state owner and magic"
        );
        let old_counter = u64::from_le_bytes(old_data[40..].try_into()?);
        let new_counter = u64::from_le_bytes(new_data[40..].try_into()?);
        ensure!(old_counter.checked_add(1) == Some(new_counter), "counter update must increment exactly once without overflow");
        // Both crates expose the same checked profile; decode avoids a cross-crate type assumption.
        let statement =
            wire::Statement::decode(&statement.encode()).map_err(|error| anyhow::anyhow!("ZK statement encoding: {error:?}"))?;
        Ok(Self { transaction, statement, witness_index, proved: None })
    }

    pub fn statement(&self) -> &wire::Statement {
        &self.statement
    }

    /// Performs native Groth16 verification before installing the metadata-encoded payload.
    pub fn attach_proof(
        &mut self,
        compiled: &cellscript::CompileResult,
        handle: &[u8],
        key: &[u8],
        proof: &[u8],
    ) -> Result<TransactionView> {
        self.proved = None;
        let contracts: Vec<_> =
            compiled.metadata.runtime.zk_verifiers.iter().filter(|contract| contract.entry == "action:increment").collect();
        let [contract] = contracts.as_slice() else {
            anyhow::bail!("increment must have exactly one ZK contract");
        };
        ensure!(contract.verification_key_hash == hex::encode(counter::hash(key)), "VK differs from compiled parent metadata");
        ensure!(
            contract.exact_handle_hash == hex::encode(counter::hash(handle)),
            "exact handle differs from compiled parent metadata"
        );
        ensure!(
            contract.domain == hex::encode(counter::domain()) && contract.action == hex::encode(counter::action()),
            "counter domain/action differs from compiled parent metadata"
        );
        counter::verify(key, &self.statement, proof)
            .context("counter proof rejected locally: check secret, VK and finalized transaction")?;
        let action =
            compiled.metadata.actions.iter().find(|action| action.name == "increment").context("increment metadata missing")?;
        let payload = action.entry_witness_args(&[
            cellscript::EntryWitnessArg::Bytes(proof.to_vec()),
            cellscript::EntryWitnessArg::Bytes(handle.to_vec()),
        ])?;
        let mut witnesses: Vec<_> = self.transaction.witnesses().into_iter().collect();
        witnesses.resize_with(witnesses.len().max(self.witness_index + 1), || Bytes::new().pack());
        let previous = witnesses[self.witness_index].raw_data();
        let args = if previous.is_empty() {
            packed::WitnessArgs::default()
        } else {
            packed::WitnessArgs::from_slice(&previous).context("counter witness is not WitnessArgs")?
        };
        witnesses[self.witness_index] = args.as_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes().pack();
        let proved = self.transaction.as_advanced_builder().set_witnesses(witnesses).build();
        self.proved = Some(proved.clone());
        Ok(proved)
    }

    pub fn check_signed(&self, proved: &TransactionView, signed: &TransactionView) -> Result<()> {
        let accepted =
            self.proved.as_ref().context("no completed proof for this prepared transaction; attach proof before signing")?;
        ensure!(
            proved.hash() == self.transaction.hash() && signed.hash() == self.transaction.hash(),
            "ZK raw transaction changed after proving; finalize fees/dependencies and generate a new proof"
        );
        check_witness_fields(accepted, proved)?;
        check_witness_fields(proved, signed)?;
        let witness = signed.witnesses().get(self.witness_index).context("counter witness missing")?.raw_data();
        ensure!(packed::WitnessArgs::from_slice(&witness)?.input_type().to_opt().is_some(), "counter proof witness missing");
        Ok(())
    }
}

fn check_witness_fields(proved: &TransactionView, signed: &TransactionView) -> Result<()> {
    ensure!(proved.witnesses().len() == signed.witnesses().len(), "wallet changed witness count after proving");
    for (before, after) in proved.witnesses().into_iter().zip(signed.witnesses()) {
        if before == after {
            continue;
        }
        let before = packed::WitnessArgs::from_slice(&before.raw_data()).context("original witness is not WitnessArgs")?;
        let after = packed::WitnessArgs::from_slice(&after.raw_data()).context("signed witness is not WitnessArgs")?;
        ensure!(
            before.input_type() == after.input_type() && before.output_type() == after.output_type(),
            "wallet changed a proof or non-Lock witness field"
        );
    }
    Ok(())
}
